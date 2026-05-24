// Page layout and rendering functions for the transfer pair review UI.
//
// All render_* functions live here. Handlers (handlers.rs) mutate state, then
// call into this module to produce HTML responses.
//
// ┌──────────────────────────────────────────────────────────────────────┐
// │ render_page_shell  (GET /)                                           │
// │   Locks state, selects first pair if needed, delegates to            │
// │   render_full_page.                                                  │
// │                                                                      │
// │  ┌────────────────────────────────────────────────────────────────┐  │
// │  │ render_full_page                                               │  │
// │  │   Complete <html> document: <head>, CSS, body layout, JS.      │  │
// │  │                                                                │  │
// │  │ ┌───────────────────┐  ┌─────────────────────────────────────┐ │  │
// │  │ │ render_queue      │  │ render_detail                       │ │  │
// │  │ │   #queue          │  │   #detail                           │ │  │
// │  │ │                   │  │                                     │ │  │
// │  │ │ [filter buttons]  │  │ [confidence + amount header]        │ │  │
// │  │ │ [conf filters]    │  │ [side-by-side txn cards A | B]      │ │  │
// │  │ │ [scrollable       │  │ [prior transfer history]            │ │  │
// │  │ │  pair list]       │  │ [Y confirm / N reject / S skip]     │ │  │
// │  │ └───────────────────┘  └─────────────────────────────────────┘ │  │
// │  │                                                                │  │
// │  │ ┌────────────────────────────────────────────────────────────┐ │  │
// │  │ │ render_activity                                            │ │  │
// │  │ │   #activity                                                │ │  │
// │  │ │ [confirmed/rejected/skipped/undone counts]                 │ │  │
// │  │ │ [scrollable activity log with undo buttons]                │ │  │
// │  │ └────────────────────────────────────────────────────────────┘ │  │
// │  └────────────────────────────────────────────────────────────────┘  │
// └──────────────────────────────────────────────────────────────────────┘
//
// HTMX fragment endpoints (partial page swaps, no full reload):
//
//   render_detail_fragment  (GET /pair/{id})   → swaps #detail only
//   render_queue_fragment   (GET /queue?...)   → swaps #queue only
//   render_current_page     (after mutations)  → full <body> replacement

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use maud::{html, Markup, PreEscaped, DOCTYPE};

use pocketsmith_sync::db::transfer_pairs::{self, TransferPairRow};
use pocketsmith_sync::transfers::{self, Status};

use crate::css::CSS;
use crate::helpers::{
    confidence_class, confidence_reason, count_confirmed_in_db, count_decisions, derive_decision,
    find_pair_index, format_dollars, format_short_date, get_filtered_pairs, get_prior_pairs,
};
use crate::js::JS;
use crate::state::{AppState, Decision};

// Renders the complete HTML page including <head>, queue sidebar, detail panel, activity bar, and <script>.
// Called by: render_current_page, render_page_shell.
// Calls: render_queue, render_detail, render_activity, find_pair_index, get_prior_pairs.
// Shared tab bar at the top of every page. `active` is the slug of the
// current tab ("transfers" or "normalise"). The active tab is rendered
// without a link.
pub fn render_tab_bar(active: &str) -> Markup {
    html! {
        nav.tab-bar {
            @for (slug, label, href) in [("transfers", "Transfers", "/transfers/"), ("normalise", "Normalise", "/normalise/")] {
                @if slug == active {
                    span.tab.active { (label) }
                } @else {
                    a.tab href=(href) { (label) }
                }
            }
        }
    }
}

pub fn render_full_page(state: &AppState, pairs: &[TransferPairRow], status_filter: &str, confidence_filter: &str) -> Markup {
    let selected = state.active_pair
        .and_then(|id| find_pair_index(pairs, id).map(|_| id))
        .or_else(|| pairs.first().map(|p| (p.txn_id_a, p.txn_id_b)));

    let active = selected.and_then(|id| find_pair_index(pairs, id)).map(|i| &pairs[i]);
    let prior = active
        .map(|p| get_prior_pairs(&state.conn, &p.account_name_a, &p.account_name_b))
        .unwrap_or_default();

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Transfer Pairs" }
                script src="https://unpkg.com/htmx.org@2.0.4" {}
                style { (PreEscaped(CSS)) }
            }
            body {
                (render_tab_bar("transfers"))
                div.layout {
                    div.queue-panel #queue {
                        (render_queue(pairs, selected, status_filter, confidence_filter, &state.decisions))
                    }
                    div.detail-panel #detail {
                        @if let Some(pair) = active {
                            (render_detail(pair, &prior))
                        } @else {
                            div.empty-state { p { "No pairs to show" } }
                        }
                    }
                }
                div.activity-panel #activity {
                    (render_activity(state))
                }
                script { (PreEscaped(JS)) }
            }
        }
    }
}

