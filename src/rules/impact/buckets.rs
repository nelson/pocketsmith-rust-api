//! The bucket diffing engine: run the full pipeline for every payee on
//! the committed rules and on a scratch ruleset (committed + mutation),
//! then attribute the per-payee diff at the target stage. The scratch
//! ruleset is built by applying the mutation inside a rolled-back
//! transaction, so row ids are preserved and the committed DB is never
//! modified.

use anyhow::Result;
use rusqlite::Connection;

use super::super::crud;
use super::super::model::Mutation;
use super::super::Stage;
use super::{BucketCount, Buckets, PayeeSample};
use crate::normalise::{normalise, NormalisationResult, PipelineCtx, RuleCache};

/// Compute the dry-run buckets for `mutation` at `stage` over `payees`.
/// Pure with respect to the DB: the mutation is applied inside a
/// transaction that is always rolled back.
pub fn compute_buckets(
    conn: &Connection,
    stage: Stage,
    mutation: &Mutation,
    payees: &[PayeeSample],
) -> Result<Buckets> {
    // Base: committed rules, cold cache.
    let base = run_all(conn, payees);
    compute_buckets_with_base(conn, stage, mutation, payees, &base)
}

/// Run the full pipeline for each payee on the committed rules. Exposed so
/// a long-running host (serve) can cache this expensive pass across
/// re-evaluates and reuse it as the base for [`compute_buckets_with_base`]
/// (editable-rules-ui §4). The base only changes when a rule is committed.
pub fn run_base(conn: &Connection, payees: &[PayeeSample]) -> Vec<NormalisationResult> {
    run_all(conn, payees)
}

/// Like [`compute_buckets`] but reusing a precomputed `base` (the
/// committed-rules pass).
///
/// **Loop stages** (prefix/suffix/expand) re-run the full scratch pass:
/// the loop re-feeds its own output, so a payee far from the edited rule
/// can still change — there's no safe "affected subset".
///
/// **First-match stages** only run the scratch pipeline for the *affected*
/// payees (editable-rules-ui §9): a payee can change vs base only if the
/// candidate matches its cleaned input, or the base attributed it to the
/// edited/deleted rule. Everyone else keeps the base outcome (unchanged).
/// This is the slow part of evaluate, so the subset is the speedup; it's
/// provably identical to the full pass (see the equivalence test).
pub fn compute_buckets_with_base(
    conn: &Connection,
    stage: Stage,
    mutation: &Mutation,
    payees: &[PayeeSample],
    base: &[NormalisationResult],
) -> Result<Buckets> {
    if is_loop_stage(stage) {
        let scratch = {
            let tx = conn.unchecked_transaction()?;
            apply_mutation(&tx, mutation)?;
            let res = run_all(&tx, payees);
            drop(tx);
            res
        };
        return Ok(bucket_loop(stage, payees, base, &scratch));
    }
    compute_first_match_subset(conn, stage, mutation, payees, base)
}

