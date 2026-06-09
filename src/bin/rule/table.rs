//! Aligned table renderer. Column widths come from the *plain* text so
//! ANSI colour codes in the shown cell never throw off alignment
//! (rule-cli §14.1).

use crate::colours::Style;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Align {
    Left,
    Right,
}

/// One cell: `plain` drives width/alignment, `shown` is what's printed
/// (may carry ANSI). For uncoloured cells the two are equal.
pub(crate) struct Cell {
    plain: String,
    shown: String,
}

impl Cell {
    pub(crate) fn text(s: impl Into<String>) -> Cell {
        let s = s.into();
        Cell { plain: s.clone(), shown: s }
    }
    pub(crate) fn coloured(plain: impl Into<String>, shown: impl Into<String>) -> Cell {
        Cell { plain: plain.into(), shown: shown.into() }
    }
}

/// Render an aligned table: bold headers, a rule separator, then rows.
/// Each line is indented two spaces to match the rest of the CLI.
pub(crate) fn render_table(style: &Style, headers: &[(&str, Align)], rows: &[Vec<Cell>]) -> String {
    let ncol = headers.len();
    let mut widths = vec![0usize; ncol];
    for (i, (h, _)) in headers.iter().enumerate() {
        widths[i] = h.chars().count();
    }
    for row in rows {
        for (i, c) in row.iter().enumerate() {
            widths[i] = widths[i].max(c.plain.chars().count());
        }
    }
    let pad = |plain_len: usize, width: usize, align: Align, shown: &str| -> String {
        let gap = width.saturating_sub(plain_len);
        match align {
            Align::Left => format!("{shown}{}", " ".repeat(gap)),
            Align::Right => format!("{}{shown}", " ".repeat(gap)),
        }
    };
    let mut out = String::new();
    // Header row.
    let head: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(i, (h, a))| style.bold(&pad(h.chars().count(), widths[i], *a, h)))
        .collect();
    out.push_str(&format!("  {}\n", head.join("  ")));
    // Separator.
    let sep: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
    out.push_str(&format!("  {}\n", sep.join("  ")));
    // Body.
    for row in rows {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, c)| pad(c.plain.chars().count(), widths[i], headers[i].1, &c.shown))
            .collect();
        out.push_str(&format!("  {}\n", cells.join("  ").trim_end()));
    }
    out
}
