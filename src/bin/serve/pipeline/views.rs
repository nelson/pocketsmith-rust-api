//! Page layout for the `/pipeline/*` tab (editable-rules-v3 §4).
//!
//! PR 3 ships the shell: a queue listing the eight pipeline stages in
//! execution order (two-line layout: name + rule count, then attribute
//! tags), an empty/stub detail panel, and an activity panel. Editing,
//! the Edit/Evaluate editor card, and categorical impact land in the
//! per-stage conversion PRs (4–8).

use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use pocketsmith_sync::rules::{self, Stage};

use crate::render::render_page;
use crate::state::AppState;

/// A queue row: a stage plus its current rule count.
#[derive(Debug, Clone, Copy)]
pub struct StageView {
    pub stage: Stage,
    pub count: i64,
}

/// Human label for a stage in the queue.
pub fn stage_name(stage: Stage) -> &'static str {
    match stage {
        Stage::Prefixes => "Prefix",
        Stage::Suffixes => "Suffix",
        Stage::Expansions => "Expand",
        Stage::Persons => "Persons",
        Stage::Employers => "Employers",
        Stage::Merchants => "Merchants",
        Stage::BankingOps => "Banking ops",
        Stage::Locations => "Locations",
    }
}

/// Attribute tags shown on the second line of a queue row (§4.3). These
/// communicate the stage's evaluation semantics at a glance:
///   * `loop`          — applied repeatedly until no rule matches
///   * `order matters`  — rule order changes the result
///   * `first match`    — first matching rule wins, order is cosmetic
///   * `captures`       — rules can extract features (account/date/…)
///   * `aux`            — auxiliary table consumed by another stage
pub fn stage_tags(stage: Stage) -> &'static [&'static str] {
    match stage {
        Stage::Prefixes => &["loop", "order matters", "captures"],
        Stage::Suffixes => &["loop", "order matters", "captures"],
        Stage::Expansions => &["loop", "order matters"],
        Stage::Persons => &["first match"],
        Stage::Employers => &["first match"],
        Stage::Merchants => &["first match"],
        Stage::BankingOps => &["first match", "captures"],
        Stage::Locations => &["aux"],
    }
}

/// Top-level page render: lists every stage with its live rule count,
/// selects an active stage (the one last clicked, else the first), and
/// renders the three-pane shell.
pub fn render_page_shell(state: &Arc<Mutex<AppState>>) -> Markup {
    let mut st = state.lock().unwrap();
    let stages = stage_views(&st.conn);

    let active = st
        .pipeline_active
        .clone()
        .and_then(|s| Stage::from_name(&s))
        .or_else(|| stages.first().map(|s| s.stage));
    st.pipeline_active = active.map(|s| s.name().to_string());

    let queue = render_queue(&stages, active);
    let detail = match active {
        Some(stage) => render_detail(&st.conn, stage),
        None => empty_detail(),
    };
    let activity = render_activity();
    render_page("pipeline", "Pipeline", queue, detail, activity)
}

/// Detail fragment for one stage (HTMX target of a queue click / arrow
/// nav). Records the active stage so a full page re-render keeps it.
pub fn render_detail_fragment(state: &Arc<Mutex<AppState>>, stage_slug: &str) -> Markup {
    let mut st = state.lock().unwrap();
    let Some(stage) = Stage::from_name(stage_slug) else {
        return html! { div.empty-state { p { "Unknown pipeline stage." } } };
    };
    st.pipeline_active = Some(stage.name().to_string());
    render_detail(&st.conn, stage)
}

/// Build the queue rows (one per stage, in execution order) with live
/// rule counts.
fn stage_views(conn: &rusqlite::Connection) -> Vec<StageView> {
    Stage::all()
        .into_iter()
        .map(|stage| StageView {
            stage,
            count: rules::count(conn, stage).unwrap_or(0),
        })
        .collect()
}

/// Pure queue render. Two-line rows: name + count, then attribute tags.
fn render_queue(stages: &[StageView], active: Option<Stage>) -> Markup {
    html! {
        div.queue-header {
            h2 { "Pipeline stages" }
        }
        div.queue-list {
            @for sv in stages {
                (render_queue_row(sv, active == Some(sv.stage)))
            }
        }
    }
}

fn render_queue_row(sv: &StageView, is_selected: bool) -> Markup {
    let detail_url = format!("/pipeline/stage/{}", sv.stage.name());
    html! {
        div.queue-item.pipeline-stage-item.(if is_selected { "selected" } else { "" })
            hx-get=(detail_url)
            hx-target="#detail"
            hx-swap="innerHTML"
            data-detail-url=(detail_url)
            data-detail-target="#detail"
        {
            div.pipeline-stage-line1 {
                span.pipeline-stage-name { (stage_name(sv.stage)) }
                span.pipeline-stage-count { (sv.count) " rules" }
            }
            div.pipeline-stage-tags {
                @for tag in stage_tags(sv.stage) {
                    span.pipeline-tag { (tag) }
                }
            }
        }
    }
}

