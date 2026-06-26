//! Page layout for the `/pipeline/*` tab (editable-rules-v3 §4).
//!
//! PR 3 ships the shell: a queue listing the eight pipeline stages in
//! execution order (two-line layout: name + rule count, then attribute
//! tags), an empty/stub detail panel, and an activity panel. Editing,
//! the Edit/Evaluate editor card, and categorical impact land in the
//! per-stage conversion PRs (4–8).

use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use pocketsmith_sync::db::payee_normalisations as pn;
use pocketsmith_sync::rules::impact::SAMPLE_LIMIT;
use pocketsmith_sync::rules::model::{LocationKind, Rule, RuleData};
use pocketsmith_sync::rules::{self, crud, Stage};

use super::editor::{self, Card, Mode};
use super::regex_hl;
use crate::serve::render::render_page;
use crate::serve::state::{AppState, PipelineBase};

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
/// renders the three-pane shell. The right column of the detail shows the
/// editor card for the active rule (if any), else a help placeholder.
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
        Some(stage) => {
            let card = match st.pipeline_active_rule {
                Some(id) => edit_card(&st.conn, stage, id, None),
                None => empty_card(),
            };
            render_detail(&st.conn, stage, st.pipeline_active_rule, card)
        }
        None => empty_detail(),
    };
    let activity = render_activity(&st);
    render_page("pipeline", "Pipeline", queue, detail, activity)
}

/// Stage-detail fragment (HTMX target of a queue click / arrow nav, and
/// the editor's Cancel). Clears any open editor card so the detail shows
/// the list with a help placeholder.
pub fn render_detail_fragment(state: &Arc<Mutex<AppState>>, stage_slug: &str) -> Markup {
    let mut st = state.lock().unwrap();
    let Some(stage) = Stage::from_name(stage_slug) else {
        return html! { div.empty-state { p { "Unknown pipeline stage." } } };
    };
    st.pipeline_active = Some(stage.name().to_string());
    st.pipeline_active_rule = None;
    render_detail(&st.conn, stage, None, empty_card())
}

/// Editor card in **edit** mode for an existing rule (GET
/// `/pipeline/stage/<slug>/rule/<id>`). Returns just the editor-column
/// content (the rule list is left untouched, so the panel doesn't jump),
/// and ensures the base pass for loop-stage "matching payees". Records the
/// active rule so a full page re-render keeps it.
pub fn render_edit_fragment(state: &Arc<Mutex<AppState>>, stage_slug: &str, id: i64) -> Markup {
    let mut st = state.lock().unwrap();
    let Some(stage) = Stage::from_name(stage_slug) else {
        return html! { div.empty-state { p { "Unknown pipeline stage." } } };
    };
    st.pipeline_active = Some(stage.name().to_string());
    st.pipeline_active_rule = Some(id);
    // Loop-stage "matching payees" reads the cached base pass; ensure it.
    if is_loop_stage(stage) {
        st.ensure_pipeline_base();
    }
    let base = st.pipeline_base.as_ref();
    edit_card(&st.conn, stage, id, base)
}

/// Editor card in **new** mode (GET `/pipeline/stage/<slug>/new`). Returns
/// just the editor-column content.
pub fn render_new_fragment(state: &Arc<Mutex<AppState>>, stage_slug: &str) -> Markup {
    let mut st = state.lock().unwrap();
    let Some(stage) = Stage::from_name(stage_slug) else {
        return html! { div.empty-state { p { "Unknown pipeline stage." } } };
    };
    st.pipeline_active = Some(stage.name().to_string());
    st.pipeline_active_rule = None;
    new_card(stage)
}

