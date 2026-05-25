//! Page layout for the `/transactions/*` tab. The first slice renders
//! an empty three-pane shell with the correct active tab. Real queue,
//! detail, and activity content arrive in subsequent commits.
//!
//! The tab is, by design, mostly a *view* over data the existing
//! handlers already manage. Mutation goes through the staging
//! endpoints exposed by the `transfers` and `normalise` tabs (plus the
//! new `/transfer-decisions/*` endpoints once they land).

use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use crate::state::AppState;

/// Render the full `/transactions/` page. The queue/detail/activity
/// panels are placeholders for now â the goal of this commit is to get
/// the shell rendering with the correct tab-bar entry highlighted, so
/// later commits can swap content into the panels via HTMX without
/// needing to revisit the shell.
pub fn render_page_shell(_state: &Arc<Mutex<AppState>>) -> Markup {
    let queue = html! {
        div.queue-header {
            h2 { "Transactions" }
        }
        div.queue-list {
            div.empty-state { p { "Queue rendering lands in the next commit." } }
        }
    };
    let detail = html! {
        div.empty-state { p { "Select a transaction from the queue." } }
    };
    let activity = html! {
        div.activity-header {
            span.stat { "Transactions tab â placeholder shell." }
        }
    };
    crate::render::render_page("transactions", "Transactions", queue, detail, activity)
}
