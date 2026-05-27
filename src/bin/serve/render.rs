//! Shared view fragments rendered by both the transfers and normalise
//! tabs. The two tabs deliberately share a visual vocabulary (queue +
//! detail + activity, the same Y/N/S buttons, the same activity stat
//! row), so the bits of HTML that are identical live here.

use maud::{html, Markup, PreEscaped, DOCTYPE};

use crate::css::CSS;
use crate::js::JS;
use crate::state::AppState;

/// Top-of-page navigation between the four tabs. `active` is the slug
/// (`"dashboard"`, `"transactions"`, `"transfers"`, or
/// `"normalise"`) of the tab being rendered; that tab gets a
/// non-clickable label while the others are links.
///
/// The Review tab is planned but not implemented yet (see
/// `.claude/plans/review-tab-mvp.md`) and is therefore omitted from
/// the nav for now.
///
/// The order of entries matters: Tab and Shift-Tab cycle through them
/// left-to-right (see `js.rs`). The canonical order is
/// Dashboard → Transactions → Transfers → Normalise.
pub fn render_tab_bar(active: &str) -> Markup {
    let tabs = [
        ("dashboard", "Dashboard", "/dashboard/"),
        ("transactions", "Transactions", "/transactions/"),
        ("transfers", "Transfers", "/transfers/"),
        ("normalise", "Normalise", "/normalise/"),
    ];
    html! {
        nav.tab-bar {
            @for (slug, label, href) in tabs {
                @if slug == active {
                    span.tab.active { (label) }
                } @else {
                    a.tab href=(href) { (label) }
                }
            }
        }
    }
}

/// The three Y/N/S action buttons that sit in the detail panel of either
/// tab. `action_base` is the URL prefix used for the HTMX POSTs (e.g.
/// `/transfers/pair/3-4` or `/normalise/item/abc123`). The skip button
/// flips to an Unskip button when the row is in the session skip set.
pub fn render_actions(action_base: &str, is_skipped: bool) -> Markup {
    let skip_verb = if is_skipped { "unskip" } else { "skip" };
    let skip_label = if is_skipped { "[S] Unskip" } else { "[S] Skip" };
    html! {
        div.actions data-action-base=(action_base) {
            button.btn.btn-confirm
                hx-post=(format!("{action_base}/confirm"))
                hx-target="body"
            { "[Y] Confirm" }
            button.btn.btn-reject
                hx-post=(format!("{action_base}/reject"))
                hx-target="body"
            { "[N] Reject" }
            button.btn.btn-skip
                hx-post=(format!("{action_base}/{skip_verb}"))
                hx-target="body"
            { (skip_label) }
        }
    }
}

/// The full HTML page skeleton: doctype, head with CSS/JS/htmx, body
/// with the header (tab bar + last-sync chip), three-pane layout
/// (queue / detail / activity), and the keyboard-hints overlay.
/// The three Markup arguments are the inner contents of the panes.
///
/// All tabs use the same HTML element ids (`#queue`, `#detail`,
/// `#activity`) so the JS keyboard handler and the HTMX swap targets
/// in views.rs can be tab-agnostic.
/// Convenience wrapper: same as [`render_page_with_sync`] but with no
/// sync timestamp. Retained because the page-shell tests render the
/// skeleton without setting up a DB row.
#[allow(dead_code)]
pub fn render_page(
    tab_slug: &str,
    title: &str,
    queue: Markup,
    detail: Markup,
    activity: Markup,
) -> Markup {
    render_page_with_sync(tab_slug, title, None, queue, detail, activity)
}

