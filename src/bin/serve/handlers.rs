use std::sync::{Arc, Mutex};

use pocketsmith_sync::db::transfer_pairs;
use pocketsmith_sync::review::Status;
use pocketsmith_sync::transfers;

use crate::helpers::{get_filtered_pairs, pairs_eligible_for_bulk};
use crate::state::{ActivityEntry, AppState, Decision};
use crate::tab::next_after;

/// Build an [`ActivityEntry`] for a transfer pair, fetching account/amount
/// info from the DB. Returns None if the pair has disappeared.
fn activity_for(state: &AppState, key: (i64, i64), decision: Decision) -> Option<ActivityEntry> {
    transfer_pairs::get_pair_by_id(&state.conn, key.0, key.1)
        .ok()
        .flatten()
        .map(|p| ActivityEntry {
            pair_id: key,
            decision,
            amount_cents: p.amount_cents,
            account_a: p.account_name_a,
            account_b: p.account_name_b,
        })
}

/// Re-fetch the visible pair set with the current filters.
fn visible_pairs(state: &AppState) -> Vec<pocketsmith_sync::db::transfer_pairs::TransferPairRow> {
    get_filtered_pairs(
        &state.conn,
        &state.status_filter,
        &state.confidence_filter,
        &state.transfers.decisions,
    )
}

/// Apply a decision to a single pair. Confirm/Reject also flips the DB
/// status. After every action `state.transfers.active` advances to the
/// next visible pair (or the tail of the new visible set). Mirrors
/// `normalise::handlers::act`.
pub fn act(state: &Arc<Mutex<AppState>>, key: (i64, i64), decision: Decision) {
    let mut st = state.lock().unwrap();
    let current = visible_pairs(&st);
    let next = next_after(&current, |p| (p.txn_id_a, p.txn_id_b) == key)
        .map(|p| (p.txn_id_a, p.txn_id_b));

    let activity = activity_for(&st, key, decision);

    match decision {
        Decision::Confirm => {
            let _ = transfer_pairs::update_status(&st.conn, key.0, key.1, Status::Confirmed);
        }
        Decision::Reject => {
            let _ = transfer_pairs::update_status(&st.conn, key.0, key.1, Status::Rejected);
        }
        Decision::Skip => {}
    }
    st.transfers.decisions.insert(key, decision);
    if let Some(a) = activity {
        st.transfers.push_activity(a);
    }

    let new_pairs = visible_pairs(&st);
    st.transfers.active = next
        .filter(|n| new_pairs.iter().any(|p| (p.txn_id_a, p.txn_id_b) == *n))
        .or_else(|| new_pairs.last().map(|p| (p.txn_id_a, p.txn_id_b)));
}

/// Revert a decision. Resets DB status to Pending and bumps the undone
/// counter only when undoing a real DB-touching decision
/// (Confirm/Reject). Undoing a Skip is a session-only operation.
/// Mirrors `normalise::handlers::undo`.
pub fn undo(state: &Arc<Mutex<AppState>>, key: (i64, i64)) {
    let mut st = state.lock().unwrap();
    let prior = st.transfers.decisions.get(&key).copied();
    if matches!(prior, Some(Decision::Confirm | Decision::Reject)) {
        let _ = transfer_pairs::update_status(&st.conn, key.0, key.1, Status::Pending);
        st.transfers.undone += 1;
    }
    st.transfers.decisions.remove(&key);
    st.transfers.activity.retain(|e| e.pair_id != key);
}

/// Drop every active Skip decision in one shot.
pub fn clear_all_skipped(state: &Arc<Mutex<AppState>>) {
    let mut st = state.lock().unwrap();
    st.transfers.activity.retain(|e| e.decision != Decision::Skip);
    st.transfers.decisions.retain(|_, v| *v != Decision::Skip);
}

/// Apply a single decision to every pair currently visible in the queue
/// (excluding session-skipped). Decision must be Confirm or Reject.
pub fn bulk_act(state: &Arc<Mutex<AppState>>, decision: Decision) {
    let status = match decision {
        Decision::Confirm => Status::Confirmed,
        Decision::Reject => Status::Rejected,
        Decision::Skip => return, // bulk-skip is not a thing
    };
    let mut st = state.lock().unwrap();
    let pairs = visible_pairs(&st);
    let eligible = pairs_eligible_for_bulk(&pairs, &st.transfers.decisions);

    for key in &eligible {
        let activity = activity_for(&st, *key, decision);
        let _ = transfer_pairs::update_status(&st.conn, key.0, key.1, status);
        st.transfers.decisions.insert(*key, decision);
        if let Some(a) = activity {
            st.transfers.push_activity(a);
        }
    }
    let new_pairs = visible_pairs(&st);
    st.transfers.active = new_pairs.last().map(|p| (p.txn_id_a, p.txn_id_b));
}

