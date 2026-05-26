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
    get_filtered_normalisations, NormClassFilter, NormStatusFilter,
};
use crate::tab::next_after;

/// Look up the [`pn::PayeeNormalisationRow`] for a slug. Returns None if no
/// row is staged under that slug (e.g. the user is replaying a stale URL).
fn row_for_slug(state: &AppState, slug: &str) -> Option<pn::PayeeNormalisationRow> {
    pn::get_by_slug(&state.conn, slug).ok().flatten()
}

fn refilter(state: &AppState) -> Vec<pn::PayeeNormalisationRow> {
    let status = NormStatusFilter::parse(&state.norm_status_filter);
    let class = NormClassFilter::parse(&state.norm_class_filter);
    get_filtered_normalisations(&state.conn, status, class, &state.normalise.decisions)
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
    let next = next_after(&current_view, |r| r.slug == slug).map(|r| r.slug.clone());

    match decision {
        Decision::Confirm => {
            let _ = pn::update_status(&st.conn, &row.original_payee, Status::Confirmed);
        }
        Decision::Reject => {
            let _ = pn::update_status(&st.conn, &row.original_payee, Status::Rejected);
        }
        Decision::Skip => {}
    }

    st.normalise.decisions.insert(row.original_payee.clone(), decision);
    st.normalise.push_activity(NormActivityEntry {
        slug: row.slug.clone(),
        original_payee: row.original_payee.clone(),
        proposed_payee: row.proposed_payee.clone(),
        txn_count: row.txn_count,
        decision,
    });

    // Re-filter (the just-acted-on row may have left the visible set) and
    // pick the new active slug: prefer the previously-computed `next`
    // slug if it's still visible, else fall back to the tail.
    let new_view = refilter(&st);
    st.normalise.active = next
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
    st.normalise.undone += 1;
    st.normalise.decisions.remove(&row.original_payee);
    st.normalise.activity.retain(|e| e.slug != row.slug);
    // Restore the just-undone row as active. Contextually undo means
    // 'I made a mistake, let me look at this again' -- the detail
    // panel must show the row in question.
    st.normalise.active = Some(row.slug.clone());
}

/// Drop every active Skip decision in one shot. Mirrors
/// `handle_clear_all_skipped` in `crate::handlers`.
pub fn clear_all_skipped(state: &Arc<Mutex<AppState>>) {
    let mut st = state.lock().unwrap();
    st.normalise.decisions.retain(|_, d| *d != Decision::Skip);
}

// `act` and `undo` are the only entry points the route table needs;
// confirm/reject/skip are just `act(state, slug, Decision::*)` and
// unskip is `undo` (clearing the session decision).