/// First-match buckets via the affected-subset scratch (§9).
fn compute_first_match_subset(
    conn: &Connection,
    stage: Stage,
    mutation: &Mutation,
    payees: &[PayeeSample],
    base: &[NormalisationResult],
) -> Result<Buckets> {
    // The candidate's compiled matcher (Add/Edit); `None` for Delete.
    let cand_re = match mutation {
        Mutation::Add(d) | Mutation::Edit { data: d, .. } => {
            super::tester::candidate_regex(stage, d)
        }
        _ => None,
    };
    // The committed pattern of the edited/deleted rule, to spot the payees
    // the base currently attributes to it.
    let old_pattern: Option<String> = match mutation {
        Mutation::Edit { id, .. } | Mutation::Delete { id, .. } => {
            crud::get(conn, stage, *id)?.and_then(|r| r.data.pattern().map(|p| p.to_string()))
        }
        _ => None,
    };
    let tag = trace_name(stage);
    let owned = |res: &NormalisationResult| -> bool {
        match &old_pattern {
            Some(op) => res.trace.iter().any(|t| {
                t.stage == tag && t.match_info.as_ref().map(|m| m.pattern.as_str()) == Some(op.as_str())
            }),
            None => false,
        }
    };

    let mut newly_matched = BucketCount::default();
    let mut stolen = BucketCount::default();
    let mut new_fallthrough = BucketCount::default();
    let mut unchanged_payees = 0i64;

    let tx = conn.unchecked_transaction()?;
    apply_mutation(&tx, mutation)?;
    let scratch_cache = RuleCache::new();
    let sctx = PipelineCtx::new(&tx, &scratch_cache);

    for (i, p) in payees.iter().enumerate() {
        let b = stage_match(&base[i], stage);
        let affected = owned(&base[i])
            || cand_re.as_ref().map(|re| re.is_match(&base[i].matcher_input)).unwrap_or(false);
        if !affected {
            // Only this rule changed and it neither matches nor owned this
            // payee → its first-match outcome is identical to base.
            unchanged_payees += 1;
            continue;
        }
        let s = stage_match(&normalise(&p.original_payee, &sctx), stage);
        match (&b, &s) {
            (None, None) => unchanged_payees += 1,
            (Some(bc), Some(sc)) if bc == sc => unchanged_payees += 1,
            (None, Some(sc)) => newly_matched.add(p, None, Some(sc.clone())),
            (Some(bc), None) => new_fallthrough.add(p, Some(bc.clone()), None),
            (Some(bc), Some(sc)) => stolen.add(p, Some(bc.clone()), Some(sc.clone())),
        }
    }
    drop(tx);
    newly_matched.finish();
    stolen.finish();
    new_fallthrough.finish();
    Ok(Buckets::FirstMatch { newly_matched, stolen, new_fallthrough, unchanged_payees })
}

/// Stages whose order/loop semantics give them the 2-bucket model.
fn is_loop_stage(stage: Stage) -> bool {
    matches!(stage, Stage::Prefixes | Stage::Suffixes | Stage::Expansions)
}

/// The pipeline trace `stage` tag for a [`Stage`].
fn trace_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Prefixes => "prefix",
        Stage::Suffixes => "suffix",
        Stage::Expansions => "expand",
        Stage::Locations => "locations",
        Stage::Persons => "persons",
        Stage::Employers => "employers",
        Stage::Merchants => "merchants",
        Stage::BankingOps => "banking_ops",
    }
}