/// Same as [`render_page`] but lets the caller supply a precomputed
/// last-sync timestamp + age in seconds (avoids reaching back into
/// the DB from inside view code). Pass `None` to omit the chip.
pub fn render_page_with_sync(
    tab_slug: &str,
    title: &str,
    last_sync: Option<(&str, i64)>,
    queue: Markup,
    detail: Markup,
    activity: Markup,
) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                script src="https://unpkg.com/htmx.org@2.0.4" {}
                style { (PreEscaped(CSS)) }
            }
            body class=(format!("tab-{tab_slug}")) {
                (render_header(tab_slug, last_sync))
                div.layout {
                    div.queue-panel #queue { (queue) }
                    div.detail-panel #detail { (detail) }
                }
                div.activity-panel #activity { (activity) }
                (render_hints_overlay())
                script { (PreEscaped(JS)) }
            }
        }
    }
}

/// Top-of-page header strip: tab bar on the left, last-sync chip and
/// `?` keyboard-hints trigger on the right. The chip prompts the user
/// to run `cargo run --bin sync` when the data is getting stale.
fn render_header(tab_slug: &str, last_sync: Option<(&str, i64)>) -> Markup {
    html! {
        div.header {
            (render_tab_bar(tab_slug))
            div.header-right {
                @if let Some((ts, age)) = last_sync {
                    span.sync-chip.(sync_chip_class(age)) title=(format!("last PocketSmith sync at {ts} \u{2014} re-sync with `cargo run --bin sync`")) {
                        span.sync-chip-dot {}
                        span.sync-chip-label { "synced " (humanise_sync_age(age)) }
                    }
                } @else {
                    span.sync-chip.sync-chip-never title="no sync recorded yet \u{2014} run `cargo run --bin sync`" {
                        span.sync-chip-dot {}
                        span.sync-chip-label { "never synced" }
                    }
                }
                button.hints-trigger title="keyboard shortcuts (?)" onclick="document.getElementById('hints-overlay').classList.toggle('open')" { "?" }
            }
        }
    }
}

/// Most-recent `sync` row from `_operations`: `(created_at, age_seconds)`.
/// Computed in SQLite so we don't need a date-time crate just for this.
/// Returns `None` on empty table or any DB error — the chip then
/// renders "never synced".
pub fn last_sync_info(conn: &rusqlite::Connection) -> Option<(String, i64)> {
    let mut stmt = conn
        .prepare(
            "SELECT created_at, CAST(strftime('%s','now') - strftime('%s', created_at) AS INTEGER) \
               FROM _operations WHERE reason = 'sync' \
               ORDER BY id DESC LIMIT 1",
        )
        .ok()?;
    stmt.query_row([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))).ok()
}

/// Bucket the age into a CSS class so the dot can colour itself:
/// fresh (≤ 24h, green), stale (≤ 7d, yellow), old (> 7d, red).
fn sync_chip_class(age_seconds: i64) -> &'static str {
    if age_seconds <= 86_400 {
        "sync-chip-fresh"
    } else if age_seconds <= 7 * 86_400 {
        "sync-chip-stale"
    } else {
        "sync-chip-old"
    }
}

/// Compact "3h ago" / "2d ago" string. Negative ages (clock skew) fall
/// back to "just now".
fn humanise_sync_age(age_seconds: i64) -> String {
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

/// Keyboard-hints overlay. Toggled by `?` (via JS) or by clicking the
/// header trigger. Hidden by default; the `.open` class flips it on.
/// Lists every key currently bound by the JS handler.
fn render_hints_overlay() -> Markup {
    html! {
        div.hints-overlay #hints-overlay onclick="if(event.target.id==='hints-overlay')this.classList.remove('open')" {
            div.hints-card {
                div.hints-card-header {
                    h2 { "Keyboard shortcuts" }
                    button.hints-close onclick="document.getElementById('hints-overlay').classList.remove('open')" { "\u{00d7}" }
                }
                div.hints-grid {
                    span.kbd { "\u{2191}" } span { "previous row in queue" }
                    span.kbd { "\u{2193}" } span { "next row in queue" }
                    span.kbd { "Tab" } span { "next tab (Shift-Tab for previous)" }
                    span.kbd { "Y" } span { "confirm the pending review" }
                    span.kbd { "N" } span { "reject the pending review" }
                    span.kbd { "S" } span { "skip the pending review" }
                    span.kbd { "U" } span { "undo the most recent action" }
                    span.kbd { "[" } span { "previous month (Dashboard tab)" }
                    span.kbd { "]" } span { "next month (Dashboard tab)" }
                    span.kbd { "?" } span { "toggle this overlay" }
                    span.kbd { "Esc" } span { "close this overlay" }
                }
                div.hints-foot { "Y / N / S / U act on whichever pillar is currently up for review on the active row." }
            }
        }
    }
}

