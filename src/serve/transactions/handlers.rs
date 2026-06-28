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

use pocketsmith::normalise::slug_for;

use crate::serve::state::{AppState, Decision, TxnActionPillar, TxnActivityEntry};

use super::helpers::{filtered_transactions, TxnFilter};

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

/// Pick the txn id that should be active *after* the user acts on
/// `current_id` under `filter`. Walks `pre_visible` (the queue right
/// before the action) starting after `current_id` and returns the
/// first id still present in `post_visible`. If no row after
/// `current_id` survives, walks backwards through earlier rows. If
/// neither direction finds a survivor, falls back to `current_id`
/// itself — the user stays anchored on the row they just acted on,
/// so the detail panel can show its updated state instead of going
/// blank.
///
/// Why two-direction with backwards fallback: when several siblings
/// share the same `original_payee`, confirming the rule resolves all
/// of them at once. The pre-computed naive 'next' (one position ahead)
/// may itself be one of those resolved siblings; the next surviving
/// row could be several positions further along, or there may be no
/// rows further along at all (acted on the tail row of a small group).
fn pick_next_active(
    pre_visible: &[i64],
    post_visible: &[i64],
    current_id: i64,
) -> Option<i64> {
    use std::collections::HashSet;
    let post_set: HashSet<i64> = post_visible.iter().copied().collect();
    let pos = pre_visible.iter().position(|id| *id == current_id);

    // Forward: first surviving row after current_id.
    let forward = pos.and_then(|p| {
        pre_visible
            .iter()
            .skip(p + 1)
            .find(|id| post_set.contains(id))
            .copied()
    });
    if forward.is_some() {
        return forward;
    }
    // Backward: most-recent surviving row before current_id (closest
    // first, hence .rev()). Useful when the user acted on the tail of
    // a small filtered queue and the only remaining work is above.
    let backward = pos.and_then(|p| {
        pre_visible
            .iter()
            .take(p)
            .rev()
            .find(|id| post_set.contains(id))
            .copied()
    });
    if backward.is_some() {
        return backward;
    }
    // No surviving row in either direction (queue exhausted).
    // Stay on the acted-on row so the detail panel reflects the
    // updated state rather than going blank.
    Some(current_id)
}

/// Confirm/reject/skip the normalisation proposal that owns this txn.
/// Delegates straight into `normalise::handlers::act`. No-op if the
/// txn has no `original_payee` (and hence no slug to act on).
pub fn act_norm(state: &Arc<Mutex<AppState>>, txn_id: i64, decision: Decision) {
    let (slug, snapshot, pre_visible, filter) = {
        let st = state.lock().unwrap();
        let filter = TxnFilter::parse(&st.txn_filter);
        let slug = slug_for_txn(&st.conn, txn_id);
        let snapshot = txn_snapshot(&st.conn, txn_id);
        // Capture pre-action queue so we can pick the next active id
        // post-action even when several rows resolve together.
        let pre_visible: Vec<i64> = filtered_transactions(&st.conn, filter, 1000)
            .unwrap_or_default()
            .iter()
            .map(|r| r.id)
            .collect();
        (slug, snapshot, pre_visible, filter)
    };
    let Some(slug) = slug else { return };
    crate::serve::normalise::handlers::act(state, &slug, decision);

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
    let post_visible: Vec<i64> = filtered_transactions(&st.conn, filter, 1000)
        .unwrap_or_default()
        .iter()
        .map(|r| r.id)
        .collect();
    st.txn_active = pick_next_active(&pre_visible, &post_visible, txn_id);
}

/// Undo the previous normalisation decision for the txn's slug.
pub fn undo_norm(state: &Arc<Mutex<AppState>>, txn_id: i64) {
    let slug = {
        let st = state.lock().unwrap();
        slug_for_txn(&st.conn, txn_id)
    };
    if let Some(slug) = slug {
        crate::serve::normalise::handlers::undo(state, &slug);
    }
    let mut st = state.lock().unwrap();
    st.txn_activity.retain(|e| e.txn_id != txn_id);
    st.txn_undone += 1;
    // Anchor the user on the just-undone row so they can review it.
    st.txn_active = Some(txn_id);
}

/// Confirm/reject/skip the transfer pair this txn participates in.
/// Delegates into `transfers::handlers::act`. No-op if the txn has no
/// pair row.
pub fn act_pair(state: &Arc<Mutex<AppState>>, txn_id: i64, decision: Decision) {
    let (key, snapshot, pre_visible, filter) = {
        let st = state.lock().unwrap();
        let filter = TxnFilter::parse(&st.txn_filter);
        let key = pair_key_for_txn(&st.conn, txn_id);
        let snapshot = txn_snapshot(&st.conn, txn_id);
        let pre_visible: Vec<i64> = filtered_transactions(&st.conn, filter, 1000)
            .unwrap_or_default()
            .iter()
            .map(|r| r.id)
            .collect();
        (key, snapshot, pre_visible, filter)
    };
    let Some(k) = key else { return };
    crate::serve::transfers::handlers::act(state, k, decision);

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
    let post_visible: Vec<i64> = filtered_transactions(&st.conn, filter, 1000)
        .unwrap_or_default()
        .iter()
        .map(|r| r.id)
        .collect();
    st.txn_active = pick_next_active(&pre_visible, &post_visible, txn_id);
}

