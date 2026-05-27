//! Server-rendered HTML for the Dashboard tab.
//!
//! MVP scope: months queue on the left, selected-month detail on the
//! right (header + Sankey + breakdown table). No client JS chart
//! library; the Sankey is a small handwritten SVG so it ships with
//! the page and prints / inspects cleanly.

use std::sync::{Arc, Mutex};

use maud::{html, Markup, PreEscaped};

use crate::helpers::format_dollars_compact;
use crate::state::AppState;

use super::helpers::{
    month_category_breakdown, monthly_summary, CategoryBreakdownRow, MonthRow,
};

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
    let sync = crate::render::last_sync_info(&st.conn);
    crate::render::render_page_with_sync(
        "dashboard",
        "Dashboard",
        sync.as_ref().map(|(s, a)| (s.as_str(), *a)),
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

/// Choose the month to show: explicit user selection if it's still
/// present in the data, else the most recent month with data, else
/// `None` (empty DB).
fn pick_active_month(stash: &Option<String>, months: &[MonthRow]) -> Option<String> {
    if let Some(s) = stash {
        if months.iter().any(|m| &m.month == s) {
            return Some(s.clone());
        }
    }
    months.first().map(|m| m.month.clone())
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

fn hyg_class(frac: f64) -> &'static str {
    if frac >= 0.9 {
        "hyg-on"
    } else if frac >= 0.5 {
        "hyg-warn"
    } else {
        "hyg-bad"
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

fn pretty_month(ym: &str) -> String {
    // Input is YYYY-MM; output is "Month YYYY" (e.g. "April 2026").
    let mut parts = ym.split('-');
    let (Some(y), Some(m)) = (parts.next(), parts.next()) else { return ym.to_string() };
    let name = match m {
        "01" => "January", "02" => "February", "03" => "March", "04" => "April",
        "05" => "May", "06" => "June", "07" => "July", "08" => "August",
        "09" => "September", "10" => "October", "11" => "November", "12" => "December",
        _ => return ym.to_string(),
    };
    format!("{name} {y}")
}

/// Render the SVG Sankey. Top-N income categories on the left funnel
/// into a single "Inflow" node in the middle, which then fans out to
/// the top-N expense categories on the right. If outflow > inflow, a
/// "Deficit" sink is drawn under the expense column.
///
/// We render a simple ribbon-style sankey directly as SVG: bezier
/// flows whose vertical heights are proportional to dollars. Nothing
/// fancy: the goal is a single page-render that's readable and prints,
/// not a fully interactive d3-sankey.
fn render_sankey(rows: &[CategoryBreakdownRow], total_in: f64, total_out: f64) -> Markup {
    const W: f64 = 720.0;
    const H: f64 = 360.0;
    const NODE_W: f64 = 14.0;
    const TOP_N: usize = 6;
    let pad = 12.0;

    let inflow: Vec<&CategoryBreakdownRow> = top_n(rows.iter().filter(|r| r.signed_total > 0.0), TOP_N);
    let outflow: Vec<&CategoryBreakdownRow> = top_n(rows.iter().filter(|r| r.signed_total < 0.0), TOP_N);

    // Edge case: nothing to render.
    if inflow.is_empty() && outflow.is_empty() {
        return html! { div.empty-state-row { "No category data for this month." } };
    }

    let max_side = total_in.max(total_out).max(1.0);
    let usable_h = H - 2.0 * pad;
    let scale = usable_h / max_side; // dollars -> pixel height

    // Layout left column (income sources).
    let left_x = 20.0;
    let mid_x = (W - 40.0) / 2.0;
    let right_x = W - 40.0;
    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg class=\"chart-svg dash-sankey\" viewBox=\"0 0 {W} {H}\" preserveAspectRatio=\"xMidYMid meet\">"
    ));

    // Middle inflow node spans the full inflow height.
    let mid_height = (total_in * scale).max(2.0);
    let mid_y = pad + (usable_h - mid_height) / 2.0;
    svg.push_str(&format!(
        "<rect x=\"{mid_x}\" y=\"{mid_y}\" width=\"{NODE_W}\" height=\"{mid_height}\" fill=\"var(--accent)\"/>\
         <text x=\"{label_x}\" y=\"{label_y}\" fill=\"var(--fg)\" font-size=\"11\" text-anchor=\"middle\">Inflow {label}</text>",
        label_x = mid_x + NODE_W / 2.0,
        label_y = mid_y - 4.0,
        label = compact_dollars(total_in),
    ));

    // Left column.
    let mut y_src = mid_y;
    let mut y_mid_left = mid_y;
    for (i, r) in inflow.iter().enumerate() {
        let h = (r.signed_total * scale).max(1.0);
        let opacity = 0.55 + 0.45 * (1.0 - (i as f64) / (TOP_N as f64).max(1.0));
        // Source node.
        svg.push_str(&format!(
            "<rect x=\"{left_x}\" y=\"{y_src}\" width=\"{NODE_W}\" height=\"{h}\" fill=\"var(--green)\" opacity=\"{opacity:.2}\"/>",
        ));
        // Label to the right of the node, inside the chart.
        svg.push_str(&format!(
            "<text x=\"{tx}\" y=\"{ty}\" fill=\"var(--fg)\" font-size=\"11\">{label}</text>",
            tx = left_x + NODE_W + 4.0,
            ty = y_src + h.min(12.0).max(10.0),
            label = svg_escape(&format!("{} {}", r.category_title, compact_dollars(r.signed_total))),
        ));
        // Ribbon to the middle.
        let src_top = y_src;
        let src_bot = y_src + h;
        let mid_top = y_mid_left;
        let mid_bot = y_mid_left + h;
        let cx = (left_x + NODE_W + mid_x) / 2.0;
        svg.push_str(&format!(
            "<path d=\"M{x1},{y1} C{cx},{y1} {cx},{y3} {x2},{y3} L{x2},{y4} C{cx},{y4} {cx},{y2} {x1},{y2} Z\" fill=\"var(--green)\" opacity=\"0.18\"/>",
            x1 = left_x + NODE_W, y1 = src_top, y2 = src_bot,
            x2 = mid_x, y3 = mid_top, y4 = mid_bot,
        ));
        y_src += h;
        y_mid_left += h;
    }

    // Right column. Includes a deficit row if outflow > inflow.
    let deficit = (total_out - total_in).max(0.0);
    let mid_height_right = ((total_out).max(total_in) * scale).max(2.0);
    let mid_y_right = pad + (usable_h - mid_height_right) / 2.0;
    let mut y_dst = mid_y_right;
    let mut y_mid_right = mid_y_right;
    for (i, r) in outflow.iter().enumerate() {
        let dollars = -r.signed_total;
        let h = (dollars * scale).max(1.0);
        let opacity = 0.55 + 0.45 * (1.0 - (i as f64) / (TOP_N as f64).max(1.0));
        svg.push_str(&format!(
            "<rect x=\"{right_x}\" y=\"{y_dst}\" width=\"{NODE_W}\" height=\"{h}\" fill=\"var(--red)\" opacity=\"{opacity:.2}\"/>",
        ));
        svg.push_str(&format!(
            "<text x=\"{tx}\" y=\"{ty}\" fill=\"var(--fg)\" font-size=\"11\" text-anchor=\"end\">{label}</text>",
            tx = right_x - 4.0,
            ty = y_dst + h.min(12.0).max(10.0),
            label = svg_escape(&format!("{} {}", r.category_title, compact_dollars(dollars))),
        ));
        let dst_top = y_dst;
        let dst_bot = y_dst + h;
        let mid_top = y_mid_right;
        let mid_bot = y_mid_right + h;
        let cx = (mid_x + NODE_W + right_x) / 2.0;
        svg.push_str(&format!(
            "<path d=\"M{x1},{y1} C{cx},{y1} {cx},{y3} {x2},{y3} L{x2},{y4} C{cx},{y4} {cx},{y2} {x1},{y2} Z\" fill=\"var(--red)\" opacity=\"0.18\"/>",
            x1 = mid_x + NODE_W, y1 = mid_top, y2 = mid_bot,
            x2 = right_x, y3 = dst_top, y4 = dst_bot,
        ));
        y_dst += h;
        y_mid_right += h;
    }
    if deficit > 0.0 {
        let h = (deficit * scale).max(1.0);
        svg.push_str(&format!(
            "<rect x=\"{right_x}\" y=\"{y_dst}\" width=\"{NODE_W}\" height=\"{h}\" fill=\"var(--yellow)\"/>",
        ));
        svg.push_str(&format!(
            "<text x=\"{tx}\" y=\"{ty}\" fill=\"var(--yellow)\" font-size=\"11\" text-anchor=\"end\">Deficit {label} \u{26a0}</text>",
            tx = right_x - 4.0,
            ty = y_dst + h.min(12.0).max(10.0),
            label = compact_dollars(deficit),
        ));
        let cx = (mid_x + NODE_W + right_x) / 2.0;
        let mid_top = y_mid_right;
        let mid_bot = y_mid_right + h;
        svg.push_str(&format!(
            "<path d=\"M{x1},{y1} C{cx},{y1} {cx},{y3} {x2},{y3} L{x2},{y4} C{cx},{y4} {cx},{y2} {x1},{y2} Z\" fill=\"var(--yellow)\" opacity=\"0.25\"/>",
            x1 = mid_x + NODE_W, y1 = mid_top, y2 = mid_bot,
            x2 = right_x, y3 = y_dst, y4 = y_dst + h,
        ));
    }

    svg.push_str("</svg>");
    html! { (PreEscaped(svg)) }
}

fn render_breakdown_table(rows: &[CategoryBreakdownRow], total_in: f64, total_out: f64) -> Markup {
    let inflow: Vec<&CategoryBreakdownRow> = rows.iter().filter(|r| r.signed_total > 0.0).collect();
    let outflow: Vec<&CategoryBreakdownRow> = rows.iter().filter(|r| r.signed_total < 0.0).collect();
    html! {
        table.dash-breakdown {
            thead {
                tr {
                    th.align-left { "Category" }
                    th.align-right { "Amount" }
                    th.align-right { "%" }
                    th.align-right { "Txns" }
                }
            }
            tbody {
                @if !inflow.is_empty() {
                    tr.dash-section-row { td colspan="4" { "Inflow" } }
                    @for r in &inflow {
                        (render_breakdown_row(r, total_in, true))
                    }
                }
                @if !outflow.is_empty() {
                    tr.dash-section-row { td colspan="4" { "Outflow" } }
                    @for r in &outflow {
                        (render_breakdown_row(r, total_out, false))
                    }
                }
            }
        }
    }
}

fn render_breakdown_row(r: &CategoryBreakdownRow, denom: f64, is_in: bool) -> Markup {
    let amount = r.signed_total.abs();
    let pct = if denom > 0.0 { amount / denom * 100.0 } else { 0.0 };
    let amount_cls = if is_in { "amount-positive" } else { "amount-negative" };
    let signed = if is_in {
        format!("+{}", format_dollars_compact((amount * 100.0) as i64))
    } else {
        format!("-{}", format_dollars_compact((amount * 100.0) as i64))
    };
    html! {
        tr {
            td { (r.category_title) }
            td.align-right.(amount_cls) { (signed) }
            td.align-right { (format!("{pct:.0}%")) }
            td.align-right { (r.txn_count) }
        }
    }
}

fn top_n<'a, I: Iterator<Item = &'a CategoryBreakdownRow>>(it: I, n: usize) -> Vec<&'a CategoryBreakdownRow> {
    let mut v: Vec<&CategoryBreakdownRow> = it.collect();
    v.sort_by(|a, b| b.signed_total.abs().partial_cmp(&a.signed_total.abs()).unwrap());
    v.truncate(n);
    v
}

