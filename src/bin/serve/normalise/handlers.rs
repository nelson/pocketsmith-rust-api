//! Action handlers for the `/normalise/*` tab.
//!
//! Mirrors `crate::handlers` (transfers tab) in shape: each action records
//! a session decision, pushes an activity entry, advances `norm_active_slug`
//! to the next visible row, and (for confirm/reject) flips the staging
//! row's status. Undo reverts everything.

use std::sync::{Arc, Mutex};

use pocketsmith_sync::db::payee_normalisations as pn;
use pocketsmith_sync::normalise::apply as norm_apply;
use pocketsmith_sync::transfers::Status;

use crate::state::{AppState, Decision, NormActivityEntry};

use super::helpers::{
    get_filtered_normalisations, next_slug_after, NormClassFilter, NormStatusFilter,
};

/// Look up the [`pn::PayeeNormalisationRow`] for a slug. Returns None if no
/// row is staged under that slug (e.g. the user is replaying a stale URL).
fn row_for_slug(state: &AppState, slug: &str) -> Option<pn::PayeeNormalisationRow> {
    pn::get_by_slug(&state.conn, slug).ok().flatten()
}

fn refilter(state: &AppState) -> Vec<pn::PayeeNormalisationRow> {
    let status = NormStatusFilter::parse(&state.norm_status_filter);
    let class = NormClassFilter::parse(&state.norm_class_filter);
    get_filtered_normalisations(&state.conn, status, class, &state.norm_decisions)
}

/// Apply a decision to a slug. Confirm/Reject also flips the DB status.
/// After every action the next visible slug becomes the active row, so
/// keyboard-driven review keeps moving through the queue. Mirrors
/// `handle_action` in `crate::handlers`.
pub fn act(state: &Arc<Mutex<AppState>>, slug: &str, decision: Decision) {
    let mut st = state.lock().unwrap();

    let row = match row_for_slug(&st, slug) {
        Some(r) => r,
        None => return,
    };

    // Compute the next slug *before* mutating, against the current view.
    let current_view = refilter(&st);
    let next = next_slug_after(&current_view, slug);

    match decision {
        Decision::Confirm => {
            let _ = pn::update_status(&st.conn, &row.original_payee, Status::Confirmed);
        }
        Decision::Reject => {
            let _ = pn::update_status(&st.conn, &row.original_payee, Status::Rejected);
        }
        Decision::Skip => {}
    }

    st.norm_decisions.insert(row.original_payee.clone(), decision);
    st.norm_activity.push(NormActivityEntry {
        slug: row.slug.clone(),
        original_payee: row.original_payee.clone(),
        proposed_payee: row.proposed_payee.clone(),
        txn_count: row.txn_count,
        decision,
    });
    if st.norm_activity.len() > 100 {
        st.norm_activity.remove(0);
    }

    // Re-filter (the just-acted-on row may have left the visible set) and
    // pick the new active slug: prefer the previously-computed `next`
    // slug if it's still visible, else fall back to the tail.
    let new_view = refilter(&st);
    st.norm_active_slug = next
        .filter(|n| new_view.iter().any(|r| r.slug == *n))
        .or_else(|| new_view.last().map(|r| r.slug.clone()));
}

/// Revert a confirm/reject/skip on a slug. Resets DB status to Pending,
/// drops the session decision, removes the activity entry, bumps the undo
/// counter. Mirrors `handle_undo` in `crate::handlers`.
pub fn undo(state: &Arc<Mutex<AppState>>, slug: &str) {
    let mut st = state.lock().unwrap();
    let row = match row_for_slug(&st, slug) {
        Some(r) => r,
        None => return,
    };
    let _ = pn::update_status(&st.conn, &row.original_payee, Status::Pending);
    st.norm_undone += 1;
    st.norm_decisions.remove(&row.original_payee);
    st.norm_activity.retain(|e| e.slug != row.slug);
}

/// Drop every active Skip decision in one shot. Mirrors
/// `handle_clear_all_skipped` in `crate::handlers`.
pub fn clear_all_skipped(state: &Arc<Mutex<AppState>>) {
    let mut st = state.lock().unwrap();
    st.norm_decisions.retain(|_, d| *d != Decision::Skip);
}

// Backwards-compatible thin wrappers used by the route table in main.rs.
pub fn confirm(state: &Arc<Mutex<AppState>>, slug: &str) { act(state, slug, Decision::Confirm); }
pub fn reject(state: &Arc<Mutex<AppState>>, slug: &str) { act(state, slug, Decision::Reject); }
pub fn skip(state: &Arc<Mutex<AppState>>, slug: &str) { act(state, slug, Decision::Skip); }
pub fn unskip(state: &Arc<Mutex<AppState>>, slug: &str) { undo(state, slug); }