/// Drain confirmed proposals: write `transactions.payee` and delete the
/// staging row for each. Bumps `norm_applied`.
pub fn apply(state: &Arc<Mutex<AppState>>) {
    let mut st = state.lock().unwrap();
    if let Ok(stats) = norm_apply::apply_confirmed(&st.conn) {
        st.normalise.applied += stats.transactions_updated;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::initialize_in_memory;
    use pocketsmith_sync::test_support::{seed_account, seed_pn, seed_txn};

    fn make_state() -> Arc<Mutex<AppState>> {
        let conn = initialize_in_memory().unwrap();
        Arc::new(Mutex::new(AppState::new(conn)))
    }

    fn seed(state: &Arc<Mutex<AppState>>, original: &str, proposed: &str, status: Status, txn_count: i64) -> String {
        let st = state.lock().unwrap();
        seed_pn(&st.conn, original, proposed, status, txn_count).unwrap()
    }

    fn insert_txn(state: &Arc<Mutex<AppState>>, id: i64, original_payee: &str, payee: &str) {
        let st = state.lock().unwrap();
        seed_account(&st.conn, 1, "Test").unwrap();
        seed_txn(&st.conn, id, 1, original_payee, payee).unwrap();
    }

    fn current_status(state: &Arc<Mutex<AppState>>, original: &str) -> Option<Status> {
        let st = state.lock().unwrap();
        pn::get_by_original(&st.conn, original).unwrap().map(|r| r.status)
    }

    #[test]
    fn confirm_records_decision_pushes_activity_and_flips_status() {
        let state = make_state();
        let slug = seed(&state, "WOOLIES", "Woolworths", Status::Pending, 7);
        act(&state, &slug, Decision::Confirm);
        assert_eq!(current_status(&state, "WOOLIES"), Some(Status::Confirmed));
        let st = state.lock().unwrap();
        assert_eq!(st.normalise.decisions.get("WOOLIES"), Some(&Decision::Confirm));
        assert_eq!(st.normalise.activity.len(), 1);
        assert_eq!(st.normalise.activity[0].txn_count, 7);
    }

    #[test]
    fn reject_flips_status_to_rejected() {
        let state = make_state();
        let slug = seed(&state, "COLES", "Coles", Status::Pending, 1);
        act(&state, &slug, Decision::Reject);
        assert_eq!(current_status(&state, "COLES"), Some(Status::Rejected));
        assert_eq!(state.lock().unwrap().normalise.decisions.get("COLES"), Some(&Decision::Reject));
    }

    #[test]
    fn skip_records_session_decision_without_db_status_change() {
        let state = make_state();
        let slug = seed(&state, "ALDI", "ALDI", Status::Pending, 1);
        act(&state, &slug, Decision::Skip);
        let st = state.lock().unwrap();
        assert_eq!(st.normalise.decisions.get("ALDI"), Some(&Decision::Skip));
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
        state.lock().unwrap().normalise.active = Some(a.clone());
        act(&state, &a, Decision::Confirm);
        let st = state.lock().unwrap();
        assert_eq!(st.normalise.active.as_deref(), Some(pn::slug_for("B")).as_deref());
    }

    #[test]
    fn undo_reverts_decision_and_status_and_clears_activity() {
        let state = make_state();
        let slug = seed(&state, "WOOLIES", "Woolworths", Status::Pending, 1);
        act(&state, &slug, Decision::Confirm);
        undo(&state, &slug);
        assert_eq!(current_status(&state, "WOOLIES"), Some(Status::Pending));
        let st = state.lock().unwrap();
        assert!(st.normalise.decisions.is_empty());
        assert!(st.normalise.activity.is_empty());
        assert_eq!(st.normalise.undone, 1);
        // Round-5: undo restores the just-undone row as active so the
        // user lands on it for review.
        assert_eq!(st.normalise.active.as_deref(), Some(slug.as_str()));
    }

    #[test]
    fn clear_all_skipped_drops_only_skip_decisions() {
        let state = make_state();
        let s1 = seed(&state, "X", "x", Status::Pending, 1);
        let s2 = seed(&state, "Y", "y", Status::Pending, 1);
        let s3 = seed(&state, "Z", "z", Status::Pending, 1);
        act(&state, &s1, Decision::Skip);
        act(&state, &s2, Decision::Confirm);
        act(&state, &s3, Decision::Skip);
        clear_all_skipped(&state);
        let st = state.lock().unwrap();
        assert_eq!(st.normalise.decisions.len(), 1);
        assert_eq!(st.normalise.decisions.get("Y"), Some(&Decision::Confirm));
    }

    #[test]
    fn apply_drains_confirmed_rows_and_bumps_session_counter() {
        let state = make_state();
        insert_txn(&state, 1, "WOOLIES", "WOOLIES");
        insert_txn(&state, 2, "WOOLIES", "WOOLIES");
        let _slug = seed(&state, "WOOLIES", "Woolworths", Status::Confirmed, 2);
        apply(&state);
        assert_eq!(state.lock().unwrap().normalise.applied, 2);
    }

    #[test]
    fn act_is_noop_for_unknown_slug() {
        let state = make_state();
        act(&state, "0000000000000000", Decision::Confirm);
        assert!(state.lock().unwrap().normalise.decisions.is_empty());
    }
}
