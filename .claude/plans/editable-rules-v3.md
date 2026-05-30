# Plan: Editable normalisation rules (v3)

> Branch: `feature/editable-rules` (to be created from `master` after this
> plan is agreed). Supersedes [`editable-rules-v2.md`](./editable-rules-v2.md).
> Status: **plan only — code starts after sign-off.**
>
> Mockups for the new Pipeline tab: [`pipeline-A-merchants.html`](../../mockups/pipeline-A-merchants.html),
> [`pipeline-B-prefix.html`](../../mockups/pipeline-B-prefix.html).
>
> v2 → v3 → v3 (this revision) deltas: see commit history of this file.
> The biggest shape change in this revision is that all 8 stages get
> first-class UI in v3 (no more Tier A / Tier B split), housed on a new
> top-level **Pipeline** tab rather than tucked into Normalise.

## 1. Trigger and goal

The trigger described in the v2 plan has fired: rule edits are happening
weekly, and the user wants to fix a mis-normalisation from the web UI
without re-building. Specifically: "let me add normalisation rules,
merchant, and person names dynamically".

**Goal:** every dictionary that today lives as `const …: &[…] = &[…]`
in `src/normalise/` becomes a row set in SQLite, editable from a new
Pipeline tab. The compiled pipeline keeps doing what it does today;
only the source of its tables changes.

**Non-goal:** scripted/expressive rules. Rules stay declarative: a
pattern, a few flags, a few captures. A persisted rule is the same
shape as a `const Prefix {…}` today.

## 2. What's actually in code today

Eight pipeline stages, in this order ([`src/normalise/mod.rs`](../../src/normalise/mod.rs)):

| # | Stage          | Shape                                          | Count | Order  |
|---|----------------|------------------------------------------------|------:|--------|
| 1 | `prefix`       | regex + optional gateway/operation + captures  |    42 | matters (loop, first-match-per-iter) |
| 2 | `suffix`       | regex + optional gateway/operation/institution + captures | 37 | matters (loop, first-match-per-iter) |
| 3 | `expand`       | regex → canonical (literal substitution)       |   102 | matters (loop, first-match-per-iter) |
| 4 | `persons`      | canonical + literal-substring patterns         |    82 | doesn't matter (alphabetical UI) |
| 5 | `employers`    | canonical + regex patterns                     |     4 | doesn't matter (alphabetical UI) |
| 6 | `merchants`    | regex → canonical, sets `class=Merchant`       |   146 | doesn't matter (alphabetical UI) |
| 7 | `banking_ops`  | regex + operation + optional account capture   |    10 ops × 38 patterns | doesn't matter (grouped by op) |
| — | `locations`    | literal list, used by `suffix`                 |    91 | n/a |

All eight are pure functions today: `fn apply(&mut NormalisationResult)`.
The `merchants` / `banking_ops` / `persons` / `employers` stages also
now stash `last_matched_pattern` for the trace (commit `87f2ba5`).

Three more pieces matter:

- **`run_traced`** in `mod.rs` is the only place that calls a stage. It
  already takes `fn(&mut NormalisationResult)`, snapshots before/after,
  and appends a `TraceEntry`. The DB transition keeps the same
  signature.
- **`scan::scan(conn)`** in `scan.rs` walks every distinct
  `original_payee` in `transactions` and writes proposals into
  `payee_normalisations`. We reuse it verbatim; the change is purely
  inside the per-payee `normalise()` call.
- **`payee_normalisations`** (already a DB table) is the staging buffer
  for proposals. It's the existing data-driven layer. Editable rules
  push the *source* of those proposals into the DB too.

## 3. Scope of v3

**Every stage is editable from the UI.** The previous Tier A / Tier B
split is dropped. Stages do still differ in how their detail panels
look (capture flags, sort-order semantics), but the framework is one
piece of code parameterised over the stage's schema.

### Build order — by stage, not by tier

The user's constraint: "tiers A and B work hand in hand. Some
normalisations only make sense if cleaning was done beforehand". So we
build in the order data flows through the pipeline. Each stage is its
own PR.

1. **Schema + seed-loader infrastructure** (no stage converted yet)
2. **`prefix`** + **`suffix`** (paired — they share the loop shape and
   the capture-flag matrix)
