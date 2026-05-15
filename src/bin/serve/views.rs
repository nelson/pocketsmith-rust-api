use std::collections::HashMap;

use maud::{html, Markup, PreEscaped, DOCTYPE};

use pocketsmith_sync::db::transfer_pairs::TransferPairRow;
use pocketsmith_sync::transfers::{self, Status};

use crate::css::CSS;
use crate::helpers::{
    confidence_class, confidence_reason, find_pair_index, format_dollars, format_short_date,
    get_prior_pairs,
};
use crate::js::JS;
use crate::state::{AppState, Decision};

pub fn full_page(state: &AppState, pairs: &[TransferPairRow], status_filter: &str, confidence_filter: &str) -> Markup {
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

pub fn render_queue(pairs: &[TransferPairRow], selected: Option<(i64, i64)>, status_filter: &str, confidence_filter: &str, decisions: &HashMap<(i64, i64), Decision>) -> Markup {
    html! {
        div.queue-header {
            h2 { (pairs.len()) " pairs" }
            div.filter-row {
                @for f in &["all", "pending", "confirmed", "rejected", "skipped"] {
                    button.filter-btn
                        .(if *f == status_filter { "active" } else { "" })
                        hx-get=(format!("/queue?filter={f}&conf={confidence_filter}"))
                        hx-target="#queue"
                        hx-swap="innerHTML"
                    { (f.to_uppercase()) }
                }
            }
            div.filter-row {
                @for f in &["all", "high", "medium", "low"] {
                    button.filter-btn.conf-filter
                        .(if *f == confidence_filter { "active" } else { "" })
                        hx-get=(format!("/queue?filter={status_filter}&conf={f}"))
                        hx-target="#queue"
                        hx-swap="innerHTML"
                    { (f.to_uppercase()) }
                }
                @let num_skipped = decisions.values().filter(|v| **v == Decision::Skip).count();
                @if num_skipped > 0 && status_filter == "skipped" {
                    button.filter-btn.clear-skipped-btn
                        hx-post="/clear-all-skipped"
                        hx-target="body"
                    { "CLEAR SKIPPED (" (num_skipped) ")" }
                }
            }
        }
        div.queue-list {
            @for pair in pairs {
                @let pair_id = format!("{}-{}", pair.txn_id_a, pair.txn_id_b);
                @let is_selected = selected == Some((pair.txn_id_a, pair.txn_id_b));
                @let decision = decisions.get(&(pair.txn_id_a, pair.txn_id_b)).copied();
                div.queue-item
                    .(if is_selected { "selected" } else { "" })
                    .(confidence_class(&pair.confidence))
                    .(decision.map(|d| d.css_class()).unwrap_or(""))
                    hx-get=(format!("/pair/{pair_id}"))
                    hx-target="#detail"
                    hx-swap="innerHTML"
                    data-pair-id=(pair_id)
                {
                    @if let Some(Decision::Skip) = decision {
                        span.status-indicator.skip-indicator
                            hx-post=(format!("/pair/{pair_id}/unskip"))
                            hx-target="body"
                            title="Click to unskip"
                            onclick="event.stopPropagation()"
                        { "\u{2298}" }
                    } @else if let Some(Decision::Confirm) = decision {
                        span.status-indicator.confirm-indicator
                            hx-post=(format!("/pair/{pair_id}/undo"))
                            hx-target="body"
                            title="Click to undo"
                            onclick="event.stopPropagation()"
                        { "\u{2713}" }
                    } @else if let Some(Decision::Reject) = decision {
                        span.status-indicator.reject-indicator
                            hx-post=(format!("/pair/{pair_id}/undo"))
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
        div.actions data-pair-id=(pair_id) {
            button.btn.btn-confirm
                hx-post=(format!("/pair/{pair_id}/confirm"))
                hx-target="body"
            { "[Y] Confirm" }
            button.btn.btn-reject
                hx-post=(format!("/pair/{pair_id}/reject"))
                hx-target="body"
            { "[N] Reject" }
            button.btn.btn-skip
                hx-post=(format!("/pair/{pair_id}/skip"))
                hx-target="body"
            { "[S] Skip" }
        }
    }
}

pub fn decision_count(state: &AppState, d: Decision) -> usize {
    state.decisions.values().filter(|v| **v == d).count()
}

pub fn render_activity(state: &AppState) -> Markup {
    html! {
        div.activity-header {
            span.stat { "Confirmed " span.count-confirmed { (decision_count(state, Decision::Confirm)) } }
            span.stat { "Rejected " span.count-rejected { (decision_count(state, Decision::Reject)) } }
            span.stat { "Skipped " span.count-skipped { (decision_count(state, Decision::Skip)) } }
            span.stat { "Undone " span.count-undone { (state.undone) } }
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
                            hx-post=(format!("/pair/{pair_id}/unskip"))
                            hx-target="body"
                        { "unskip" }
                    } @else {
                        button.undo-btn
                            hx-post=(format!("/pair/{pair_id}/undo"))
                            hx-target="body"
                        { "undo" }
                    }
                }
            }
        }
    }
}
