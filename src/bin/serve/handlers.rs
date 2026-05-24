use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use pocketsmith_sync::db::transfer_pairs;
use pocketsmith_sync::transfers::{self, Status};

use crate::helpers::{
    get_filtered_pairs, pairs_eligible_for_bulk, parse_pair_id,
};
use crate::state::{ActivityEntry, AppState, Decision};
use crate::tab::next_after;
use crate::views::render_current_page;

// Handles confirm/reject/skip actions on a pair. Parses pair ID from path, updates DB status
// (for confirm/reject), records the decision in memory, logs activity, advances to next pair.
// Called by: main::handle_request (POST /pair/{id}/confirm, /reject, /skip).
// Calls: parse_pair_id, get_filtered_pairs, next_pair_after, find_pair_index,
//        transfer_pairs::get_pair_by_id, transfer_pairs::update_status, render_current_page.
pub fn handle_action(state: &Arc<Mutex<AppState>>, path: &str, action: &str) -> Markup {
    let id = parse_pair_id(path, "/transfers/pair/");
    if let Some((a, b)) = id {
        let mut state = state.lock().unwrap();

        let decision = match action {
            "confirm" => Decision::Confirm,
            "reject" => Decision::Reject,
            "skip" => Decision::Skip,
            _ => return html! { p { "Invalid action" } },
        };

        let current_pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
        let next = next_after(&current_pairs, |p| (p.txn_id_a, p.txn_id_b) == (a, b))
            .map(|p| (p.txn_id_a, p.txn_id_b));

        let pair_info = transfer_pairs::get_pair_by_id(&state.conn, a, b)
            .ok()
            .flatten()
            .map(|p| (p.amount_cents, p.account_name_a, p.account_name_b));

        match decision {
            Decision::Confirm => {
                let _ = transfer_pairs::update_status(&state.conn, a, b, Status::Confirmed);
            }
            Decision::Reject => {
                let _ = transfer_pairs::update_status(&state.conn, a, b, Status::Rejected);
            }
            Decision::Skip => {}
        }
        state.decisions.insert((a, b), decision);

        if let Some((amount, acct_a, acct_b)) = pair_info {
            state.activity.push(ActivityEntry {
                pair_id: (a, b),
                decision,
                amount_cents: amount,
                account_a: acct_a,
                account_b: acct_b,
            });
            if state.activity.len() > 100 {
                state.activity.remove(0);
            }
        }

        let new_pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
        if let Some(next_id) = next {
            if new_pairs.iter().any(|p| (p.txn_id_a, p.txn_id_b) == next_id) {
                state.active_pair = Some(next_id);
            } else {
                state.active_pair = new_pairs.last().map(|p| (p.txn_id_a, p.txn_id_b));
            }
        } else {
            state.active_pair = new_pairs.last().map(|p| (p.txn_id_a, p.txn_id_b));
        }

        return render_current_page(&state);
    }

    html! { p { "Invalid request" } }
}

// Reverts a confirm/reject/skip: resets DB status to Pending, removes the decision, and clears
// the activity entry. Increments the undone counter.
// Called by: main::handle_request (POST /pair/{id}/undo).
// Calls: parse_pair_id, transfer_pairs::update_status, render_current_page.
pub fn handle_undo(state: &Arc<Mutex<AppState>>, path: &str) -> Markup {
    let id = parse_pair_id(path, "/transfers/pair/");
    if let Some((a, b)) = id {
        let mut state = state.lock().unwrap();
        let _ = transfer_pairs::update_status(&state.conn, a, b, Status::Pending);
        state.undone += 1;
        state.decisions.remove(&(a, b));
        state.activity.retain(|e| e.pair_id != (a, b));
        return render_current_page(&state);
    }

    let state = state.lock().unwrap();
    render_current_page(&state)
}

// Removes all skip decisions at once, clearing the skipped filter view.
// Called by: main::handle_request (POST /clear-all-skipped).
// Calls: render_current_page.
pub fn handle_clear_all_skipped(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut state = state.lock().unwrap();
    state.activity.retain(|e| e.decision != Decision::Skip);
    state.decisions.retain(|_, v| *v != Decision::Skip);
    render_current_page(&state)
}

