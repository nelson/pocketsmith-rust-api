//! Page layout for the `/normalise/*` tab. Mirrors the structure of the
//! transfer-side views (queue + detail + activity), but renders
//! `payee_normalisations` rows instead of `transfer_pairs`.

use std::sync::{Arc, Mutex};

use maud::{html, Markup, PreEscaped, DOCTYPE};

use pocketsmith_sync::db::payee_normalisations::PayeeNormalisationRow;
use pocketsmith_sync::transfers::Status;

use crate::css::CSS;
use crate::js::JS;
use crate::state::AppState;
use crate::views::render_tab_bar;

use super::helpers::{get_filtered_normalisations, NormClassFilter, NormStatusFilter};

/// Top-level page render: locks state, computes the filtered queue, picks
/// an active row if needed, and delegates to [`render_full_page`].
pub fn render_page_shell(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut st = state.lock().unwrap();
    let status = NormStatusFilter::parse(&st.norm_status_filter);
    let class = NormClassFilter::parse(&st.norm_class_filter);
    let rows = get_filtered_normalisations(&st.conn, status, class, &st.norm_skipped);

    // Auto-select the first visible row if the active slug isn't in the
    // current view.
    let active_slug = st
        .norm_active_slug
        .clone()
        .filter(|s| rows.iter().any(|r| &r.slug == s))
        .or_else(|| rows.first().map(|r| r.slug.clone()));
    st.norm_active_slug = active_slug.clone();

    render_full_page(&st, &rows, status, class, active_slug.as_deref())
}

/// Returns the queue fragment only (HTMX swap target `#norm-queue`).
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
    let rows = get_filtered_normalisations(&st.conn, status, class, &st.norm_skipped);
    let active = st.norm_active_slug.clone();
    render_queue(&rows, active.as_deref(), status, class)
}

/// Returns the detail fragment for a single row (HTMX swap target
/// `#norm-detail`).
pub fn render_detail_fragment(state: &Arc<Mutex<AppState>>, slug: &str) -> Markup {
    let mut st = state.lock().unwrap();
    st.norm_active_slug = Some(slug.to_string());
    match pocketsmith_sync::db::payee_normalisations::get_by_slug(&st.conn, slug) {
        Ok(Some(row)) => render_detail(&row, st.norm_skipped.contains_key(&row.original_payee)),
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

    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Payee Normalisations" }
                script src="https://unpkg.com/htmx.org@2.0.4" {}
                style { (PreEscaped(CSS)) }
            }
            body {
                (render_tab_bar("normalise"))
                div.layout {
                    div.queue-panel #norm-queue {
                        (render_queue(rows, active_slug, status, class))
                    }
                    div.detail-panel #norm-detail {
                        @if let Some(row) = active_row {
                            (render_detail(row, state.norm_skipped.contains_key(&row.original_payee)))
                        } @else {
                            div.empty-state { p { "No proposals to show. Run `normalise` to populate the staging table." } }
                        }
                    }
                }
                div.activity-panel #norm-activity {
                    (render_activity(state))
                }
                script { (PreEscaped(JS)) }
            }
        }
    }
}

fn render_queue(
    rows: &[PayeeNormalisationRow],
    active_slug: Option<&str>,
    status: NormStatusFilter,
    class: NormClassFilter,
) -> Markup {
    html! {
        div.queue-header {
            h2 { (rows.len()) " proposals" }
            div.filter-row {
                @for f in &NormStatusFilter::ALL {
                    button.filter-btn
                        .(if *f == status { "active" } else { "" })
                        hx-get=(format!("/normalise/queue?filter={}&class={}", f.as_str(), class.as_str()))
                        hx-target="#norm-queue"
                        hx-swap="innerHTML"
                    { (f.as_str().to_uppercase()) }
                }
            }
            div.filter-row {
                @for f in &NormClassFilter::ALL {
                    button.filter-btn
                        .(if *f == class { "active" } else { "" })
                        hx-get=(format!("/normalise/queue?filter={}&class={}", status.as_str(), f.as_str()))
                        hx-target="#norm-queue"
                        hx-swap="innerHTML"
                    { (f.as_str().to_uppercase()) }
                }
            }
        }
        div.queue-list {
            @for row in rows {
                @let is_active = active_slug == Some(row.slug.as_str());
                div.queue-item
                    .(if is_active { "active" } else { "" })
                    hx-get=(format!("/normalise/item/{}", row.slug))
                    hx-target="#norm-detail"
                    hx-swap="innerHTML"
                {
                    span.amount { (row.txn_count) "\u{00a0}txn" }
                    span.payee { (row.proposed_payee) }
                    span.gap { (row.class.as_deref().unwrap_or("?")) }
                    span.conf-badge { (status_label(row.status)) }
                }
            }
        }
    }
}

fn status_label(s: Status) -> &'static str {
    match s {
        Status::Pending => "P",
        Status::Confirmed => "C",
        Status::Rejected => "R",
    }
}

fn render_detail(row: &PayeeNormalisationRow, is_skipped: bool) -> Markup {
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
                " \u{00b7} " (row.txn_count) " transaction(s)"
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
        div.action-buttons {
            button.action-btn.confirm-btn
                hx-post=(format!("/normalise/item/{}/confirm", row.slug))
                hx-target="body"
            { "Confirm" }
            button.action-btn.reject-btn
                hx-post=(format!("/normalise/item/{}/reject", row.slug))
                hx-target="body"
            { "Reject" }
            @if is_skipped {
                button.action-btn
                    hx-post=(format!("/normalise/item/{}/unskip", row.slug))
                    hx-target="body"
                { "Unskip" }
            } @else {
                button.action-btn
                    hx-post=(format!("/normalise/item/{}/skip", row.slug))
                    hx-target="body"
                { "Skip" }
            }
        }
    }
}

fn render_activity(state: &AppState) -> Markup {
    html! {
        div.activity-header {
            span { "Applied this session: " (state.norm_applied) " transactions" }
            button.apply-btn
                hx-post="/normalise/apply"
                hx-target="body"
            { "Apply confirmed" }
        }
    }
}
