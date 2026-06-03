//! Page layout for the `/normalise/*` tab. Mirrors the structure of the
//! transfer-side views (queue + detail + activity), but renders
//! `payee_normalisations` rows instead of `transfer_pairs`.

use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use pocketsmith_sync::db::payee_normalisations::PayeeNormalisationRow;
use pocketsmith_sync::normalise::{normalise as run_normalise, NormalisationResult, TraceEntry};
use pocketsmith_sync::review::Status;

use crate::helpers::{format_dollars, format_short_date};
use crate::render::render_actions;
use crate::state::{AppState, Decision};
use crate::tab::count_decisions;

use super::helpers::{
    get_filtered_normalisations, matching_transactions, MatchingTxn, NormClassFilter,
    NormStatusFilter,
};

/// Top-level page render: locks state, computes the filtered queue, picks
/// an active row if needed, and delegates to [`render_full_page`].
pub fn render_page_shell(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut st = state.lock().unwrap();
    let status = NormStatusFilter::parse(&st.norm_status_filter);
    let class = NormClassFilter::parse(&st.norm_class_filter);
    let rows = get_filtered_normalisations(&st.conn, status, class, &st.normalise.decisions);

    let active_slug = st
        .normalise.active
        .clone()
        .filter(|s| rows.iter().any(|r| &r.slug == s))
        .or_else(|| rows.first().map(|r| r.slug.clone()));
    st.normalise.active = active_slug.clone();

    render_full_page(&st, &rows, status, class, active_slug.as_deref())
}

pub fn render_queue_fragment(
    state: &Arc<Mutex<AppState>>,
    status_str: &str,
    class_str: &str,
) -> Markup {
    let mut st = state.lock().unwrap();
    st.norm_status_filter = status_str.to_string();
    st.norm_class_filter = class_str.to_string();
    let status = NormStatusFilter::parse(status_str);
    let class = NormClassFilter::parse(class_str);
    let rows = get_filtered_normalisations(&st.conn, status, class, &st.normalise.decisions);
    let active = st.normalise.active.clone();
    render_queue(&rows, active.as_deref(), status, class, &st.normalise.decisions)
}

pub fn render_detail_fragment(state: &Arc<Mutex<AppState>>, slug: &str) -> Markup {
    let mut st = state.lock().unwrap();
    st.normalise.active = Some(slug.to_string());
    match pocketsmith_sync::db::payee_normalisations::get_by_slug(&st.conn, slug) {
        Ok(Some(row)) => {
            let txns = matching_transactions(&st.conn, &row.original_payee);
            let is_skipped = st.normalise.decisions.get(&row.original_payee) == Some(&Decision::Skip);
            let pipeline = run_normalise(&row.original_payee);
            render_detail(&row, &txns, is_skipped, &pipeline)
        }
        _ => html! { div.empty-state { p { "Item not found." } } },
    }
}

fn render_full_page(
    state: &AppState,
    rows: &[PayeeNormalisationRow],
    status: NormStatusFilter,
    class: NormClassFilter,
    active_slug: Option<&str>,
) -> Markup {
    let active_row = active_slug.and_then(|s| rows.iter().find(|r| r.slug == s));
    let active_txns = active_row
        .map(|r| matching_transactions(&state.conn, &r.original_payee))
        .unwrap_or_default();

    let queue = render_queue(rows, active_slug, status, class, &state.normalise.decisions);
    let detail = match active_row {
        Some(row) => {
            let is_skipped = state.normalise.decisions.get(&row.original_payee) == Some(&Decision::Skip);
            let pipeline = run_normalise(&row.original_payee);
            render_detail(row, &active_txns, is_skipped, &pipeline)
        }
        None => html! { div.empty-state { p { "No proposals to show. Run `normalise` to populate the staging table." } } },
    };
    let activity = render_activity(state);

    let chips = crate::freshness::header_chips(&state.conn);
    crate::render::render_page_with_chips(
        "normalise",
        "Payee Normalisations",
        chips,
        queue,
        detail,
        activity,
    )
}

