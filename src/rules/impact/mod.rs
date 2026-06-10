//! Dry-run impact engine (rule-cli §3.4) — the heart of "evaluate".
//!
//! This `mod.rs` holds the payee population ([`PayeeSample`] /
//! [`load_payees`]) and the bucket result types ([`Buckets`] /
//! [`BucketCount`] / [`BucketSample`]). The diffing engine lives in
//! [`buckets`] ([`compute_buckets`]) and the single-string tester in
//! [`tester`] ([`test_one`]).

use anyhow::Result;
use rusqlite::Connection;

mod buckets;
mod tester;

pub mod attribution;

pub use attribution::{attribute, load_for_stage, write_impacts, RuleImpact};
pub use buckets::compute_buckets;
pub use tester::{test_one, TestResult};

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
    /// Record one affected payee. Used by [`buckets`] (a descendant
    /// module, so this stays private to `impact`).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;
    use crate::rules::crud;
    use crate::rules::model::{Mutation, RuleData};
    use crate::rules::Stage;
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
        // the generic Uber. The Eats rule has the lower id so it wins where
        // it matches.
        let _eats = crud::insert_rule(&conn, &merchant("Uber Eats", r"(?i)UBEREATS")).unwrap();
        let uber = crud::insert_rule(&conn, &merchant("Uber", "(?i)UBER")).unwrap();
        seed_txn(&conn, 1, 1, "UBEREATS HELP", "UBEREATS HELP").unwrap();
        seed_txn(&conn, 2, 1, "UBER TRIP", "UBER TRIP").unwrap();
        seed_txn(&conn, 3, 1, "UBER ONE", "UBER ONE").unwrap();

        // Edit the generic Uber rule to only match "UBER TRIP", so "UBER ONE"
        // falls through and "UBEREATS HELP" is unaffected.
        let m = Mutation::Edit { id: uber, data: merchant("Uber", "(?i)UBER TRIP") };
        let payees = load_payees(&conn).unwrap();
        let b = compute_buckets(&conn, Stage::Merchants, &m, &payees).unwrap();
        match b {
            Buckets::FirstMatch { newly_matched, stolen, new_fallthrough, unchanged_payees } => {
                assert_eq!(newly_matched.payees, 0);
                assert_eq!(stolen.payees, 0, "no payee's canonical switched rules");
                assert_eq!(new_fallthrough.payees, 1);
                assert_eq!(new_fallthrough.samples[0].original_payee, "UBER ONE");
                assert_eq!(new_fallthrough.samples[0].was.as_deref(), Some("Uber"));
                assert_eq!(new_fallthrough.samples[0].now, None);
                assert_eq!(unchanged_payees, 2);
            }
            _ => panic!("first-match"),
        }
    }

    #[test]
    fn edit_first_match_stolen_when_canonical_changes() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "A").unwrap();
        let shop = crud::insert_rule(&conn, &merchant("Shop", r"(?i)SHOP")).unwrap();
        let _mall = crud::insert_rule(&conn, &merchant("Mall", r"(?i)SHOP 12")).unwrap();
        // id order: shop (lower) wins for "SHOP 12" in base → "Shop".
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