3. **`expand`**
4. **`persons`** + **`employers`** + **`merchants`** (the three
   first-match-wins entity-extraction stages — share the same UI)
5. **`locations`** (small, plain literal list)
6. **`banking_ops`** (last because the others feed into it)

This means we get something usable end-to-end after step 2 (you can
fix prefix/suffix bugs without re-building), and the most valuable
edits — adding merchants/persons — are unblocked at step 4.

### Out of scope

- **`payee_overrides`** and the **promote-override nudge** — both cut
  per user comment. The Y/N/S workflow on `payee_normalisations`
  already gives per-payee control.
- **The Review-tab "Rules" sub-pane** (from the v2 plan). The Pipeline
  tab supersedes it.
- **Version history on rules.** Out. The `_changes` infrastructure can
  capture it later if we want it.
- **Auto-re-scan on rule change.** The activity panel surfaces a
  one-click "re-scan now" link with a count of payees that would be
  re-staged.

### One small Transactions-tab fix in scope

The Transactions tab today renders "no normalisation rule" whenever
`norm_status` is missing, which is wrong: a payee that hit a merchant
rule but had no prefix/suffix to strip still has a non-empty pipeline
trace. The pillar should only flag "no normalisation" when the trace
is **completely empty** (no stage transformed the string and no
features were extracted). Fix as a single commit before the migration
work begins so the new UI text is honest.

## 4. The Pipeline tab

A new top-level tab — fifth in nav order. (Mockups:
[`pipeline-A-merchants.html`](../../mockups/pipeline-A-merchants.html)
for a single-match stage; [`pipeline-B-prefix.html`](../../mockups/pipeline-B-prefix.html)
for the loop-with-captures shape.)

### 4.1 Layout

Same three-pane shell as every other tab.

- **Queue** = one row per pipeline stage in execution order. Each row
  shows the stage number, name, shape (loop / first-match), and
  current rule count. Clicking selects the stage; arrow keys
  navigate.
- **Detail** = the rule editor for the selected stage (described
  below).
- **Activity** = the recent rule-change log for this stage + a
  "re-scan now" link with the count of payees whose proposal would
  change.

Tab order in nav: Dashboard / Transactions / Transfers / Normalise /
**Pipeline**. The Normalise tab keeps its current job — per-payee
proposal review with Y/N/S — and stays simple. No rule-editing UI on
Normalise.

### 4.2 The detail panel: anatomy

Three sections from top to bottom, all server-rendered Maud, all live
inside `#detail`:

1. **Stage header** — name, rule count, shape ("first match wins" /
   "loop, order matters"), one-line description, and a search box.
2. **Rule list** — the rules in the stage's natural order
   (alphabetical for persons/employers/merchants/banking-ops, by
   `sort_order` for prefix/suffix/expand). Click selects a rule.
3. **Focused-rule editor** — fields for the rule's pattern, canonical,
   capture flags (when applicable), `sort_order` (when applicable),
   and an editable `note`. Y / N / S = save / cancel / delete.
4. **Tester + impact preview** — described in §4.3.

The choice between "list of regex patterns" and "more interactive"
goes hard towards interactive. The mockups show:

- **Regex tester.** Paste a candidate raw string, see `✓ matches → canonical: …` or `✗ misses` *as you type*. For loop-shape stages
  (prefix/suffix/expand) the tester runs the full stage loop and
  shows the per-iteration trace, marking the iteration where the
  edited rule fires.
- **Impact bars.** Three numbers, all as horizontal bars over the
  same denominator (= count of distinct `original_payee` in the DB):
  - **Hit count** — how many raw payees this rule's pattern matches
    today.
  - **+N newly matched** — payees the *edited* pattern catches that
    the saved version doesn't.
  - **−N no longer matched** — payees that fall through after the
    edit.
- **Sample matches.** First ~5 of each (kept, newly added, newly
  removed), each with the txn count and account so the user can
  judge "is this a real win?". Loop stages show what the rule
  would extract as features.
- **Conflict / overlap detection.** For first-match-wins stages, list
  any payee the edited rule would catch that's currently caught by a
  *different* rule in the same stage. The card explains the
  alphabetical-order tie-break and how to resolve it (rename or
  tighten one of the patterns). For loop stages, conflict detection
  is omitted — multiple rules in the same iteration can compose
  legitimately.