/// The canonical a first-match / additive stage produced in one run, or
/// `None` if the stage didn't match. Identity is the **output**
/// (canonical), not the rule pattern: editing a rule's pattern while it
/// keeps producing the same canonical is "unchanged", whereas a payee
/// whose canonical changes from one rule's output to another's is
/// "stolen" — matching the mockup semantics (rule-cli §3.4, §5.2).
fn stage_match(result: &NormalisationResult, stage: Stage) -> Option<String> {
    let name = trace_name(stage);
    let entry = result.trace.iter().find(|t| t.stage == name)?;
    // The stage fired (has a trace entry); its canonical is the feature
    // it populated. Guard on match_info so a string-only effect doesn't
    // count as a "match" for the first-match model.
    entry.match_info.as_ref()?;
    let canonical = entry
        .feature_values
        .iter()
        .find(|(k, _)| matches!(*k, "entity_name" | "operation" | "location" | "region"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    Some(canonical)
}

/// A loop stage's net `(before, after)` transformation in one run, or
/// `None` if the stage didn't fire.
fn stage_signature(result: &NormalisationResult, stage: Stage) -> Option<(String, String)> {
    let name = trace_name(stage);
    result.trace.iter().find(|t| t.stage == name).map(|e| (e.before.clone(), e.after.clone()))
}

/// Apply a mutation to `conn` via the typed CRUD ops (used inside a
/// rolled-back transaction by [`compute_buckets`]).
fn apply_mutation(conn: &Connection, m: &Mutation) -> Result<()> {
    match m {
        Mutation::Add(data) => {
            crud::insert_rule(conn, data)?;
        }
        Mutation::Edit { id, data } => crud::update_rule(conn, *id, data)?,
        Mutation::Delete { stage, id } => crud::delete_rule(conn, *stage, *id)?,
        Mutation::Move { stage, id, target } => crud::move_rule(conn, *stage, *id, *target)?,
    }
    Ok(())
}

/// Run the whole pipeline for each payee, returning the per-payee result.
fn run_all(conn: &Connection, payees: &[PayeeSample]) -> Vec<NormalisationResult> {
    let cache = RuleCache::new();
    let ctx = PipelineCtx::new(conn, &cache);
    payees.iter().map(|p| normalise(&p.original_payee, &ctx)).collect()
}

/// Full (non-subset) scratch computation — the reference oracle the
/// equivalence test compares the §9 fast path against.
#[cfg(test)]
pub(crate) fn compute_buckets_full(
    conn: &Connection,
    stage: Stage,
    mutation: &Mutation,
    payees: &[PayeeSample],
) -> Result<Buckets> {
    let base = run_all(conn, payees);
    let scratch = {
        let tx = conn.unchecked_transaction()?;
        apply_mutation(&tx, mutation)?;
        let r = run_all(&tx, payees);
        drop(tx);
        r
    };
    Ok(if is_loop_stage(stage) {
        bucket_loop(stage, payees, &base, &scratch)
    } else {
        bucket_first_match(stage, payees, &base, &scratch)
    })
}

#[cfg(test)]
fn bucket_first_match(
    stage: Stage,
    payees: &[PayeeSample],
    base: &[NormalisationResult],
    scratch: &[NormalisationResult],
) -> Buckets {
    let mut newly_matched = BucketCount::default();
    let mut stolen = BucketCount::default();
    let mut new_fallthrough = BucketCount::default();
    let mut unchanged_payees = 0i64;

    for (i, p) in payees.iter().enumerate() {
        let b = stage_match(&base[i], stage);
        let s = stage_match(&scratch[i], stage);
        match (&b, &s) {
            (None, None) => unchanged_payees += 1,
            (Some(bc), Some(sc)) if bc == sc => unchanged_payees += 1, // same output
            (None, Some(sc)) => newly_matched.add(p, None, Some(sc.clone())),
            (Some(bc), None) => new_fallthrough.add(p, Some(bc.clone()), None),
            (Some(bc), Some(sc)) => stolen.add(p, Some(bc.clone()), Some(sc.clone())),
        }
    }
    newly_matched.finish();
    stolen.finish();
    new_fallthrough.finish();
    Buckets::FirstMatch { newly_matched, stolen, new_fallthrough, unchanged_payees }
}

fn bucket_loop(
    stage: Stage,
    payees: &[PayeeSample],
    base: &[NormalisationResult],
    scratch: &[NormalisationResult],
) -> Buckets {
    let mut newly_affected = BucketCount::default();
    let mut no_longer_affected = BucketCount::default();
    let mut unchanged_payees = 0i64;

    for (i, p) in payees.iter().enumerate() {
        let b = stage_signature(&base[i], stage);
        let s = stage_signature(&scratch[i], stage);
        match (b, s) {
            (None, None) => unchanged_payees += 1,
            (Some(bs), Some(ss)) if bs == ss => unchanged_payees += 1,
            (None, Some(ss)) => newly_affected.add(p, Some(ss.0), Some(ss.1)),
            (Some(bs), None) => no_longer_affected.add(p, Some(bs.0), Some(bs.1)),
            // Both fire but differently: the change now affects this payee.
            (Some(_), Some(ss)) => newly_affected.add(p, Some(ss.0), Some(ss.1)),
        }
    }
    newly_affected.finish();
    no_longer_affected.finish();
    Buckets::Loop { newly_affected, no_longer_affected, unchanged_payees }
}
