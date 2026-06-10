//! Per-rule impact attribution (editable-rules-ui §3.7). Maps each
//! distinct payee to the rule that won its stage, summing the payee's txn
//! count + magnitude onto that rule. Used by `normalise scan` to populate
//! the `rule_impact` cache that the Pipeline rule list reads.
//!
//! Attribution reuses the pipeline trace: each matcher stage records the
//! firing rule's authored `pattern` (== the rule's `data.pattern()`) and
//! its produced canonical, so the winning rule id is a lookup, not a
//! re-implementation of each matcher. Only the five matcher stages
//! (persons / employers / merchants / banking_ops / locations) record a
//! match; the three loop stages (prefix / suffix / expand) don't surface
//! a single "winning rule", so they carry no per-rule impact number.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::Connection;

use super::super::crud;
use super::super::model::Rule;
use super::super::Stage;
use super::PayeeSample;
use crate::normalise::{normalise, PipelineCtx, RuleCache};

/// One row of the `rule_impact` cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleImpact {
    pub stage: Stage,
    pub rule_id: i64,
    pub txn_count: i64,
    pub total_cents: i64,
}

/// The matcher stages whose trace entry names equal their stage name and
/// which record a winning rule's pattern/canonical.
const MATCHER_STAGES: [Stage; 5] = [
    Stage::Persons,
    Stage::Employers,
    Stage::Merchants,
    Stage::BankingOps,
    Stage::Locations,
];

/// Attribute every payee to the rule that won each matcher stage,
/// returning the folded per-rule totals. Pure read-side: never mutates.
pub fn attribute(conn: &Connection, payees: &[PayeeSample]) -> Result<Vec<RuleImpact>> {
    // Pre-load each matcher stage's rules once (id + typed data).
    let mut rules_by_stage: HashMap<Stage, Vec<Rule>> = HashMap::new();
    for s in MATCHER_STAGES {
        rules_by_stage.insert(s, crud::list(conn, s)?);
    }

    let cache = RuleCache::new();
    let ctx = PipelineCtx::new(conn, &cache);
    // Fold totals keyed by (stage, rule_id).
    let mut acc: HashMap<(Stage, i64), (i64, i64)> = HashMap::new();

    for p in payees {
        let result = normalise(&p.original_payee, &ctx);
        for s in MATCHER_STAGES {
            let name = s.name();
            let Some(entry) = result.trace.iter().find(|t| t.stage == name) else {
                continue;
            };
            let Some(mi) = entry.match_info.as_ref() else {
                continue;
            };
            let canon = entry
                .feature_values
                .iter()
                .find(|(k, _)| matches!(*k, "entity_name" | "operation" | "location" | "region"))
                .map(|(_, v)| v.as_str());
            if let Some(id) = find_rule(&rules_by_stage[&s], &mi.pattern, canon) {
                let e = acc.entry((s, id)).or_default();
                e.0 += p.txn_count;
                e.1 += p.total_cents;
            }
        }
    }

    let mut out: Vec<RuleImpact> = acc
        .into_iter()
        .map(|((stage, rule_id), (txn_count, total_cents))| RuleImpact {
            stage,
            rule_id,
            txn_count,
            total_cents,
        })
        .collect();
    out.sort_by(|a, b| (a.stage.name(), a.rule_id).cmp(&(b.stage.name(), b.rule_id)));
    Ok(out)
}

/// Find the rule that produced a match, preferring an exact
/// pattern (+ canonical) match and falling back to canonical alone
/// (locations match on text, so they have no `pattern`).
fn find_rule(rules: &[Rule], pattern: &str, canon: Option<&str>) -> Option<i64> {
    if !pattern.is_empty() {
        if let Some(c) = canon {
            if let Some(r) = rules
                .iter()
                .find(|r| r.data.pattern() == Some(pattern) && r.data.canonical() == Some(c))
            {
                return Some(r.id);
            }
        }
        if let Some(r) = rules.iter().find(|r| r.data.pattern() == Some(pattern)) {
            return Some(r.id);
        }
    }
    if let Some(c) = canon {
        if let Some(r) = rules.iter().find(|r| r.data.canonical() == Some(c)) {
            return Some(r.id);
        }
    }
    None
}