/// Loop stages re-feed their output; matcher stages are first-match.
fn is_loop_stage(stage: Stage) -> bool {
    matches!(stage, Stage::Prefixes | Stage::Suffixes | Stage::Expansions)
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

/// Base URL for a stage's editor endpoints.
fn base(stage: Stage) -> String {
    format!("/pipeline/stage/{}", stage.name())
}

/// Two-column stage detail: the rule list (left) + the editor card or
/// help placeholder (right, passed in as `card`). `active_rule` highlights
/// the selected row. Shared by the GET fragments and the mutation
/// handlers (which pass an evaluate-mode card).
pub fn render_detail(
    conn: &rusqlite::Connection,
    stage: Stage,
    active_rule: Option<i64>,
    card: Markup,
) -> Markup {
    let count = rules::count(conn, stage).unwrap_or(0);
    let listing = crud::list(conn, stage).unwrap_or_default();
    let movable = crud::is_movable(stage);
    // Prefix/suffix rules have no canonical, so that column is always
    // empty for them — drop it.
    let has_canon = !matches!(stage, Stage::Prefixes | Stage::Suffixes);
    let impact = rules::impact::load_for_stage(conn, stage).unwrap_or_default();
    let base = base(stage);
    html! {
        // Compact one-line header (name · count · tags) with Add on the
        // right, outside the dark rule pane.
        div.detail-header {
            h2 { (stage_name(stage)) }
            span.head-meta {
                "\u{00b7} " (count) " rules"
                @for tag in stage_tags(stage) {
                    " \u{00b7} " (tag)
                }
            }
            button.btn.btn-shortcut.add type="button"
                hx-get=(format!("{base}/new")) hx-target="#editor-col" hx-swap="innerHTML"
            { "[A] Add rule" }
        }
        div.detail-2col {
            div.rules-pane.(if has_canon { "" } else { "no-canon" })
                data-stage=(stage.name()) data-movable=(if movable { "1" } else { "0" })
            {
                div.rule-list-header {
                    span {}
                    @if has_canon { span { "Canonical" } }
                    span { "Pattern" }
                    span.num title="Txns matched, refreshed on re-scan" { "Txns" }
                    span.num title="$ value, refreshed on re-scan" { "Value" }
                }
                div.rule-list {
                    @if listing.is_empty() {
                        div.rule-empty { "No rules yet \u{2014} [A] add the first." }
                    }
                    @for r in &listing {
                        (render_rule_row(stage, r, movable, has_canon, active_rule == Some(r.id), impact.get(&r.id).copied()))
                    }
                }
            }
            div.editor-col #editor-col { (card) }
        }
    }
}

/// One clickable rule-list row. Clicking refreshes only the editor column
/// (`#editor-col`) so the list keeps its scroll position. The pattern uses
/// the CLI token colouring; Txns / Value are the cached per-rule impact
/// (from the last re-scan), or dim dashes when not yet computed.
fn render_rule_row(
    stage: Stage,
    r: &Rule,
    movable: bool,
    has_canon: bool,
    selected: bool,
    impact: Option<(i64, i64)>,
) -> Markup {
    let url = format!("/pipeline/stage/{}/rule/{}", stage.name(), r.id);
    let (txns, value) = match impact {
        Some((t, c)) => (t.to_string(), crate::serve::helpers::format_dollars_compact(c)),
        None => ("\u{2014}".to_string(), "\u{2014}".to_string()),
    };
    html! {
        div.rule-row.(if selected { "selected" } else { "" })
            data-rule-id=(r.id)
            hx-get=(url) hx-target="#editor-col" hx-swap="innerHTML"
        {
            span.rule-handle { @if movable { "\u{283f}" } }
            @if has_canon {
                span.canonical { (r.data.canonical().unwrap_or("\u{2014}")) }
            }
            span.pattern {
                @match r.data.pattern() {
                    Some(p) => (regex_hl::highlight(p)),
                    None => "\u{2014}",
                }
            }
            span.num.impact-txns data-impact-rule=(r.id) { (txns) }
            span.num.impact-value { (value) }
        }
    }
}

/// Editor card in edit mode for an existing rule, or the help placeholder
/// if the id no longer resolves. Followed by a "payees matching this rule"
/// panel: matcher stages read the `payee_normalisations` cache; loop
/// stages read the supplied base pass (editable-rules-ui §8,9).
pub fn edit_card(
    conn: &rusqlite::Connection,
    stage: Stage,
    id: i64,
    base: Option<&PipelineBase>,
) -> Markup {
    match crud::get(conn, stage, id) {
        Ok(Some(rule)) => html! {
            (editor::render(&Card {
                stage,
                mode: Mode::Edit,
                id: Some(id),
                data: &rule.data,
                error: None,
                eval_body: html! {},
            }))
            (matches_panel(conn, stage, &rule.data, base))
        },
        _ => empty_card(),
    }
}

/// The `(json_key, value)` identifying which `payee_normalisations` rows
/// resulted from a matcher rule, or `None` for loop stages.
fn feature_key(stage: Stage, data: &RuleData) -> Option<(&'static str, String)> {
    match stage {
        Stage::Persons | Stage::Employers | Stage::Merchants => {
            data.canonical().map(|c| ("entity_name", c.to_string()))
        }
        Stage::BankingOps => data.canonical().map(|op| ("operation", op.to_string())),
        Stage::Locations => match data {
            RuleData::Location { location, kind, .. } => {
                let key = match kind {
                    LocationKind::Region => "region",
                    LocationKind::Location => "location",
                };
                Some((key, location.clone()))
            }
            _ => None,
        },
        _ => None,
    }
}

/// The loop-stage trace tag (differs from `stage.name()`).
fn loop_trace_name(stage: Stage) -> Option<&'static str> {
    match stage {
        Stage::Prefixes => Some("prefix"),
        Stage::Suffixes => Some("suffix"),
        Stage::Expansions => Some("expand"),
        _ => None,
    }
}

