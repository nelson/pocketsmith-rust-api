//! Per-transaction cleaning state for the three pillars: Pair,
//! Normalise, Categorise. Each pillar reduces to an enum whose variants
//! line up 1:1 with the emoji classes in `mock.css` and the eventual
//! `css.rs` rules:
//!
//! ```text
//! Pair      Confirmed -> g-pair-confirmed (chain link)
//!           Pending   -> g-pair-pending   (cycle arrows)
//!           Orphan    -> g-pair-orphan    (warning)
//!           Rejected  -> g-pair-rejected  (scissors)
//!           NotApplicable (slot blanked, dim dot in queue)
//!
//! Norm      Confirmed -> g-norm-confirmed (tag)
//!           Pending   -> g-norm-pending   (memo)
//!           Missing   -> g-norm-missing   (question mark)
//!           Rejected  -> g-norm-rejected  (prohibited)
//!
//! Cat       Confirmed -> g-cat-confirmed  (file cabinet)
//!           Missing   -> g-cat-missing    (parcel)
//! ```
//!
//! V1 only emits `Confirmed` and `Missing` for `CatState`; the staging
//! flow for category decisions doesn't exist yet (`PLAN-editable-rules-v2`).

use anyhow::Result;
use rusqlite::Connection;

use pocketsmith_sync::review::Status;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairState {
    /// Row participates in a confirmed `transfer_pairs` row.
    Confirmed,
    /// Row participates in a pending `transfer_pairs` proposal awaiting
    /// the user's decision.
    Pending,
    /// Row participates in a rejected `transfer_pairs` row.
    Rejected,
    /// `is_transfer = 1` but no row exists in `transfer_pairs`. The
    /// pairing pipeline has not (yet) found a counterpart â either it
    /// will be paired in a future sync, or the user will mark it
    /// "not_a_transfer" / "snoozed" via `transfer_decisions` (Â§8 of plan).
    Orphan,
    /// `is_transfer = 0` and no row exists in `transfer_pairs`.
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormState {
    /// `payee_normalisations` row exists with `status = confirmed`.
    Confirmed,
    /// `payee_normalisations` row exists with `status = pending`.
    Pending,
    /// `payee_normalisations` row exists with `status = rejected`.
    Rejected,
    /// No `payee_normalisations` row exists for this `original_payee`.
    /// Reading: "the pipeline has nothing to say about this payee yet â
    /// either teach it a new rule, or the payee is so unique it doesn't
    /// merit one."
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatState {
    Confirmed,
    Missing,
}

/// Derive the pair state for one transaction. The query checks both
/// sides of `transfer_pairs` because either side carries the same
/// status. `is_transfer` is supplied by the caller because it's
/// already in `TxnQueueRow` â saves a redundant SELECT.
pub fn derive_pair_state(conn: &Connection, txn_id: i64, is_transfer: bool) -> Result<PairState> {
    // Single SELECT covers both halves of the pair via OR.
    let status: Option<i32> = conn
        .query_row(
            "SELECT status FROM transfer_pairs
              WHERE txn_id_a = ?1 OR txn_id_b = ?1
              LIMIT 1",
            rusqlite::params![txn_id],
            |row| row.get(0),
        )
        .ok();

    Ok(match status {
        Some(s) => match Status::from_i32(s) {
            Some(Status::Confirmed) => PairState::Confirmed,
            Some(Status::Pending) => PairState::Pending,
            Some(Status::Rejected) => PairState::Rejected,
            None => PairState::NotApplicable,
        },
        None if is_transfer => PairState::Orphan,
        None => PairState::NotApplicable,
    })
}

/// Derive the normalisation state for one transaction by looking up
/// its `original_payee` in `payee_normalisations`. Pass `None` for a
/// row whose `original_payee` is NULL â such a row can never have a
/// rule, so the answer is always `Missing`.
pub fn derive_norm_state(conn: &Connection, original_payee: Option<&str>) -> Result<NormState> {
    let Some(op) = original_payee else {
        return Ok(NormState::Missing);
    };
    let status: Option<i32> = conn
        .query_row(
            "SELECT status FROM payee_normalisations WHERE original_payee = ?1",
            rusqlite::params![op],
            |row| row.get(0),
        )
        .ok();
    Ok(match status {
        Some(s) => match Status::from_i32(s) {
            Some(Status::Confirmed) => NormState::Confirmed,
            Some(Status::Pending) => NormState::Pending,
            Some(Status::Rejected) => NormState::Rejected,
            None => NormState::Missing,
        },
        None => NormState::Missing,
    })
}

/// Derive the category state. Pure function of the supplied
/// `category_id` â `None` means uncategorised. V1 doesn't model
/// pending/rejected category decisions; the staging flow for those
/// lives in the future v2 milestone.
pub fn derive_cat_state(category_id: Option<i64>) -> CatState {
    match category_id {
        Some(_) => CatState::Confirmed,
        None => CatState::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::{initialize_in_memory, transfer_pairs, with_operation};
    use pocketsmith_sync::test_support::{seed_account, seed_pn, seed_txn};
    use pocketsmith_sync::transfers::{Confidence, TransferPair};

    fn fresh() -> Connection {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Cheque").unwrap();
        seed_account(&conn, 2, "Savings").unwrap();
        conn
    }

    fn insert_pair(conn: &Connection, a: i64, b: i64, status: Status) {
        with_operation(conn, "test-seed", |c| {
            transfer_pairs::insert_pair(
                c,
                &TransferPair {
                    txn_id_a: a,
                    txn_id_b: b,
                    amount_cents: 5000,
                    confidence: Confidence::High,
                    status,
                },
            )
        })
        .unwrap();
    }

    // --- PairState ---

    #[test]
    fn pair_state_confirmed_when_pair_status_is_confirmed() {
        let conn = fresh();
        seed_txn(&conn, 10, 1, "FROM", "FROM").unwrap();
        seed_txn(&conn, 11, 2, "TO", "TO").unwrap();
        insert_pair(&conn, 10, 11, Status::Confirmed);

        assert_eq!(derive_pair_state(&conn, 10, true).unwrap(), PairState::Confirmed);
        // Both sides of the pair report the same state.
        assert_eq!(derive_pair_state(&conn, 11, true).unwrap(), PairState::Confirmed);
    }

    #[test]
    fn pair_state_pending_when_pair_awaiting_review() {
        let conn = fresh();
        seed_txn(&conn, 10, 1, "FROM", "FROM").unwrap();
        seed_txn(&conn, 11, 2, "TO", "TO").unwrap();
        insert_pair(&conn, 10, 11, Status::Pending);

        assert_eq!(derive_pair_state(&conn, 10, true).unwrap(), PairState::Pending);
    }

    #[test]
    fn pair_state_rejected_when_pair_was_rejected() {
        let conn = fresh();
        seed_txn(&conn, 10, 1, "FROM", "FROM").unwrap();
        seed_txn(&conn, 11, 2, "TO", "TO").unwrap();
        insert_pair(&conn, 10, 11, Status::Rejected);

        assert_eq!(derive_pair_state(&conn, 10, true).unwrap(), PairState::Rejected);
    }

    #[test]
    fn pair_state_orphan_when_is_transfer_but_no_pair_row() {
        let conn = fresh();
        seed_txn(&conn, 10, 1, "FROM", "FROM").unwrap();
        // is_transfer=true but no pair row exists yet.
        assert_eq!(derive_pair_state(&conn, 10, true).unwrap(), PairState::Orphan);
    }

    #[test]
    fn pair_state_not_applicable_when_not_a_transfer_and_no_pair() {
        let conn = fresh();
        seed_txn(&conn, 10, 1, "X", "X").unwrap();
        assert_eq!(
            derive_pair_state(&conn, 10, false).unwrap(),
            PairState::NotApplicable
        );
    }

    #[test]
    fn pair_state_unknown_txn_id_is_not_applicable() {
        // Defensive: querying a txn id that doesn't exist in
        // transfer_pairs should fall through to the
        // is_transfer-driven branch.
        let conn = fresh();
        assert_eq!(
            derive_pair_state(&conn, 999, false).unwrap(),
            PairState::NotApplicable
        );
        assert_eq!(
            derive_pair_state(&conn, 999, true).unwrap(),
            PairState::Orphan
        );
    }

    // --- NormState ---

    #[test]
    fn norm_state_confirmed_when_pn_status_confirmed() {
        let conn = fresh();
        seed_pn(&conn, "WOOLIES", "Woolworths", Status::Confirmed, 5).unwrap();
        assert_eq!(
            derive_norm_state(&conn, Some("WOOLIES")).unwrap(),
            NormState::Confirmed
        );
    }

    #[test]
    fn norm_state_pending_when_pn_pending() {
        let conn = fresh();
        seed_pn(&conn, "WOOLIES", "Woolworths", Status::Pending, 5).unwrap();
        assert_eq!(
            derive_norm_state(&conn, Some("WOOLIES")).unwrap(),
            NormState::Pending
        );
    }

    #[test]
    fn norm_state_rejected_when_pn_rejected() {
        let conn = fresh();
        seed_pn(&conn, "WOOLIES", "Woolworths", Status::Rejected, 5).unwrap();
        assert_eq!(
            derive_norm_state(&conn, Some("WOOLIES")).unwrap(),
            NormState::Rejected
        );
    }

    #[test]
    fn norm_state_missing_when_no_pn_row() {
        let conn = fresh();
        assert_eq!(
            derive_norm_state(&conn, Some("UNKNOWN PAYEE")).unwrap(),
            NormState::Missing
        );
    }

    #[test]
    fn norm_state_missing_when_original_payee_is_none() {
        let conn = fresh();
        // A NULL original_payee can never have a rule: the answer
        // must be Missing without touching the database.
        assert_eq!(derive_norm_state(&conn, None).unwrap(), NormState::Missing);
    }

    // --- CatState ---

    #[test]
    fn cat_state_confirmed_when_category_id_some() {
        assert_eq!(derive_cat_state(Some(42)), CatState::Confirmed);
    }

    #[test]
    fn cat_state_missing_when_category_id_none() {
        assert_eq!(derive_cat_state(None), CatState::Missing);
    }
}