// Bulk-action buttons that sit under the filter rows. When idle, two buttons:
// "Confirm all" and "Reject all". Clicking either swaps the bar to an inline
// confirmation prompt (no modal) via HTMX -- the user has to confirm a second
// time before any DB writes happen. The prompt's "Cancel" swaps back here.
//
// The count shows what's currently visible; the bulk action then targets the
// same set (minus session-skipped) when executed.
//
// Called by: render_queue, render_bulk_buttons_fragment (GET /bulk-buttons).
pub fn render_bulk_actions(visible_count: usize) -> Markup {
    html! {
        button.bulk-btn.bulk-confirm-btn
            hx-get="/transfers/bulk-prompt?action=confirm"
            hx-target="#bulk-actions"
            hx-swap="innerHTML"
            disabled[visible_count == 0]
        { "Confirm all (" (visible_count) ")" }
        button.bulk-btn.bulk-reject-btn
            hx-get="/transfers/bulk-prompt?action=reject"
            hx-target="#bulk-actions"
            hx-swap="innerHTML"
            disabled[visible_count == 0]
        { "Reject all (" (visible_count) ")" }
    }
}

// The inline confirmation form shown after "Confirm all" / "Reject all" is
// clicked. Two buttons: a destructive "Yes, confirm/reject all" that hits the
// real /bulk-{action} endpoint, and a "Cancel" that swaps the bar back to
// render_bulk_actions via GET /bulk-buttons.
//
// Called by: render_bulk_prompt_fragment (GET /bulk-prompt?action=X).
pub fn render_bulk_prompt(action: &str, visible_count: usize) -> Markup {
    let verb = match action { "reject" => "reject", _ => "confirm" };
    let yes_label = format!("Yes, {verb} {visible_count}");
    let post_url = format!("/transfers/bulk-{verb}");
    let yes_class = if verb == "reject" { "bulk-yes bulk-reject-btn" } else { "bulk-yes bulk-confirm-btn" };
    html! {
        span.bulk-prompt-text { "Apply to " (visible_count) " visible pair" @if visible_count != 1 { "s" } "?" }
        button.bulk-btn.(yes_class)
            hx-post=(post_url)
            hx-target="body"
        { (yes_label) }
        button.bulk-btn.bulk-cancel-btn
            hx-get="/transfers/bulk-buttons"
            hx-target="#bulk-actions"
            hx-swap="innerHTML"
        { "Cancel" }
    }
}

