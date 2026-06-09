//! Dry-run impact engine (rule-cli §3.4) — the heart of "evaluate".
//!
//! [`compute_buckets`] is pure over (committed rules, a candidate
//! [`Mutation`], the payee population): it runs the FULL pipeline for
//! every payee twice — once on the committed rules, once on a scratch
//! ruleset = committed + mutation — and attributes the per-payee diff at
//! the target stage. The scratch ruleset is built by applying the
//! mutation inside a rolled-back transaction, so row ids are preserved
//! and the committed DB is never modified.
//!
//! The same function backs the GUI Evaluate card; the CLI renders the
//! result as text/JSON, the GUI as coloured bucket cards.

use anyhow::Result;
use rusqlite::Connection;

use super::crud;
use super::model::{Mutation, RuleData};
use super::Stage;
use crate::normalise::{normalise, NormalisationResult, PipelineCtx, RuleCache};

/// Default number of sample payees surfaced per bucket (rule-cli §10);
/// the CLI shows this many unless `--all` is given.
pub const SAMPLE_LIMIT: usize = 6;

/// One distinct `original_payee` plus its aggregate weight, the unit
/// `compute_buckets` attributes into buckets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayeeSample {
    pub original_payee: String,
    pub txn_count: i64,
    pub total_cents: i64,
    pub account: Option<String>,
}

/// Load every distinct `original_payee` with its txn count, summed
/// magnitude (in cents), and a representative account name.
pub fn load_payees(conn: &Connection) -> Result<Vec<PayeeSample>> {
    let mut stmt = conn.prepare(
        "SELECT t.original_payee, COUNT(*), \
                CAST(ROUND(SUM(ABS(t.amount)) * 100) AS INTEGER), MIN(a.name) \
           FROM transactions t \
           LEFT JOIN transaction_accounts a ON a.id = t.transaction_account_id \
          WHERE t.original_payee IS NOT NULL \
          GROUP BY t.original_payee \
          ORDER BY t.original_payee",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(PayeeSample {
            original_payee: row.get(0)?,
            txn_count: row.get(1)?,
            total_cents: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
            account: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// One sampled payee in a bucket, with the before/after canonical for
/// the `was: X → now: Y` rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BucketSample {
    pub original_payee: String,
    pub txn_count: i64,
    pub total_cents: i64,
    pub account: Option<String>,
    /// Previous canonical at this stage (for stolen / fallthrough).
    pub was: Option<String>,
    /// New canonical at this stage, or `None` when now unmatched.
    pub now: Option<String>,
}

/// Aggregate weight + samples for one bucket.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BucketCount {
    pub payees: i64,
    pub txns: i64,
    pub total_cents: i64,
    /// All affected payees (the renderer truncates to [`SAMPLE_LIMIT`]
    /// unless `--all`). Sorted by txn_count desc, then total_cents desc.
    pub samples: Vec<BucketSample>,
}

impl BucketCount {
    fn add(&mut self, p: &PayeeSample, was: Option<String>, now: Option<String>) {
        self.payees += 1;
        self.txns += p.txn_count;
        self.total_cents += p.total_cents;
        self.samples.push(BucketSample {
            original_payee: p.original_payee.clone(),
            txn_count: p.txn_count,
            total_cents: p.total_cents,
            account: p.account.clone(),
            was,
            now,
        });
    }

    fn finish(&mut self) {
        self.samples
            .sort_by(|a, b| b.txn_count.cmp(&a.txn_count).then(b.total_cents.cmp(&a.total_cents)));
    }
}

/// The bucketed dry-run result. First-match / additive stages produce
/// four buckets; loop stages produce two (rule-cli §3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Buckets {
    FirstMatch {
        newly_matched: BucketCount,
        stolen: BucketCount,
        new_fallthrough: BucketCount,
        unchanged_payees: i64,
    },
    Loop {
        newly_affected: BucketCount,
        no_longer_affected: BucketCount,
        unchanged_payees: i64,
    },
}

impl Buckets {
    /// Total payees that would re-stage (everything except unchanged) —
    /// the headline "N payees would re-stage" the CLI prints.
    pub fn changed_payees(&self) -> i64 {
        match self {
            Buckets::FirstMatch { newly_matched, stolen, new_fallthrough, .. } => {
                newly_matched.payees + stolen.payees + new_fallthrough.payees
            }
            Buckets::Loop { newly_affected, no_longer_affected, .. } => {
                newly_affected.payees + no_longer_affected.payees
            }
        }
    }
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

    // Scratch: apply the mutation, run again, then roll back.
    let scratch = {
        let tx = conn.unchecked_transaction()?;
        apply_mutation(&tx, mutation)?;
        let res = run_all(&tx, payees);
        // Drop without commit → ROLLBACK; the committed DB is untouched.
        drop(tx);
        res
    };

    if is_loop_stage(stage) {
        Ok(bucket_loop(stage, payees, &base, &scratch))
    } else {
        Ok(bucket_first_match(stage, payees, &base, &scratch))
    }
}

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

/// Outcome of the single-string tester (rule-cli §3.4, the GUI inline
/// tester / CLI `rule test`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestResult {
    /// The candidate matched; `canonical` is its output (empty for
    /// stages without one), `span` the matched byte range in `input`.
    Matches { canonical: String, span: Option<(usize, usize)> },
    Misses,
    SyntaxError(String),
}

