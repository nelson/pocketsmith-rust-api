//! PR-A integration tests for the Pipeline editor: drive the real
//! mutation handlers + view fragments end-to-end against an in-memory DB
//! (editable-rules-ui §3.10). These exercise the handler → library
//! `commit` → cache-invalidation → `.sql` dump → pipeline-output path the
//! browser drives, without HTTP.

use std::sync::{Arc, Mutex};

use pocketsmith::db::initialize_in_memory;
use pocketsmith::normalise::{format_payee, normalise, PipelineCtx, RuleCache};
use pocketsmith::review::Status;
use pocketsmith::rules::{crud, Stage};
use pocketsmith::test_support::{seed_account, seed_pn, seed_txn};

use crate::serve::pipeline::{handlers, views};
use crate::serve::state::AppState;

/// A unique temp dir for the synchronous `.sql` dump (no global state →
/// parallel-safe).
fn tmpdir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!(
        "serve-pipeline-it-{}-{:?}-{n}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn state_with(dir: std::path::PathBuf) -> Arc<Mutex<AppState>> {
    let conn = initialize_in_memory().unwrap();
    seed_account(&conn, 1, "Cheque").unwrap();
    let mut app = AppState::new(conn);
    app.rules_dir_override = Some(dir);
    Arc::new(Mutex::new(app))
}

/// The pipeline's proposed payee for `raw`, computed fresh (own cache) so
/// it reflects the committed rules.
fn proposed(state: &Arc<Mutex<AppState>>, raw: &str) -> String {
    let st = state.lock().unwrap();
    let cache = RuleCache::new();
    let ctx = PipelineCtx::new(&st.conn, &cache);
    format_payee(&normalise(raw, &ctx))
}

#[test]
fn create_evaluate_save_flow_then_pipeline_resolves() {
    let dir = tmpdir();
    let state = state_with(dir.clone());
    {
        let st = state.lock().unwrap();
        seed_txn(&st.conn, 1, 1, "BUNNINGS 391 KOTARA", "BUNNINGS 391 KOTARA").unwrap();
        seed_txn(&st.conn, 2, 1, "WOOLWORTHS METRO", "WOOLWORTHS METRO").unwrap();
    }

    // 1. New-rule card (just the editor column; the Add button lives in the
    //    panel header rendered by render_detail).
    let new_card = views::render_new_fragment(&state, "merchants").into_string();
    assert!(new_card.contains("new rule"), "{new_card}");
    assert!(new_card.contains("name=\"pattern\""), "editable form: {new_card}");
    // The Add button is in the stage detail header.
    let detail = views::render_detail_fragment(&state, "merchants").into_string();
    assert!(detail.contains("[A] Add rule"), "{detail}");

    // 2. Evaluate the candidate (writes nothing) → impact buckets show the
    //    newly-matched payee, Save enabled.
    let body = "canonical=Bunnings&pattern=BUNNINGS&test_string=BUNNINGS+391+KOTARA";
    let ev = handlers::evaluate(&state, "merchants", None, body).into_string();
    assert!(ev.contains("evaluate mode"), "{ev}");
    assert!(ev.contains("newly matched"), "{ev}");
    assert!(ev.contains("BUNNINGS 391 KOTARA"), "sample payee: {ev}");
    assert!(ev.contains("[Y] Save"), "{ev}");
    // Nothing committed yet.
    assert_eq!(crud::list(&state.lock().unwrap().conn, Stage::Merchants).unwrap().len(), 0);

    // 3. Save (create).
    let saved = handlers::create(&state, "merchants", body).into_string();
    assert!(saved.contains("Bunnings"), "{saved}");
    assert!(saved.contains("edit mode"), "saved rule opens in edit mode: {saved}");

    // 4. The list now shows the new rule.
    let listed = views::render_detail_fragment(&state, "merchants").into_string();
    assert!(listed.contains("Bunnings"), "{listed}");

    // 5. The `.sql` mirror was (synchronously) re-dumped.
    let f = dir.join("merchants.sql");
    assert!(f.exists(), "dump file missing");
    assert!(std::fs::read_to_string(&f).unwrap().contains("Bunnings"), "dump lacks rule");

    // 6. The pipeline now resolves the payee to the new canonical.
    let st = state.lock().unwrap();
    let cache = RuleCache::new();
    let ctx = PipelineCtx::new(&st.conn, &cache);
    let r = normalise("BUNNINGS 391 KOTARA", &ctx);
    assert_eq!(r.features.entity_name.as_deref(), Some("Bunnings"));
}

