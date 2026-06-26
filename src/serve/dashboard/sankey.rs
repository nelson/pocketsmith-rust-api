//! Server-rendered SVG Sankey for the Dashboard month detail.
//!
//! Top-N income categories on the left funnel into a single "Inflow"
//! node in the middle, which then fans out to the top-N expense
//! categories on the right. If outflow > inflow, a "Deficit" sink is
//! drawn under the expense column.
//!
//! We render a simple ribbon-style sankey directly as SVG: bezier
//! flows whose vertical heights are proportional to dollars. Nothing
//! fancy: the goal is a single page-render that's readable and prints,
//! not a fully interactive d3-sankey.

use maud::{html, Markup, PreEscaped};

use super::helpers::CategoryBreakdownRow;

/// Render the SVG Sankey for a single month's category breakdown.
pub fn render_sankey(rows: &[CategoryBreakdownRow], total_in: f64, total_out: f64) -> Markup {
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

    fn cat(id: i64, title: &str, total: f64, n: i64) -> CategoryBreakdownRow {
        CategoryBreakdownRow {
            category_id: Some(id),
            category_title: title.to_string(),
            signed_total: total,
            txn_count: n,
        }
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
}
