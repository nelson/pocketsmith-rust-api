//! Action handlers for the `/normalise/*` tab.
//!
//! Each handler takes a shared `AppState` (locked internally) and mutates
//! either the database (confirm/reject = status flip), the session skip
//! set (skip/unskip = in-memory HashMap), or both (apply = drain confirmed
//! rows from the staging table and bump the session counter).
//!
//! Handlers are intentionally small and return nothing — the caller in
//! `main.rs` re-renders the page after each successful action.

use std::sync::{Arc, Mutex};

use pocketsmith_sync::db::payee_normalisations as pn;
use pocketsmith_sync::normalise::apply as norm_apply;
use pocketsmith_sync::transfers::Status;

use crate::state::AppState;

/// Locate the original_payee for a URL slug. Returns None if no row is
/// staged under that slug (e.g. the user is replaying a stale URL).
fn slug_to_original(state: &AppState, slug: &str) -> Option<String> {
    pn::get_by_slug(&state.conn, slug)
        .ok()
        .flatten()
        .map(|row| row.original_payee)
}

/// Set the staging row's status to Confirmed. No-op if the slug is unknown.
/// Confirming clears any prior session "skip" mark for the same row.
pub fn confirm(state: &Arc<Mutex<AppState>>, slug: &str) {
    let mut st = state.lock().unwrap();
    if let Some(orig) = slug_to_original(&st, slug) {
        let _ = pn::update_status(&st.conn, &orig, Status::Confirmed);
        st.norm_skipped.remove(&orig);
    }
}

/// Set the staging row's status to Rejected.
pub fn reject(state: &Arc<Mutex<AppState>>, slug: &str) {
    let mut st = state.lock().unwrap();
    if let Some(orig) = slug_to_original(&st, slug) {
        let _ = pn::update_status(&st.conn, &orig, Status::Rejected);
        st.norm_skipped.remove(&orig);
    }
}

/// Add the row to the session-only skip set. The DB row stays pending —
/// skip just hides it from the Pending queue for the rest of this serve
/// run.
pub fn skip(state: &Arc<Mutex<AppState>>, slug: &str) {
    let mut st = state.lock().unwrap();
    if let Some(orig) = slug_to_original(&st, slug) {
        st.norm_skipped.insert(orig, ());
    }
}

/// Remove the row from the session skip set.
pub fn unskip(state: &Arc<Mutex<AppState>>, slug: &str) {
    let mut st = state.lock().unwrap();
    if let Some(orig) = slug_to_original(&st, slug) {
        st.norm_skipped.remove(&orig);
    }
}

/// Drain all confirmed rows: write `transactions.payee` for each and
/// delete the staging row. Increments `norm_applied` by the number of
/// transactions touched.
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

    fn seed(state: &Arc<Mutex<AppState>>, original: &str, proposed: &str, status: Status) -> String {
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
                txn_count: 1,
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
        pn::get_by_original(&st.conn, original)
            .unwrap()
            .map(|r| r.status)
    }

    #[test]
    fn confirm_flips_status_and_clears_session_skip() {
        let state = make_state();
        let slug = seed(&state, "WOOLIES", "Woolworths", Status::Pending);
        // Pre-skip; confirm should clear it.
        state.lock().unwrap().norm_skipped.insert("WOOLIES".into(), ());
        confirm(&state, &slug);
        assert_eq!(current_status(&state, "WOOLIES"), Some(Status::Confirmed));
        assert!(!state.lock().unwrap().norm_skipped.contains_key("WOOLIES"));
    }

    #[test]
    fn reject_flips_status() {
        let state = make_state();
        let slug = seed(&state, "COLES", "Coles", Status::Pending);
        reject(&state, &slug);
        assert_eq!(current_status(&state, "COLES"), Some(Status::Rejected));
    }

    #[test]
    fn skip_and_unskip_toggle_session_set_without_touching_db_status() {
        let state = make_state();
        let slug = seed(&state, "ALDI", "ALDI", Status::Pending);
        skip(&state, &slug);
        assert!(state.lock().unwrap().norm_skipped.contains_key("ALDI"));
        assert_eq!(current_status(&state, "ALDI"), Some(Status::Pending));

        unskip(&state, &slug);
        assert!(!state.lock().unwrap().norm_skipped.contains_key("ALDI"));
        assert_eq!(current_status(&state, "ALDI"), Some(Status::Pending));
    }

    #[test]
    fn apply_drains_confirmed_rows_and_bumps_session_counter() {
        let state = make_state();
        seed_txn(&state, 1, "WOOLIES", "WOOLIES");
        seed_txn(&state, 2, "WOOLIES", "WOOLIES");
        let _slug = seed(&state, "WOOLIES", "Woolworths", Status::Confirmed);

        apply(&state);

        // Counter reflects 2 transactions updated.
        assert_eq!(state.lock().unwrap().norm_applied, 2);
        // Confirmed staging row gone.
        assert!(state.lock().unwrap().conn.query_row(
            "SELECT COUNT(*) FROM payee_normalisations WHERE original_payee = 'WOOLIES'",
            [],
            |r| r.get::<_, i64>(0),
        ).unwrap() == 0);
        // Transaction payee written.
        let payee: String = state
            .lock()
            .unwrap()
            .conn
            .query_row("SELECT payee FROM transactions WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(payee, "Woolworths");
    }

    #[test]
    fn handlers_are_noops_for_unknown_slug() {
        let state = make_state();
        // No seed; slug doesn't resolve.
        confirm(&state, "0000000000000000");
        reject(&state, "0000000000000000");
        skip(&state, "0000000000000000");
        unskip(&state, "0000000000000000");
        assert!(state.lock().unwrap().norm_skipped.is_empty());
    }
}