The tester reuses `cache.merchants(conn)` (etc.) — same cache the
production pipeline uses, just temporarily overlaid with the in-flight
edit so the preview matches what would happen on save. Implementation:
build a one-shot `RuleSet` value that the regex compiler can take, run
it against the cached set of distinct `original_payee` strings, return
the diff. ≈ 100 ms for a 423-payee × ~150-rule stage on the dev
machine.

### 4.3 What "conflict" means precisely

- **First-match-wins stages.** Two rules conflict when they both match
  some `original_payee`. The earlier-sorted rule wins. Surface the
  conflicting payees and the winning rule's name.
- **Loop stages.** No conflict concept. Reorder freely; the loop runs
  to fixed point either way for most realistic edits. The mockup
  surfaces a "heads up" note instead of a conflict card.
- **Cross-stage** conflicts (e.g. a person-stage rule shadowing a
  merchant-stage rule) are not surfaced. The pipeline order is fixed
  and the reviewer can see in the trace which stage caught a payee.

### 4.4 The "no rule matched" affordance from Normalise / Transactions

When the user is on a Transactions or Normalise detail panel and the
active row's pipeline trace shows no entity_name was extracted, the
detail panel shows a single button:

```
[ + Add rule for this payee ]
```

No three-button picker; no modal. Clicking it navigates to the
Pipeline tab with the appropriate stage pre-selected and the rule
editor pre-filled. The stage is auto-chosen by these heuristics, in
order:

1. If the post-pipeline string is a single recognisable name
   (alphabetic-only, ≤ 4 words), default to **persons**.
2. Else if it contains common merchant-y tokens (`PTY`, `LTD`, `INC`,
   `LLC`, all-caps short tokens, digits) — default to **merchants**.
3. Else default to **merchants** anyway.

The user can change the stage in the editor before saving (a small
stage selector at the top of the rule-editor card) — the heuristic is
a starting point, not a lock-in. This keeps the affordance "one
button" without forcing the wrong stage.

If the heuristic is contentious, an alternative is a single
`[+ Add rule]` button with a stage selector in the URL fragment
(`/pipeline/?stage=merchants&prefill=AMAZON+MARKETPLACE`); same
single-button feel, lets the user override before any DB write
happens.

## 5. Schema

Eight tables. All have `note TEXT`, `created_at`, `updated_at`. Stages
where order matters add `sort_order INTEGER NOT NULL`. `sort_order` is
editable from the UI for those stages.

```sql
-- ===== entity-extraction (alphabetical, order doesn't matter) =====
CREATE TABLE rule_persons (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    canonical   TEXT NOT NULL,
    pattern     TEXT NOT NULL,              -- literal substring (case-insensitive)
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (canonical, pattern)
);

CREATE TABLE rule_merchants (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    canonical   TEXT NOT NULL,
    pattern     TEXT NOT NULL,              -- regex source
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (pattern)
);

CREATE TABLE rule_employers (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    canonical   TEXT NOT NULL,
    pattern     TEXT NOT NULL,              -- regex source
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (pattern)
);

-- ===== loop stages (sort_order matters) =====
CREATE TABLE rule_prefixes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern     TEXT NOT NULL UNIQUE,       -- regex source
    gateway     TEXT,
    operation   TEXT,                       -- BankingOperation::display_name() or NULL
    has_account INTEGER NOT NULL DEFAULT 0,
    has_date    INTEGER NOT NULL DEFAULT 0,
    note        TEXT,
    sort_order  INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE rule_suffixes (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern           TEXT NOT NULL UNIQUE,
    gateway           TEXT,
    operation         TEXT,
    institution       TEXT,
    has_account       INTEGER NOT NULL DEFAULT 0,
    has_date          INTEGER NOT NULL DEFAULT 0,
    has_location      INTEGER NOT NULL DEFAULT 0,
    has_currency_code INTEGER NOT NULL DEFAULT 0,
    has_amount        INTEGER NOT NULL DEFAULT 0,
    note              TEXT,
    sort_order        INTEGER NOT NULL,
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE rule_expansions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern     TEXT NOT NULL UNIQUE,       -- regex source
    canonical   TEXT NOT NULL,
    note        TEXT,
    sort_order  INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- ===== first-match-wins, grouped by op =====
CREATE TABLE rule_banking_ops (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    operation   TEXT NOT NULL,              -- BankingOperation::display_name()
    pattern     TEXT NOT NULL,              -- regex source
    has_account INTEGER NOT NULL DEFAULT 0,
    note        TEXT,
    sort_order  INTEGER NOT NULL,           -- preserves "patterns within an op" order
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (operation, pattern)
);

-- ===== aux =====
CREATE TABLE rule_locations (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    location    TEXT NOT NULL UNIQUE,       -- "NORTH STRATHFIELD"
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
```