/// Test a single candidate rule against one input string, mirroring how
/// the stage compiles its pattern at runtime.
pub fn test_one(_conn: &Connection, stage: Stage, candidate: &RuleData, input: &str) -> TestResult {
    let pattern = match candidate.pattern() {
        Some(p) => p,
        // Locations match on text, not a pattern.
        None => return test_location(candidate, input),
    };
    let re = match compile_for_stage(stage, pattern) {
        Ok(re) => re,
        Err(e) => return TestResult::SyntaxError(e.to_string()),
    };
    match re.find(input) {
        Some(m) => TestResult::Matches {
            canonical: candidate.canonical().unwrap_or("").to_string(),
            span: Some((m.start(), m.end())),
        },
        None => TestResult::Misses,
    }
}

fn test_location(candidate: &RuleData, input: &str) -> TestResult {
    let loc = candidate.canonical().unwrap_or("");
    let re = match regex::Regex::new(&format!(r"(?i)\b{}\b", regex::escape(loc))) {
        Ok(re) => re,
        Err(e) => return TestResult::SyntaxError(e.to_string()),
    };
    match re.find(input) {
        Some(m) => TestResult::Matches { canonical: loc.to_string(), span: Some((m.start(), m.end())) },
        None => TestResult::Misses,
    }
}

