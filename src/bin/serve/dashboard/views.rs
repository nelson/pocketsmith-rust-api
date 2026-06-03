//! Server-rendered HTML for the Dashboard tab.
//!
//! MVP scope: months queue on the left, selected-month detail on the
//! right (header + Sankey + breakdown table). No client JS chart
//! library; the Sankey is a small handwritten SVG so it ships with
//! the page and prints / inspects cleanly.

use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use crate::helpers::format_dollars_compact;
use crate::state::AppState;

use super::breakdown::render_breakdown_table;
use super::helpers::{
    hyg_class, month_category_breakdown, monthly_summary, pick_active_month, pretty_month, MonthRow,
};
use super::sankey::render_sankey;

/// Render the full `/dashboard/` page. The selected month is read from
/// `state.dash_active_month`; if unset (initial render) we fall back
/// to the most recent month with data so the page is never blank.
pub fn render_page_shell(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut st = state.lock().unwrap();
    let months = monthly_summary(&st.conn).unwrap_or_default();
    let active = pick_active_month(&st.dash_active_month, &months);
    // Persist the chosen month so subsequent re-renders without an
    // explicit URL still highlight the same row.
    st.dash_active_month = active.clone();

    let queue = render_months_queue(&months, active.as_deref());
    let detail = match active.as_deref() {
        Some(ym) => render_month_detail(&st.conn, ym),
        None => html! { div.empty-state { p { "No transactions yet \u{2014} run `cargo run --bin sync`." } } },
    };
    let activity = render_activity_placeholder(active.as_deref());
    let chips = crate::freshness::header_chips(&st.conn);
    crate::render::render_page_with_chips(
        "dashboard",
        "Dashboard",
        chips,
        queue,
        detail,
        activity,
    )
}

/// Detail-only fragment served by `GET /dashboard/month/<YYYY-MM>`.
/// Updates `state.dash_active_month` so a subsequent body-target swap
/// keeps the row highlighted.
pub fn render_month_detail_fragment(state: &Arc<Mutex<AppState>>, ym: &str) -> Markup {
    let mut st = state.lock().unwrap();
    st.dash_active_month = Some(ym.to_string());
    render_month_detail(&st.conn, ym)
}

fn render_months_queue(months: &[MonthRow], active: Option<&str>) -> Markup {
    html! {
        div.queue-header {
            h2 { (months.len()) " months" }
            div.dash-queue-help {
                "in / out / net \u{00b7} two hygiene dots (categorised \u{00b7} normalised)"
            }
        }
        div.queue-list {
            @for m in months {
                (render_month_row(m, active == Some(m.month.as_str())))
            }
        }
    }
}

fn render_month_row(m: &MonthRow, is_selected: bool) -> Markup {
    let detail_url = format!("/dashboard/month/{}", m.month);
    let net_cls = if m.net >= 0.0 { "net-pos" } else { "net-neg" };
    let net_str = if m.net >= 0.0 {
        format!("+{}", format_dollars_compact((m.net * 100.0) as i64))
    } else {
        format!("-{}", format_dollars_compact((m.net.abs() * 100.0) as i64))
    };
    html! {
        div.queue-item.month-row.(if is_selected { "selected" } else { "" })
            hx-get=(detail_url)
            hx-target="#detail"
            hx-swap="innerHTML"
            data-detail-url=(detail_url)
            data-detail-target="#detail"
        {
            span.month-label { (m.month) }
            span.month-figs {
                "in " span.amount-positive { "+" (format_dollars_compact((m.total_in * 100.0) as i64)) }
                " \u{00b7} "
                "out " span.amount-negative { "-" (format_dollars_compact((m.total_out * 100.0) as i64)) }
                " \u{00b7} "
                span.(net_cls) { (net_str) }
            }
            span.hyg-dots title="left: % categorised \u{00b7} right: % normalised" {
                span.hyg-dot.(hyg_class(m.frac_categorised)) {}
                span.hyg-dot.(hyg_class(m.frac_normalised)) {}
            }
        }
    }
}

fn render_month_detail(conn: &rusqlite::Connection, ym: &str) -> Markup {
    let rows = month_category_breakdown(conn, ym).unwrap_or_default();
    let total_in: f64 = rows.iter().filter(|r| r.signed_total > 0.0).map(|r| r.signed_total).sum();
    let total_out: f64 = rows.iter().filter(|r| r.signed_total < 0.0).map(|r| -r.signed_total).sum();
    let net = total_in - total_out;
    let net_cls = if net >= 0.0 { "amount-positive" } else { "amount-negative" };
    let net_str = if net >= 0.0 {
        format!("+{}", format_dollars_compact((net * 100.0) as i64))
    } else {
        format!("-{}", format_dollars_compact((net.abs() * 100.0) as i64))
    };
    let total_txns: i64 = rows.iter().map(|r| r.txn_count).sum();
    let pretty = pretty_month(ym);
    html! {
        div.detail-header {
            div.row {
                h2 { (pretty) }
                span.amount-big.(net_cls) { "net " (net_str) }
            }
            div.meta {
                span { (total_txns) " transactions, transfers excluded" }
                span.chip { "in " (format_dollars_compact((total_in * 100.0) as i64)) }
                span.chip { "out " (format_dollars_compact((total_out * 100.0) as i64)) }
            }
        }
        div.dash-month-grid {
            div.dash-sankey-wrap {
                h3 { "Where the money went" }
                div.sub { "Width is dollars. Sources at left, categories at right." }
                (render_sankey(&rows, total_in, total_out))
            }
            div.dash-breakdown-wrap {
                h3 { "Category breakdown" }
                div.sub { "Sorted by absolute value. Inflow above, outflow below." }
                (render_breakdown_table(&rows, total_in, total_out))
            }
        }
    }
}

/// Render the months strip's activity-panel content. MVP: just a
/// reminder of the keyboard shortcuts; the data-quality scorecard
/// from the original plan is deferred until the Review tab is built.
fn render_activity_placeholder(active: Option<&str>) -> Markup {
    html! {
        div.activity-header {
            @if let Some(ym) = active {
                span.stat { "Viewing " strong { (pretty_month(ym)) } }
            }
            span.stat { "Press " span.kbd-inline { "\u{2191}" } " / " span.kbd-inline { "\u{2193}" } " or " span.kbd-inline { "[" } " / " span.kbd-inline { "]" } " to step months." }
            span.stat { "Press " span.kbd-inline { "?" } " for the full shortcuts list." }
        }
    }
}