Notes:

- `note` is editable from the UI for all stages.
- `sort_order` is editable for prefix / suffix / expand / banking_ops.
  Insertion appends with `MAX(sort_order)+1`; explicit edits update.
- For persons / merchants / employers, the UI shows alphabetical
  order. The underlying iteration order is still "first-match wins
  in the order returned by the SELECT", so the SQL is
  `ORDER BY canonical COLLATE NOCASE, id`. New rules slot in
  alphabetically; the user is not asked to think about iteration
  order.
- No `_changes` audit table. Mutations write through `with_operation`
  with a fresh `reason` ("rule-edit"); the row history rolls up via
  `updated_at` for the UI's "edited 3d ago" stamp.

## 6. The `rules` module — file-system canonical store

User raised: "We need the serialised set because the database may be
blown away from time to time, but we don't ever want to lose the
rules. The canonical rules will be written to
`src/rules/[pipeline-stage].sql`."

**Verdict: yes, do it. It's the right size for what's needed.**
Argument:

- Without it, the rule store is "the SQLite file on this machine",
  which is exactly the failure mode the user named.
- A SQL file per stage is the smallest representation that's both
  human-readable (so git diffs are reviewable) and machine-readable
  (so the seed loader is a `conn.execute_batch(&fs::read_to_string(...))`).
- Stage-per-file scopes diffs to "I edited merchants this week" and
  keeps each file ≤ 200 lines today, ≤ 1000 in any plausible future.
- Deterministic dump (sort by canonical / sort_order; pretty-print
  with one row per line) means the on-disk diff is meaningful and
  not just "everything moved by one byte".

But I'd refine the lifecycle the user proposed. Re-dumping on serve
*close* is fragile — process kills, panics, OOMs all skip the cleanup
path, and rule edits silently disappear from disk. Instead:

```
on serve startup:
    if rule tables are empty (or schema-version bumped):
        load src/rules/*.sql into the rule tables.
    cache.bump()

on every rule mutation (create / edit / delete / reorder):
    write to DB inside with_operation("rule-edit", …)
    after the transaction commits:
        re-dump *that stage's* SQL file to src/rules/<stage>.sql
        cache.bump_for(stage)

on serve shutdown:
    nothing special — disk is already up to date.
```

This makes the SQL files the always-current canonical form,
cheap to git-diff, and safe against ungraceful exit. The dump cost is
trivial (one stage = one SELECT + a couple hundred `INSERT` lines, ≈
1 ms) and runs in a background task so it doesn't block the HTTP
response.

A new module `src/rules/mod.rs` exposes:

- `pub fn load_into_db(conn: &Connection) -> Result<()>` — runs once
  on serve startup if the rule tables are empty.
- `pub fn dump_stage(conn: &Connection, stage: Stage) -> Result<()>`
  — fires after a mutation. Idempotent.
- `pub fn dump_all(conn: &Connection) -> Result<()>` — for a one-shot
  CLI binary used during the initial seed bootstrap.

Schema-versioning: a `schema_version` row in `_operations` (or a new
`_meta` k/v table) lets us re-load when the in-tree SQL files
introduce a new column. Only triggers a load when the on-disk version
> the DB's stored version.

(One caveat: `serve` is the only writer in our deployment model. If
the user ever runs `cargo run --bin normalise` while serve is also
running, the SQL dump can race. We sidestep this by having the CLI
binary read-only when it comes to rule tables: only `serve` mutates
them.)

## 7. Caching

User asked: "why is the generation counter needed?".

You're right — it isn't. The simpler design works:

```rust
pub struct RuleCache {
    persons:     RwLock<Option<Arc<Vec<CompiledPerson>>>>,
    merchants:   RwLock<Option<Arc<Vec<CompiledMerchant>>>>,
    employers:   RwLock<Option<Arc<Vec<CompiledEmployer>>>>,
    prefixes:    RwLock<Option<Arc<Vec<CompiledPrefix>>>>,
    suffixes:    RwLock<Option<Arc<Vec<CompiledSuffix>>>>,
    expansions:  RwLock<Option<Arc<Vec<CompiledExpansion>>>>,
    banking_ops: RwLock<Option<Arc<Vec<CompiledBankingOp>>>>,
    locations:   RwLock<Option<Arc<Vec<String>>>>,
}

impl RuleCache {
    pub fn merchants(&self, conn: &Connection) -> Result<Arc<Vec<CompiledMerchant>>> {
        // Read lock → Some(arc) → return clone of arc.
        // None → upgrade to write lock, SELECT + compile + store.
    }
    pub fn invalidate(&self, stage: Stage) {
        match stage { Stage::Merchants => *self.merchants.write() = None, … }
    }
}
```

Per-stage invalidation, no generation counter. A rule edit on
merchants only invalidates `merchants`; the next read recompiles just
that stage's regex set. No global lock-step.

The original generation-counter design assumed concurrent readers and
writers in flight, which was overkill — `serve` is single-threaded
today and even if it weren't, an `Arc<Vec<…>>` swap is the right
primitive, not a counter.

## 8. Pipeline integration

Each stage gains a sibling `apply_with_db(result, conn, cache)`. The
pure-function `apply()` is kept temporarily as a shim so a partial
migration compiles, but is deleted at the end of the stage's PR.

`normalise()` (the public entry point) changes from:

```rust
pub fn normalise(original: &str) -> NormalisationResult { … }
```

to:

```rust
pub fn normalise(original: &str, ctx: &PipelineCtx) -> NormalisationResult { … }
```

`PipelineCtx` bundles `&Connection + &RuleCache`. Call sites (≈ 30,
mostly tests) get touched.

For tests, `PipelineCtx::with_seeded_in_memory()` constructs an
in-memory DB and runs the seed. Hot tests that don't care about rules
keep using a process-wide `OnceLock<PipelineCtx>` so they share a
seeded DB and don't pay setup cost per test.

## 9. Persisting `matched_rule_id` on proposals — **deferred**

User pushback: "Why is this needed? What would be the consequence of
not doing it. If possible I would like to defer to limit scope."

I'm convinced. Cutting it from v3.