/// Compact dollar string (`$1.2k`, `$340`, `$5.6M`) for SVG labels.
/// Operates on dollars (not cents) so the sankey calling convention
/// stays in the same unit as `amount_in_base_currency`.
fn compact_dollars(dollars: f64) -> String {
    let d = dollars.abs();
    if d >= 1_000_000.0 {
        format!("${:.1}M", d / 1_000_000.0)
    } else if d >= 1_000.0 {
        format!("${:.1}k", d / 1_000.0)
    } else {
        format!("${d:.0}")
    }
}

fn svg_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::helpers::CategoryBreakdownRow;

    fn cat(id: i64, title: &str, total: f64, n: i64) -> CategoryBreakdownRow {
        CategoryBreakdownRow {
            category_id: Some(id),
            category_title: title.to_string(),
            signed_total: total,
            txn_count: n,
        }
    }

    #[test]
    fn pretty_month_handles_known_months() {
        assert_eq!(pretty_month("2026-04"), "April 2026");
        assert_eq!(pretty_month("2025-12"), "December 2025");
        // Unknown shape passes through.
        assert_eq!(pretty_month("garbage"), "garbage");
    }

    #[test]
    fn compact_dollars_picks_a_sensible_suffix() {
        assert_eq!(compact_dollars(500.0), "$500");
        assert_eq!(compact_dollars(1234.0), "$1.2k");
        assert_eq!(compact_dollars(1_500_000.0), "$1.5M");
    }

    #[test]
    fn render_sankey_with_no_data_returns_empty_state() {
        let html = render_sankey(&[], 0.0, 0.0).into_string();
        assert!(html.contains("No category data"), "html:\n{html}");
    }

    #[test]
    fn render_sankey_includes_inflow_label_and_category_names() {
        let rows = vec![
            cat(1, "Salary",     5000.0, 1),
            cat(2, "Eating Out",  -120.0, 3),
            cat(3, "Groceries",   -240.0, 5),
        ];
        let html = render_sankey(&rows, 5000.0, 360.0).into_string();
        assert!(html.contains("Inflow $5.0k"), "expected inflow label in svg: {html}");
        assert!(html.contains("Salary"),     "salary label missing: {html}");
        assert!(html.contains("Eating Out"), "eating out label missing: {html}");
    }

    #[test]
    fn render_sankey_draws_deficit_when_outflow_exceeds_inflow() {
        let rows = vec![cat(1, "Mortgage", -10000.0, 1)];
        let html = render_sankey(&rows, 0.0, 10000.0).into_string();
        assert!(html.contains("Deficit"), "deficit ribbon missing: {html}");
    }

    #[test]
    fn render_breakdown_table_splits_inflow_and_outflow_sections() {
        let rows = vec![
            cat(1, "Salary",     5000.0, 1),
            cat(2, "Mortgage", -3000.0, 1),
        ];
        let html = render_breakdown_table(&rows, 5000.0, 3000.0).into_string();
        assert!(html.contains("Inflow"));
        assert!(html.contains("Outflow"));
        assert!(html.contains("Salary"));
        assert!(html.contains("Mortgage"));
        // Percentages: 5000 / 5000 = 100%, 3000 / 3000 = 100%.
        assert!(html.matches("100%").count() >= 2, "percentages missing: {html}");
    }

    #[test]
    fn pick_active_month_falls_back_to_most_recent() {
        let months = vec![
            MonthRow { month: "2026-04".into(), total_in: 0.0, total_out: 0.0, net: 0.0,
                txn_count: 0, frac_categorised: 1.0, frac_normalised: 1.0 },
            MonthRow { month: "2026-03".into(), total_in: 0.0, total_out: 0.0, net: 0.0,
                txn_count: 0, frac_categorised: 1.0, frac_normalised: 1.0 },
        ];
        assert_eq!(pick_active_month(&None, &months), Some("2026-04".to_string()));
        assert_eq!(pick_active_month(&Some("2026-03".into()), &months), Some("2026-03".into()));
        // Stale stash falls back to newest.
        assert_eq!(pick_active_month(&Some("1999-01".into()), &months), Some("2026-04".into()));
        // Empty data => None.
        assert_eq!(pick_active_month(&None, &[]), None);
    }
}