/// Make sure the borrow of `state` stays alive only as long as needed.
/// (Convenience re-export to keep call sites short.)
#[allow(dead_code)] // referenced by view helpers in both tabs
pub fn _appstate_marker(_: &AppState) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tab bar must render four entries in this exact left-to-right
    /// order: Dashboard, Transactions, Transfers, Normalise. Review is
    /// intentionally omitted for now (planned, not implemented).
    /// Order matters because Tab/Shift-Tab cycles through them.
    #[test]
    fn tab_bar_renders_four_tabs_in_canonical_order() {
        let html = render_tab_bar("dashboard").into_string();
        let positions: Vec<usize> = ["Dashboard", "Transactions", "Transfers", "Normalise"]
            .iter()
            .map(|label| {
                html.find(label)
                    .unwrap_or_else(|| panic!("tab {label:?} not found in: {html}"))
            })
            .collect();
        // Strictly increasing positions => same DOM order.
        for w in positions.windows(2) {
            assert!(
                w[0] < w[1],
                "tab order wrong; positions: {:?} html:\n{}",
                positions,
                html
            );
        }
    }

    /// The active tab is rendered as a non-clickable <span>; the others
    /// are <a href="..."> links pointing at each tab's index URL.
    #[test]
    fn tab_bar_marks_active_tab_and_links_others() {
        let html = render_tab_bar("transactions").into_string();
        // Active = span with active class containing the label.
        assert!(
            html.contains("class=\"tab active\">Transactions"),
            "active tab not rendered as span.tab.active: {html}"
        );
        // The three others should be links to their canonical tab URLs.
        for href in [
            "href=\"/dashboard/\"",
            "href=\"/transfers/\"",
            "href=\"/normalise/\"",
        ] {
            assert!(
                html.contains(href),
                "expected inactive link {href} in: {html}"
            );
        }
        // The active tab must NOT be rendered as an anchor.
        assert!(
            !html.contains("href=\"/transactions/\""),
            "active tab should not have an href link: {html}"
        );
    }

    /// Each of the five tab slugs is acceptable as `active`. None of the
    /// other tabs should appear as <span class="tab active"> in the
    /// rendered HTML.
    #[test]
    fn tab_bar_active_slug_is_exclusive() {
        for active in ["dashboard", "transactions", "transfers", "normalise"] {
            let html = render_tab_bar(active).into_string();
            let count = html.matches("class=\"tab active\"").count();
            assert_eq!(
                count, 1,
                "expected exactly one active tab when active={active:?}, got {count} in: {html}"
            );
        }
    }
}

#[cfg(test)]
mod page_tests {
    use super::*;
    use maud::html;

    /// The full page must tag its <body> with the active tab slug as a
    /// class so per-tab CSS rules can scope themselves without
    /// duplicating selectors. Without this hook, every tab shares the
    /// same .queue-item layout and the new tabs collide with the
    /// old tabs' grid-template-columns.
    #[test]
    fn render_page_tags_body_with_tab_slug_class() {
        for slug in ["dashboard", "transactions", "transfers", "normalise"] {
            let html = render_page(
                slug,
                "x",
                html! {},
                html! {},
                html! {},
            )
            .into_string();
            let needle = format!("class=\"tab-{slug}\"");
            assert!(
                html.contains(&needle),
                "expected body to carry {needle:?} so per-tab CSS can scope; html:\n{html}"
            );
        }
    }
}