#[test]
fn reorder_prefix_changes_pipeline_output() {
    let state = state_with(tmpdir());

    // Two overlapping prefix rules. `^X ` (A) strips just "X "; `^X Y ` (B)
    // strips "X Y ". The prefix loop applies the first matching rule each
    // pass, so order decides how much of "X Y REST" is stripped.
    handlers::create(&state, "prefixes", "pattern=%5EX+"); // ^X␠
    handlers::create(&state, "prefixes", "pattern=%5EX+Y+"); // ^X␠Y␠

    let (a, b) = {
        let st = state.lock().unwrap();
        let ids: Vec<i64> = crud::list(&st.conn, Stage::Prefixes).unwrap().iter().map(|r| r.id).collect();
        (ids[0], ids[1])
    };

    // Initial order [A, B]: A wins → only "X " stripped → "Y" survives.
    let before = proposed(&state, "X Y REST");
    assert!(before.to_uppercase().contains("Y"), "expected Y to survive, got {before:?}");

    // Move B before A → [B, A]: B wins → "X Y " stripped → no "Y".
    handlers::reorder(&state, "prefixes", &format!("id={b}&dir=before&anchor={a}"));
    let after = proposed(&state, "X Y REST");
    assert!(!after.to_uppercase().contains("Y"), "expected Y stripped, got {after:?}");
    assert_ne!(before, after, "reorder must change the pipeline output");
}

#[test]
fn save_logs_added_then_dirty_banner_then_rescan_clears() {
    let state = state_with(tmpdir());
    {
        let st = state.lock().unwrap();
        seed_txn(&st.conn, 1, 1, "BUNNINGS 391 KOTARA", "BUNNINGS 391 KOTARA").unwrap();
        // Seed the staging row with the current (rule-free) proposal so the
        // pipeline starts in sync — no dirty banner.
        let cache = RuleCache::new();
        let ctx = PipelineCtx::new(&st.conn, &cache);
        let fresh = format_payee(&normalise("BUNNINGS 391 KOTARA", &ctx));
        seed_pn(&st.conn, "BUNNINGS 391 KOTARA", &fresh, Status::Pending, 1).unwrap();
    }

    // Initially in sync — no dirty banner.
    let shell0 = views::render_page_shell(&state).into_string();
    assert!(!shell0.contains("would re-stage"), "should start clean: {shell0}");

    // Create a merchant rule → activity logs "+ added".
    handlers::create(&state, "merchants", "canonical=Bunnings&pattern=BUNNINGS");
    {
        let st = state.lock().unwrap();
        assert_eq!(st.pipeline_activity.len(), 1);
        assert!(
            st.pipeline_activity[0].line.starts_with("+ added"),
            "unexpected activity line: {:?}",
            st.pipeline_activity[0].line
        );
    }

    // The new rule changes the proposal → dirty banner appears, with the
    // activity line shown.
    let shell1 = views::render_page_shell(&state).into_string();
    assert!(shell1.contains("would re-stage"), "dirty banner missing: {shell1}");
    assert!(shell1.contains("+ added"), "activity line missing: {shell1}");

    // Re-scan refreshes proposals → banner clears.
    handlers::rescan(&state);
    let shell2 = views::render_page_shell(&state).into_string();
    assert!(!shell2.contains("would re-stage"), "banner should clear after re-scan: {shell2}");
}