// Renders the queue sidebar: filter buttons (status + confidence) and the scrollable list of pair items.
// Called by: render_full_page, render_queue_fragment.
// Calls: confidence_class, format_dollars, format_short_date, transfers::date_diff_days.
pub fn render_queue(pairs: &[TransferPairRow], selected: Option<(i64, i64)>, status_filter: &str, confidence_filter: &str, decisions: &HashMap<(i64, i64), Decision>) -> Markup {
    html! {
        div.queue-header {
            h2 { (pairs.len()) " pairs" }
            div.filter-row {
                @for f in &["all", "pending", "confirmed", "rejected", "skipped"] {
                    button.filter-btn
                        .(if *f == status_filter { "active" } else { "" })
                        hx-get=(format!("/transfers/queue?filter={f}&conf={confidence_filter}"))
                        hx-target="#queue"
                        hx-swap="innerHTML"
                    { (f.to_uppercase()) }
                }
            }
            div.filter-row {
                @for f in &["all", "high", "medium", "low"] {
                    button.filter-btn.conf-filter
                        .(if *f == confidence_filter { "active" } else { "" })
                        hx-get=(format!("/transfers/queue?filter={status_filter}&conf={f}"))
                        hx-target="#queue"
                        hx-swap="innerHTML"
                    { (f.to_uppercase()) }
                }
                @let num_skipped = decisions.values().filter(|v| **v == Decision::Skip).count();
                @if num_skipped > 0 && status_filter == "skipped" {
                    button.filter-btn.clear-skipped-btn
                        hx-post="/transfers/clear-all-skipped"
                        hx-target="body"
                    { "CLEAR SKIPPED (" (num_skipped) ")" }
                }
            }
            div.bulk-actions #bulk-actions {
                @let eligible_count = crate::helpers::pairs_eligible_for_bulk(pairs, decisions).len();
                (render_bulk_actions(eligible_count))
            }
        }
        div.queue-list {
            @for pair in pairs {
                @let pair_id = format!("{}-{}", pair.txn_id_a, pair.txn_id_b);
                @let is_selected = selected == Some((pair.txn_id_a, pair.txn_id_b));
                @let decision = derive_decision(pair, decisions);
                div.queue-item
                    .(if is_selected { "selected" } else { "" })
                    .(confidence_class(&pair.confidence))
                    .(decision.map(|d| d.css_class()).unwrap_or(""))
                    hx-get=(format!("/transfers/pair/{pair_id}"))
                    hx-target="#detail"
                    hx-swap="innerHTML"
                    data-pair-id=(pair_id)
                    data-detail-url=(format!("/transfers/pair/{pair_id}"))
                    data-detail-target="#detail"
                {
                    @if let Some(Decision::Skip) = decision {
                        span.status-indicator.skip-indicator
                            hx-post=(format!("/transfers/pair/{pair_id}/unskip"))
                            hx-target="body"
                            title="Click to unskip"
                            onclick="event.stopPropagation()"
                        { "\u{2298}" }
                    } @else if let Some(Decision::Confirm) = decision {
                        span.status-indicator.confirm-indicator
                            hx-post=(format!("/transfers/pair/{pair_id}/undo"))
                            hx-target="body"
                            title="Click to undo"
                            onclick="event.stopPropagation()"
                        { "\u{2713}" }
                    } @else if let Some(Decision::Reject) = decision {
                        span.status-indicator.reject-indicator
                            hx-post=(format!("/transfers/pair/{pair_id}/undo"))
                            hx-target="body"
                            title="Click to undo"
                            onclick="event.stopPropagation()"
                        { "\u{2717}" }
                    } @else {
                        span.conf-badge { (pair.confidence.as_str().chars().next().unwrap_or('?').to_uppercase().to_string()) }
                    }
                    span.amount { (format_dollars(pair.amount_cents)) }
                    span.date { (format_short_date(&pair.date_a)) }
                    span.gap { (transfers::date_diff_days(&pair.date_a, &pair.date_b)) "d" }
                }
            }
        }
    }
}

