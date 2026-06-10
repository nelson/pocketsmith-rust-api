//! Create / edit / delete / reorder / evaluate handlers for the Pipeline
//! editor (editable-rules-ui §3.2). Each mutating handler builds exactly
//! one [`Mutation`] and hands it to the shared `rules::commit` seam (which
//! writes the row, invalidates the compiled-rule cache, and re-dumps the
//! stage's `.sql` mirror per the [`AppState::rule_dump_policy`]). The
//! handlers own no rule logic — validation, impact, and persistence all
//! live in the library `rules::` core that the `rule` CLI also drives.

use std::sync::{Arc, Mutex};

use maud::{html, Markup};

use pocketsmith_sync::normalise::scan;
use pocketsmith_sync::rules::crud;
use pocketsmith_sync::rules::impact::{compute_buckets, load_payees, test_one};
use pocketsmith_sync::rules::model::{MoveTarget, Mutation, RuleData};
use pocketsmith_sync::rules::validate::validate_draft;
use pocketsmith_sync::rules::{commit, Stage};

use super::editor::{self, Card, Mode};
use super::form::{build_rule_data, parse_urlencoded};
use super::impact as impact_view;
use super::views;
use crate::state::AppState;

/// Resolve a stage slug, or render the unknown-stage placeholder.
macro_rules! stage_or_bail {
    ($slug:expr) => {
        match Stage::from_name($slug) {
            Some(s) => s,
            None => return html! { div.empty-state { p { "Unknown pipeline stage." } } },
        }
    };
}

/// POST evaluate: re-render the editor card in evaluate mode for the
/// posted (unsaved) form values. `id` is `Some` for an existing rule,
/// `None` for a new one. Computes the impact buckets + tester result;
/// an invalid pattern surfaces inline and disables Save.
pub fn evaluate(
    state: &Arc<Mutex<AppState>>,
    slug: &str,
    id: Option<i64>,
    body: &str,
) -> Markup {
    let stage = stage_or_bail!(slug);
    let form = parse_urlencoded(body);
    let data = build_rule_data(stage, &form);
    let test_string = form.get("test_string").cloned().unwrap_or_default();

    let st = state.lock().unwrap();
    let card_markup = build_eval_card(&st.conn, stage, id, &data, &test_string);
    views::render_detail(&st.conn, stage, id, card_markup)
}

/// Build the evaluate-mode editor card (validation error → no buckets,
/// Save disabled; otherwise tester result + impact buckets).
fn build_eval_card(
    conn: &rusqlite::Connection,
    stage: Stage,
    id: Option<i64>,
    data: &RuleData,
    test_string: &str,
) -> Markup {
    let evaluate_url = match id {
        Some(id) => format!("/pipeline/stage/{}/rule/{id}/evaluate", stage.name()),
        None => format!("/pipeline/stage/{}/new/evaluate", stage.name()),
    };

    // Validate first: an un-compilable pattern can't be evaluated.
    if let Err(e) = validate_draft(data) {
        let msg = if e.is_syntax() {
            format!("syntax error: {e}")
        } else {
            format!("cannot save: {e}")
        };
        return editor::render(&Card {
            stage,
            mode: Mode::Evaluate,
            id,
            data,
            error: Some(&msg),
            eval_body: html! {},
        });
    }

    let mutation = match id {
        Some(id) => Mutation::Edit { id, data: data.clone() },
        None => Mutation::Add(data.clone()),
    };
    let payees = load_payees(conn).unwrap_or_default();
    let payee_total = payees.len() as i64;
    let eval_body = match compute_buckets(conn, stage, &mutation, &payees) {
        Ok(buckets) => {
            let test_result =
                (!test_string.is_empty()).then(|| test_one(conn, stage, data, test_string));
            impact_view::render(&impact_view::Eval {
                test_string,
                test_result: test_result.as_ref(),
                buckets: &buckets,
                payee_total,
                evaluate_url: &evaluate_url,
            })
        }
        Err(_) => html! { div.editor-error { "could not evaluate this rule" } },
    };

    editor::render(&Card {
        stage,
        mode: Mode::Evaluate,
        id,
        data,
        error: None,
        eval_body,
    })
}

/// POST create: commit a new rule. On success the detail re-renders with
/// the saved rule open in edit mode; on a validation / duplicate error it
/// re-renders the evaluate card with the message and Save disabled.
pub fn create(state: &Arc<Mutex<AppState>>, slug: &str, body: &str) -> Markup {
    let stage = stage_or_bail!(slug);
    let data = build_rule_data(stage, &parse_urlencoded(body));
    commit_mutation(state, stage, None, Mutation::Add(data))
}