/// Drain confirmed proposals: write `transactions.payee` and delete the
/// staging row for each. Bumps `norm_applied`.
pub fn apply(state: &Arc<Mutex<AppState>>) {
    let mut st = state.lock().unwrap();
    if let Ok(stats) = norm_apply::apply_confirmed(&st.conn) {
        st.norm_applied += stats.transactions_updated;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::initialize_in_memory;
    use pocketsmith_sync::db::payee_normalisations::PayeeNormalisationRow;
    use rusqlite::params;

    fn make_state() -> Arc<Mutex<AppState>> {
        let conn = initialize_in_memory().unwrap();
        Arc::new(Mutex::new(AppState::new(conn)))
    }

    fn seed(state: &Arc<Mutex<AppState>>, original: &str, proposed: &str, status: Status, txn_count: i64) -> String {
        let st = state.lock().unwrap();
        let slug = pn::slug_for(original);
        pn::upsert(
            &st.conn,
            &PayeeNormalisationRow {
                original_payee: original.into(),
                proposed_payee: proposed.into(),
                slug: slug.clone(),
                class: Some("merchant".into()),
                features_json: "{}".into(),
                txn_count,
                status,
            },
        )
        .unwrap();
        slug
    }

    fn seed_txn(state: &Arc<Mutex<AppState>>, id: i64, original_payee: &str, payee: &str) {
        let st = state.lock().unwrap();
        st.conn
            .execute(
                "INSERT INTO transaction_accounts (id, name) VALUES (1, 'Test') ON CONFLICT DO NOTHING",
                [],
            )
            .unwrap();
        pocketsmith_sync::db::with_operation(&st.conn, "test-seed", |conn| {
            conn.execute(
                "INSERT INTO transactions (id, transaction_account_id, date, amount, original_payee, payee)
                 VALUES (?1, 1, '2026-01-01', -10.0, ?2, ?3)",
                params![id, original_payee, payee],
            )?;
            Ok(())
        })
        .unwrap();
    }

    fn current_status(state: &Arc<Mutex<AppState>>, original: &str) -> Option<Status> {
        let st = state.lock().unwrap();
        pn::get_by_original(&st.conn, original).unwrap().map(|r| r.status)
    }

    #[test]
    fn confirm_records_decision_pushes_activity_and_flips_status() {
        let state = make_state();
        let slug = seed(&state, "WOOLIES", "Woolworths", Status::Pending, 7);
        confirm(&state, &slug);
        assert_eq!(current_status(&state, "WOOLIES"), Some(Status::Confirmed));
        let st = state.lock().unwrap();
        assert_eq!(st.norm_decisions.get("WOOLIES"), Some(&Decision::Confirm));
        assert_eq!(st.norm_activity.len(), 1);
        assert_eq!(st.norm_activity[0].txn_count, 7);
    }

    #[test]
    fn reject_flips_status_to_rejected() {
        let state = make_state();
        let slug = seed(&state, "COLES", "Coles", Status::Pending, 1);
        reject(&state, &slug);
        assert_eq!(current_status(&state, "COLES"), Some(Status::Rejected));
        assert_eq!(state.lock().unwrap().norm_decisions.get("COLES"), Some(&Decision::Reject));
    }

    #[test]
    fn skip_records_session_decision_without_db_status_change() {
        let state = make_state();
        let slug = seed(&state, "ALDI", "ALDI", Status::Pending, 1);
        skip(&state, &slug);
        let st = state.lock().unwrap();
        assert_eq!(st.norm_decisions.get("ALDI"), Some(&Decision::Skip));
        drop(st);
        assert_eq!(current_status(&state, "ALDI"), Some(Status::Pending));
    }

    #[test]
    fn act_advances_active_slug_to_next_visible_row() {
        let state = make_state();
        // Seed three pending rows (txn_count desc -> A, B, C ordering).
        let a = seed(&state, "A", "a", Status::Pending, 10);
        let _b = seed(&state, "B", "b", Status::Pending, 5);
        let _c = seed(&state, "C", "c", Status::Pending, 1);
        // Filter set to Pending so confirming A pushes us off A onto B.
        state.lock().unwrap().norm_status_filter = "pending".into();
        state.lock().unwrap().norm_active_slug = Some(a.clone());
        confirm(&state, &a);
        let st = state.lock().unwrap();
        assert_eq!(st.norm_active_slug.as_deref(), Some(pn::slug_for("B")).as_deref());
    }

    #[test]
    fn undo_reverts_decision_and_status_and_clears_activity() {
        let state = make_state();
        let slug = seed(&state, "WOOLIES", "Woolworths", Status::Pending, 1);
        confirm(&state, &slug);
        undo(&state, &slug);
        assert_eq!(current_status(&state, "WOOLIES"), Some(Status::Pending));
        let st = state.lock().unwrap();
        assert!(st.norm_decisions.is_empty());
        assert!(st.norm_activity.is_empty());
        assert_eq!(st.norm_undone, 1);
    }

    #[test]
    fn clear_all_skipped_drops_only_skip_decisions() {
        let state = make_state();
        let s1 = seed(&state, "X", "x", Status::Pending, 1);
        let s2 = seed(&state, "Y", "y", Status::Pending, 1);
        let s3 = seed(&state, "Z", "z", Status::Pending, 1);
        skip(&state, &s1);
        confirm(&state, &s2);
        skip(&state, &s3);
        clear_all_skipped(&state);
        let st = state.lock().unwrap();
        assert_eq!(st.norm_decisions.len(), 1);
        assert_eq!(st.norm_decisions.get("Y"), Some(&Decision::Confirm));
    }

    #[test]
    fn apply_drains_confirmed_rows_and_bumps_session_counter() {
        let state = make_state();
        seed_txn(&state, 1, "WOOLIES", "WOOLIES");
        seed_txn(&state, 2, "WOOLIES", "WOOLIES");
        let _slug = seed(&state, "WOOLIES", "Woolworths", Status::Confirmed, 2);
        apply(&state);
        assert_eq!(state.lock().unwrap().norm_applied, 2);
    }

    #[test]
    fn act_is_noop_for_unknown_slug() {
        let state = make_state();
        confirm(&state, "0000000000000000");
        assert!(state.lock().unwrap().norm_decisions.is_empty());
    }
}
