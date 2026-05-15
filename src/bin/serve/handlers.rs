use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use pocketsmith_sync::db::transfer_pairs;
use pocketsmith_sync::transfers::Status;

use crate::helpers::{
    find_pair_index, get_filtered_pairs, get_prior_pairs, next_pair_after, parse_pair_id,
};
use crate::state::{ActivityEntry, AppState, Decision};
use crate::views::{full_page, render_detail};

pub fn refresh_page(state: &AppState) -> Markup {
    let pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
    full_page(state, &pairs, &state.status_filter, &state.confidence_filter)
}

pub fn action_handler(state: &Arc<Mutex<AppState>>, path: &str, action: &str) -> Markup {
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

        return refresh_page(&state);
    }

    html! { p { "Invalid request" } }
}

pub fn undo_handler(state: &Arc<Mutex<AppState>>, path: &str) -> Markup {
    let id = parse_pair_id(path, "/pair/");
    if let Some((a, b)) = id {
        let mut state = state.lock().unwrap();
        let _ = transfer_pairs::update_status(&state.conn, a, b, Status::Pending);
        state.undone += 1;
        state.decisions.remove(&(a, b));
        state.activity.retain(|e| e.pair_id != (a, b));
        return refresh_page(&state);
    }

    let state = state.lock().unwrap();
    refresh_page(&state)
}

pub fn clear_all_skipped(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut state = state.lock().unwrap();
    state.activity.retain(|e| e.decision != Decision::Skip);
    state.decisions.retain(|_, v| *v != Decision::Skip);
    refresh_page(&state)
}

pub fn unskip_handler(state: &Arc<Mutex<AppState>>, path: &str) -> Markup {
    let id = parse_pair_id(path, "/pair/");
    if let Some((a, b)) = id {
        let mut state = state.lock().unwrap();
        state.decisions.remove(&(a, b));
        state.activity.retain(|e| !(e.pair_id == (a, b) && e.decision == Decision::Skip));
        return refresh_page(&state);
    }
    let state = state.lock().unwrap();
    refresh_page(&state)
}

pub fn detail_fragment(state: &Arc<Mutex<AppState>>, txn_a: i64, txn_b: i64) -> Markup {
    let mut state = state.lock().unwrap();
    state.active_pair = Some((txn_a, txn_b));
    match transfer_pairs::get_pair_by_id(&state.conn, txn_a, txn_b) {
        Ok(Some(pair)) => {
            let prior = get_prior_pairs(&state.conn, &pair.account_name_a, &pair.account_name_b);
            render_detail(&pair, &prior)
        }
        _ => html! { div.empty-state { p { "Pair not found" } } },
    }
}

pub fn queue_fragment(state: &Arc<Mutex<AppState>>, status_filter: &str, confidence_filter: &str) -> Markup {
    let mut state = state.lock().unwrap();
    state.status_filter = status_filter.to_string();
    state.confidence_filter = confidence_filter.to_string();
    let pairs = get_filtered_pairs(&state.conn, status_filter, confidence_filter, &state.decisions);
    let current = state.active_pair;
    let in_new_list = current.and_then(|id| find_pair_index(&pairs, id)).is_some();
    if !in_new_list {
        state.active_pair = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
    }
    let selected = state.active_pair;
    crate::views::render_queue(&pairs, selected, status_filter, confidence_filter, &state.decisions)
}

pub fn page_shell(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut state = state.lock().unwrap();
    let pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
    if state.active_pair.is_none() {
        state.active_pair = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
    }
    full_page(&state, &pairs, &state.status_filter, &state.confidence_filter)
}