fn render_queue(
    rows: &[PayeeNormalisationRow],
    active_slug: Option<&str>,
    status: NormStatusFilter,
    class: NormClassFilter,
    decisions: &std::collections::HashMap<String, Decision>,
) -> Markup {
    html! {
        div.queue-header {
            h2 { (rows.len()) " proposals" }
            div.filter-row {
                @for f in &NormStatusFilter::ALL {
                    button.filter-btn
                        .(if *f == status { "active" } else { "" })
                        hx-get=(format!("/normalise/queue?filter={}&class={}", f.as_str(), class.as_str()))
                        hx-target="#queue"
                        hx-swap="innerHTML"
                    { (f.as_str().to_uppercase()) }
                }
                @let n_skipped = count_decisions(decisions, Decision::Skip);
                @if n_skipped > 0 && status == NormStatusFilter::Skipped {
                    button.filter-btn.clear-skipped-btn
                        hx-post="/normalise/clear-all-skipped"
                        hx-target="body"
                    { "CLEAR SKIPPED (" (n_skipped) ")" }
                }
            }
            div.filter-row {
                @for f in &NormClassFilter::ALL {
                    button.filter-btn
                        .(if *f == class { "active" } else { "" })
                        hx-get=(format!("/normalise/queue?filter={}&class={}", status.as_str(), f.as_str()))
                        hx-target="#queue"
                        hx-swap="innerHTML"
                    { (f.as_str().to_uppercase()) }
                }
            }
        }
        div.queue-list {
            @for row in rows {
                @let is_active = active_slug == Some(row.slug.as_str());
                @let session_decision = decisions.get(&row.original_payee).copied();
                @let row_status = effective_status(row.status, session_decision);
                div.queue-item
                    .(if is_active { "selected" } else { "" })
                    .((row_status_css(row_status)))
                    hx-get=(format!("/normalise/item/{}", row.slug))
                    hx-target="#detail"
                    hx-swap="innerHTML"
                    data-detail-url=(format!("/normalise/item/{}", row.slug))
                    data-detail-target="#detail"
                {
                    @if session_decision == Some(Decision::Skip) {
                        span.status-indicator.skip-indicator
                            hx-post=(format!("/normalise/item/{}/unskip", row.slug))
                            hx-target="body"
                            title="Click to unskip"
                            onclick="event.stopPropagation()"
                        { "\u{2298}" }
                    } @else if row_status == Some(Status::Confirmed) {
                        span.status-indicator.confirm-indicator
                            hx-post=(format!("/normalise/item/{}/undo", row.slug))
                            hx-target="body"
                            title="Click to undo"
                            onclick="event.stopPropagation()"
                        { "\u{2713}" }
                    } @else if row_status == Some(Status::Rejected) {
                        span.status-indicator.reject-indicator
                            hx-post=(format!("/normalise/item/{}/undo", row.slug))
                            hx-target="body"
                            title="Click to undo"
                            onclick="event.stopPropagation()"
                        { "\u{2717}" }
                    } @else {
                        span.conf-badge { (row.txn_count) }
                    }
                    span.payee { (row.proposed_payee) }
                    span.gap { (row.class.as_deref().unwrap_or("?")) }
                }
            }
        }
    }
}

fn effective_status(stored: Status, session: Option<Decision>) -> Option<Status> {
    match session {
        Some(Decision::Confirm) => Some(Status::Confirmed),
        Some(Decision::Reject) => Some(Status::Rejected),
        _ => Some(stored),
    }
}

fn row_status_css(s: Option<Status>) -> &'static str {
    match s {
        Some(Status::Confirmed) => "decided-confirmed",
        Some(Status::Rejected) => "decided-rejected",
        _ => "",
    }
}

fn render_detail(
    row: &PayeeNormalisationRow,
    txns: &[MatchingTxn],
    is_skipped: bool,
    pipeline: &NormalisationResult,
) -> Markup {
    let action_base = format!("/normalise/item/{}", row.slug);
    html! {
        div.detail-header {
            h2 {
                (row.proposed_payee)
                @if row.status != Status::Pending {
                    " " span.status-badge.((match row.status {
                        Status::Confirmed => "status-confirmed",
                        Status::Rejected => "status-rejected",
                        _ => "",
                    })) {
                        @match row.status {
                            Status::Confirmed => { "\u{2713}" },
                            Status::Rejected => { "\u{2717}" },
                            _ => {},
                        }
                    }
                }
                @if is_skipped {
                    " " span.status-badge { "(skipped this session)" }
                }
            }
            div.confidence-reason {
                "class: " (row.class.as_deref().unwrap_or("unclassified"))
                " \u{00b7} " (row.txn_count) " transaction(s) sharing this original_payee"
            }
        }

        div.comparison {
            div.txn-cards {
                div.txn-card {
                    div.txn-card-header { span.card-label { "ORIGINAL" } }
                    div.txn-card-body {
                        div.field { span.field-label { "Payee" } span.field-value { (row.original_payee) } }
                    }
                }
                div.txn-card {
                    div.txn-card-header { span.card-label { "PROPOSED" } }
                    div.txn-card-body {
                        div.field { span.field-label { "Payee" } span.field-value { (row.proposed_payee) } }
                    }
                }
            }
        }

        (render_pipeline_trace(pipeline))

        (render_actions(&action_base, is_skipped))

        @if !txns.is_empty() {
            div.prior-section {
                h3 { "Matching transactions (" (txns.len()) ")" }
                div.prior-list.norm-txn-list {
                    @for t in txns {
                        div.prior-row {
                            span { (format_short_date(&t.date)) }
                            span { (t.payee.as_deref().unwrap_or("\u{2014}")) }
                            span.((if t.amount_cents >= 0 { "amount-positive" } else { "amount-negative" })) {
                                (format_dollars(t.amount_cents))
                            }
                            span.norm-txn-acct { (t.account_name.as_deref().unwrap_or("?")) }
                        }
                    }
                }
            }
        }
    }
}