/// "Payees matching this rule": for matcher stages, the distinct payees
/// whose last scan produced this rule's feature (from `payee_normalisations`);
/// for loop stages, the payees whose cached base pass fired this rule.
fn matches_panel(
    conn: &rusqlite::Connection,
    stage: Stage,
    data: &RuleData,
    base: Option<&PipelineBase>,
) -> Markup {
    // Loop stages: read the cached base pass (fired patterns).
    if let Some(tag) = loop_trace_name(stage) {
        let Some(pattern) = data.pattern() else {
            return html! {};
        };
        let Some(base) = base else {
            return html! {
                div.matches-panel {
                    h3 { "Payees matching this rule" }
                    div.sub { "Re-scan or re-select to compute matches." }
                }
            };
        };
        let mut rows: Vec<(&str, i64)> = Vec::new();
        for (p, res) in base.payees.iter().zip(base.results.iter()) {
            let fired = res
                .trace
                .iter()
                .any(|t| t.stage == tag && t.fired.iter().any(|f| f == pattern));
            if fired {
                rows.push((p.original_payee.as_str(), p.txn_count));
            }
        }
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        return matches_table(&rows);
    }

    // Matcher stages: cheap query against the staging cache.
    let Some((key, value)) = feature_key(stage, data) else {
        return html! {};
    };
    let rows = pn::payees_with_feature(conn, key, &value).unwrap_or_default();
    let rows: Vec<(&str, i64)> = rows.iter().map(|(p, n)| (p.as_str(), *n)).collect();
    matches_table(&rows)
}