// Renders the detail panel for a single transfer pair: header with confidence/amount, side-by-side
// transaction cards, prior transfer history, and confirm/reject/skip action buttons.
// Called by: render_full_page, render_detail_fragment.
// Calls: confidence_class, confidence_reason, format_dollars, format_short_date, transfers::date_diff_days.
pub fn render_detail(pair: &TransferPairRow, prior: &[(String, i64, Status)]) -> Markup {
    let pair_id = format!("{}-{}", pair.txn_id_a, pair.txn_id_b);
    let days = transfers::date_diff_days(&pair.date_a, &pair.date_b);

    html! {
        div.detail-header {
            h2 {
                span.(confidence_class(&pair.confidence)) {
                    (pair.confidence.as_str().to_uppercase())
                }
                " \u{00b7} " (format_dollars(pair.amount_cents))
                @if pair.status != Status::Pending {
                    span.status-badge.((match pair.status {
                        Status::Confirmed => "status-confirmed",
                        Status::Rejected => "status-rejected",
                        _ => "",
                    })) {
                        @match pair.status {
                            Status::Confirmed => { " \u{2713}" },
                            Status::Rejected => { " \u{2717}" },
                            _ => {},
                        }
                    }
                }
            }
            div.confidence-reason {
                (confidence_reason(pair))
            }
        }
        div.comparison {
            div.comparison-meta {
                div.meta-item {
                    span.meta-label { "DATE DIFF" }
                    span.meta-value {
                        (days) "d"
                        @if days >= 2 { " \u{26a0}\u{fe0f}" }
                    }
                }
                div.meta-item {
                    span.meta-label { "Amount" }
                    span.meta-value { "\u{2705}" }
                }
            }
            div.txn-cards {
                div.txn-card {
                    div.txn-card-header {
                        span.card-label { "A" }
                        span.card-account { (&pair.account_name_a) }
                    }
                    div.txn-card-body {
                        div.field { span.field-label { "Date" } span.field-value { (format_short_date(&pair.date_a)) } }
                        div.field { span.field-label { "Payee" } span.field-value { (&pair.payee_a) } }
                        div.field { span.field-label { "Amount" } span.field-value.amount-positive { "+" (format_dollars(pair.amount_cents)) } }
                    }
                }
                div.txn-card {
                    div.txn-card-header {
                        span.card-label { "B" }
                        span.card-account { (&pair.account_name_b) }
                    }
                    div.txn-card-body {
                        div.field { span.field-label { "Date" } span.field-value { (format_short_date(&pair.date_b)) } }
                        div.field { span.field-label { "Payee" } span.field-value { (&pair.payee_b) } }
                        div.field { span.field-label { "Amount" } span.field-value.amount-negative { "-" (format_dollars(pair.amount_cents)) } }
                    }
                }
            }
        }
        @if !prior.is_empty() {
            div.prior-section {
                h3 { "Prior: " (&pair.account_name_a) " \u{2194} " (&pair.account_name_b) }
                div.prior-list {
                    @for (date, amount, status) in prior {
                        div.prior-row {
                            span { (format_short_date(date)) }
                            span { (format_dollars(*amount)) }
                            span.((if *status == Status::Confirmed { "status-confirmed" } else { "status-rejected" })) {
                                @if *status == Status::Confirmed { "\u{2713}" } @else { "\u{2717}" }
                            }
                        }
                    }
                }
            }
        }
        div.actions data-pair-id=(pair_id) data-action-base=(format!("/transfers/pair/{pair_id}")) {
            button.btn.btn-confirm
                hx-post=(format!("/transfers/pair/{pair_id}/confirm"))
                hx-target="body"
            { "[Y] Confirm" }
            button.btn.btn-reject
                hx-post=(format!("/transfers/pair/{pair_id}/reject"))
                hx-target="body"
            { "[N] Reject" }
            button.btn.btn-skip
                hx-post=(format!("/transfers/pair/{pair_id}/skip"))
                hx-target="body"
            { "[S] Skip" }
        }
    }
}

// Renders the activity bar at the bottom: summary stats (confirmed/rejected/skipped/undone counts)
// plus a scrollable list of the 20 most recent actions with undo/unskip buttons.
// Called by: render_full_page.
// Calls: count_decisions.
pub fn render_activity(state: &AppState) -> Markup {
    html! {
        div.activity-header {
            span.stat { "Confirmed " span.count-confirmed { (count_decisions(&state.decisions, Decision::Confirm)) } }
            span.stat { "Rejected " span.count-rejected { (count_decisions(&state.decisions, Decision::Reject)) } }
            span.stat { "Skipped " span.count-skipped { (count_decisions(&state.decisions, Decision::Skip)) } }
            span.stat { "Undone " span.count-undone { (state.undone) } }
            span.stat { "Applied " span.count-applied { (state.applied) } }
            @let confirmed_in_db = count_confirmed_in_db(&state.conn);
            button.apply-btn
                hx-post="/transfers/apply"
                hx-target="body"
                disabled[confirmed_in_db == 0]
                title=(if confirmed_in_db == 0 { "No confirmed pairs to apply" } else { "Tag both transactions of every confirmed pair as _Transfer and remove the pair row" })
            { "Apply all changes (" (confirmed_in_db) ")" }
        }
        div.activity-list {
            @for entry in state.activity.iter().rev().take(20) {
                @let pair_id = format!("{}-{}", entry.pair_id.0, entry.pair_id.1);
                div.activity-row {
                    span.((match entry.decision {
                        Decision::Confirm => "status-confirmed",
                        Decision::Reject => "status-rejected",
                        Decision::Skip => "status-skipped",
                    })) {
                        @match entry.decision {
                            Decision::Confirm => { "\u{2713} confirmed" },
                            Decision::Reject => { "\u{2717} rejected" },
                            Decision::Skip => { "\u{2298} skipped" },
                        }
                    }
                    span { "#" (entry.pair_id.0) }
                    span { (format_dollars(entry.amount_cents)) }
                    span { (&entry.account_a) " \u{2192} " (&entry.account_b) }
                    @if entry.decision == Decision::Skip {
                        button.undo-btn
                            hx-post=(format!("/transfers/pair/{pair_id}/unskip"))
                            hx-target="body"
                        { "unskip" }
                    } @else {
                        button.undo-btn
                            hx-post=(format!("/transfers/pair/{pair_id}/undo"))
                            hx-target="body"
                        { "undo" }
                    }
                }
            }
        }
    }
}

