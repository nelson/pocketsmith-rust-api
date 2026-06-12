# Plan: Editable rules UI — the editor surface + uniform pipeline trace

> Branch base: `master` (PRs 0–8 of v3 merged — every stage is DB-backed
> and fidelity-proven; the Pipeline tab is a working **read-only** rule
> browser). Continues the editable-rules work now that v3's DB conversion
> has landed.
> Status: **in progress — PRs 1, 2, 4, 5 done; the parameterised editor
> (branch `editor-gui-framework`, PR #39) ships the GUI for *all* stages,
> folding in 6–9. Remaining: `pipeline-trace-cli` (3) and
> `transaction-add-rule` (10). See Progress below.**
>
> Mockups (unchanged from v3):
> - [`pipeline-A-merchants.html`](../../mockups/pipeline-A-merchants.html) — edit mode + activity-log add/edit/delete vocabulary
> - [`pipeline-A2-merchants-eval.html`](../../mockups/pipeline-A2-merchants-eval.html) — evaluate mode + four impact buckets
> - [`pipeline-B-prefix.html`](../../mockups/pipeline-B-prefix.html) — loop stage, capture flags, drag-reorder
> - [`pipeline-C-add-from-transaction.html`](../../mockups/pipeline-C-add-from-transaction.html) — "Add rule for this payee"

## 0. Decisions locked in this round (from the planning Q&A)

1. **Editor interaction model = Edit→Evaluate split** (v3 §4.4 stands).
   No live as-you-type recompute. Impact is shown when the user clicks
   `[E] Evaluate`. "Real-time feedback" is delivered as (a) the
   categorical impact buckets on Evaluate, and (b) the activity-log
   add/edit/delete vocabulary that updates on every committed mutation.
2. **Cache/scaffolding cleanup = all three:**
   - retire the `#[cfg(test)]` const oracle + `--features fidelity`
     scaffolding (hard prerequisite — an edited rule makes `const ≠ DB`);
   - wire `RuleCache::invalidate(stage)` into every mutation handler;
   - add `updated_at` auto-update triggers to the eight `rule_*` tables.
3. **Pipeline trace = uniform two-line structure on every stage**, with
   `TraceEntry` extended to capture the matched rule's pattern + match
   span. Lands as its own first PR.
4. **Scope = the entire remaining editor arc** (this document).
5. **Per-rule "Impact" column = built, but cached** — recomputed only on
   re-scan, never on a plain list render.

### Carried-over §14 defaults (no further sign-off sought unless flagged)
- Impact bucket sample size: **6 rows** with a "show all" expander.
- Stage selector: hidden once a rule is saved (re-stage = delete+recreate).
- `heuristic ✓` chip click: semantics-free dismiss.
- Keyboard reorder for loop stages: `Alt+↑/↓` + drag.
- **First-match-wins entity stages (persons / employers / merchants /
  locations) are auto-ordered by a deterministic comparator**, not by a
  manual `sort_order`: case-insensitive alphabetical, **except a longer
  string sorts before any shorter string it contains as a
  prefix/substring** — so the specific `Gamma Radiation Scans` precedes
  the generic `Gamma Rad`, while `Alpha Corp` / `Beta Corp` sort normally:

  ```
  Alpha Corp
  Beta Corp
  Gamma Radiation Scans   ← longer, sorts before its substring variant
  Gamma Rad
  ```

  This single order is **both** the displayed order **and** the apply
  order, so first-match-wins is correct-by-construction. Consequences:
  **no manual `sort_order` column, no drag handles, no append-only hack**
  — a new/edited rule auto-slots into its sorted position. This replaces
  v3 progress Decision #2 (the persons id-order workaround) and the
  merchants "more specific pattern must appear first" hand-ordering
  comment with one rule the code enforces. Implementation: `load_compiled`
  (and `list_display`) load all rows then sort with this comparator in
  Rust (SQL can't express the prefix-length tie-break); the comparator
  lives once in `rules::` and is shared by apply + display. Each pattern
  is expected to match a single payee; any genuine catch-all (e.g. a bare
  title prefix) must be expressed so it doesn't violate that, or carried
  as a documented exception.

---

## 1. The uniform two-line pipeline trace (PR 1)

### 1.1 Goal
Every stage that appears in the trace renders exactly two lines, on the
Transactions detail panel **and** the Normalise tab (they share
`render_pipeline_trace` data — `NormalisationResult`):

- **Line 1 — string transform or match:**
  - Modifying stages (prefix / suffix / expand / banking-op strip /
    empty-fallback): `{before} → {after}` (current behaviour).
  - Non-modifying matcher stages (persons / employers / merchants /
    locations, and banking-op classify-only): `{pattern} ~= {string}`
    where `pattern` is rendered in red, `string` in white, and the
    **matched substring within the string is highlighted green**.
- **Line 2 — extracted features / class:** the existing
  `+entity_name (Woolworths)`, `class = merchant`, `+operation (BPay)`
  list. Unchanged.

Today non-modifying matchers render only line 2 (no line 1) because the
string didn't change and we never recorded which pattern matched or
where. This PR fixes that.

### 1.2 Data-model change
`TraceEntry` gains an optional match descriptor:

```rust
pub struct MatchInfo {
    /// The rule pattern that fired (regex source or literal).
    pub pattern: String,
    /// The string the pattern was tested against (== `before`, but
    /// captured explicitly so the renderer is self-contained).
    pub haystack: String,
    /// Byte range of the matched substring within `haystack`, for the
    /// green highlight. `None` when the match position is not meaningful
    /// (e.g. a literal substring stage chooses to report whole-string).
    pub span: Option<(usize, usize)>,
}
```

**Sanity check on `span` (annotation #2 — "what if the match is snake
eyes?").** Every matcher stage decides via `regex.is_match` /
`regex.find` (or a literal `str::find`), and the **overall** regex match
is always a single contiguous run — even alternations and multiple capture
groups still resolve to one outer match span. So a single `span` fully
describes "the matched substring" for all current patterns. We do *not*
highlight individual capture groups (which could be disjoint). If we ever
want per-group multi-highlight, this becomes `spans: Vec<(usize, usize)>`
— a localised change behind the same `MatchInfo`. Carrying on with the
single-span design.

```rust

pub struct TraceEntry {
    // …existing fields…
    /// Present when a first-match-wins / matcher stage fired. Drives the
    /// `{pattern} ~= {string}` line-1 rendering.
    pub match_info: Option<MatchInfo>,
}
```

### 1.3 Capture mechanism (simplest path, reuses `run_traced`)
- `NormalisationResult` gets a private transient slot
  `pending_match: Option<MatchInfo>`.
- Each matcher's `run_match` sets `result.pending_match = Some(MatchInfo{…})`
  at the point it decides a rule wins (it already has the `Regex` and the
  `&result.normalised` haystack; `regex.find(...)` gives the span; literal
  stages compute the byte offset via `find`).
- `run_traced` (in `mod.rs`) reads `result.pending_match.take()` after the
  stage runs and stores it on the `TraceEntry`. This keeps all trace
  assembly in one place and means stages only set a field.
- Loop stages (prefix/suffix/expand) leave `pending_match = None`; their
  line 1 is the `before → after` diff as today.

### 1.4 Rendering
- Extend `render_trace_entry` so line 1 is:
  - the diff `div.norm-trace-diff` when `before != after` (unchanged), else
  - a new `div.norm-trace-match` rendering
    `{pattern}` (`.norm-trace-pattern`, red) + ` ~= ` +
    the haystack split into `before-span / matched-span (green) / after-span`
    when `match_info` is present.
- A trace entry now always emits a line 1 (diff or match) plus the
  optional line 2. The "stage matched but didn't change anything and has
  no match_info" case cannot occur for the matcher stages once capture is
  in place.
- New CSS classes in `css.rs`: `.norm-trace-match`, `.norm-trace-pattern`,
  `.norm-trace-hay`, `.norm-trace-hay-hit` (green). Mirror existing
  `.norm-trace-*` styling.

### 1.5 Tests (red-green)
- **Unit (mod.rs):** a merchant-class match produces a `TraceEntry` whose
  `match_info` has the right pattern + span; a prefix strip produces
  `match_info == None` and a `before→after` diff. (Use hermetic in-DB
  rules, the established template.)
- **Unit (views):** `render_trace_entry` renders `~=` + a green
  `.norm-trace-hay-hit` span for a matcher entry, and the diff arrow for a
  modifying entry. Markup-only, no HTTP.
- **Unit:** span highlight handles a match that is the whole string, a
  prefix of it, and an interior substring (byte-offset correctness incl.
  a multi-byte UTF-8 payee).

> Note: this PR touches only the **DB path** (`apply_with_db` + the matcher
> `run_match`s). The `#[cfg(test)]` const `apply()` oracle does not write
> `pending_match`; fidelity compares `normalised` + `features_json`, not
> the trace, so PR 1 keeps the fidelity gate green. PR 2 then deletes the
> oracle entirely.

---

## 2. Retire the const oracle + fidelity scaffolding (PR 2)

Hard prerequisite before any mutation can touch the live DB.

### 2.1 Delete
- Each `src/normalise/<stage>.rs`: the `#[cfg(test)] fn apply()`, the
  `#[cfg(test)]` const dictionaries (`MERCHANTS`, `PREFIXES`, `SUFFIXES`,
  `EXPANSIONS`, person/employer/banking/location consts), the
  `compiled_*()` const compilers, and the per-module
  `db_apply_matches_const_oracle` tests.
- `src/normalise/mod.rs`: the `--features fidelity`
  `converted_stages_db_matches_const_on_real_payees` test.
- `Cargo.toml`: the `fidelity` feature **flag is retained** but its
  remaining user becomes only the location-coverage test
  (`location_extraction_coverage_on_real_payees`), which is a
  content-coverage check, not a const oracle. If that test is also judged
  dispensable, drop the feature entirely — flagged as a sub-decision in §8.

### 2.2 Replace coverage
The hermetic per-stage `*_stage_reads_its_rules_from_the_db` tests (already
present for every stage) are the permanent replacement: they prove the
load→compile→apply→capture machinery against rules-defined-in-test,
independent of seed content. No new tests needed; this PR is net-negative
LOC.

### 2.3 `updated_at` triggers
Add a `BEFORE UPDATE` (or `AFTER UPDATE`) trigger per `rule_*` table in
`db/schema.rs` that stamps `updated_at = strftime(...)`, mirroring the
existing `payee_normalisations` trigger. Idempotent (`CREATE TRIGGER IF
NOT EXISTS`). Bump `RULES_SCHEMA_VERSION` only if a column changes — a
trigger addition does not require a re-seed, so the version stays at 1.
- **Test:** `UPDATE rule_merchants SET canonical=… ` bumps `updated_at`;
  an `INSERT` leaves `created_at == updated_at`.

---

## 3. The editor GUI framework — prefix + suffix (branch `editor-gui-framework`)

This is the keystone GUI PR: it builds the reusable Pipeline-tab editor
machinery (which the per-stage branches then parameterise over) and ships
the first stage editor (prefix + suffix). It **consumes the rule-editing
library core** — typed CRUD + `compute_buckets` — already introduced by the
`rule-cli` branch; this PR adds the web layer (editor card, mutation
*handlers*, dirty banner, `rule_impact` cache) on top.

### 3.1 Routes (all under `/pipeline/`, tiny_http, HTMX fragments)
| Method | Path | Returns |
|--------|------|---------|
| GET  | `/pipeline/stage/<slug>/rule/<id>` | editor card in **edit** mode |
| GET  | `/pipeline/stage/<slug>/new` | editor card in **new-rule** mode |
| POST | `/pipeline/stage/<slug>/rule/<id>/evaluate` | editor card in **evaluate** mode (form values posted, not yet saved) |
| POST | `/pipeline/stage/<slug>/rule` | create (returns refreshed list + card) |
| POST | `/pipeline/stage/<slug>/rule/<id>` | save edit |
| POST | `/pipeline/stage/<slug>/rule/<id>/delete` | delete |
| POST | `/pipeline/stage/<slug>/reorder` | persist new `sort_order` (loop stages only) |

Evaluate is a POST (it carries the unsaved form state); Save re-POSTs the
same fields. No field state lives server-side between Evaluate and Save —
the evaluate fragment re-embeds the values as hidden/disabled inputs, so
Save is self-contained (matches the mockup's read-only evaluate form).

### 3.2 New module layout
- `src/bin/serve/pipeline/editor.rs` — the parameterised editor card
  (edit + evaluate + new), shared by all stages. Stage-specific bits
  (which fields, which flags) come from a small `StageSchema` descriptor.
- `src/rules/impact.rs` (**library**, introduced by the `rule-cli`
  branch) — `compute_buckets`, a pure function over
  `(stage, candidate_rule, saved_rules, payees)`, reused as-is here; this
  PR adds only the HTML bucket *rendering* in
  `src/bin/serve/pipeline/impact.rs` (the CLI renders text/JSON instead).
- `src/bin/serve/pipeline/mutations.rs` — create/edit/delete/reorder
  handlers. Each wraps `db::with_operation("rule-edit", …)`, then
  `cache.invalidate(stage)`, then `rules::schedule_dump(stage, db_path)`,
  then pushes a rule-change activity entry (§3.6).
- `src/rules/mod.rs` typed CRUD (`insert_rule`, `update_rule`,
  `delete_rule`, `reorder`) is also introduced by `rule-cli`; the
  handlers above just call it.

### 3.3 `StageSchema` descriptor (one source of truth per stage)
Drives the editor form, the rule-list columns, and the CRUD column set, so
adding a stage editor is "fill in a descriptor + register routes":

```rust
struct StageSchema {
    stage: Stage,
    fields: &'static [Field],     // text | select(options) | flag | regex
    list_columns: &'static [&'static str],
    ordered: bool,                // loop stage → drag handles + reorder
    sets_class: Option<PayeeClass>,
}
```

### 3.4 Edit / Evaluate / New card
Render the three states from the mockups:
- **Edit:** enabled fields, capture-flag checkboxes (prefix/suffix),
  `+ add note` toggle (collapsed unless note present), actions
  `[E] Evaluate · [N] Cancel · Delete`. No Save.
- **Evaluate:** read-only field values, single-string tester
  (`✓ matches → canonical` / `✗ misses` / `syntax error: …`), the four
  impact buckets (§3.5), actions `[Y] Save · [B] Back to edit`. **No
  Delete here** (see below).

> **Delete button — purpose + placement (annotations #3, #4).** `Delete`
> permanently removes an existing rule (vs `Cancel`, which only discards
> unsaved field edits). It is only meaningful for a *saved* rule and only
> needs to exist once, so: **Delete lives in Edit mode only**, removed
> from Evaluate and absent in New-rule mode (nothing to delete yet).
> It stays **mouse-only with an inline click-to-confirm** (`Delete →
> confirm?`) and is deliberately **not** bound to Backspace: the editor
> card is full of focused text inputs where Backspace is the normal
> character-delete key, so binding a destructive, irreversible action to
> it would mis-fire constantly. Destructive actions in this app already
> avoid single-key shortcuts for the same reason (v3 §4.4.1).
- **New:** edit-mode card with a green border + source banner (for the
  PR-10 entry) + stage picker (PR 10); otherwise identical.
- **Invalid regex (v3 §13.5):** evaluate shows `syntax error: …` inline,
  keeps the last valid impact, disables Save until the pattern compiles.

### 3.5 Categorical impact (`compute_buckets`)
Pure function: given the stage, the candidate (edited/new) rule, and the
saved rule set, classify every distinct `original_payee` into one of:
**newly matched** (green), **stolen from another rule** (yellow),
**new fallthrough** (red), **unchanged** (dim). For each payee we run the
stage at its true pipeline position (i.e. on the post-upstream-cleaning
string), so the attribution matches reality. Output: counts + up-to-6
sample payees per bucket (txn count, account, $; "was: X" for stolen).
- **First-match stages** (persons/employers/merchants/banking_ops) use all
  four buckets. **Loop stages** (prefix/suffix/expand) collapse to two —
  **newly affected** / **no longer affected** — since first-match
  attribution doesn't apply (the loop re-feeds output). See §8.3.
- Tested as a pure fn over fixture payees + fixture rule sets (no HTTP).

### 3.6 Activity log — add / edit / delete vocabulary
The activity panel gains a rule-change log (mockup pipeline-A): newest
first, capped at 100, each entry categorised:
- `+ added {canonical} {pattern}` (green)
- `~ edited {canonical} {old-pattern} → {new-pattern}` (neutral)
- `− deleted {canonical} {pattern}` (red)

State lives in `AppState` (a `Vec<RuleChangeEntry>` + helper, mirroring
`push_txn_activity`). This is the "categorise these changes so they are
clear" surface, distinct from the Evaluate buckets.

### 3.7 Per-rule Impact column (cached, re-scan-only)
- New cache table `rule_impact(stage TEXT, rule_id INTEGER, txn_count
  INTEGER, total_cents INTEGER, PRIMARY KEY(stage, rule_id))`, populated
  during `scan::scan` by attributing each distinct payee (with its txn
  count + summed amount) to the rule that won its stage. One extra pass
  folded into the scan the user already triggers.
- The rule list `LEFT JOIN`s `rule_impact` for the per-row
  `"412 txns · $8.4k"`. A plain list render never recomputes — it reads
  the table. Stale-after-edit is acceptable and is exactly what the dirty
  banner (§3.8) signals.
- **Test:** scan populates `rule_impact`; the list view renders the cached
  number; editing a rule does **not** change the cached number until
  re-scan.

### 3.8 Dirty-rules banner + re-scan (v3 §4.5; built here, reused by all)
- Dirty = `MAX(_operations.created_at WHERE reason='rule-edit') >
  MAX(... reason='normalise-scan')`.
- When dirty, the activity panel shows
  `⚠ N payees would re-stage since the last scan · re-scan now ↻`, N
  computed lazily by simulating the pipeline over distinct payees.
- Re-scan runs `scan::scan` (which now also refreshes `rule_impact`) and
  clears the banner.

### 3.9 Keyboard + buttons
`[A]` add, `[E]` evaluate, `[N]` cancel, `[Y]` save, `[B]` back,
`Alt+↑/↓` reorder; `?` overlay gains `A/E/B` when Pipeline is active.
Reuse the existing global key-dispatch + `data-detail-url` HTMX pattern.

### 3.10 Tests (pyramid, red-green)
- **Unit (broad):** `compute_buckets` (≥6 cases incl. stolen + fallthrough
  + invalid regex), editor markup (edit/evaluate/new render correct
  `hx-post` targets + field names), tester result strings, dirty-derivation
  helper, `rule_impact` attribution helper, CRUD column mapping.
- **Integration (medium):** drive handlers directly — create → list shows
  it → evaluate shows buckets → save → activity logs `+ added` → dirty
  banner appears → re-scan → banner clears + `rule_impact` updated;
  reorder a prefix rule → pipeline output changes for an affected payee;
  mutation invalidates the cache slot and re-dumps `src/rules/<stage>.sql`.
- **E2E (narrow):** none new (rule editing is local-only).

---

## 4. Remaining stage editors — parameterised over §3 (PRs 4–7)

Each reuses `editor.rs` / `impact.rs` / `mutations.rs` via a
`StageSchema`; the per-PR work is the descriptor + a hermetic render test +
an integration test. One PR per the v3 "don't overwhelm me" constraint.

| PR | Stage(s) | Notable shape |
|----|----------|---------------|
| 4 (=v3 5b) | `expand` | loop, `pattern → canonical`, drag-reorder |
| 5 (=v3 6b) | `persons` + `employers` + `merchants` | first-match-wins; **auto-ordered by the §0 comparator** (alphabetical, longer-substring-first) for both display and apply; no manual reorder. Switching apply order from id-order to the comparator is a deliberate behaviour refinement — covered by a unit test that `Gamma Radiation Scans` wins over `Gamma Rad`. |
| 6 (=v3 7)  | `locations` | literal list; additive (no string change); editor is the simplest (location + note); same §0 comparator ordering |
| 7 (=v3 8b) | `banking_ops` | first-match-wins grouped by operation; `has_account` flag |

Acceptance per PR: add/edit/delete a rule end-to-end; evaluate shows
buckets; dirty banner + re-scan; dump file rewritten.

---

## 5. Polish + "Add rule for this payee" (PR 8, = v3 PR 10)

- **`guess_stage` heuristic** (v3 §4.7.1): four-line title-prefix rule;
  default merchants. ≥10 unit cases.
- Transactions/Normalise detail, when the active row's trace is
  **completely empty** (PR-0 `Missing`), shows a single
  `[ + Add rule for this payee ]` button.
- Clicking navigates to the Pipeline tab in **new-rule** state (mockup C):
  stage pre-chosen by heuristic (editable), pattern prefilled with the
  regex-escaped post-pipeline string, canonical empty + autofocused,
  source banner with txn id + raw payee + amount + date, `heuristic ✓` chip.
- **Integration test:** click on a trace-empty txn → land in Pipeline new
  state with merchants pre-selected + pattern prefilled.

---

## 6. Schema summary (new in this arc)

```sql
-- per-rule impact cache, refreshed only by scan (§3.7)
CREATE TABLE IF NOT EXISTS rule_impact (
    stage       TEXT    NOT NULL,
    rule_id     INTEGER NOT NULL,
    txn_count   INTEGER NOT NULL DEFAULT 0,
    total_cents INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (stage, rule_id)
);

-- updated_at auto-stamp per rule_* table (§2.3), e.g.:
CREATE TRIGGER IF NOT EXISTS _rule_merchants_touch_updated_at
AFTER UPDATE ON rule_merchants FOR EACH ROW
BEGIN
  UPDATE rule_merchants SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
  WHERE id = NEW.id;
END;
```

No changes to the eight rule tables' columns → `RULES_SCHEMA_VERSION`
stays `1`.

## 7. PR sequence at a glance

Status key: ✅ done · ⏳ next · (blank) not started.

**PRs are identified by branch name, not number** — the *Order* column is
the current plan and can be resequenced freely as long as each row's
*Depends on* is still satisfied. The order below already applies the
requested swap: the headless CLIs (`pipeline-trace-cli`, `rule-cli`) come
**before** the GUI, so the rule-editing **library core** (typed CRUD in
`rules::mod.rs` + `compute_buckets` in `rules::impact.rs`) is introduced
by `rule-cli` and merely *consumed* by `editor-gui-framework`.

| Order | Branch | Status | Scope | Depends on |
|------:|--------|--------|-------|------------|
| 1 | `feat/pipeline-trace-two-line` | ✅ | uniform two-line pipeline trace (matched pattern + span) | — |
| 2 | `remove-const-oracle` | ✅ | retire const oracle + `--features fidelity` scaffolding; `rule_*` `updated_at` triggers | 1 |
| 3 | `pipeline-trace-cli` | ⏳ | headless CLI: print the per-stage pipeline trace for a payee / txn (text + `--json`) | 1 (trace data); slot after 2 |
| 4 | `rule-cli` | ✅ | scriptable rule editing: introduces the **library core** (CRUD + `compute_buckets`) + `--evaluate`/`--apply`/`--json` | 2 (edits must post-date oracle removal) |
| 5 | `editor-gui-framework` | ✅ | Pipeline-tab editor: Edit/Evaluate card, impact buckets, mutation handlers, activity log, dirty banner, `rule_impact` cache — *consumes* the rule-cli library core (PR #39) | 4 |
| 6 | `expand-editor` | ✅ | `pipeline(expand)` editor — delivered via the parameterised editor in PR #39 | 5 |
| 7 | `entity-editor` | ✅ | `pipeline(persons+employers+merchants)` editor + alphabetical (longer-substring-first) ordering — delivered in PR #39 | 5 |
| 8 | `location-editor` | ✅ | `pipeline(locations)` editor — delivered in PR #39 | 5 |
| 9 | `ops-editor` | ✅ | `pipeline(banking_ops)` editor — delivered in PR #39 | 5 |
| 10 | `transaction-add-rule` | ⏳ | `transactions: "+ Add rule for this payee"` + `guess_stage` heuristic | 5 |

Acceptance gates per branch:
- `remove-const-oracle` — net-negative LOC; hermetic per-stage tests still cover; triggers stamp `updated_at`.
- `pipeline-trace-cli` — `normalise trace "<payee>"` prints the two-line trace; `--json` is stable; exit-coded (see §11).
- `rule-cli` — `--evaluate` prints impact buckets and writes nothing; `--apply` commits + dumps `.sql`; invalid regex exits non-zero (see §10).
- `editor-gui-framework` — full create→evaluate→save→dirty→rescan flow tested.
- `expand-editor` / `entity-editor` / `location-editor` / `ops-editor` — add/edit/delete a rule end-to-end for that stage.
- `transaction-add-rule` — click a trace-empty txn → land in Pipeline new-rule state, stage pre-chosen, pattern prefilled.

Every PR: fidelity-style gate (all prior tests pass) + red-green commit
sequence (failing test → pass → refactor) for any PR ≥ 200 LOC.

## 8. Resolved decisions (formerly open questions)

1. **`fidelity` feature after PR 2 → delete it outright.** The flag gates
   two tests today: the const-oracle real-DB comparison (deleted in PR 2)
   and `location_extraction_coverage_on_real_payees` (asserts >4000 real
   payees get a suburb). The second isn't a const oracle — it's a coverage
   assertion — but it only runs when the local `pocketsmith.db` is present
   (skipped in CI / clean checkouts), so its regression value is low and
   it can't gate CI anyway. Given the preference for net-negative LOC, PR 2
   **deletes this test too and removes the `fidelity` feature entirely**.
   The hermetic per-stage tests remain the permanent coverage.
2. **`rule_impact` for non-first-match stages → confirmed.** Loop stages
   (prefix/suffix/expand): a rule's hits = payees whose pass it touched
   (attribute to **every** rule that fired during that payee's pass).
   `locations`: hits = payees whose location it set.
3. **Evaluate impact for loop stages → confirmed.** Loop stages collapse
   the four buckets to two — **newly affected** / **no longer affected** —
   vs. the saved rule. First-match stages keep all four buckets.

No open questions remain. Sign-off → start at PR 1.

## 9. Progress log

### PR 1 — uniform two-line pipeline trace — ✅ done (branch `feat/pipeline-trace-two-line`)

Landed the data-model + capture + shared renderer for the two-line trace:

- `TraceEntry` gained `match_info: Option<MatchInfo { pattern, haystack,
  span }>`, captured via a transient `NormalisationResult.pending_match`
  slot that `run_traced` drains per stage (never leaks across stages).
- Matcher stages (persons / employers / merchants / banking_ops /
  locations) record the firing rule's **authored** pattern + the match
  byte-span. `record_match(pattern, span)` reads the haystack from
  `self.normalised` (the review removed a redundant haystack arg).
- Extracted the duplicated trace renderer from `transactions/views.rs`
  and `normalise/views.rs` into a shared `src/bin/serve/trace.rs`. Line 1
  is the `before → after` diff for modifying stages, else
  `{pattern} ~= {string}` with the matched substring in a green
  `.norm-trace-hay-hit` span; line 2 (features/class) unchanged.
- **Tests (8):** 4 in `normalise/mod.rs` (match-info capture, modifying
  stage has none, multi-byte byte-span correctness, authored-vs-wrapped
  pattern) + 4 in `trace.rs` (diff vs match render, green hit, span-less
  fallback, empty placeholder).
- **Verification:** `cargo test --features web` — 302 lib + 154 serve, 0
  failures; `cargo test --features fidelity --lib` — 304, 0 failures (the
  const oracle is still intact here; its retirement is PR 2). Code review
  passed with no changes requested.

**Next: PR 2** — retire the const oracle + `--features fidelity`
scaffolding and add `rule_*` `updated_at` triggers.

### PRs 2, 4 — const-oracle removal + rule-cli — ✅ done (merged on `master`)

The const oracle + `--features fidelity` scaffolding were retired and the
`rule_*` `updated_at` triggers added; the scriptable `rule` CLI landed the
rule-editing **library core** (typed CRUD, `commit`, `compute_buckets`,
`test_one`, `validate_draft`, `RuleChange`, `dirty::would_restage`) that
the GUI consumes.

### PR 5 (+ 6–9) — editor GUI for all stages — ✅ done (branch `editor-gui-framework`, PR #39)

The Pipeline tab is now a full editable rule editor. Built as three
reviewable sub-PRs then polished over two follow-up rounds; the editor is
**parameterised over every stage**, so the per-stage editors (6–9) are
delivered by this one branch rather than separately.

- **Editor core:** urlencoded form decode + `RuleData` builder; the
  Edit/New/Evaluate card (`editor.rs`); CRUD/reorder handlers over the
  shared `rules::commit` seam; create→evaluate→save→dirty→re-scan flow.
- **Activity log + dirty banner:** rule-change log (add/edit/delete
  vocabulary) in `AppState`; `⚠ N payees would re-stage · re-scan` banner
  via `would_restage`; `/pipeline/rescan`.
- **Per-rule impact cache:** `rule_impact` table, refreshed by `scan`;
  attribution covers **loop stages** too (prefix/suffix/expand record
  fired patterns in the trace; hits summed per rule).
- **Evaluate impact:** CLI-style summary + detail tables, expandable to
  show every affected payee; single-string tester; `moved` bucket label.
- **Safety / UX:** delete routes through an impact preview with a
  mouse-only confirm; pattern editable in evaluate mode with a dynamic
  Save⇄Evaluate toggle; regex token colouring matching the CLI; inline
  ring spinner (no layout shift); compact one-line header; Add button in
  the panel header; split Txns/Value columns; clicking a rule swaps only
  the editor column (no scroll jump) and shows a “payees matching this
  rule” panel (matcher stages from the `payee_normalisations` cache, loop
  stages from the cached base pass).
- **Entity ordering:** persons/employers/merchants/locations are ordered
  alphabetically (longer-substring-first, on each pattern's literal core)
  for **both** display and apply, so first-match is correct-by-
  construction; `dump` stays id-ordered so the `.sql` seeds are unchanged.
- **Performance:** the committed-rules base pipeline pass is cached on
  `AppState` (invalidated on commit/re-scan); first-match evaluate runs
  the scratch pass only over the **affected subset** (payees matching the
  candidate ∪ payees the rule owns). Loop stages still run the full
  scratch pass. A full-scratch oracle backs an equivalence test.
- **Verification:** `cargo test --features web` — 243 lib + 190 serve, 0
  failures, 0 warnings. `rules/*.sql` unchanged; dump round-trip intact.

**Remaining:** `pipeline-trace-cli` (3) — headless trace inspector; and
`transaction-add-rule` (10) — “+ Add rule for this payee” + `guess_stage`.

## 10. Rule-editing CLI — scriptable evaluate + apply (branch `rule-cli`)

The headless, scriptable path to rule editing (evaluate, then confirm),
for scripts, fixtures, and tests. No HTTP. Because it lands **before** the
GUI, this PR **introduces the rule-editing library core** that the GUI
later consumes. Prereq: `remove-const-oracle` (#2) — once rules diverge
from the in-code consts, the fidelity oracle must already be gone.

### 10.1 What it introduces (the library core)
This PR adds the correctness-bearing primitives to the **library**, so
the GUI (and tests) reuse them:
- **Evaluate** → `rules::impact::compute_buckets` (new), a pure fn over
  `(stage, candidate_rule, saved_rules, payees)`. CLI prints; GUI renders.
- **Confirm** → `rules::{insert_rule,update_rule,delete_rule,reorder}`
  (new) wrapped in `db::with_operation("rule-edit", …)`, then a
  **synchronous** `rules::dump_stage` (the process exits, so no
  `schedule_dump` background thread and no `RuleCache::invalidate`).
- Pipeline execution at the rule's true position reuses
  `normalise()` + `PipelineCtx`/`RuleCache` exactly as serve will.

### 10.2 Surface (a subcommand on the existing `normalise` binary)
Two flags realise the two stages; default is evaluate-only (dry-run):
```
normalise rule --stage merchants --pattern '(?i)FOO' --canonical 'Foo' [--note …] [--flags …]
    --evaluate        # default: compute + print impact buckets, write nothing
    --apply           # commit the create/edit; implies a fresh evaluate first
    --json            # machine-readable impact + result (for tests/scripts)
normalise rule --stage merchants --id 42 --delete [--apply]
normalise rule --stage prefixes --id 7 --move-before 3 [--apply]
```
- Without `--apply`, the command is a pure dry-run (the evaluate stage):
  prints the four impact buckets (counts + up-to-6 samples), exits 0;
  invalid regex exits non-zero with `syntax error: …` (matches §3.4).
- With `--apply`, it re-runs evaluate, performs the mutation under
  `with_operation`, re-dumps `src/rules/<stage>.sql`, and prints the
  activity-log line (`+ added` / `~ edited` / `− deleted`, §3.6).
- Re-scan stays explicit: the existing `normalise` (scan) command
  refreshes `payee_normalisations` + `rule_impact`; the CLI just notes
  "N payees would re-stage — run `normalise` to re-scan" (the §3.8 dirty
  signal, headless form).

### 10.3 Testing value
- Gives §3.10's create→evaluate→save→dirty→rescan flow an **HTTP-free**
  end-to-end harness: assert bucket output, then the `rule_*` row, the
  re-dumped `.sql`, and the dirty derivation — driving the same library
  calls the handlers do.
- `--json` output makes golden-file tests of impact trivial and stable.
- The evaluate/apply split is the dry-run/commit safety rail in CLI form.

### 10.4 Cost
~150–200 LOC of presentation + wiring on top of the library core it
introduces (arg parsing + dispatch + text/JSON bucket renderer + the thin
mutation wrapper). The GUI framework (`editor-gui-framework`) then reuses
the same CRUD + `compute_buckets` for its handlers and Evaluate card.

## 11. Pipeline-trace CLI — headless trace inspector (branch `pipeline-trace-cli`)

A scriptable companion to PR 1: print the per-stage normalisation trace
for a payee (or a stored transaction) on the command line, so the
pipeline can be debugged without the web UI. Slots right after
`remove-const-oracle` (#2); its only real dependency is the trace data
(`TraceEntry` / `MatchInfo`) shipped in PR 1.

### 11.1 Reuse
No new pipeline logic: it calls `normalise(payee, &ctx)` and walks the
returned `result.trace`. The two-line shape is exactly what the web
renderer (`serve/trace.rs`) shows — this is the text/JSON projection of
the same `TraceEntry { before, after, match_info, features_added,
feature_values, class_set }`.

### 11.2 Surface (a subcommand on the existing `normalise` binary)
```
normalise trace "<raw payee string>"     # ad-hoc string
normalise trace --txn <id>               # look up original_payee from the DB
    --json                               # machine-readable trace (array of stage entries)
```
Text output mirrors the UI's two lines per stage:
- line 1: `before → after` for modifying stages, else `{pattern} ~= {string}`
  with the matched span marked (e.g. `[…]` around the hit, since there's no
  colour in a pipe);
- line 2: `+feature (value)` / `class = …`.
The final `proposed_payee` (via `format_payee`) is printed at the end.

### 11.3 Testing value
- Golden text/JSON of the trace for a few fixed payees — a fast,
  HTTP-free regression on pipeline *behaviour* and on the trace data
  itself (complements PR 1's render tests, which assert markup).
- `--json` gives scripts a stable contract for “what did the pipeline do
  to this string”.

### 11.4 Cost
~80–120 LOC: arg parsing + a text renderer (share the
matched-span-splitting helper conceptually with `serve/trace.rs`, but the
CLI has its own tiny text formatter) + the optional `--txn` DB lookup. No
library changes beyond what PR 1 already landed.