/// Render the matching-payees table (expandable to show everything).
fn matches_table(rows: &[(&str, i64)]) -> Markup {
    let txns: i64 = rows.iter().map(|(_, n)| n).sum();
    html! {
        div.matches-panel {
            h3 {
                "Payees matching this rule "
                span.matches-count { "(" (rows.len()) " payees \u{00b7} " (txns) " txns)" }
            }
            @if rows.is_empty() {
                div.sub { "None staged \u{2014} run re-scan to refresh, or this rule matches no payee." }
            } @else {
                table.impact-table.matches-table {
                    thead { tr { th.l { "Payee" } th.r { "Txns" } } }
                    tbody {
                        @for (i, (payee, n)) in rows.iter().enumerate() {
                            tr.(if i >= SAMPLE_LIMIT { "impact-extra" } else { "" }) {
                                td.payee { (payee) }
                                td.r { (n) }
                            }
                        }
                        @if rows.len() > SAMPLE_LIMIT {
                            tr.impact-more {
                                td colspan="2" onclick="this.closest('table').classList.add('show-all')" {
                                    "show " (rows.len() - SAMPLE_LIMIT) " more"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Editor card in new-rule mode, prefilled with empty fields for `stage`.
pub fn new_card(stage: Stage) -> Markup {
    let data = editor::empty(stage);
    editor::render(&Card {
        stage,
        mode: Mode::New,
        id: None,
        data: &data,
        error: None,
        eval_body: html! {},
    })
}

/// Right-column placeholder shown when no rule is selected.
fn empty_card() -> Markup {
    html! {
        div.editor-empty {
            p { "Select a rule to edit, or " strong { "[A]" } " add a new one." }
            p.sub { "Editing never saves blindly: click " strong { "[E] Evaluate" } " to see the impact, then " strong { "[Y] Save" } "." }
        }
    }
}

fn empty_detail() -> Markup {
    html! { div.empty-state { p { "Select a pipeline stage from the queue." } } }
}

/// Activity panel: the dirty-rules banner (when rule edits have
/// out-paced the last scan) atop the rule-change log (newest first, with
/// the add/edit/delete colour vocabulary).
fn render_activity(st: &AppState) -> Markup {
    let count = st.pipeline_activity.len();
    let dirty = pocketsmith_sync::rules::dirty::would_restage(&st.conn).unwrap_or(0);
    html! {
        div.activity-header {
            @if dirty > 0 {
                span.dirty-banner {
                    span.warn { "\u{26a0} " (dirty) " payees would re-stage" }
                    " since the last scan \u{00b7} "
                    button.rescan-btn type="button"
                        hx-post="/pipeline/rescan" hx-target="body" hx-swap="innerHTML"
                    { "re-scan now \u{21bb}" }
                }
            } @else {
                span.stat { "Rule edits this session " span.count-confirmed { (count) } }
            }
        }
        div.activity-list {
            @if st.pipeline_activity.is_empty() {
                div.activity-empty { "No rule changes yet." }
            }
            @for e in st.pipeline_activity.iter().rev() {
                div.activity-row.(e.kind.css_class()) { (e.line) }
            }
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
        let html = render_detail(&conn, Stage::Persons, None, empty_card()).into_string();
        assert!(html.contains("Persons"), "{html}");
        assert!(html.contains("118 rules"), "{html}");
    }

    #[test]
    fn detail_renders_rule_list_with_headers_and_rows() {
        let conn = pocketsmith_sync::db::initialize_in_memory().unwrap();
        pocketsmith_sync::rules::load_into_db(&conn).unwrap();
        let html = render_detail(&conn, Stage::Merchants, None, empty_card()).into_string();
        // Two-column detail with the rule list + an [A] Add button.
        assert!(html.contains("rule-list"), "{html}");
        assert!(html.contains("detail-2col"), "{html}");
        assert!(html.contains("[A] Add rule"), "{html}");
        assert!(html.contains("Canonical") && html.contains("Pattern"), "{html}");
        // A known seeded merchant canonical appears as a row, with an
        // edit link carrying its id.
        assert!(html.contains("Woolworths"), "expected a merchant row: {html}");
        assert!(html.contains("/pipeline/stage/merchants/rule/"), "row links to editor: {html}");
    }

    #[test]
    fn detail_marks_active_rule_selected() {
        let conn = pocketsmith_sync::db::initialize_in_memory().unwrap();
        let id = pocketsmith_sync::rules::crud::insert_rule(
            &conn,
            &pocketsmith_sync::rules::model::RuleData::Merchant {
                canonical: "Uber".into(),
                pattern: "(?i)UBER".into(),
                note: None,
            },
        )
        .unwrap();
        let card = edit_card(&conn, Stage::Merchants, id, None);
        let html = render_detail(&conn, Stage::Merchants, Some(id), card).into_string();
        assert!(html.contains("rule-row selected"), "active row highlighted: {html}");
        // The edit card is shown in the right column.
        assert!(html.contains("edit mode"), "{html}");
    }

    #[test]
    fn every_stage_has_a_name_and_at_least_one_tag() {
        for stage in Stage::all() {
            assert!(!stage_name(stage).is_empty());
            assert!(!stage_tags(stage).is_empty(), "stage {stage:?} has no tags");
        }
    }
}