/// POST save-edit: commit changes to an existing rule.
pub fn save_edit(state: &Arc<Mutex<AppState>>, slug: &str, id: i64, body: &str) -> Markup {
    let stage = stage_or_bail!(slug);
    let data = build_rule_data(stage, &parse_urlencoded(body));
    commit_mutation(state, stage, Some(id), Mutation::Edit { id, data })
}

/// GET delete preview: render the editor card in *evaluate-delete* mode —
/// the rule's fields read-only plus the impact of removing it, with a
/// mouse-only Confirm delete. A pure read (no mutation); the actual
/// removal is the POST handled by [`delete`].
pub fn delete_preview(state: &Arc<Mutex<AppState>>, slug: &str, id: i64) -> Markup {
    let stage = stage_or_bail!(slug);
    let mut st = state.lock().unwrap();
    st.pipeline_active = Some(stage.name().to_string());
    st.pipeline_active_rule = Some(id);

    let Some(rule) = crud::get(&st.conn, stage, id).ok().flatten() else {
        // Rule already gone — fall back to the list with no card open.
        st.pipeline_active_rule = None;
        return views::render_detail(&st.conn, stage, None, empty_card_markup());
    };

    let payees = load_payees(&st.conn).unwrap_or_default();
    let payee_total = payees.len() as i64;
    let mutation = Mutation::Delete { stage, id };
    let eval_body = match compute_buckets(&st.conn, stage, &mutation, &payees) {
        Ok(buckets) => html! {
            div.eval-section {
                h3 { "Impact of deleting this rule" }
                div.sub {
                    "What changes when this rule is removed, against "
                    strong { (payee_total) } " distinct raw payees."
                }
                (impact_view::render_buckets(&buckets))
            }
        },
        Err(_) => html! { div.editor-error { "could not evaluate this deletion" } },
    };

    let card = editor::render(&Card {
        stage,
        mode: Mode::EvaluateDelete,
        id: Some(id),
        data: &rule.data,
        error: None,
        eval_body,
    });
    views::render_detail(&st.conn, stage, Some(id), card)
}

/// POST delete: remove a rule, then re-render the list with no card open.
pub fn delete(state: &Arc<Mutex<AppState>>, slug: &str, id: i64) -> Markup {
    let stage = stage_or_bail!(slug);
    let mut st = state.lock().unwrap();
    let policy = st.rule_dump_policy();
    let mutation = Mutation::Delete { stage, id };
    match commit(&st.conn, &mutation, policy, Some(&st.rule_cache)) {
        Ok(res) => {
            st.pipeline_active_rule = None;
            st.push_rule_change(res.change);
            views::render_detail(&st.conn, stage, None, empty_card_markup())
        }
        Err(e) => {
            // Deletion failure is rare (e.g. row already gone); fall back
            // to the edit card carrying the message.
            let data = build_error_data(&st.conn, stage, id);
            let msg = format!("cannot delete: {e}");
            let card = editor::render(&Card {
                stage,
                mode: Mode::Edit,
                id: Some(id),
                data: &data,
                error: Some(&msg),
                eval_body: html! {},
            });
            views::render_detail(&st.conn, stage, Some(id), card)
        }
    }
}

/// POST reorder: move a loop-stage rule before/after an anchor. Form
/// fields: `id`, `dir` (`before`|`after`), `anchor`.
pub fn reorder(state: &Arc<Mutex<AppState>>, slug: &str, body: &str) -> Markup {
    let stage = stage_or_bail!(slug);
    let form = parse_urlencoded(body);
    let id = form.get("id").and_then(|s| s.parse::<i64>().ok());
    let anchor = form.get("anchor").and_then(|s| s.parse::<i64>().ok());
    let (id, anchor) = match (id, anchor) {
        (Some(i), Some(a)) => (i, a),
        _ => {
            let st = state.lock().unwrap();
            return views::render_detail(&st.conn, stage, st.pipeline_active_rule, empty_card_markup());
        }
    };
    let target = match form.get("dir").map(|s| s.as_str()) {
        Some("before") => MoveTarget::Before(anchor),
        _ => MoveTarget::After(anchor),
    };
    let mut st = state.lock().unwrap();
    let policy = st.rule_dump_policy();
    let mutation = Mutation::Move { stage, id, target };
    if let Ok(res) = commit(&st.conn, &mutation, policy, Some(&st.rule_cache)) {
        st.push_rule_change(res.change);
    }
    // Keep the moved rule selected so the user keeps context.
    st.pipeline_active_rule = Some(id);
    let card = views::edit_card(&st.conn, stage, id);
    views::render_detail(&st.conn, stage, Some(id), card)
}