/// Undo the previous pair decision for the txn's pair.
pub fn undo_pair(state: &Arc<Mutex<AppState>>, txn_id: i64) {
    let key = {
        let st = state.lock().unwrap();
        pair_key_for_txn(&st.conn, txn_id)
    };
    if let Some(k) = key {
        crate::serve::transfers::handlers::undo(state, k);
    }
    let mut st = state.lock().unwrap();
    st.txn_activity.retain(|e| e.txn_id != txn_id);
    st.txn_undone += 1;
    st.txn_active = Some(txn_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith::db::{initialize_in_memory, transfer_pairs, with_operation};
    use pocketsmith::review::Status;
    use pocketsmith::test_support::{seed_account, seed_pn, seed_txn};
    use pocketsmith::transfers::{Confidence, TransferPair};

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
    fn act_norm_skips_resolved_siblings_when_advancing() {
        // Two consecutive txns share original_payee 'AMAZON'. Acting on
        // the first under the rule-pending filter confirms the rule for
        // both, so the precomputed 'next' (id=50) drops out of the
        // filter alongside the acted-on row. The resolver must skip
        // the resolved sibling and pick the next still-pending row.
        let state = fresh();
        state.lock().unwrap().txn_filter = "rule-pending".to_string();
        {
            let st = state.lock().unwrap();
            // Two txns sharing 'AMAZON' (same pn row, pending).
            seed_txn(&st.conn, 50, 1, "AMAZON", "AMAZON").unwrap();
            seed_txn(&st.conn, 51, 1, "AMAZON", "AMAZON").unwrap();
            seed_pn(&st.conn, "AMAZON", "Amazon", Status::Pending, 2).unwrap();
        }
        // rule-pending pre_visible by id DESC: 51, 50, 10
        // (txn 10's WOOLIES NORTH STRATHF pn is also pending from fresh()).
        state.lock().unwrap().txn_active = Some(51);

        act_norm(&state, 51, Decision::Confirm);

        // Acting on 51 confirms AMAZON's pn -> both 51 and 50 drop
        // out. Pre-computed 'next' (50) is now resolved. The resolver
        // must walk forward in pre_visible past the resolved sibling
        // and land on 10 (still pending).
        let active = state.lock().unwrap().txn_active;
        assert_eq!(
            active,
            Some(10),
            "resolver should skip resolved sibling 50 and land on 10"
        );
    }

    #[test]
    fn act_norm_stays_on_acted_row_when_no_other_rows_remain() {
        // Last user-visible row scenario: AMAZON is the only pending
        // payee, two txns share it. After acting on 51, the rule-
        // pending queue is empty. The resolver should fall back to
        // the acted-on row id so the detail panel renders its updated
        // state (Confirmed) instead of jumping to an empty placeholder.
        let state = fresh();
        state.lock().unwrap().txn_filter = "rule-pending".to_string();
        {
            let st = state.lock().unwrap();
            st.conn
                .execute("DELETE FROM payee_normalisations", [])
                .unwrap();
            seed_txn(&st.conn, 50, 1, "AMAZON", "AMAZON").unwrap();
            seed_txn(&st.conn, 51, 1, "AMAZON", "AMAZON").unwrap();
            seed_pn(&st.conn, "AMAZON", "Amazon", Status::Pending, 2).unwrap();
        }
        state.lock().unwrap().txn_active = Some(50);

        act_norm(&state, 50, Decision::Confirm);

        let active = state.lock().unwrap().txn_active;
        assert_eq!(
            active,
            Some(50),
            "resolver should fall back to the acted-on row when no other rows remain"
        );
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
        assert_eq!(entry.pillar, crate::serve::state::TxnActionPillar::Norm);
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
        // Round-5: undo restores the just-undone row as active so the
        // user can review it (contextually, undo means 'I made a
        // mistake, let me look at this again').
        assert_eq!(st.txn_active, Some(10));
    }

    #[test]
    fn act_pair_pushes_an_activity_entry_with_pair_pillar() {
        let state = fresh();
        act_pair(&state, 20, Decision::Confirm);
        let st = state.lock().unwrap();
        assert_eq!(st.txn_activity.len(), 1);
        assert_eq!(st.txn_activity[0].pillar, crate::serve::state::TxnActionPillar::Pair);
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
