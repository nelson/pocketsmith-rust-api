//! Dirty derivation (rule-cli §1.2, §3.5): the headless equivalent of
//! the GUI's "⚠ N payees would re-stage" banner.

use anyhow::Result;
use rusqlite::Connection;

use crate::db::payee_normalisations as pn;
use crate::normalise::{format_payee, normalise, PipelineCtx, RuleCache};

/// Count the distinct `original_payee`s whose freshly-computed pipeline
/// proposal differs from the stored `payee_normalisations.proposed_payee`.
/// After a rule edit this is the number of payees whose staged proposal
/// is now stale — i.e. how many would re-stage on the next `normalise`
/// scan. Pure read-side: never mutates.
pub fn would_restage(conn: &Connection) -> Result<usize> {
    let rows = pn::list_all(conn)?;
    let cache = RuleCache::new();
    let ctx = PipelineCtx::new(conn, &cache);
    let mut n = 0;
    for r in rows {
        let fresh = format_payee(&normalise(&r.original_payee, &ctx));
        if fresh != r.proposed_payee {
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;
    use crate::rules::{crud, model::RuleData};
    use crate::review::Status;
    use crate::test_support::{seed_account, seed_pn, seed_txn};

    #[test]
    fn counts_only_stale_proposals() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "A").unwrap();
        // One merchant rule so "UBER TRIP" proposes "Uber".
        crud::insert_rule(
            &conn,
            &RuleData::Merchant { canonical: "Uber".into(), pattern: "(?i)UBER".into(), note: None },
        )
        .unwrap();
        seed_txn(&conn, 1, 1, "UBER TRIP", "UBER TRIP").unwrap();
        seed_txn(&conn, 2, 1, "WOOLWORTHS", "WOOLWORTHS").unwrap();

        // Stored proposal matches the fresh pipeline output → not stale.
        seed_pn(&conn, "UBER TRIP", "Uber", Status::Pending, 1).unwrap();
        // Stored proposal is stale (fresh output would be "WOOLWORTHS" since
        // no rule matches, so format_payee returns the normalised string).
        seed_pn(&conn, "WOOLWORTHS", "Some Old Proposal", Status::Confirmed, 1).unwrap();

        assert_eq!(would_restage(&conn).unwrap(), 1);
    }

    #[test]
    fn zero_when_all_in_sync() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "A").unwrap();
        seed_txn(&conn, 1, 1, "PLAIN PAYEE", "PLAIN PAYEE").unwrap();
        seed_pn(&conn, "PLAIN PAYEE", "PLAIN PAYEE", Status::Pending, 1).unwrap();
        assert_eq!(would_restage(&conn).unwrap(), 0);
    }
}