// Re-fetches pairs with current filters and renders a full page replacement.
// Called by: handle_action, handle_undo, handle_clear_all_skipped, handle_unskip (after every mutation).
// Calls: get_filtered_pairs, render_full_page.
pub fn render_current_page(state: &AppState) -> Markup {
    let pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
    render_full_page(state, &pairs, &state.status_filter, &state.confidence_filter)
}

// Renders the initial full page on GET /. Locks state, selects first pair if none active.
// Called by: main::handle_request (GET /).
// Calls: get_filtered_pairs, render_full_page.
pub fn render_page_shell(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut state = state.lock().unwrap();
    let pairs = get_filtered_pairs(&state.conn, &state.status_filter, &state.confidence_filter, &state.decisions);
    if state.active_pair.is_none() {
        state.active_pair = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
    }
    render_full_page(&state, &pairs, &state.status_filter, &state.confidence_filter)
}

// Returns an HTMX fragment with the detail panel for a single pair (no full page).
// Sets active_pair so subsequent full-page renders highlight this pair.
// Called by: main::handle_request (GET /pair/{id}), also triggered by JS arrow-key navigation.
// Calls: transfer_pairs::get_pair_by_id, get_prior_pairs, render_detail.
pub fn render_detail_fragment(state: &Arc<Mutex<AppState>>, txn_a: i64, txn_b: i64) -> Markup {
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

// Returns an HTMX fragment with just the queue sidebar after a filter change.
// Persists the new filter values in state and resets active_pair if it's no longer visible.
// Called by: main::handle_request (GET /queue?filter=X&conf=Y).
// Calls: get_filtered_pairs, find_pair_index, render_queue.
pub fn render_queue_fragment(state: &Arc<Mutex<AppState>>, status_filter: &str, confidence_filter: &str) -> Markup {
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
    render_queue(&pairs, selected, status_filter, confidence_filter, &state.decisions)
}

// Returns the inline confirmation prompt for a bulk action. The visible-count
// in the prompt reflects exactly what /bulk-{action} would touch (minus
// session-skipped pairs); the user sees the same number twice (filter row +
// prompt) so the action is unambiguous.
// Called by: main::handle_request (GET /bulk-prompt?action=X).
pub fn render_bulk_prompt_fragment(state: &Arc<Mutex<AppState>>, action: &str) -> Markup {
    let state = state.lock().unwrap();
    let pairs = get_filtered_pairs(
        &state.conn,
        &state.status_filter,
        &state.confidence_filter,
        &state.decisions,
    );
    let eligible = crate::helpers::pairs_eligible_for_bulk(&pairs, &state.decisions);
    render_bulk_prompt(action, eligible.len())
}

// Returns the default "Confirm all / Reject all" buttons. Used by the Cancel
// button in the inline prompt to swap back to the idle state.
// Called by: main::handle_request (GET /bulk-buttons).
pub fn render_bulk_buttons_fragment(state: &Arc<Mutex<AppState>>) -> Markup {
    let state = state.lock().unwrap();
    let pairs = get_filtered_pairs(
        &state.conn,
        &state.status_filter,
        &state.confidence_filter,
        &state.decisions,
    );
    let eligible = crate::helpers::pairs_eligible_for_bulk(&pairs, &state.decisions);
    render_bulk_actions(eligible.len())
}
