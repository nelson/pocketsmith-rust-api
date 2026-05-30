//! Freshness chips for the header strip (editable-rules-v3 §4.2).
//!
//! The header shows a `synced N ago` chip; this module generalises that
//! logic so a sibling `pushed N ago` chip can sit beside it, driven by a
//! different `_operations.reason` (`'push'` vs `'sync'`). Same fresh /
//! stale / old buckets (≤ 24h / ≤ 7d / older).
//!
//! NOTE (PR 1): the header strip itself lives on the (as-yet-unmerged)
//! dashboard branch's `render.rs` (`render_header`). On `master` there is
//! no header to host the chips, so these helpers are not yet wired into a
//! page — they are the reusable, tested data+markup layer that the
//! dashboard header will call once the two branches converge. See
//! `.claude/plans/editable-rules-v3-progress.md`.

#![allow(dead_code)] // wired in once the dashboard header lands on this base

use maud::{html, Markup};
use rusqlite::Connection;

/// Freshness bucket for colouring the chip dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// ≤ 24h — green.
    Fresh,
    /// ≤ 7d — yellow.
    Stale,
    /// > 7d — red.
    Old,
    /// No operation of this reason recorded yet.
    Never,
}

impl Freshness {
    /// CSS modifier class for the chip.
    pub fn class(&self) -> &'static str {
        match self {
            Freshness::Fresh => "freshness-fresh",
            Freshness::Stale => "freshness-stale",
            Freshness::Old => "freshness-old",
            Freshness::Never => "freshness-never",
        }
    }
}

/// Bucket an age in seconds. Negative (clock-skew) ages count as Fresh.
pub fn bucket(age_seconds: i64) -> Freshness {
    if age_seconds <= 86_400 {
        Freshness::Fresh
    } else if age_seconds <= 7 * 86_400 {
        Freshness::Stale
    } else {
        Freshness::Old
    }
}

/// Most-recent `(created_at, age_seconds)` for the given operation
/// `reason`. Computed in SQLite so we don't need a date-time crate.
/// `None` on empty / error.
pub fn last_op_info(conn: &Connection, reason: &str) -> Option<(String, i64)> {
    let mut stmt = conn
        .prepare(
            "SELECT created_at, CAST(strftime('%s','now') - strftime('%s', created_at) AS INTEGER) \
               FROM _operations WHERE reason = ?1 \
               ORDER BY id DESC LIMIT 1",
        )
        .ok()?;
    stmt.query_row([reason], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .ok()
}

/// Compact "3h ago" / "2d ago". Negative ages fall back to "just now".
pub fn humanise_age(age_seconds: i64) -> String {
    let s = age_seconds.max(0);
    if s < 60 {
        "just now".to_string()
    } else if s < 3600 {
        format!("{}m ago", s / 60)
    } else if s < 86_400 {
        format!("{}h ago", s / 3600)
    } else if s < 86_400 * 30 {
        format!("{}d ago", s / 86_400)
    } else {
        format!("{}mo ago", s / (86_400 * 30))
    }
}

/// Render one freshness chip. `verb` is the past-tense action shown
/// (`"synced"`, `"pushed"`); `command` is the CLI prompted in the
/// tooltip when the data is stale or absent (`"cargo run --bin push"`).
pub fn freshness_chip(conn: &Connection, reason: &str, verb: &str, command: &str) -> Markup {
    match last_op_info(conn, reason) {
        Some((ts, age)) => {
            let f = bucket(age);
            let tip = format!("last {verb} at {ts} \u{2014} re-run `{command}`");
            html! {
                span.freshness-chip.(f.class()) title=(tip) {
                    span.freshness-chip-dot {}
                    span.freshness-chip-label { (verb) " " (humanise_age(age)) }
                }
            }
        }
        None => {
            let tip = format!("no {verb} recorded yet \u{2014} run `{command}`");
            html! {
                span.freshness-chip.(Freshness::Never.class()) title=(tip) {
                    span.freshness-chip-dot {}
                    span.freshness-chip-label { "never " (verb) }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::{initialize_in_memory, with_operation};

    #[test]
    fn bucket_boundaries() {
        assert_eq!(bucket(0), Freshness::Fresh);
        assert_eq!(bucket(86_400), Freshness::Fresh);
        assert_eq!(bucket(86_401), Freshness::Stale);
        assert_eq!(bucket(7 * 86_400), Freshness::Stale);
        assert_eq!(bucket(7 * 86_400 + 1), Freshness::Old);
        assert_eq!(bucket(-5), Freshness::Fresh);
    }

    #[test]
    fn humanise_age_units() {
        assert_eq!(humanise_age(-10), "just now");
        assert_eq!(humanise_age(30), "just now");
        assert_eq!(humanise_age(120), "2m ago");
        assert_eq!(humanise_age(3 * 3600), "3h ago");
        assert_eq!(humanise_age(3 * 86_400), "3d ago");
        assert_eq!(humanise_age(90 * 86_400), "3mo ago");
    }

    #[test]
    fn last_op_info_none_when_absent() {
        let conn = initialize_in_memory().unwrap();
        assert!(last_op_info(&conn, "push").is_none());
    }

    #[test]
    fn last_op_info_some_after_operation() {
        let conn = initialize_in_memory().unwrap();
        with_operation(&conn, "push", |_| Ok(())).unwrap();
        let (_, age) = last_op_info(&conn, "push").expect("push op recorded");
        assert!(age >= 0, "age should be non-negative, got {age}");
        // Sibling reason is independent.
        assert!(last_op_info(&conn, "sync").is_none());
    }

    #[test]
    fn chip_renders_verb_and_never_state() {
        let conn = initialize_in_memory().unwrap();
        // Never state.
        let html = freshness_chip(&conn, "push", "pushed", "cargo run --bin push").into_string();
        assert!(html.contains("never pushed"), "html:\n{html}");
        assert!(html.contains("freshness-never"), "html:\n{html}");
        assert!(html.contains("cargo run --bin push"), "tooltip command missing:\n{html}");

        // Fresh state after an op.
        with_operation(&conn, "push", |_| Ok(())).unwrap();
        let html = freshness_chip(&conn, "push", "pushed", "cargo run --bin push").into_string();
        assert!(html.contains("pushed "), "expected 'pushed <age>': {html}");
        assert!(html.contains("freshness-fresh"), "expected fresh class: {html}");
    }
}