/// POST re-scan: refresh `payee_normalisations` from the current rules,
/// which clears the dirty banner. Re-renders the whole Pipeline page so
/// the (now-cleared) banner + counts update.
pub fn rescan(state: &Arc<Mutex<AppState>>) -> Markup {
    {
        let st = state.lock().unwrap();
        let _ = scan::scan(&st.conn);
    }
    views::render_page_shell(state)
}

/// Shared create/edit commit path. On success shows the saved rule in
/// edit mode; on error re-renders the evaluate card with the message.
fn commit_mutation(
    state: &Arc<Mutex<AppState>>,
    stage: Stage,
    id: Option<i64>,
    mutation: Mutation,
) -> Markup {
    let mut st = state.lock().unwrap();
    let policy = st.rule_dump_policy();
    match commit(&st.conn, &mutation, policy, Some(&st.rule_cache)) {
        Ok(res) => {
            let saved_id = id.or(res.new_id);
            st.pipeline_active_rule = saved_id;
            st.push_rule_change(res.change);
            let card = match saved_id {
                Some(sid) => views::edit_card(&st.conn, stage, sid),
                None => empty_card_markup(),
            };
            views::render_detail(&st.conn, stage, saved_id, card)
        }
        Err(e) => {
            // Re-show the evaluate card so the user sees the failure in
            // context with Save disabled.
            let data = mutation_data(&mutation);
            let msg = format!("cannot save: {e}");
            let card = editor::render(&Card {
                stage,
                mode: Mode::Evaluate,
                id,
                data: &data,
                error: Some(&msg),
                eval_body: html! {},
            });
            views::render_detail(&st.conn, stage, id, card)
        }
    }
}

/// The candidate [`RuleData`] carried by an Add/Edit mutation.
fn mutation_data(mutation: &Mutation) -> RuleData {
    match mutation {
        Mutation::Add(d) | Mutation::Edit { data: d, .. } => d.clone(),
        _ => unreachable!("commit_mutation only handles Add/Edit"),
    }
}

/// Best-effort current data for an error card after a failed delete.
fn build_error_data(conn: &rusqlite::Connection, stage: Stage, id: i64) -> RuleData {
    crud::get(conn, stage, id)
        .ok()
        .flatten()
        .map(|r| r.data)
        .unwrap_or_else(|| editor::empty(stage))
}

