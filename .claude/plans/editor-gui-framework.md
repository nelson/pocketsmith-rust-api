# Plan: Rule editor UI for the web server (`editor-gui-framework`)

> ✅ **Delivered** on branch `editor-gui-framework` (PR #39).

> Order 5 / branch `editor-gui-framework` in
> [`editable-rules-ui.md`](./editable-rules-ui.md) §3. The keystone GUI PR:
> turns the read-only Pipeline tab into an **editable** rule surface.
> Consumes the rule-editing library core (typed CRUD + `compute_buckets`
> + `validate_draft` + `RuleChange`) already merged via `rule-cli`.

## Context

The Pipeline tab (`/pipeline/*`) currently renders a read-only three-pane
shell: a queue of the 8 stages, a detail panel that lists a stage's rules
in a flat table, and a stub activity panel. All the *logic* for editing
rules already exists in the library (`src/rules/`): typed CRUD, the
single-mutation `commit` seam, dry-run impact `compute_buckets`, the
single-string `test_one`, `validate_draft`, the `RuleChange` activity
vocabulary, and `dirty::would_restage`. The headless `rule` CLI already
drives all of it.

This work adds the **web layer** on top: the two-column detail (rule list +
Edit/Evaluate editor card), mutation handlers, the activity log, the
dirty-rules banner + re-scan, and the cached per-rule impact column.

## Split into three PRs

The master plan's single keystone is split into three reviewable PRs. The
first stage wired is **prefix + suffix** (loop stages — capture flags +
reorder — the hardest shape, per the master plan), so the framework is
proven against the worst case.

- **PR-A — editor core**: two-column detail (rule list + editor card),
  Edit/Evaluate/New card, evaluate impact buckets + single-string tester,
  create / edit / delete / reorder mutation handlers. No persisted impact
  column, no activity log, no dirty banner. Self-contained: an edit
  commits + re-dumps `.sql` and refreshes the list.
- **PR-B — activity log + dirty banner**: the newest-first capped-100
  rule-change log in the activity panel; the `⚠ N payees would re-stage`
  banner via `would_restage`; `/pipeline/rescan` runs `scan` + clears it.
- **PR-C — per-rule impact cache**: the `rule_impact` table, the scan
  attribution pass, and the cached `"412 txns · $8.4k"` impact column in
  the rule list (read-only join; refreshed only by re-scan).

Keyboard shortcuts land with the surface they drive (A/E/N/Y/B in PR-A;
the re-scan affordance in PR-B).

## Approach

Mirror the existing tab architecture (`normalise` / `transfers` tabs are
the closest analogs): a `views.rs` (pure render + state-locking entry
points) + `handlers.rs` (mutations that lock state, call the library
`commit`, push activity, re-render the shell). Drive the editor form from
the existing `StageSchema` descriptor (`src/rules/validate.rs`) so adding
later stages is "fill in a descriptor". Build the framework around the
**prefix + suffix** stages first (loop stages — capture flags + reorder —
the hardest shape), exactly as the master plan locks in; the other six
stages follow in their own PRs (§4 of the master plan).

All interactions are HTMX fragments POSTed under `/pipeline/`, no
client-side rule state. Edit→Evaluate→Save split: Evaluate (POST, carries
unsaved form values) shows impact buckets + tester; Save re-POSTs the same
fields. Server keeps no per-edit state between Evaluate and Save.

## Files to modify / create

**Library (`src/rules/`)**
- `src/db/schema.rs` — add `rule_impact` cache table (+ `IF NOT EXISTS`).
- `src/normalise/scan.rs` — fold a `rule_impact` refresh pass into `scan`.
- `src/rules/impact/` — add an attribution helper that maps each distinct
  payee → the rule id that won its stage (reused by the scan pass).

**Serve (`src/bin/serve/`)**
- `src/bin/serve/main.rs` — register the new `/pipeline/*` routes.
- `src/bin/serve/pipeline/mod.rs` — declare new submodules.
- `src/bin/serve/pipeline/views.rs` — two-column detail; rule list with
  cached impact column; wire selection + active-rule state.
- `src/bin/serve/pipeline/editor.rs` — **new**: parameterised editor card
  (edit / evaluate / new) driven by a per-stage field descriptor.
- `src/bin/serve/pipeline/impact.rs` — **new**: HTML rendering of the four
  (first-match) / two (loop) `compute_buckets` outcome buckets.
- `src/bin/serve/pipeline/handlers.rs` — **new**: create / edit / delete /
  reorder / evaluate / re-scan handlers.
- `src/bin/serve/state.rs` — add a pipeline rule-change activity log + the
  active-rule pointer.
- `src/bin/serve/css.rs` — editor/impact/rule-list classes (port from the
  mockups' inline CSS).
- `src/bin/serve/js.rs` — pipeline keyboard shortcuts (A/E/N/Y/B, Alt+↑/↓).

## Reuse (already built — do NOT reimplement)

- `rules::commit(conn, &Mutation, DumpPolicy::Background{db_path}, Some(&cache))`
  — the single atomic save seam (`src/rules/commit.rs`). Invalidates the
  cache + schedules the `.sql` re-dump. Returns `CommitResult { change,
  dirty_payees, new_id }`.
- `rules::impact::compute_buckets(conn, stage, &Mutation, &payees)` +
  `load_payees` + `Buckets` (`src/rules/impact/`).
- `rules::impact::test_one(conn, stage, &cand, input) -> TestResult`
  (single-string tester).
- `rules::validate::{validate_draft, StageSchema}` — field schema + the
  one validator. The editor form reads `StageSchema::for_stage`.
- `rules::activity::RuleChange::describe(&Mutation, before)` — the
  `+ added` / `~ edited` / `− deleted` / `moved` activity line.
- `rules::dirty::would_restage(conn)` — the "N payees would re-stage" count.
- `rules::crud::{get, list, is_movable, insert_rule, update_rule,
  delete_rule, move_rule}` + `model::{Mutation, RuleData, MoveTarget}`.
- `crate::render::render_page_with_chips` — page shell (`#queue`/`#detail`/
  `#activity`). `freshness::header_chips` for the chips.
- `normalise::scan::scan(conn)` — the re-scan the banner triggers.
- The CLI bucket renderer (`src/bin/rule/render.rs::evaluate`) — reference
  for bucket ordering/labels; the GUI renders HTML per the mockups.
- AppState patterns: `push_txn_activity`, `TabState::push_activity`,
  `pipeline_active` (already present).

## Routes (HTMX fragments, all under `/pipeline/`)

| Method | Path | Returns |
|--------|------|---------|
| GET  | `/pipeline/stage/<slug>` | stage detail (rule list + empty editor) — exists, extend to 2-col |
| GET  | `/pipeline/stage/<slug>/rule/<id>` | editor card in **edit** mode |
| GET  | `/pipeline/stage/<slug>/new` | editor card in **new** mode |
| POST | `/pipeline/stage/<slug>/rule/<id>/evaluate` | editor card in **evaluate** mode (unsaved form posted) |
| POST | `/pipeline/stage/<slug>/new/evaluate` | evaluate a brand-new rule |
| POST | `/pipeline/stage/<slug>/rule` | create (refresh list + card) |
| POST | `/pipeline/stage/<slug>/rule/<id>` | save edit |
| POST | `/pipeline/stage/<slug>/rule/<id>/delete` | delete |
| POST | `/pipeline/stage/<slug>/reorder` | persist new order (loop stages only) |
| POST | `/pipeline/rescan` | run `scan` + clear dirty banner |

Need a POST body parser (tiny_http exposes the body as a reader; existing
routes are all GET/param-based) — add a small `application/x-www-form-urlencoded`
form-decode helper in `helpers.rs`.

## Steps

### PR-A — editor core
- [x] **Form decode**: add a `urlencoded` body parser (tiny_http body
      reader) + a `RuleData` builder from form fields driven by
      `StageSchema`, shared by evaluate/create/edit handlers. Unit-tested
      over prefix + suffix fields (and the other stages' schemas).
- [x] **Editor card** (`editor.rs`): render edit / evaluate / new from a
      per-stage field descriptor (text / select / flag / regex), capture
      flags for prefix+suffix, collapsible note, correct `hx-post` targets
      + field names. Invalid-regex inline `syntax error:` (disables Save).
      Markup-only unit tests.
- [x] **Impact rendering** (`impact.rs`): render `Buckets` (2 buckets for
      the loop prefix/suffix stages; 4 for first-match, ready for later
      stages) + tester result string per the mockup (counts, ≤6 samples,
      "show all", `was: X → Y`). Pure-fn unit tests over fixture buckets.
- [x] **Two-column detail** (`views.rs`): rule list (left) with `[A] Add`
      + selection; editor card (right). Replace the current flat read-only
      table. (Impact column placeholder/omitted until PR-C.)
- [x] **Mutation handlers** (`handlers.rs`): create / edit / delete /
      reorder via `commit(..., DumpPolicy::Background, Some(&cache))`;
      delete uses inline click-to-confirm. Reorder for loop stages only.
- [x] **Keyboard** (`js.rs`): A/E/N/Y/B + Alt+↑/↓ reorder; extend the `?`
      hints overlay for the Pipeline tab.
- [x] **Integration test (PR-A)**: create → list shows it → evaluate shows
      buckets → save → mutation re-dumps `rules/<stage>.sql`; reorder a
      prefix changes pipeline output for an affected payee.

### PR-B — activity log + dirty banner
- [x] **Activity log** (`state.rs` + `views.rs`): a pipeline rule-change
      log (newest-first, capped 100) populated from
      `RuleChange::describe(&Mutation, before)` on every commit; rendered
      in the activity panel with add/edit/delete colour vocabulary.
- [x] **Dirty banner + re-scan** (`views.rs` + `handlers.rs`):
      `⚠ N payees would re-stage · re-scan now ↻` via `would_restage`;
      `/pipeline/rescan` runs `scan::scan` + clears the banner.
- [x] **Integration test (PR-B)**: save logs `+ added`; dirty banner
      appears after an edit; re-scan clears it.

### PR-C — per-rule impact cache
- [x] **Schema + scan**: add `rule_impact` table to `db/schema.rs`; add an
      attribution helper in `impact/` mapping each distinct payee → the
      rule id that won its stage; fold a `rule_impact` refresh into
      `scan::scan`.
- [x] **Impact column** (`views.rs`): `LEFT JOIN rule_impact` for the
      per-row `"412 txns · $8.4k"`; a plain list render never recomputes.
- [x] **Integration test (PR-C)**: scan populates `rule_impact`; the list
      renders the cached number; editing a rule does not change the cached
      number until re-scan.

## Verification

- `cargo test` (library) and `cargo test --features web` (serve) — all green.
- Manual: `cargo run --bin serve`, open `/pipeline/`, pick the prefix
  stage, add/edit/delete/reorder a rule, click Evaluate, Save, watch the
  activity log + dirty banner, click re-scan; confirm `rules/prefixes.sql`
  is re-dumped on disk.
- `git diff rules/*.sql` after edits is human-reviewable (the §6 guarantee).

## Decisions (resolved with reviewer)

1. **Scope** — split the keystone into three PRs (A: editor core; B:
   activity log + dirty banner; C: per-rule impact cache).
2. **First stage wired** — prefix + suffix (loop stages), per the master
   plan; the other six stages follow in their own PRs (master plan §4).
