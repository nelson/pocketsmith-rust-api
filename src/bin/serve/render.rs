//! Shared view fragments rendered by both the transfers and normalise
//! tabs. The two tabs deliberately share a visual vocabulary (queue +
//! detail + activity, the same Y/N/S buttons, the same activity stat
//! row), so the bits of HTML that are identical live here.

use maud::{html, Markup, PreEscaped, DOCTYPE};

use crate::css::CSS;
use crate::js::JS;
use crate::state::AppState;

/// Top-of-page navigation between the five tabs. `active` is the slug
/// (`"dashboard"`, `"transactions"`, `"review"`, `"transfers"`, or
/// `"normalise"`) of the tab being rendered; that tab gets a
/// non-clickable label while the others are links.
///
/// The order of entries matters: Tab and Shift-Tab cycle through them
/// left-to-right (see `js.rs`). The canonical order is
/// Dashboard → Transactions → Review → Transfers → Normalise.
pub fn render_tab_bar(active: &str) -> Markup {
    let tabs = [
        ("dashboard", "Dashboard", "/dashboard/"),
        ("transactions", "Transactions", "/transactions/"),
        ("review", "Review", "/review/"),
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
/// with the tab bar plus a three-pane layout (queue / detail /
/// activity). The three Markup arguments are the inner contents of
/// those three panes.
///
/// Both tabs use the same HTML element ids (`#queue`, `#detail`,
/// `#activity`) so the JS keyboard handler and the HTMX swap targets
/// in views.rs can be tab-agnostic.
pub fn render_page(
    tab_slug: &str,
    title: &str,
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
            body {
                (render_tab_bar(tab_slug))
                div.layout {
                    div.queue-panel #queue { (queue) }
                    div.detail-panel #detail { (detail) }
                }
                div.activity-panel #activity { (activity) }
                script { (PreEscaped(JS)) }
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

    /// The tab bar must render five entries in this exact left-to-right
    /// order: Dashboard, Transactions, Review, Transfers, Normalise.
    /// Order matters because Tab/Shift-Tab cycles through them.
    #[test]
    fn tab_bar_renders_five_tabs_in_canonical_order() {
        let html = render_tab_bar("dashboard").into_string();
        let positions: Vec<usize> = ["Dashboard", "Transactions", "Review", "Transfers", "Normalise"]
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
        // The four others should be links to their canonical tab URLs.
        for href in [
            "href=\"/dashboard/\"",
            "href=\"/review/\"",
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
        for active in ["dashboard", "transactions", "review", "transfers", "normalise"] {
            let html = render_tab_bar(active).into_string();
            let count = html.matches("class=\"tab active\"").count();
            assert_eq!(
                count, 1,
                "expected exactly one active tab when active={active:?}, got {count} in: {html}"
            );
        }
    }
}