// Apply confirm or reject to every pair currently visible in the queue, except
// session-skipped ones. The current filters (status + confidence) come from
// AppState so this matches exactly what the user sees on screen. Each affected
// pair gets a DB write, an in-memory decision, and an activity entry. Active
// pair advances to the last remaining visible pair (or None if none).
//
// Called by: main::handle_request (POST /bulk-confirm, POST /bulk-reject).
// Calls: get_filtered_pairs, pairs_eligible_for_bulk, transfer_pairs::update_status,
//        transfer_pairs::get_pair_by_id, render_current_page.
pub fn handle_bulk_action(state: &Arc<Mutex<AppState>>, action: &str) -> Markup {
    let (decision, status) = match action {
        "confirm" => (Decision::Confirm, Status::Confirmed),
        "reject" => (Decision::Reject, Status::Rejected),
        _ => return html! { p { "Invalid bulk action" } },
    };

    let mut state = state.lock().unwrap();
    let pairs = get_filtered_pairs(
        &state.conn,
        &state.status_filter,
        &state.confidence_filter,
        &state.decisions,
    );
    let eligible = pairs_eligible_for_bulk(&pairs, &state.decisions);

    for (a, b) in &eligible {
        let pair_info = transfer_pairs::get_pair_by_id(&state.conn, *a, *b)
            .ok()
            .flatten()
            .map(|p| (p.amount_cents, p.account_name_a, p.account_name_b));
        let _ = transfer_pairs::update_status(&state.conn, *a, *b, status);
        state.decisions.insert((*a, *b), decision);
        if let Some((amount, acct_a, acct_b)) = pair_info {
            state.activity.push(ActivityEntry {
                pair_id: (*a, *b),
                decision,
                amount_cents: amount,
                account_a: acct_a,
                account_b: acct_b,
            });
        }
    }
    while state.activity.len() > 100 {
        state.activity.remove(0);
    }

    let new_pairs = get_filtered_pairs(
        &state.conn,
        &state.status_filter,
        &state.confidence_filter,
        &state.decisions,
    );
    state.active_pair = new_pairs.last().map(|p| (p.txn_id_a, p.txn_id_b));

    render_current_page(&state)
}

// Removes a single skip decision (unlike handle_undo, does not touch DB status or undone counter).
// Called by: main::handle_request (POST /pair/{id}/unskip).
// Calls: parse_pair_id, render_current_page.
pub fn handle_unskip(state: &Arc<Mutex<AppState>>, path: &str) -> Markup {
    let id = parse_pair_id(path, "/transfers/pair/");
    if let Some((a, b)) = id {
        let mut state = state.lock().unwrap();
        state.decisions.remove(&(a, b));
        state.activity.retain(|e| !(e.pair_id == (a, b) && e.decision == Decision::Skip));
        return render_current_page(&state);
    }
    let state = state.lock().unwrap();
    render_current_page(&state)
}

// Apply all confirmed pairs by calling transfers::apply_confirmed (which tags
// transactions with the _Transfer category and deletes the pair rows). Then:
//   - Bump state.applied by the number of pairs processed.
//   - Clear in-memory Confirm decisions for pairs that are no longer in
//     transfer_pairs (so the "Confirmed N" stat in the activity header stays
//     truthful after apply).
//   - Reset active_pair to whatever's still visible (or None).
//
// Errors from apply_confirmed (e.g. missing _Transfer category) are silently
// swallowed for now; surfacing them in the UI is a future improvement.
//
// Called by: main::handle_request (POST /apply).
// Calls: transfers::apply_confirmed, get_filtered_pairs, render_current_page.
pub fn handle_apply(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut state = state.lock().unwrap();
    let stats = match transfers::apply_confirmed(&state.conn) {
        Ok(s) => s,
        Err(_) => {
            // Surface errors in the UI in a later iteration; for now no-op.
            return render_current_page(&state);
        }
    };
    state.applied += stats.rows_drained;

    // After apply, confirmed pairs are deleted from transfer_pairs. Clear any
    // in-memory Confirm decisions for pair-ids that no longer exist so the
    // activity header counts reflect reality. Two-step to satisfy the borrow
    // checker: collect ids first while holding only &state.conn, then mutate
    // state.decisions.
    let stale_confirms: Vec<(i64, i64)> = state
        .decisions
        .iter()
        .filter(|(_, d)| **d == Decision::Confirm)
        .map(|(k, _)| *k)
        .filter(|(a, b)| {
            let exists: i64 = state
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
        state.decisions.remove(&id);
    }

    let new_pairs = get_filtered_pairs(
        &state.conn,
        &state.status_filter,
        &state.confidence_filter,
        &state.decisions,
    );
    state.active_pair = new_pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));

    render_current_page(&state)
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
            s.decisions = decisions;
            s
        }))
    }

    #[test]
    fn handle_apply_deletes_confirmed_pair_and_bumps_counter() {
        let state = fixture_with_confirmed_pair();
        let _ = handle_apply(&state);
        let s = state.lock().unwrap();
        assert_eq!(s.applied, 1, "applied counter should be 1 after applying 1 pair");
        assert!(
            !s.decisions.contains_key(&(1, 2)),
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
        let _ = handle_apply(&state);
        let s = state.lock().unwrap();
        assert_eq!(s.applied, 0);
    }
}