/// Render the per-stage transformation trace for the active row. One row
/// per pipeline stage that mutated `normalised` or attached a feature.
/// Lets the reviewer see *what each rule did*, intuitively.
fn render_pipeline_trace(p: &NormalisationResult) -> Markup {
    if p.trace.is_empty() {
        return html! {
            div.norm-trace {
                h3 { "Pipeline trace" }
                div.norm-trace-empty { "(no rules matched \u{2014} normalised string equals the original)" }
            }
        };
    }
    html! {
        div.norm-trace {
            h3 { "Pipeline trace" }
            div.norm-trace-list {
                @for entry in &p.trace {
                    (render_trace_entry(entry))
                }
            }
        }
    }
}

fn render_trace_entry(entry: &TraceEntry) -> Markup {
    let changed_string = entry.before != entry.after;
    let values: std::collections::HashMap<&str, &str> = entry
        .feature_values
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();
    html! {
        div.norm-trace-row {
            span.norm-trace-stage { (entry.stage) }
            div.norm-trace-body {
                @if changed_string {
                    div.norm-trace-diff {
                        span.norm-trace-before { (entry.before) }
                        span.norm-trace-arrow { " \u{2192} " }
                        span.norm-trace-after { (entry.after) }
                    }
                }
                @if !entry.features_added.is_empty() || entry.class_set.is_some() {
                    div.norm-trace-extracted {
                        @if let Some(c) = &entry.class_set {
                            span.norm-trace-class { "class = " (format!("{:?}", c).to_lowercase()) }
                        }
                        @for feat in &entry.features_added {
                            @if let Some(v) = values.get(feat) {
                                span.norm-trace-feat {
                                    "+" (feat) " "
                                    span.norm-trace-feat-val { "(" (v) ")" }
                                }
                            } @else {
                                span.norm-trace-feat { "+" (feat) }
                            }
                        }
                    }
                }
                @if let Some(pat) = entry.matched_pattern {
                    div.norm-trace-pattern {
                        span.norm-trace-pattern-label { "matched" }
                        " "
                        code.norm-trace-pattern-src { (pat) }
                    }
                }
            }
        }
    }
}

fn render_activity(state: &AppState) -> Markup {
    let confirmed_in_db = count_confirmed_in_db(&state.conn);
    html! {
        div.activity-header {
            span.stat { "Confirmed " span.count-confirmed { (count_decisions(&state.normalise.decisions, Decision::Confirm)) } }
            span.stat { "Rejected " span.count-rejected { (count_decisions(&state.normalise.decisions, Decision::Reject)) } }
            span.stat { "Skipped " span.count-skipped { (count_decisions(&state.normalise.decisions, Decision::Skip)) } }
            span.stat { "Undone " span.count-undone { (state.normalise.undone) } }
            span.stat { "Applied " span.count-applied { (state.normalise.applied) } }
            button.apply-btn
                hx-post="/normalise/apply"
                hx-target="body"
                disabled[confirmed_in_db == 0]
                title=(if confirmed_in_db == 0 { "No confirmed proposals to apply" } else { "Write transactions.payee for every confirmed proposal and drain the staging row" })
            { "Apply confirmed (" (confirmed_in_db) ")" }
        }
        div.activity-list {
            @for entry in state.normalise.activity.iter().rev().take(20) {
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
                    span { (entry.proposed_payee) }
                    span { (entry.txn_count) " txn" }
                    @if entry.decision == Decision::Skip {
                        button.undo-btn
                            hx-post=(format!("/normalise/item/{}/unskip", entry.slug))
                            hx-target="body"
                        { "unskip" }
                    } @else {
                        button.undo-btn
                            hx-post=(format!("/normalise/item/{}/undo", entry.slug))
                            hx-target="body"
                        { "undo" }
                    }
                }
            }
        }
    }
}

/// Count confirmed proposals still in the DB (i.e. pending an apply).
fn count_confirmed_in_db(conn: &rusqlite::Connection) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM payee_normalisations WHERE status = 1",
        [],
        |r| r.get(0),
    )
    .unwrap_or(0)
}