/// Drain confirmed pairs: tag both legs of each pair and delete the
/// pair row. Bumps `transfers.applied`. Drops in-memory Confirm
/// decisions whose pair rows are gone.
pub fn apply(state: &Arc<Mutex<AppState>>) {
    let mut st = state.lock().unwrap();
    let stats = match transfers::apply_confirmed(&st.conn) {
        Ok(s) => s,
        // Errors surface to the UI in a later iteration; no-op for now.
        Err(_) => return,
    };
    st.transfers.applied += stats.rows_drained;

    // Clear stale in-memory Confirm decisions whose pair rows are gone
    // (so the 'Confirmed N' stat in the activity header stays truthful).
    let stale_confirms: Vec<(i64, i64)> = st
        .transfers
        .decisions
        .iter()
        .filter(|(_, d)| **d == Decision::Confirm)
        .map(|(k, _)| *k)
        .filter(|(a, b)| {
            let exists: i64 = st
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM transfer_pairs WHERE txn_id_a = ?1 AND txn_id_b = ?2",
                    rusqlite::params![a, b],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            exists == 0
        })
        .collect();
    for id in stale_confirms {
        st.transfers.decisions.remove(&id);
    }

    let new_pairs = visible_pairs(&st);
    st.transfers.active = new_pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::{
        initialize_in_memory, transfer_pairs as tp, upsert_category, upsert_transaction,
        upsert_transaction_account, with_operation,
    };
    use pocketsmith_sync::models::{Category, Transaction, TransactionAccount};
    use pocketsmith_sync::transfers::{Confidence, TransferPair};
    use std::collections::HashMap;

    fn mk_account(id: i64, name: &str) -> TransactionAccount {
        TransactionAccount {
            id,
            name: Some(name.to_string()),
            number: None,
            currency_code: None,
            account_type: None,
            current_balance: None,
            current_balance_date: None,
            current_balance_in_base_currency: None,
            current_balance_exchange_rate: None,
            safe_balance: None,
            safe_balance_in_base_currency: None,
            starting_balance: None,
            starting_balance_date: None,
            created_at: None,
            updated_at: None,
        }
    }

    fn mk_txn(id: i64, acct: TransactionAccount, amount: f64) -> Transaction {
        Transaction {
            id,
            transaction_type: None,
            payee: Some("p".into()),
            amount: Some(amount),
            amount_in_base_currency: None,
            date: Some("2026-03-01".into()),
            cheque_number: None,
            memo: None,
            is_transfer: Some(false),
            category: None,
            note: None,
            labels: None,
            original_payee: Some("p".into()),
            upload_source: None,
            closing_balance: None,
            transaction_account: Some(acct),
            status: None,
            needs_review: None,
            created_at: None,
            updated_at: None,
        }
    }

    fn fixture_with_confirmed_pair() -> Arc<Mutex<AppState>> {
        let conn = initialize_in_memory().unwrap();

        let transfer_cat = Category {
            id: 999,
            title: Some("_Transfer".into()),
            colour: None,
            children: None,
            parent_id: None,
            is_transfer: Some(true),
            is_bill: Some(false),
            roll_up: Some(false),
            refund_behaviour: None,
            created_at: None,
            updated_at: None,
        };
        upsert_category(&conn, &transfer_cat).unwrap();

        let acct1 = mk_account(100, "Savings");
        let acct2 = mk_account(200, "Everyday");
        upsert_transaction_account(&conn, &acct1).unwrap();
        upsert_transaction_account(&conn, &acct2).unwrap();

        with_operation(&conn, "test", |conn| {
            upsert_transaction(conn, &mk_txn(1, acct1.clone(), 500.0))?;
            upsert_transaction(conn, &mk_txn(2, acct2.clone(), -500.0))?;
            Ok(())
        })
        .unwrap();

        tp::insert_pair(
            &conn,
            &TransferPair {
                txn_id_a: 1,
                txn_id_b: 2,
                amount_cents: 50000,
                confidence: Confidence::High,
                status: pocketsmith_sync::transfers::Status::Confirmed,
            },
        )
        .unwrap();

        let mut decisions = HashMap::new();
        decisions.insert((1, 2), Decision::Confirm);

        Arc::new(Mutex::new({
            let mut s = AppState::new(conn);
            s.transfers.decisions = decisions;
            s
        }))
    }

    #[test]
    fn handle_apply_deletes_confirmed_pair_and_bumps_counter() {
        let state = fixture_with_confirmed_pair();
        apply(&state);
        let s = state.lock().unwrap();
        assert_eq!(s.transfers.applied, 1, "applied counter should be 1 after applying 1 pair");
        assert!(
            !s.transfers.decisions.contains_key(&(1, 2)),
            "in-memory Confirm for applied pair should be cleared"
        );
        let count: i64 = s.conn
            .query_row(
                "SELECT COUNT(*) FROM transfer_pairs WHERE txn_id_a = 1 AND txn_id_b = 2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn handle_apply_with_nothing_to_apply_is_noop() {
        let conn = initialize_in_memory().unwrap();
        let state = Arc::new(Mutex::new(AppState::new(conn)));
        apply(&state);
        let s = state.lock().unwrap();
        assert_eq!(s.transfers.applied, 0);
    }
}