/// Replace the `rule_impact` cache with `impacts` (delete-all + insert).
/// Called inside the scan's `with_operation` so it shares the scan's
/// single operation row.
pub fn write_impacts(conn: &Connection, impacts: &[RuleImpact]) -> Result<()> {
    conn.execute("DELETE FROM rule_impact", [])?;
    let mut stmt = conn.prepare(
        "INSERT INTO rule_impact (stage, rule_id, txn_count, total_cents) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for r in impacts {
        stmt.execute(rusqlite::params![r.stage.name(), r.rule_id, r.txn_count, r.total_cents])?;
    }
    Ok(())
}

/// Load the cached impact for a stage as `rule_id -> (txn_count,
/// total_cents)`, for the rule-list render.
pub fn load_for_stage(conn: &Connection, stage: Stage) -> Result<HashMap<i64, (i64, i64)>> {
    let mut stmt = conn
        .prepare("SELECT rule_id, txn_count, total_cents FROM rule_impact WHERE stage = ?1")?;
    let rows = stmt.query_map([stage.name()], |r| {
        Ok((r.get::<_, i64>(0)?, (r.get::<_, i64>(1)?, r.get::<_, i64>(2)?)))
    })?;
    let mut m = HashMap::new();
    for row in rows {
        let (id, v) = row?;
        m.insert(id, v);
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;
    use crate::rules::model::RuleData;
    use crate::test_support::{seed_account, seed_txn};

    fn merchant(canonical: &str, pattern: &str) -> RuleData {
        RuleData::Merchant { canonical: canonical.into(), pattern: pattern.into(), note: None }
    }

    #[test]
    fn attributes_payees_to_winning_merchant_rule() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "A").unwrap();
        let bunnings = crud::insert_rule(&conn, &merchant("Bunnings", "(?i)BUNNINGS")).unwrap();
        let woolies = crud::insert_rule(&conn, &merchant("Woolworths", "(?i)WOOLWORTHS")).unwrap();
        // Two Bunnings payees, one Woolworths.
        seed_txn(&conn, 1, 1, "BUNNINGS 391 KOTARA", "BUNNINGS 391 KOTARA").unwrap();
        seed_txn(&conn, 2, 1, "BUNNINGS WAREHOUSE", "BUNNINGS WAREHOUSE").unwrap();
        seed_txn(&conn, 3, 1, "WOOLWORTHS METRO", "WOOLWORTHS METRO").unwrap();

        let payees = super::super::load_payees(&conn).unwrap();
        let impacts = attribute(&conn, &payees).unwrap();

        let bunnings_hit = impacts.iter().find(|r| r.rule_id == bunnings).unwrap();
        assert_eq!(bunnings_hit.stage, Stage::Merchants);
        assert_eq!(bunnings_hit.txn_count, 2);
        let woolies_hit = impacts.iter().find(|r| r.rule_id == woolies).unwrap();
        assert_eq!(woolies_hit.txn_count, 1);

        // Round-trip through the cache table.
        write_impacts(&conn, &impacts).unwrap();
        let loaded = load_for_stage(&conn, Stage::Merchants).unwrap();
        assert_eq!(loaded.get(&bunnings).unwrap().0, 2);
        assert_eq!(loaded.get(&woolies).unwrap().0, 1);
    }

    #[test]
    fn unmatched_payee_is_not_attributed() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "A").unwrap();
        let id = crud::insert_rule(&conn, &merchant("Bunnings", "(?i)BUNNINGS")).unwrap();
        seed_txn(&conn, 1, 1, "RANDOM PAYEE", "RANDOM PAYEE").unwrap();
        let payees = super::super::load_payees(&conn).unwrap();
        let impacts = attribute(&conn, &payees).unwrap();
        assert!(impacts.iter().all(|r| r.rule_id != id), "no payee matched the rule");
    }
}
