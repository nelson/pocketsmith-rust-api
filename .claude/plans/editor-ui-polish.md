# Plan: Pipeline editor UI polish + perf

> Branch: continue on `editor-gui-framework`. Nine UI/behaviour/perf
> refinements to the rule editor, grouped below. Investigation done:
> `payee_normalisations.features_json` already stores `entity_name`,
> `operation`, `location`, `region` per payee (the last scan's output),
> so "which payees match this rule" needs **no new table and no recompute**.

## Items & approach

### 1. Spinner no longer shifts the mode box
- `css.rs`: position `.spinner` **absolutely** in the editor-card top-right
  (`.editor-card { position: relative }`, `.spinner { position:absolute; top; right }`),
  toggle via `visibility` not `display`, so showing it never reflows the
  header. Keep `#card-spin` + `hx-indicator` wiring.

### 2. "stolen from other" → "moved"
- `impact.rs`: rename the first-match summary/detail label to `moved`.
- Update the impact unit test assertion.

### 3. Expandable impact table
- `impact.rs`: render up to `EXPAND_LIMIT` (100) rows per bucket; first
  `SAMPLE_LIMIT` (6) visible, the rest as `tr.impact-extra` (hidden). A
  clickable `tr.impact-more` ("show N more") sets `show-all` on the table
  (inline `onclick`), CSS reveals extras + hides the toggle. >100 → a
  static "… and N more not shown" note.
- `css.rs`: `.impact-detail .impact-extra{display:none}`,
  `.impact-detail.show-all .impact-extra{display:table-row}`,
  `.impact-detail.show-all .impact-more{display:none}`.

### 4. Speed up Evaluate (cache the base pipeline pass)
`compute_buckets` runs the full pipeline over **all** payees twice (base =
committed rules, scratch = committed+mutation). The base is identical
across every re-evaluate in an editing session (no commit between).
- `rules/impact/buckets.rs`: expose `run_base(conn, payees) -> Vec<NormalisationResult>`
  and `compute_buckets_with_base(conn, stage, mutation, payees, base)` that
  only runs the scratch pass and buckets against the supplied base.
- `state.rs`: cache `PipelineBase { payees, results }`; invalidate (set
  `None`) on every commit / delete / reorder / rescan.
- `handlers.rs`: `evaluate` + `delete_preview` build/reuse the cached base.
- Net: repeated evaluates (tweaking the pattern / tester) drop from 2
  full passes to 1. (Deeper per-stage scratch optimisation noted as a
  follow-up; not in this pass.)

### 5. Entities ordered alphabetically — display **and** matching (§0)
- `rules/mod.rs`: add `entity_cmp(a,b)` — case-insensitive alphabetical,
  except a longer string containing the other as a substring sorts first
  (so `Amazon Prime` before `Amazon`, keeping first-match correct), plus
  `is_entity_ordered(stage)` for persons/employers/merchants/locations.
- `rules/crud/read.rs::list`: sort entity stages by `entity_cmp` on the
  canonical. Since both the pipeline (`load_for_compile`) and the web list
  go through `list`, apply order == display order. `dump` keeps id-order,
  so `rules/*.sql` and the dump round-trip test are unaffected.
- **Risk:** changes apply order for entity stages → run the seeded
  persons/employers/merchants stage tests; adjust only if a genuine
  ordering bug surfaces (the comparator is designed to preserve
  first-match correctness).
- Test: `entity_cmp` orders `Gamma Radiation Scans` before `Gamma Rad`.

### 6. "Impact column is empty — when populated?"
- It is filled by **re-scan** (`scan::scan` → `rule_impact`); empty until
  the first scan after the column shipped. Make this discoverable:
  `views.rs` adds a `title=` tooltip on the Impact header ("per-rule
  txns/$, refreshed on re-scan") and the dirty banner already prompts a
  re-scan. (No data-model change.)

### 7. Top/bottom panels instead of left/right
- `css.rs`: make `.detail-2col` a single column (rule list on top with a
  capped, scrollable height; editor + impact + matches below at full
  width — better for the wide impact tables). Pure CSS; no markup change.

### 8. Editable pattern in Evaluate mode + dynamic Save⇄Evaluate
- `editor.rs`: in Evaluate mode render fields **editable** (not read-only)
  and stamp each with `data-eval-val` (the just-evaluated value). Render
  both `[Y] Save` (`.act-save`) and `[E] Evaluate` (`.act-evaluate`); the
  pattern input carries no `hx-trigger` so editing it doesn't auto-submit.
- `js.rs`: input listener on `#rule-form` toggles a `.dirty` class when any
  field differs from its `data-eval-val` (and back when it matches again —
  the bonus). CSS shows Save when clean, Evaluate when dirty. The pipeline
  key handler clicks only the **visible** button (skip `offsetParent===null`).
- `css.rs`: `#rule-form:not(.dirty) .act-evaluate{display:none}`,
  `#rule-form.dirty .act-save{display:none}`.
- EvaluateDelete stays read-only (you don't edit a deletion).

### 9. Show payees currently matching the selected rule (existing caches)
- `db/payee_normalisations.rs`: `payees_with_feature(conn, json_key, value)`
  → `SELECT original_payee, txn_count … WHERE json_extract(features_json,
  '$.'||key)=value ORDER BY txn_count DESC` (SQLite JSON1, already bundled).
- `views.rs::edit_card`: for matcher stages (persons/employers/merchants →
  `entity_name`; banking_ops → `operation`; locations → `location`/`region`),
  map the rule's canonical to its feature value and render a collapsible
  **"Payees matching this rule (N)"** panel under the editor (reusing the
  expandable-table styling). Reflects the last scan ("currently matching");
  loop stages show nothing. No new tables, no extra pipeline pass.

## Files to modify
- Library: `src/rules/mod.rs`, `src/rules/crud/read.rs`,
  `src/rules/impact/buckets.rs`, `src/rules/impact/mod.rs`,
  `src/db/payee_normalisations.rs`.
- Serve: `src/bin/serve/state.rs`, `src/bin/serve/pipeline/handlers.rs`,
  `src/bin/serve/pipeline/views.rs`, `src/bin/serve/pipeline/editor.rs`,
  `src/bin/serve/pipeline/impact.rs`, `src/bin/serve/css.rs`,
  `src/bin/serve/js.rs`.

## Verification
- `cargo test --features web` (lib + serve). Update impact/editor tests
  for the new labels, expand toggle, editable-evaluate buttons; add
  `entity_cmp` + `payees_with_feature` unit tests.
- Manual: evaluate is snappier on re-eval; spinner doesn't shift the
  header; impact table expands; entity lists are alphabetical; selecting a
  rule lists its matching payees; pattern editable in evaluate with the
  Save⇄Evaluate toggle; detail stacks top/bottom.
- Confirm `rules/*.sql` stays clean and the dump round-trip test passes.

## Open question
- Entity reordering (#5) changes apply order for persons/employers/
  merchants. Acceptable per the v3 plan §0, but flagging in case any
  seeded-data normalisation test needs its expectation revisited.
