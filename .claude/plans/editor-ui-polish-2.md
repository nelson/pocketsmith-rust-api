# Plan: Pipeline editor UI polish round 2 + loop-stage impact + faster evaluate

> Branch: continue on `editor-gui-framework`. Ten items grouped into
> layout/CSS, interaction, and library/perf. Investigation done:
> loop-stage compiled rules (`CompiledPrefix/Suffix/Expansion`) don't keep
> the authored pattern, and the trace records no fired-rule info for loop
> stages — so loop impact/matches need a small capture addition.

## A. Layout & CSS (`css.rs`, `views.rs`, `editor.rs`)

### 4 + 5 + 10 — compact one-line header, drop redundant count, Add in panel header
- `views.rs::render_detail`: collapse the header to a single line
  `Persons · 119 rules · first match` (name + count + the first
  tag), and move the **[A] Add rule** button into this panel header
  (`.detail-header`, outside the dark `.rules-pane`). Remove the
  `.rules-pane-head` (its "N rules" count is now redundant).
- `css.rs`: `.detail-header` becomes a single flex row (title, dim
  count, dim tag, Add button pushed right).

### 5 — shrink the rule list to ~30% of the panel
- `css.rs`: `.rule-list { max-height: 30vh }` (was 340px) so the list
  takes roughly a third and the editor/impact/matches get the rest.

### 3 — split the impact column into Txns and Value
- `views.rs`: rule-list header + `render_rule_row` get separate **Txns**
  and **Value** columns (was one "N txns · $X" cell). Grid template gains
  a column; `no-canon` variant updated too.

### 2 — pattern alignment
- `css.rs`: align the rule-row grid on a common baseline — set
  `.rule-row { align-items: baseline }`, give every cell the same
  `line-height`, make the numeric columns `font-variant-numeric:
  tabular-nums` and right-aligned, and ensure the regex spans don't shift
  the baseline (`.rx-* { vertical-align: baseline }`). The pattern cell
  stays monospace and left-aligned with the header.

### 6 — spinner left of the pill, no overlap/shift
- `editor.rs`: render the spinner **inline just before** the `mode-pill`
  in the `h2` (not absolutely positioned).
- `css.rs`: `.spinner` is an inline-block ring sized with a fixed box and
  toggled via `visibility` (reserves its space, so showing it never
  shifts the pill) — still rotates about its own centre.

## B. Interaction — no scroll jump on rule selection (`views.rs`, `handlers.rs`, `main.rs`, `js.rs`)

### 1 — clicking a rule must not scroll the panel to the top
Root cause: a rule click swaps the **whole** `#detail` (incl. the rule
list), resetting the panel scroll and re-rendering the list. Fix: split
the detail so rule-level interactions only swap the editor side.
- `views.rs`: wrap the editor column as `#editor-col`; add a
  `render_editor_col` that returns just the card (+ matches).
- Retarget **non-list-changing** actions to `#editor-col` (no list
  re-render, no scroll reset): GET rule edit, GET new, POST evaluate, GET
  delete-preview, and the editor's Evaluate / Back / Cancel buttons.
- Keep **list-changing** actions on `#detail`: create, save, delete
  (commit), reorder.
- `js.rs`: on `.rule-row` click, move the `.selected` class client-side
  (the list is no longer re-rendered on selection).

## C. Library & performance

### 7 — loop-stage impact (prefix / suffix / expand), summed over hits
Loop rules don't surface in the trace, so capture which fired:
- `normalise/mod.rs`: add a transient `pending_fires: Vec<String>` to
  `NormalisationResult` + `record_fire(pattern)`, drained by `run_traced`
  into a new `TraceEntry.fired: Vec<String>` (empty for matcher stages).
- `prefix.rs` / `suffix.rs` / `expand.rs`: add `pattern: String` to the
  compiled struct and call `record_fire(pattern)` each time a rule fires
  in the loop.
- `rules/impact/attribution.rs`: for loop stages, for each payee, map each
  distinct fired pattern → rule id and add the payee's txn_count /
  total_cents (a rule "hits" a payee if it fired ≥1×; summed across
  payees). Writes into the existing `rule_impact` table (already keyed by
  `stage, rule_id`), so the rule-list Impact column populates on re-scan.

### 8 — loop-stage "payees that hit this rule" panel
Matcher stages use `payee_normalisations.features_json`; loop stages have
no such feature. Reuse the cached base pass instead:
- `views.rs::matches_panel`: for loop stages, filter the cached base
  `NormalisationResult`s for payees whose loop trace `fired` contains this
  rule's pattern; list them (expandable, same styling). Matcher stages
  keep the cheap `pn` query.
- `handlers.rs`: `render_edit_fragment` calls `ensure_base` and passes the
  base results to `edit_card` so loop matches are available on selection
  (first loop-rule selection builds the base once, then it's cached).

### 9 — make evaluate genuinely fast (affected-subset scratch)
Today `compute_buckets_with_base` reuses the cached base but still runs the
**full** pipeline over **all** payees for the scratch side. For
first-match stages only the affected payees can change buckets:
- `normalise/mod.rs`: snapshot `matcher_input` (the cleaned string after
  expand/locations, before the matcher stages) on each result.
- `rules/impact/buckets.rs`: for first-match Add/Edit/Delete, compute the
  **affected subset** = payees whose `matcher_input` matches the
  candidate pattern ∪ payees the base attributed to this rule id. Run the
  scratch pipeline only over that subset; everything else keeps its base
  outcome (counted unchanged). Correct for first-match (an unaffected
  payee's first-match can't change when only this rule changed). Loop
  stages keep the full scratch pass.
- This avoids a full N-payee scratch pass on every evaluate — the common
  merchants/persons/employers edit becomes ~instant after the first base.
- **Verification:** add a timing note / assert buckets match the
  full-scratch result on a fixture (equivalence test) so the fast path is
  provably identical to the slow one.

## Files
- Library: `src/normalise/mod.rs`, `src/normalise/{prefix,suffix,expand}.rs`,
  `src/rules/impact/{attribution.rs,buckets.rs}`.
- Serve: `src/bin/serve/pipeline/{views.rs,editor.rs,handlers.rs}`,
  `src/bin/serve/main.rs`, `src/bin/serve/css.rs`, `src/bin/serve/js.rs`.

## Verification
- `cargo test --features web` (lib + serve); update editor/impact/views
  tests for the split columns, one-line header, `#editor-col` target,
  spinner placement. Add: loop `fired` capture + loop attribution test;
  affected-subset == full-scratch equivalence test; loop-matches panel
  test.
- Manual: select rules without the panel jumping; columns aligned; header
  one line; list ~30%; spinner left of the pill, no shift; loop stages
  show impact after re-scan and a matching-payees list; evaluate is snappy.
- Confirm `rules/*.sql` stays clean and the dump round-trip passes.

## Notes / risks
- The loop `fired` capture adds a `Vec<String>` per loop-stage trace entry
  (small). `TraceEntry` gains one field — check the existing trace
  renderer/tests compile.
- Affected-subset scratch is an optimisation behind an equivalence test;
  if any edge case diverges, fall back to full scratch for that case.
