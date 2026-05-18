use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use pocketsmith_sync::db::transfer_pairs;
use pocketsmith_sync::transfers::Status;

use crate::helpers::{
    find_pair_index, get_filtered_pairs, next_pair_after, parse_pair_id,
};
use crate::state::{ActivityEntry, AppState, Decision};
use crate::views::render_current_page;

// Handles confirm/reject/skip actions on a pair. Parses pair ID from path, updates DB status
// (for confirm/reject), records the decision in memory, logs activity, advances to next pair.
// Called by: main::handle_request (POST /pair/{id}/confirm, /reject, /skip).
// Calls: parse_pair_id, get_filtered_pairs, next_pair_after, find_pair_index,
//        transfer_pairs::get_pair_by_id, transfer_pairs::update_status, render_current_page.
pub fn handle_action(state: &Arc<Mutex<AppState>>, path: &str, action: &str) -> Markup {
    let id = parse_pair_id(path, "/pair/");
    if let Some((a, b)) = id {
        let mut state = state.lock().unwrap();

        let decision = match action {
            "confirm" => Decision::Confirm,
            "reject" => Decision::Reject,
            "skip" => Decision::Skip,
            _ => return html! { p { "Invalid action" } },
        };

        let current_pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
        let next = next_pair_after(&current_pairs, (a, b));

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
            if find_pair_index(&new_pairs, next_id).is_some() {
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
    let id = parse_pair_id(path, "/pair/");
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

// Removes a single skip decision (unlike handle_undo, does not touch DB status or undone counter).
// Called by: main::handle_request (POST /pair/{id}/unskip).
// Calls: parse_pair_id, render_current_page.
pub fn handle_unskip(state: &Arc<Mutex<AppState>>, path: &str) -> Markup {
    let id = parse_pair_id(path, "/pair/");
    if let Some((a, b)) = id {
        let mut state = state.lock().unwrap();
        state.decisions.remove(&(a, b));
        state.activity.retain(|e| !(e.pair_id == (a, b) && e.decision == Decision::Skip));
        return render_current_page(&state);
    }
    let state = state.lock().unwrap();
    render_current_page(&state)
}