/// Compile a candidate's pattern the way `stage` does at runtime: raw
/// regex for prefix/suffix/merchant/employer/banking_ops; escaped
/// word-boundary literal for expansion/person.
fn compile_for_stage(stage: Stage, pattern: &str) -> Result<regex::Regex, regex::Error> {
    match stage {
        Stage::Expansions => regex::Regex::new(&format!("(?i)\\b{}\\b", regex::escape(pattern))),
        Stage::Persons => {
            regex::Regex::new(&format!(r"(?i)\b{}(?:\b|\s|$)", regex::escape(pattern)))
        }
        _ => regex::Regex::new(pattern),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;
    use crate::rules::model::Mutation;
    use crate::test_support::{seed_account, seed_txn};

    fn merchant(canonical: &str, pattern: &str) -> RuleData {
        RuleData::Merchant { canonical: canonical.into(), pattern: pattern.into(), note: None }
    }

    #[test]
    fn add_first_match_newly_matched() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "CommBank Everyday").unwrap();
        seed_txn(&conn, 1, 1, "BUNNINGS 391 KOTARA", "BUNNINGS 391 KOTARA").unwrap();
        seed_txn(&conn, 2, 1, "WOOLWORTHS", "WOOLWORTHS").unwrap();

        let m = Mutation::Add(merchant("Bunnings", "(?i)BUNNINGS"));
        let payees = load_payees(&conn).unwrap();
        let b = compute_buckets(&conn, Stage::Merchants, &m, &payees).unwrap();
        match b {
            Buckets::FirstMatch { newly_matched, stolen, new_fallthrough, .. } => {
                assert_eq!(newly_matched.payees, 1);
                assert_eq!(newly_matched.samples[0].original_payee, "BUNNINGS 391 KOTARA");
                assert_eq!(newly_matched.samples[0].now.as_deref(), Some("Bunnings"));
                assert_eq!(newly_matched.samples[0].account.as_deref(), Some("CommBank Everyday"));
                assert_eq!(stolen.payees, 0);
                assert_eq!(new_fallthrough.payees, 0);
            }
            _ => panic!("merchants is a first-match stage"),
        }
    }

    #[test]
    fn edit_first_match_stolen_and_fallthrough() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "A").unwrap();
        // Two merchant rules: Uber Eats (specific, no space) ordered before
        // the generic Uber. With the generic Uber present and matching
        // first by id order... persons/merchants match by id order, and the
        // Eats rule has the lower id so it wins where it matches.
        let _eats = crud::insert_rule(&conn, &merchant("Uber Eats", r"(?i)UBEREATS")).unwrap();
        let uber = crud::insert_rule(&conn, &merchant("Uber", "(?i)UBER")).unwrap();
        // "UBEREATS HELP": Eats matches (lower id) in base AND scratch.
        seed_txn(&conn, 1, 1, "UBEREATS HELP", "UBEREATS HELP").unwrap();
        // "UBER TRIP": only generic Uber matches.
        seed_txn(&conn, 2, 1, "UBER TRIP", "UBER TRIP").unwrap();
        // "UBER ONE": only generic Uber matches in base.
        seed_txn(&conn, 3, 1, "UBER ONE", "UBER ONE").unwrap();

        // Edit the generic Uber rule to only match the literal "UBER TRIP",
        // so "UBER ONE" falls through and "UBEREATS HELP" is unaffected.
        let m = Mutation::Edit { id: uber, data: merchant("Uber", "(?i)UBER TRIP") };
        let payees = load_payees(&conn).unwrap();
        let b = compute_buckets(&conn, Stage::Merchants, &m, &payees).unwrap();
        match b {
            Buckets::FirstMatch { newly_matched, stolen, new_fallthrough, unchanged_payees } => {
                assert_eq!(newly_matched.payees, 0);
                assert_eq!(stolen.payees, 0, "no payee's canonical switched rules");
                // UBER ONE: was Uber, now unmatched.
                assert_eq!(new_fallthrough.payees, 1);
                assert_eq!(new_fallthrough.samples[0].original_payee, "UBER ONE");
                assert_eq!(new_fallthrough.samples[0].was.as_deref(), Some("Uber"));
                assert_eq!(new_fallthrough.samples[0].now, None);
                // UBEREATS HELP (Uber Eats) + UBER TRIP (still Uber) unchanged.
                assert_eq!(unchanged_payees, 2);
            }
            _ => panic!("first-match"),
        }
    }

    #[test]
    fn edit_first_match_stolen_when_canonical_changes() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "A").unwrap();
        // Generic "Uber" matches "UBER EATS" in the base (Eats rule needs no
        // space so it misses "UBER EATS"). Editing Uber to require a digit
        // makes "UBER EATS" fall to the Eats... no — build it explicitly:
        // base: only "Shop" rule matches "SHOP 12"; after edit a different
        // canonical wins.
        let shop = crud::insert_rule(&conn, &merchant("Shop", r"(?i)SHOP")).unwrap();
        let _mall = crud::insert_rule(&conn, &merchant("Mall", r"(?i)SHOP 12")).unwrap();
        // id order: shop(lower) wins for "SHOP 12" in base → "Shop".
        seed_txn(&conn, 1, 1, "SHOP 12 SYDNEY", "SHOP 12 SYDNEY").unwrap();

        // Edit Shop so it no longer matches "SHOP 12...", letting Mall win.
        let m = Mutation::Edit { id: shop, data: merchant("Shop", r"(?i)SHOPPING") };
        let payees = load_payees(&conn).unwrap();
        let b = compute_buckets(&conn, Stage::Merchants, &m, &payees).unwrap();
        match b {
            Buckets::FirstMatch { stolen, .. } => {
                assert_eq!(stolen.payees, 1, "canonical switched Shop → Mall");
                assert_eq!(stolen.samples[0].was.as_deref(), Some("Shop"));
                assert_eq!(stolen.samples[0].now.as_deref(), Some("Mall"));
            }
            _ => panic!("first-match"),
        }
    }

    #[test]
    fn delete_first_match_fallthrough() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "A").unwrap();
        let amazon = crud::insert_rule(&conn, &merchant("Amazon", "(?i)AMAZON")).unwrap();
        seed_txn(&conn, 1, 1, "AMAZON MARKETPLACE", "AMAZON MARKETPLACE").unwrap();

        let m = Mutation::Delete { stage: Stage::Merchants, id: amazon };
        let payees = load_payees(&conn).unwrap();
        let b = compute_buckets(&conn, Stage::Merchants, &m, &payees).unwrap();
        match b {
            Buckets::FirstMatch { new_fallthrough, .. } => {
                assert_eq!(new_fallthrough.payees, 1);
                assert_eq!(new_fallthrough.samples[0].was.as_deref(), Some("Amazon"));
            }
            _ => panic!("first-match"),
        }
        // The committed DB is untouched (rollback worked).
        assert!(crud::get(&conn, Stage::Merchants, amazon).unwrap().is_some());
    }

    #[test]
    fn add_loop_stage_newly_affected() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "A").unwrap();
        seed_txn(&conn, 1, 1, "POS 0241 WOOLWORTHS METRO", "POS 0241 WOOLWORTHS METRO").unwrap();
        seed_txn(&conn, 2, 1, "PLAIN PAYEE", "PLAIN PAYEE").unwrap();

        let data = RuleData::Prefix {
            pattern: r"^POS (?P<account>\d+) ".into(),
            gateway: None,
            operation: None,
            has_account: true,
            has_date: false,
            note: None,
        };
        let m = Mutation::Add(data);
        let payees = load_payees(&conn).unwrap();
        let b = compute_buckets(&conn, Stage::Prefixes, &m, &payees).unwrap();
        match b {
            Buckets::Loop { newly_affected, no_longer_affected, .. } => {
                assert_eq!(newly_affected.payees, 1);
                assert_eq!(newly_affected.samples[0].original_payee, "POS 0241 WOOLWORTHS METRO");
                assert_eq!(no_longer_affected.payees, 0);
            }
            _ => panic!("prefixes is a loop stage"),
        }
    }

    #[test]
    fn test_one_matches_misses_and_syntax() {
        let conn = initialize_in_memory().unwrap();
        let cand = merchant("Uber Eats", r"(?i)UBER ?\*?EATS");
        match test_one(&conn, Stage::Merchants, &cand, "UBER *EATS Sydney AU") {
            TestResult::Matches { canonical, span } => {
                assert_eq!(canonical, "Uber Eats");
                let (s, e) = span.unwrap();
                assert_eq!(&"UBER *EATS Sydney AU"[s..e], "UBER *EATS");
            }
            other => panic!("expected match, got {other:?}"),
        }
        assert_eq!(test_one(&conn, Stage::Merchants, &cand, "OPAL TRAVEL"), TestResult::Misses);

        let bad = merchant("x", "(?i)UBER (");
        assert!(matches!(
            test_one(&conn, Stage::Merchants, &bad, "UBER"),
            TestResult::SyntaxError(_)
        ));
    }
}