The only thing it bought was the "edit / delete this rule" link from
a Transactions detail panel ("the rule that gave me this payee →
fix it"). Without `matched_rule_id`, you reach the same outcome via:

- The pipeline trace already shows which stage and which pattern
  fired for the active row (commit `87f2ba5`).
- Click-through from the Transactions detail to the Pipeline tab
  could jump to the matched stage with the matched pattern
  pre-selected (string-match the pattern in the stage's rule list).
- "Re-apply this stage's rules to the affected payees" is just a
  full re-scan, which already exists.

The schema change was also genuinely confusing — each rule table has
its own auto-increment id, so `matched_rule_id INTEGER` without
`matched_stage TEXT` is ambiguous, and `matched_stage` doesn't help
because multiple stages contribute to a single proposal (a payee can
be processed by prefix → expand → merchants in the same pipeline
run, all of which "matter"). Storing one stage's rule id throws away
everything the other stages did.

If we ever want this, the right schema is a separate
`payee_normalisation_rule_hits (proposal_id, stage, rule_id)` row
*per stage that fired*, not a column on the proposal. That's a v4
problem.

## 10. Build order — one PR per step

User asked: "one stage one PR; or multiple stages one PR per stage?
I would like to do code reviews and don't want to be overwhelmed by
very long PRs. I also want to do user acceptance testing at
appropriate times."

Multi-PR. One PR per step below. Each PR is independently reviewable
and acceptance-testable. Steps 1–3 are the "infrastructure" PRs
(small, mechanical, no UI change); steps 4–10 are the "stage
conversion + UI" PRs (each adds behaviour you can poke at). PR
sizes are deliberately bounded.

| # | PR title | Touches | Acceptance test |
|---|----------|---------|-----------------|
| 0 | `transactions: only flag "no normalisation" when trace is empty` | Transactions tab views | Verify the pillar text changes for merchant-only payees. |
| 1 | `db+rules: schema for 8 rule tables + src/rules/*.sql seed loader + dump_all CLI` | new `src/rules/` module, schema migration, `dump_rules` binary | `cargo run --bin dump_rules` writes 8 SQL files; fresh DB seeds correctly; `pocketsmith.db` already-populated DB retains its data. |
| 2 | `normalise: PipelineCtx + RuleCache (no behaviour change yet)` | `normalise::cache`, `PipelineCtx`, every call site | All existing tests pass; pipeline still uses in-code constants. |
| 3 | `serve: Pipeline tab shell (queue + empty detail + activity panel)` | new `serve/pipeline/` module, route table, nav | Tab visible; queue lists 8 stages; clicking selects one. |
| 4 | `pipeline(prefix+suffix): convert to DB; tab UI for these two stages` | `prefix.rs`, `suffix.rs`, `pipeline/views.rs` | Edit a prefix rule, see impact preview, save, re-scan, see proposals change. |
| 5 | `pipeline(expand): convert + UI` | `expand.rs`, `pipeline/views.rs` | Same drill for expand. |
| 6 | `pipeline(persons+employers+merchants): convert + UI for the three first-match-wins stages` | three `*.rs` files, shared UI partial | Add a merchant rule from the empty-state Transactions affordance, see it land. |
| 7 | `pipeline(locations): convert + UI` | `locations.rs`, `suffix.rs` (uses it), `pipeline/views.rs` | Add/remove a suburb, see its effect on suffix matching. |
| 8 | `pipeline(banking_ops): convert + UI` | `banking_ops.rs`, `pipeline/views.rs` | Same. |
| 9 | `pipeline: re-scan banner + sample-impact preview + conflict detection` | `pipeline/views.rs`, `scan::rescan_for_stage` | Edit a rule, banner shows N affected, click re-scan, banner clears. |
| 10 | `transactions: "+ Add rule for this payee" affordance + heuristic stage selector` | Transactions detail | The single-button no-modal flow described in §4.4. |

Each PR has a fidelity-test gate: every `cargo test` from before the
PR still passes after it. Steps 4–8 also gate on a "production-DB
fidelity" test (`cargo run --bin normalise --features fidelity-check`)
that asserts every distinct `original_payee` produces the same
`payee_normalisations` row before vs. after the conversion.

Estimated cost: ~5–6 days total spread across 11 PRs. Each PR is ≤ 1
day of work; most are half a day.

## 11. Test strategy — pyramid

User constraint: "Use test pyramid strategy. Rely on a large volume
of unit tests that runs quickly and with few dependencies. Use a
medium volume of integration tests that validates user flows and
functionality across modules, potentially using mocks. Use a small
number of end-to-end tests that hits the real API."

Concretely:

### 11.1 Unit (the broad base) — fast, isolated, lots of them

- One test per stage's `apply_with_db` against a hand-written
  fixture set of rules + a single input payee. Asserts string + features +
  class. No DB beyond a `:memory:` SQLite with the schema.
- One test per `RuleCache::<stage>` for the load / invalidate /
  reload contract.
- One test per `dump_stage` round-trip: insert rows → dump → re-load
  into a fresh DB → assert table contents bitwise equal.
- Pipeline-tab view tests in the same shape as the existing
  serve smoke tests: Markup-only assertions, no HTTP, no real DB.
- Conflict-detection / impact-preview helpers tested as pure
  functions over a fixture rule set + a fixture payee corpus.

### 11.2 Integration (the medium tier) — user flows, multi-module

- `serve` integration tests that walk a fixture DB through:
  - Open Pipeline tab → click stage → select rule → edit pattern
    → save → re-scan → assert proposal count changed.
  - Add merchant rule from Transactions empty-state → assert rule
    landed in DB and dumped to `src/rules/merchants.sql`.
  - Reorder a prefix rule via the drag handle (POST endpoint) →
    assert pipeline output changes for an affected payee.
- The big fidelity test: `cargo test --features fidelity --test
  pipeline_fidelity` runs the full pipeline against every distinct
  `original_payee` in `pocketsmith.db` (skipped if no real DB
  present), asserts `(proposed_payee, class, features_json)` matches
  a snapshot taken before the migration. This is the gate for
  deleting the in-code constants and is run manually before each
  stage-conversion PR ships.

### 11.3 End-to-end (the narrow top) — few, real, slow

- Already covered: `cargo run --bin sync` against the real
  PocketSmith API. We don't add new e2e tests for editable rules —
  rule editing is a local-only concern.

### 11.4 Red-green TDD discipline per PR

User constraint: "make use of red green TDD in each commit."

For every PR:

1. Write the smallest failing unit test that captures the new
   behaviour.
2. Implement just enough to make it pass.
3. Refactor for clarity / consistency with existing modules; tests
   still pass.

For PR sizes ≥ 200 LOC, the commit log within the PR shows that
red-green sequence (commit per failing test → commit making it pass
→ commit refactoring). Reviewer can step through.

Naming + structure consistency across the codebase:

- Module shape: `src/normalise/<stage>.rs` keeps owning the stage's
  pipeline function. Rule-table SQL helpers go in
  `src/rules/<stage>.rs` (loader + dumper + Compiled type). The
  Pipeline tab views go in `src/bin/serve/pipeline/<stage>.rs` only
  if the stage's UI is meaningfully different from the framework
  default — most stages will share `pipeline/views.rs`.
- Function names: `<stage>::apply_with_db`, `<stage>::compiled`,
  `rules::<stage>::load`, `rules::<stage>::dump`. Mirrors the
  existing `<stage>::apply` convention.

## 12. Cost estimate

- PR 0 (Transactions tab fix): half a day.
- PR 1–3 (infra): 1.5 days.
- PR 4–8 (stage conversions, including UI per stage): 3 days.
- PR 9–10 (polish + Transactions affordance): 1 day.

Total: ~5–6 days. Distributed across ~2 weeks of evening work would
be a comfortable pace; "intensive" mode would compress to about a
week.

## 13. Open questions (post-revision)

The following user comments were unanswered or need confirmation:

1. **Re-scan scope.** §13.3 of the v2 plan was left blank. Two options:
   (a) re-scan all payees on every rule change (simple, ~100 ms);
   (b) re-scan only payees that *might* be affected (the ones whose
   pipeline trace touches the edited stage). Recommend (a) unless
   re-scan time becomes a bottleneck — and at 100 ms it won't, even
   on a 20 000-payee DB. **Confirm: (a) or (b)?**

2. **Pipeline tab nav order.** Plan currently says fifth position
   (Dashboard / Transactions / Transfers / Normalise / Pipeline).
   Alternative: fourth, between Normalise and Transfers, since
   Pipeline is "rules that produce Normalise's data". Or first,
   right after Dashboard, if you reach for it more than once a day.
   **Where should it sit?**

3. **Auto-dumping to `src/rules/*.sql` after every edit.** §6
   recommends post-commit dump (not on shutdown). This means every
   click in the Pipeline tab triggers a file write. Is this OK for
   your dev environment, or would you prefer batched dumps (e.g.
   debounce 5s after the last edit, or explicit "save to disk"
   button)? **Recommend post-commit per-stage; flag if you'd
   prefer otherwise.**

4. **Single "Add rule" button: heuristic vs explicit picker (§4.4).**
   Two designs: heuristic-with-override, or a single button that
   navigates to a "pick a stage" sub-view of the Pipeline tab.
   Mockup-A doesn't show this flow — should I make a third mockup
   that compares the two? **Heuristic, picker, or third option?**

5. **Live impact preview cost.** The mockup shows "as you type"
   recomputation. On a 423-payee × 150-rule stage that's ~1 ms with
   precompiled regex — negligible. On a stage where the user is
   editing a regex that doesn't compile (transient state), the
   preview should show "syntax error: …" and not fail loudly.
   **Confirm UX for invalid-regex states.**

6. **Conflict detection scope.** §4.3 limits it to same-stage,
   first-match-wins stages. **Want any cross-stage warnings?** (E.g.
   a new merchant rule whose pattern is a strict superset of an
   existing person rule's pattern → "this merchant rule will steal 7
   payees from the persons stage".) Recommendation: defer.

Sign-off on these → start at PR 0.