/// Stage detail: header + a read-only table of the stage's rules in
/// apply order. Editing (the editor card, Edit/Evaluate, impact, and
/// create/edit/delete/reorder mutations) lands in a later PR.
fn render_detail(conn: &rusqlite::Connection, stage: Stage) -> Markup {
    let count = rules::count(conn, stage).unwrap_or(0);
    let listing = rules::list_display(conn, stage);
    html! {
        div.detail-header {
            div.row {
                h2 { (stage_name(stage)) }
                span.chip { (count) " rules" }
            }
            div.meta {
                @for tag in stage_tags(stage) {
                    span.pipeline-tag { (tag) }
                }
            }
        }
        @match listing {
            Ok((headers, rows)) => (render_rule_table(&headers, &rows)),
            Err(_) => div.empty-state { p { "Could not load rules for this stage." } },
        }
        div.note {
            "Read-only for now. Editing (add / edit / delete / reorder), the "
            "Edit/Evaluate editor card, and categorical impact land in a later PR."
        }
    }
}

/// Render the rule rows as a simple table. Column headers come straight
/// from the DB column names; NULL cells render as a dim dash.
fn render_rule_table(headers: &[&str], rows: &[rules::DisplayRow]) -> Markup {
    html! {
        table.rule-table {
            thead {
                tr {
                    @for h in headers {
                        th { (h) }
                    }
                }
            }
            tbody {
                @for row in rows {
                    tr {
                        @for cell in row {
                            @match cell {
                                Some(v) => td { (v) },
                                None => td.rule-cell-null { "\u{2014}" },
                            }
                        }
                    }
                }
            }
        }
    }
}

fn empty_detail() -> Markup {
    html! { div.empty-state { p { "Select a pipeline stage from the queue." } } }
}

/// Activity panel. PR 3 stub: the recent rule-change log and the
/// dirty-rules re-scan chip land in PR 9.
fn render_activity() -> Markup {
    html! {
        div.activity-header {
            span.stat { "Rule edits this session " span.count-confirmed { "0" } }
        }
        div.activity-list {
            div.activity-empty { "No rule changes yet." }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_stage_views() -> Vec<StageView> {
        Stage::all()
            .into_iter()
            .enumerate()
            .map(|(i, stage)| StageView {
                stage,
                count: (i as i64 + 1) * 10,
            })
            .collect()
    }

    #[test]
    fn queue_lists_eight_stages_in_execution_order() {
        let html = render_queue(&all_stage_views(), None).into_string();
        let order = [
            "Prefix", "Suffix", "Expand", "Persons", "Employers", "Merchants",
            "Banking ops", "Locations",
        ];
        let positions: Vec<usize> = order
            .iter()
            .map(|n| html.find(n).unwrap_or_else(|| panic!("stage {n} missing: {html}")))
            .collect();
        for w in positions.windows(2) {
            assert!(w[0] < w[1], "stages out of order: {positions:?}");
        }
        assert_eq!(html.matches("class=\"queue-item pipeline-stage-item").count(), 8);
    }

    #[test]
    fn queue_row_two_line_layout_with_count_and_tags() {
        let sv = StageView { stage: Stage::Prefixes, count: 42 };
        let html = render_queue_row(&sv, false).into_string();
        // Line 1: name + "N rules".
        assert!(html.contains("pipeline-stage-name"), "{html}");
        assert!(html.contains("42 rules"), "{html}");
        // Line 2: attribute tags.
        assert!(html.contains("pipeline-stage-tags"), "{html}");
        for tag in ["loop", "order matters", "captures"] {
            assert!(html.contains(tag), "expected tag {tag}: {html}");
        }
    }

    #[test]
    fn queue_row_carries_htmx_nav_attributes() {
        let sv = StageView { stage: Stage::Merchants, count: 146 };
        let html = render_queue_row(&sv, false).into_string();
        assert!(html.contains("data-detail-url=\"/pipeline/stage/merchants\""), "{html}");
        assert!(html.contains("data-detail-target=\"#detail\""), "{html}");
        assert!(html.contains("hx-get=\"/pipeline/stage/merchants\""), "{html}");
    }

    #[test]
    fn queue_marks_active_stage_selected() {
        let html = render_queue(&all_stage_views(), Some(Stage::Expansions)).into_string();
        assert_eq!(html.matches("queue-item pipeline-stage-item selected").count(), 1);
    }

    #[test]
    fn detail_shows_stage_name_and_count() {
        let conn = pocketsmith_sync::db::initialize_in_memory().unwrap();
        pocketsmith_sync::rules::load_into_db(&conn).unwrap();
        let html = render_detail(&conn, Stage::Persons).into_string();
        assert!(html.contains("Persons"), "{html}");
        assert!(html.contains("118 rules"), "{html}");
    }

    #[test]
    fn detail_renders_rule_table_with_headers_and_rows() {
        let conn = pocketsmith_sync::db::initialize_in_memory().unwrap();
        pocketsmith_sync::rules::load_into_db(&conn).unwrap();
        let html = render_detail(&conn, Stage::Merchants).into_string();
        // Column headers from the DB.
        assert!(html.contains("rule-table"), "{html}");
        assert!(html.contains("canonical") && html.contains("pattern"), "{html}");
        // A known seeded merchant canonical appears as a cell.
        assert!(html.contains("Woolworths"), "expected a merchant row: {html}");
    }

    #[test]
    fn every_stage_has_a_name_and_at_least_one_tag() {
        for stage in Stage::all() {
            assert!(!stage_name(stage).is_empty());
            assert!(!stage_tags(stage).is_empty(), "stage {stage:?} has no tags");
        }
    }
}
