//! Eligibility gate for the categorise stage.
//!
//! Categorisation must not run for a payee whose normalised string is
//! still going to change — otherwise we'd spend a Google Places lookup on
//! a string that's about to be rewritten. A payee is eligible iff its
//! normalisation is **applied OR confirmed, minus pending** (the design
//! decision):
//!
//!   * **applied**   — a transaction's `payee` was written by a committed
//!     `normalise-apply` (reason `normalise-apply` or the legacy
//!     `normalisation`); the staging row was drained, so this is the only
//!     durable signal that a normalisation was committed.
//!   * **confirmed** — the payee currently has a confirmed (status=1)
//!     staging row (reviewed yes, treated as good enough even though the
//!     string is written on the next apply).
//!   * **minus pending** — any payee with a pending (status=0) staging row
//!     is excluded: its string is about to change.
//!
//! A payee has at most one staging row (PK = `original_payee`), so the
//! states are mutually exclusive; "minus pending" only removes
//! applied-but-now-re-pending payees (e.g. applied, then a rule edit
//! produced a fresh pending proposal).

use std::collections::HashSet;

use anyhow::Result;
use rusqlite::Connection;

/// The set of `original_payee`s eligible for categorisation.
pub fn eligible_payees(conn: &Connection) -> Result<HashSet<String>> {
    let mut set: HashSet<String> = HashSet::new();

    // confirmed (status = 1)
    {
        let mut stmt =
            conn.prepare("SELECT original_payee FROM payee_normalisations WHERE status = 1")?;
        for r in stmt.query_map([], |r| r.get::<_, String>(0))? {
            set.insert(r?);
        }
    }

    // applied / committed (a normalise-apply payee write exists)
    {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT t.original_payee
               FROM _transaction_changes tc
               JOIN _operations o ON o.id = tc.operation_id
               JOIN transactions t ON t.id = tc.transaction_id
              WHERE o.reason IN ('normalise-apply','normalisation')
                AND tc.payee IS NOT NULL
                AND t.original_payee IS NOT NULL",
        )?;
        for r in stmt.query_map([], |r| r.get::<_, String>(0))? {
            set.insert(r?);
        }
    }

    // minus pending (status = 0) — string is about to change
    {
        let mut stmt =
            conn.prepare("SELECT original_payee FROM payee_normalisations WHERE status = 0")?;
        for r in stmt.query_map([], |r| r.get::<_, String>(0))? {
            set.remove(&r?);
        }
    }

    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;
    use crate::review::Status;
    use crate::test_support::{seed_account, seed_pn, seed_txn};

    #[test]
    fn confirmed_payee_is_eligible() {
        let conn = initialize_in_memory().unwrap();
        seed_pn(&conn, "COLES", "Coles", Status::Confirmed, 1).unwrap();
        let set = eligible_payees(&conn).unwrap();
        assert!(set.contains("COLES"));
    }

    #[test]
    fn pending_payee_is_not_eligible() {
        let conn = initialize_in_memory().unwrap();
        seed_pn(&conn, "ALDI", "ALDI", Status::Pending, 1).unwrap();
        let set = eligible_payees(&conn).unwrap();
        assert!(!set.contains("ALDI"));
    }

    #[test]
    fn applied_payee_is_eligible_via_committed_change() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Acct").unwrap();
        seed_txn(&conn, 1, 1, "WOOLIES RAW", "WOOLIES RAW").unwrap();
        // Simulate a normalise-apply commit: write payee under that reason.
        crate::db::with_operation(&conn, "normalise-apply", |c| {
            c.execute(
                "UPDATE transactions SET payee = 'Woolworths' WHERE id = 1",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        let set = eligible_payees(&conn).unwrap();
        assert!(set.contains("WOOLIES RAW"));
    }

    #[test]
    fn applied_but_now_pending_is_excluded() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Acct").unwrap();
        seed_txn(&conn, 1, 1, "WOOLIES RAW", "WOOLIES RAW").unwrap();
        crate::db::with_operation(&conn, "normalise-apply", |c| {
            c.execute("UPDATE transactions SET payee = 'Woolworths' WHERE id = 1", [])?;
            Ok(())
        })
        .unwrap();
        // A fresh pending proposal (e.g. after a rule edit) excludes it.
        seed_pn(&conn, "WOOLIES RAW", "Woolworths Metro", Status::Pending, 1).unwrap();
        let set = eligible_payees(&conn).unwrap();
        assert!(!set.contains("WOOLIES RAW"), "re-pending payee is excluded");
    }

    #[test]
    fn untouched_payee_is_not_eligible() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Acct").unwrap();
        seed_txn(&conn, 1, 1, "MYSTERY", "MYSTERY").unwrap();
        let set = eligible_payees(&conn).unwrap();
        assert!(set.is_empty(), "a payee never normalised is not eligible");
    }
}