/// The right-column placeholder (mirrors `views`'s private one). Kept
/// here so handlers don't depend on a private view helper.
fn empty_card_markup() -> Markup {
    html! {
        div.editor-empty {
            p { "Select a rule to edit, or " strong { "[A]" } " add a new one." }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::initialize_in_memory;
    use pocketsmith_sync::test_support::{seed_account, seed_txn};

    fn tmpdir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!(
            "serve-rule-{}-{:?}-{n}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn state_with_dump_dir(dir: std::path::PathBuf) -> Arc<Mutex<AppState>> {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Cheque").unwrap();
        let mut app = AppState::new(conn);
        app.rules_dir_override = Some(dir);
        Arc::new(Mutex::new(app))
    }

    fn body_merchant(canonical: &str, pattern: &str) -> String {
        format!("canonical={canonical}&pattern={pattern}")
    }

    #[test]
    fn create_commits_and_shows_saved_rule() {
        let dir = tmpdir();
        let state = state_with_dump_dir(dir.clone());
        {
            let st = state.lock().unwrap();
            seed_txn(&st.conn, 1, 1, "BUNNINGS 391", "BUNNINGS 391").unwrap();
        }
        let html = create(&state, "merchants", &body_merchant("Bunnings", "BUNNINGS")).into_string();
        // The detail re-renders with the rule present + an editor in edit mode.
        assert!(html.contains("Bunnings"), "{html}");
        assert!(html.contains("edit mode"), "{html}");
        // The row is in the DB.
        let st = state.lock().unwrap();
        let rules = crud::list(&st.conn, Stage::Merchants).unwrap();
        assert_eq!(rules.len(), 1);
        // The .sql mirror was dumped synchronously into the injected dir.
        let f = dir.join("merchants.sql");
        assert!(f.exists() && std::fs::read_to_string(&f).unwrap().contains("Bunnings"), "dump missing");
    }

    #[test]
    fn evaluate_shows_buckets_for_new_merchant() {
        let state = state_with_dump_dir(tmpdir());
        {
            let st = state.lock().unwrap();
            seed_txn(&st.conn, 1, 1, "BUNNINGS 391", "BUNNINGS 391").unwrap();
        }
        let html = evaluate(&state, "merchants", None, &body_merchant("Bunnings", "BUNNINGS")).into_string();
        assert!(html.contains("evaluate mode"), "{html}");
        assert!(html.contains("Impact across the database"), "{html}");
        assert!(html.contains("newly matched"), "{html}");
        // Save is enabled (no error).
        assert!(html.contains("[Y] Save"), "{html}");
    }

    #[test]
    fn evaluate_invalid_regex_disables_save() {
        let state = state_with_dump_dir(tmpdir());
        let html = evaluate(&state, "merchants", None, &body_merchant("X", "(?i)UBER%28")).into_string();
        assert!(html.contains("syntax error"), "{html}");
        assert!(html.contains("disabled"), "Save must be disabled: {html}");
    }

    #[test]
    fn delete_preview_shows_impact_without_deleting() {
        let state = state_with_dump_dir(tmpdir());
        let id = {
            let st = state.lock().unwrap();
            seed_account(&st.conn, 1, "A").unwrap();
            seed_txn(&st.conn, 1, 1, "BUNNINGS 391", "BUNNINGS 391").unwrap();
            crud::insert_rule(
                &st.conn,
                &RuleData::Merchant { canonical: "Bunnings".into(), pattern: "(?i)BUNNINGS".into(), note: None },
            )
            .unwrap()
        };
        // GET preview: evaluate-delete card with the deletion impact, but
        // the rule is NOT removed.
        let h = delete_preview(&state, "merchants", id).into_string();
        assert!(h.contains("confirm delete"), "{h}");
        assert!(h.contains("Impact of deleting this rule"), "{h}");
        assert!(h.contains("\u{1f5d1} Confirm delete"), "{h}");
        // Still present in the DB — preview is a pure read.
        let st = state.lock().unwrap();
        assert!(crud::get(&st.conn, Stage::Merchants, id).unwrap().is_some(), "preview must not delete");
    }

    #[test]
    fn edit_then_delete_roundtrip() {
        let state = state_with_dump_dir(tmpdir());
        let id = {
            let st = state.lock().unwrap();
            crud::insert_rule(
                &st.conn,
                &RuleData::Merchant { canonical: "Uber".into(), pattern: "(?i)UBER".into(), note: None },
            )
            .unwrap()
        };
        // Edit the pattern.
        let html = save_edit(&state, "merchants", id, &body_merchant("Uber", "(?i)UBER%5Cb")).into_string();
        assert!(html.contains("edit mode"), "{html}");
        {
            let st = state.lock().unwrap();
            assert_eq!(
                crud::get(&st.conn, Stage::Merchants, id).unwrap().unwrap().data.pattern(),
                Some("(?i)UBER\\b")
            );
        }
        // Delete it.
        let html = delete(&state, "merchants", id).into_string();
        assert!(!html.contains("edit mode"), "card should be cleared: {html}");
        let st = state.lock().unwrap();
        assert!(crud::get(&st.conn, Stage::Merchants, id).unwrap().is_none());
        assert_eq!(st.pipeline_active_rule, None);
    }

    #[test]
    fn reorder_moves_a_prefix_rule() {
        let state = state_with_dump_dir(tmpdir());
        let (a, b) = {
            let st = state.lock().unwrap();
            let mk = |p: &str| RuleData::Prefix {
                pattern: p.into(),
                gateway: None,
                operation: None,
                has_account: false,
                has_date: false,
                note: None,
            };
            let a = crud::insert_rule(&st.conn, &mk("^A ")).unwrap();
            let b = crud::insert_rule(&st.conn, &mk("^B ")).unwrap();
            (a, b)
        };
        // Move B before A → order [B, A].
        let body = format!("id={b}&dir=before&anchor={a}");
        reorder(&state, "prefixes", &body);
        let st = state.lock().unwrap();
        let order: Vec<i64> = crud::list(&st.conn, Stage::Prefixes).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(order, vec![b, a]);
    }
}
