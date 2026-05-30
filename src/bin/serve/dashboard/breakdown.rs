//! Server-rendered category breakdown table for the Dashboard month
//! detail. Inflow rows are listed above outflow rows, each sorted by
//! absolute value (the query already returns them in that order).

use maud::{html, Markup};

use crate::helpers::format_dollars_compact;

use super::helpers::CategoryBreakdownRow;

/// Render the per-month category breakdown table, split into inflow
/// and outflow sections.
pub fn render_breakdown_table(rows: &[CategoryBreakdownRow], total_in: f64, total_out: f64) -> Markup {
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
}