#[test]
fn rule_impact_cache_is_scan_only() {
    let state = state_with(tmpdir());
    let id = {
        let st = state.lock().unwrap();
        seed_txn(&st.conn, 1, 1, "BUNNINGS 391 KOTARA", "BUNNINGS 391 KOTARA").unwrap();
        seed_txn(&st.conn, 2, 1, "BUNNINGS WAREHOUSE", "BUNNINGS WAREHOUSE").unwrap();
        crud::insert_rule(
            &st.conn,
            &pocketsmith::rules::model::RuleData::Merchant {
                canonical: "Bunnings".into(),
                pattern: "(?i)BUNNINGS".into(),
                note: None,
            },
        )
        .unwrap()
    };

    // The impact column lives in the rule list (the detail fragment). The
    // Txns cell carries data-impact-rule=<id>; assert on its content.
    let txn_cell = format!("data-impact-rule=\"{id}\">2<");

    // Before any scan the cache is empty → the impact column shows the dim
    // dash, not a count.
    let before = views::render_detail_fragment(&state, "merchants").into_string();
    assert!(!before.contains(&txn_cell), "no cached impact before scan: {before}");

    // Scan populates rule_impact (2 Bunnings payees).
    handlers::rescan(&state);
    let after = views::render_detail_fragment(&state, "merchants").into_string();
    assert!(after.contains(&txn_cell), "cached impact rendered after scan: {after}");

    // Editing the rule does NOT change the cached number until re-scan.
    handlers::save_edit(&state, "merchants", id, "canonical=Bunnings&pattern=%28%3Fi%29BUNNINGS%5Cb");
    let edited = views::render_detail_fragment(&state, "merchants").into_string();
    assert!(edited.contains(&txn_cell), "cache is stale-until-rescan: {edited}");

    // Confirm the cached row is unchanged in the table directly.
    let st = state.lock().unwrap();
    let cached = pocketsmith::rules::impact::load_for_stage(&st.conn, Stage::Merchants).unwrap();
    assert_eq!(cached.get(&id).unwrap().0, 2);
}

#[test]
fn commit_invalidates_the_base_cache() {
    let state = state_with(tmpdir());
    let id = {
        let st = state.lock().unwrap();
        seed_txn(&st.conn, 1, 1, "UBER TRIP", "UBER TRIP").unwrap();
        crud::insert_rule(
            &st.conn,
            &pocketsmith::rules::model::RuleData::Merchant {
                canonical: "Uber".into(),
                pattern: "(?i)UBER".into(),
                note: None,
            },
        )
        .unwrap()
    };
    // Evaluating builds + caches the committed-rules base pass.
    handlers::evaluate(&state, "merchants", Some(id), "canonical=Uber&pattern=%28%3Fi%29UBER");
    assert!(state.lock().unwrap().pipeline_base.is_some(), "evaluate caches the base");
    // Committing an edit must drop it so the next evaluate is fresh.
    handlers::save_edit(&state, "merchants", id, "canonical=Uber&pattern=%28%3Fi%29UBER%5Cb");
    assert!(
        state.lock().unwrap().pipeline_base.is_none(),
        "commit must invalidate the cached base"
    );
}

#[test]
fn selecting_a_rule_lists_matching_payees_from_scan_cache() {
    let state = state_with(tmpdir());
    let id = {
        let st = state.lock().unwrap();
        seed_txn(&st.conn, 1, 1, "BUNNINGS 391 KOTARA", "BUNNINGS 391 KOTARA").unwrap();
        seed_txn(&st.conn, 2, 1, "WOOLWORTHS METRO", "WOOLWORTHS METRO").unwrap();
        crud::insert_rule(
            &st.conn,
            &pocketsmith::rules::model::RuleData::Merchant {
                canonical: "Bunnings".into(),
                pattern: "(?i)BUNNINGS".into(),
                note: None,
            },
        )
        .unwrap()
    };
    // A scan stages the payees with their features (entity_name).
    handlers::rescan(&state);

    // Selecting the rule shows the payees that currently resolve to it,
    // read from payee_normalisations — no extra table, no recompute.
    let h = views::render_edit_fragment(&state, "merchants", id).into_string();
    assert!(h.contains("Payees matching this rule"), "{h}");
    assert!(h.contains("BUNNINGS 391 KOTARA"), "matching payee listed: {h}");
    assert!(!h.contains("WOOLWORTHS METRO"), "non-matching payee excluded: {h}");
}
