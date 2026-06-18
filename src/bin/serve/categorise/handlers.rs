//! Action handlers for the `/categorise/*` tab. Mirrors the normalise
//! handlers: each action records a session decision keyed by
//! `merchant_key`, pushes an activity entry, advances the active row, and
//! (for confirm/reject) flips the staging row's status. Undo reverts.

use std::sync::{Arc, Mutex};

use pocketsmith_sync::categorise::apply as cat_apply;
use pocketsmith_sync::db::category_proposals as cp;
use pocketsmith_sync::review::Status;

use crate::state::{AppState, CatActivityEntry, Decision};
use crate::tab::next_after;

use super::helpers::{category_title, get_filtered_proposals, CatStatusFilter};

fn refilter(state: &AppState) -> Vec<cp::CategoryProposalRow> {
    let status = CatStatusFilter::parse(&state.cat_status_filter);
    get_filtered_proposals(&state.conn, status, &state.categorise.decisions)
}

/// Apply a decision to a merchant key. Confirm/Reject also flips the DB
/// status. After each action the next visible row becomes active.
pub fn act(state: &Arc<Mutex<AppState>>, key: &str, decision: Decision) {
    let mut st = state.lock().unwrap();

    let row = match cp::get(&st.conn, key).ok().flatten() {
        Some(r) => r,
        None => return,
    };

    let current_view = refilter(&st);
    let next = next_after(&current_view, |r| r.merchant_key == key).map(|r| r.merchant_key.clone());

    match decision {
        Decision::Confirm => {
            let _ = cp::update_status(&st.conn, key, Status::Confirmed);
        }
        Decision::Reject => {
            let _ = cp::update_status(&st.conn, key, Status::Rejected);
        }
        Decision::Skip => {}
    }

    let title = category_title(&st.conn, row.proposed_category)
        .unwrap_or_else(|| "(unmapped)".to_string());
    st.categorise.decisions.insert(key.to_string(), decision);
    st.categorise.push_activity(CatActivityEntry {
        merchant_key: key.to_string(),
        category_title: title,
        txn_count: row.txn_count,
        decision,
    });

    let new_view = refilter(&st);
    st.categorise.active = next
        .filter(|n| new_view.iter().any(|r| r.merchant_key == *n))
        .or_else(|| new_view.last().map(|r| r.merchant_key.clone()));
}

/// Revert a confirm/reject/skip. Resets DB status to Pending, drops the
/// session decision + activity entry, bumps the undo counter.
pub fn undo(state: &Arc<Mutex<AppState>>, key: &str) {
    let mut st = state.lock().unwrap();
    if cp::get(&st.conn, key).ok().flatten().is_none() {
        return;
    }
    let _ = cp::update_status(&st.conn, key, Status::Pending);
    st.categorise.undone += 1;
    st.categorise.decisions.remove(key);
    st.categorise.activity.retain(|e| e.merchant_key != key);
    st.categorise.active = Some(key.to_string());
}

/// Drop every active Skip decision in one shot.
pub fn clear_all_skipped(state: &Arc<Mutex<AppState>>) {
    let mut st = state.lock().unwrap();
    st.categorise.decisions.retain(|_, d| *d != Decision::Skip);
}

/// Drain confirmed proposals into `transactions`. Bumps `applied`.
pub fn apply(state: &Arc<Mutex<AppState>>) {
    let mut st = state.lock().unwrap();
    if let Ok(stats) = cat_apply::apply_confirmed(&st.conn) {
        st.categorise.applied += stats.transactions_updated;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::category_proposals::CategoryProposalRow;
    use pocketsmith_sync::db::initialize_in_memory;

    fn make_state() -> Arc<Mutex<AppState>> {
        let conn = initialize_in_memory().unwrap();
        Arc::new(Mutex::new(AppState::new(conn)))
    }

    fn seed(state: &Arc<Mutex<AppState>>, key: &str, status: Status) {
        let st = state.lock().unwrap();
        cp::upsert(
            &st.conn,
            &CategoryProposalRow {
                merchant_key: key.into(),
                proposed_category: None,
                proposed_labels: vec!["cafe".into()],
                place_type: Some("cafe".into()),
                txn_count: 4,
                status,
            },
        )
        .unwrap();
    }

    fn db_status(state: &Arc<Mutex<AppState>>, key: &str) -> Option<Status> {
        let st = state.lock().unwrap();
        cp::get(&st.conn, key).unwrap().map(|r| r.status)
    }

    #[test]
    fn confirm_flips_status_and_records_activity() {
        let state = make_state();
        seed(&state, "cafe x", Status::Pending);
        act(&state, "cafe x", Decision::Confirm);
        assert_eq!(db_status(&state, "cafe x"), Some(Status::Confirmed));
        let st = state.lock().unwrap();
        assert_eq!(st.categorise.decisions.get("cafe x"), Some(&Decision::Confirm));
        assert_eq!(st.categorise.activity.len(), 1);
        assert_eq!(st.categorise.activity[0].txn_count, 4);
    }

    #[test]
    fn reject_flips_status() {
        let state = make_state();
        seed(&state, "k", Status::Pending);
        act(&state, "k", Decision::Reject);
        assert_eq!(db_status(&state, "k"), Some(Status::Rejected));
    }

    #[test]
    fn skip_is_session_only() {
        let state = make_state();
        seed(&state, "k", Status::Pending);
        act(&state, "k", Decision::Skip);
        assert_eq!(db_status(&state, "k"), Some(Status::Pending));
        assert_eq!(state.lock().unwrap().categorise.decisions.get("k"), Some(&Decision::Skip));
    }

    #[test]
    fn undo_reverts_status_and_clears() {
        let state = make_state();
        seed(&state, "k", Status::Pending);
        act(&state, "k", Decision::Confirm);
        undo(&state, "k");
        assert_eq!(db_status(&state, "k"), Some(Status::Pending));
        let st = state.lock().unwrap();
        assert!(st.categorise.decisions.is_empty());
        assert!(st.categorise.activity.is_empty());
        assert_eq!(st.categorise.undone, 1);
        assert_eq!(st.categorise.active.as_deref(), Some("k"));
    }

    #[test]
    fn act_is_noop_for_unknown_key() {
        let state = make_state();
        act(&state, "nope", Decision::Confirm);
        assert!(state.lock().unwrap().categorise.decisions.is_empty());
    }
}
