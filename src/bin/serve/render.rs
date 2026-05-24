//! Shared view fragments rendered by both the transfers and normalise
//! tabs. The two tabs deliberately share a visual vocabulary (queue +
//! detail + activity, the same Y/N/S buttons, the same activity stat
//! row), so the bits of HTML that are identical live here.

use maud::{html, Markup, PreEscaped, DOCTYPE};

use crate::css::CSS;
use crate::js::JS;
use crate::state::AppState;
use crate::views::render_tab_bar;

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
