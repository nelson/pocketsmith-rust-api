//! Action handlers for the `/transactions/*` tab.
//!
//! Per `PLAN §3.5`, the Transactions tab does not introduce new
//! mutation logic; it delegates into the existing `normalise::handlers`
//! and `transfers::handlers`. The wrappers in this file translate from
//! "the txn the user is looking at" to "the staging row that handler
//! cares about" (a normalisation slug, or a transfer-pair key).
//!
//! After the underlying handler runs, the route arm in `main.rs`
//! re-renders the **Transactions** page (not the source tab), so the
//! user keeps their context.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use pocketsmith_sync::normalise::slug_for;

use crate::state::{AppState, Decision, TxnActionPillar, TxnActivityEntry};
use crate::tab::next_after;

use super::helpers::{filtered_transactions, TxnFilter, TxnQueueRow};

/// Look up the `original_payee` for a transaction, then compute the
/// pn slug. Returns `None` if the txn doesn't exist or its
/// `original_payee` is NULL (in which case there's no normalisation
/// row to act on).
fn slug_for_txn(conn: &Connection, txn_id: i64) -> Option<String> {
    let original: Option<String> = conn
        .query_row(
            "SELECT original_payee FROM transactions WHERE id = ?1",
            rusqlite::params![txn_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    original.map(|op| slug_for(&op))
}

/// Look up the transfer-pair key (a, b) for a transaction. Returns
/// `None` if the txn doesn't participate in any pair row.
fn pair_key_for_txn(conn: &Connection, txn_id: i64) -> Option<(i64, i64)> {
    conn.query_row(
        "SELECT txn_id_a, txn_id_b FROM transfer_pairs
          WHERE txn_id_a = ?1 OR txn_id_b = ?1
          LIMIT 1",
        rusqlite::params![txn_id],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )
    .ok()
}

/// Look up the (id, payee, amount, original_payee) snapshot of a txn
/// for the activity log. Returns None for an unknown txn id.
fn txn_snapshot(conn: &Connection, txn_id: i64) -> Option<(String, i64, Option<String>)> {
    conn.query_row(
        "SELECT t.payee, t.amount, t.original_payee FROM transactions t WHERE t.id = ?1",
        rusqlite::params![txn_id],
        |row| {
            let payee: Option<String> = row.get(0)?;
            let amount: f64 = row.get(1)?;
            let original: Option<String> = row.get(2)?;
            let display = payee.or_else(|| original.clone()).unwrap_or_default();
            Ok((display, (amount * 100.0).round() as i64, original))
        },
    )
    .ok()
}

/// Compute the next visible txn id after `current_id` against the
/// active filter. Used to advance `txn_active` after a Y/N/S so the
/// keyboard-driven flow keeps moving through the queue.
fn next_txn_id(conn: &Connection, filter: TxnFilter, current_id: i64) -> Option<i64> {
    let rows = filtered_transactions(conn, filter, 1000).unwrap_or_default();
    next_after(&rows, |r: &TxnQueueRow| r.id == current_id).map(|r| r.id)
}

/// Confirm/reject/skip the normalisation proposal that owns this txn.
/// Delegates straight into `normalise::handlers::act`. No-op if the
/// txn has no `original_payee` (and hence no slug to act on).
pub fn act_norm(state: &Arc<Mutex<AppState>>, txn_id: i64, decision: Decision) {
    let (slug, snapshot, next, filter) = {
        let st = state.lock().unwrap();
        let filter = TxnFilter::parse(&st.txn_filter);
        let slug = slug_for_txn(&st.conn, txn_id);
        let snapshot = txn_snapshot(&st.conn, txn_id);
        // Compute next BEFORE the action while the row is still visible.
        let next = next_txn_id(&st.conn, filter, txn_id);
        (slug, snapshot, next, filter)
    };
    let Some(slug) = slug else { return };
    crate::normalise::handlers::act(state, &slug, decision);

    // After the action, push to the activity log and advance txn_active.
    let mut st = state.lock().unwrap();
    if let Some((payee, amount_cents, _orig)) = snapshot {
        st.push_txn_activity(TxnActivityEntry {
            txn_id,
            payee,
            amount_cents,
            decision,
            pillar: TxnActionPillar::Norm,
        });
    }
    // Re-resolve next against the post-action filter result. The
    // pre-computed `next` is the preferred candidate; fall back to
    // tail of the visible queue if it's no longer present (e.g. the
    // filter was 'rule-pending' and `next` was just confirmed too).
    let visible = filtered_transactions(&st.conn, filter, 1000).unwrap_or_default();
    st.txn_active = next
        .filter(|n| visible.iter().any(|r| r.id == *n))
        .or_else(|| visible.last().map(|r| r.id));
}

/// Undo the previous normalisation decision for the txn's slug.
pub fn undo_norm(state: &Arc<Mutex<AppState>>, txn_id: i64) {
    let slug = {
        let st = state.lock().unwrap();
        slug_for_txn(&st.conn, txn_id)
    };
    if let Some(slug) = slug {
        crate::normalise::handlers::undo(state, &slug);
    }
    // Drop the activity entry for this txn (whichever pillar) so the
    // user doesn't see a stale row in the activity panel.
    let mut st = state.lock().unwrap();
    st.txn_activity.retain(|e| e.txn_id != txn_id);
    st.txn_undone += 1;
}

/// Confirm/reject/skip the transfer pair this txn participates in.
/// Delegates into `transfers::handlers::act`. No-op if the txn has no
/// pair row.
pub fn act_pair(state: &Arc<Mutex<AppState>>, txn_id: i64, decision: Decision) {
    let (key, snapshot, next, filter) = {
        let st = state.lock().unwrap();
        let filter = TxnFilter::parse(&st.txn_filter);
        let key = pair_key_for_txn(&st.conn, txn_id);
        let snapshot = txn_snapshot(&st.conn, txn_id);
        let next = next_txn_id(&st.conn, filter, txn_id);
        (key, snapshot, next, filter)
    };
    let Some(k) = key else { return };
    crate::transfers::handlers::act(state, k, decision);

    let mut st = state.lock().unwrap();
    if let Some((payee, amount_cents, _orig)) = snapshot {
        st.push_txn_activity(TxnActivityEntry {
            txn_id,
            payee,
            amount_cents,
            decision,
            pillar: TxnActionPillar::Pair,
        });
    }
    let visible = filtered_transactions(&st.conn, filter, 1000).unwrap_or_default();
    st.txn_active = next
        .filter(|n| visible.iter().any(|r| r.id == *n))
        .or_else(|| visible.last().map(|r| r.id));
}

/// Undo the previous pair decision for the txn's pair.
pub fn undo_pair(state: &Arc<Mutex<AppState>>, txn_id: i64) {
    let key = {
        let st = state.lock().unwrap();
        pair_key_for_txn(&st.conn, txn_id)
    };
    if let Some(k) = key {
        crate::transfers::handlers::undo(state, k);
    }
    let mut st = state.lock().unwrap();
    st.txn_activity.retain(|e| e.txn_id != txn_id);
    st.txn_undone += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::{initialize_in_memory, transfer_pairs, with_operation};
    use pocketsmith_sync::review::Status;
    use pocketsmith_sync::test_support::{seed_account, seed_pn, seed_txn};
    use pocketsmith_sync::transfers::{Confidence, TransferPair};

    fn fresh() -> Arc<Mutex<AppState>> {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Cheque").unwrap();
        seed_account(&conn, 2, "Savings").unwrap();
        seed_txn(&conn, 10, 1, "WOOLIES STRATHF", "WOOLIES STRATHF").unwrap();
        seed_txn(&conn, 20, 1, "TRF FROM SAV", "TRF FROM SAV").unwrap();
        seed_txn(&conn, 21, 2, "TRF TO CHEQUE", "TRF TO CHEQUE").unwrap();
        // pn row pending for the WOOLIES txn
        seed_pn(
            &conn,
            "WOOLIES STRATHF",
            "Woolworths",
            Status::Pending,
            1,
        )
        .unwrap();
        // transfer pair pending for txns 20 & 21
        with_operation(&conn, "test-seed", |c| {
            transfer_pairs::insert_pair(
                c,
                &TransferPair {
                    txn_id_a: 20,
                    txn_id_b: 21,
                    amount_cents: 100,
                    confidence: Confidence::High,
                    status: Status::Pending,
                },
            )
        })
        .unwrap();
        Arc::new(Mutex::new(AppState::new(conn)))
    }

    fn pn_status(state: &Arc<Mutex<AppState>>, slug: &str) -> i32 {
        let st = state.lock().unwrap();
        st.conn
            .query_row(
                "SELECT status FROM payee_normalisations WHERE slug = ?1",
                rusqlite::params![slug],
                |row| row.get(0),
            )
            .unwrap()
    }

    fn pair_status(state: &Arc<Mutex<AppState>>) -> i32 {
        let st = state.lock().unwrap();
        st.conn
            .query_row(
                "SELECT status FROM transfer_pairs LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap()
    }

    #[test]
    fn act_norm_delegates_to_normalise_handler_via_slug_lookup() {
        let state = fresh();
        let slug = slug_for("WOOLIES STRATHF");

        // Pending in DB before action.
        assert_eq!(pn_status(&state, &slug), Status::Pending.to_i32());

        act_norm(&state, 10, Decision::Confirm);

        // The underlying handler flips the staging row to Confirmed.
        assert_eq!(pn_status(&state, &slug), Status::Confirmed.to_i32());
    }

    #[test]
    fn act_norm_is_noop_for_unknown_txn_id() {
        let state = fresh();
        let slug = slug_for("WOOLIES STRATHF");

        act_norm(&state, 9999, Decision::Confirm);

        // Untouched.
        assert_eq!(pn_status(&state, &slug), Status::Pending.to_i32());
    }

    #[test]
    fn undo_norm_reverts_a_prior_decision() {
        let state = fresh();
        let slug = slug_for("WOOLIES STRATHF");

        act_norm(&state, 10, Decision::Confirm);
        assert_eq!(pn_status(&state, &slug), Status::Confirmed.to_i32());

        undo_norm(&state, 10);
        // Status returns to pending after undo.
        assert_eq!(pn_status(&state, &slug), Status::Pending.to_i32());
    }

    #[test]
    fn act_norm_advances_txn_active_to_next_visible_row() {
        let state = fresh();
        // Seed an additional txn with its own pending pn so there are
        // two rows the queue can navigate between. The fresh fixture
        // uses date='2026-01-01' for all seeded rows, so order is
        // id DESC: 30, 21, 20, 10.
        {
            let st = state.lock().unwrap();
            seed_txn(&st.conn, 30, 1, "COLES NORTH", "COLES NORTH").unwrap();
            seed_pn(&st.conn, "COLES NORTH", "Coles", Status::Pending, 1).unwrap();
        }
        // Mark txn 30 (the head) as active. Acting on it should advance
        // to txn 21 (the next-newest in the unfiltered queue).
        state.lock().unwrap().txn_active = Some(30);

        act_norm(&state, 30, Decision::Confirm);

        let active = state.lock().unwrap().txn_active;
        assert_eq!(active, Some(21), "txn_active should advance to id=21");
    }

    #[test]
    fn act_norm_pushes_an_activity_entry() {
        let state = fresh();
        act_norm(&state, 10, Decision::Confirm);
        let st = state.lock().unwrap();
        assert_eq!(st.txn_activity.len(), 1);
        let entry = &st.txn_activity[0];
        assert_eq!(entry.txn_id, 10);
        assert_eq!(entry.decision, Decision::Confirm);
        assert_eq!(entry.pillar, crate::state::TxnActionPillar::Norm);
    }

    #[test]
    fn undo_norm_drops_the_activity_entry_and_bumps_undone() {
        let state = fresh();
        act_norm(&state, 10, Decision::Confirm);
        assert_eq!(state.lock().unwrap().txn_activity.len(), 1);

        undo_norm(&state, 10);

        let st = state.lock().unwrap();
        assert_eq!(st.txn_activity.len(), 0, "activity entry removed");
        assert_eq!(st.txn_undone, 1, "undone counter incremented");
    }

    #[test]
    fn act_pair_pushes_an_activity_entry_with_pair_pillar() {
        let state = fresh();
        act_pair(&state, 20, Decision::Confirm);
        let st = state.lock().unwrap();
        assert_eq!(st.txn_activity.len(), 1);
        assert_eq!(st.txn_activity[0].pillar, crate::state::TxnActionPillar::Pair);
    }

    #[test]
    fn act_pair_delegates_to_transfers_handler_via_pair_lookup() {
        let state = fresh();

        assert_eq!(pair_status(&state), Status::Pending.to_i32());

        act_pair(&state, 20, Decision::Confirm);

        assert_eq!(pair_status(&state), Status::Confirmed.to_i32());
    }

    #[test]
    fn act_pair_works_from_either_side_of_the_pair() {
        let state = fresh();
        // Acting via the b-side txn id should also work: pair_key_for_txn
        // matches either side via OR.
        act_pair(&state, 21, Decision::Reject);
        assert_eq!(pair_status(&state), Status::Rejected.to_i32());
    }

    #[test]
    fn act_pair_is_noop_for_txn_with_no_pair_row() {
        let state = fresh();
        // txn 10 (WOOLIES) is not part of any pair.
        act_pair(&state, 10, Decision::Confirm);
        // Pair status untouched (still pending for the 20-21 pair).
        assert_eq!(pair_status(&state), Status::Pending.to_i32());
    }
}
