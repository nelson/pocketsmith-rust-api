# Plan: Editable normalisation rules (v3)

> Branch: `feature/editable-rules` (to be created from `master` after this
> plan is agreed). Supersedes [`editable-rules-v2.md`](./editable-rules-v2.md),
> which predates several pipeline additions (persons/employers split,
> banking_ops, TraceEntry plumbing, the matched-pattern feature).
> Status: **plan only — code starts after sign-off.**

## 1. Trigger and goal

The trigger described in the v2 plan has fired: rule edits are happening
weekly, and the user wants to fix a mis-normalisation from the web UI
without re-building. Specifically: "let me add normalisation rules,
merchant, and person names dynamically".

**Goal:** every dictionary that today lives as `const …: &[…] = &[…]`
in `src/normalise/` becomes a row set in SQLite, editable from the
Normalise tab. The compiled pipeline keeps doing what it does today;
only the source of its tables changes.

**Non-goal:** scripted/expressive rules (Option D from the old v1
plan). Rules stay declarative: a pattern, a few flags, a few captures.
A persisted rule is the same shape as a `const Prefix {…}` today.

## 2. What's actually in code today

Eight pipeline stages, in this order ([`src/normalise/mod.rs`](../../src/normalise/mod.rs)):

| # | Stage          | Shape                                          | Count |
|---|----------------|------------------------------------------------|------:|
| 1 | `prefix`       | regex + optional gateway/operation + captures  |   42  |
| 2 | `suffix`       | regex + optional gateway/operation/institution + captures | 37 |
| 3 | `expand`       | regex → canonical (literal string substitution)|  102  |
| 4 | `persons`      | canonical + list of literal patterns           |   82  |
| 5 | `employers`    | canonical + list of regex patterns             |    4  |
| 6 | `merchants`    | regex → canonical, sets class=Merchant         |  146  |
| 7 | `banking_ops`  | regex + operation + optional account capture   |   10 (×N patterns each) |
| 8 | empty-fallback | code, not data                                 |    — |

All eight are pure functions today: `fn apply(&mut NormalisationResult)`.
The `merchants` / `banking_ops` / `persons` / `employers` stages also
now stash `last_matched_pattern` for the trace (commit `87f2ba5`).

Three more pieces matter:

- **`run_traced`** in `mod.rs` is the only place that calls a stage. It
  already takes `fn(&mut NormalisationResult)`, snapshots before/after,
  and appends a `TraceEntry`. The DB transition lets us keep the same
  signature.
- **`scan::scan(conn)`** in `scan.rs` is the entry point that walks
  every distinct `original_payee` in `transactions` and writes
  proposals into `payee_normalisations`. This is what `cargo run --bin
  normalise` calls. We will reuse it verbatim — the change is purely
  inside the per-payee `normalise()` call.
- **`payee_normalisations`** (already a DB table) is the staging buffer
  for proposals. It's the existing data-driven layer. Editable rules
  just push the *source* of those proposals into the DB too.

## 3. Scope of v3

We split the dictionaries into two tiers:

> Comment: tiers A and B work hand in hand. Some normalisations only make sense if cleaning was
> done beforehand, so we will need both implemented for it to be useful. So please build it in the
> following order: prefix+suffix, expand, persons+merchants+employers, locations, banking_ops

### Tier A — first-class editable from the UI

The three dictionaries the user named:

1. **`persons`** — list of `(canonical, pattern)` literal-substring rows.
2. **`merchants`** — list of `(canonical, regex_pattern)` rows.
3. **`employers`** — list of `(canonical, regex_pattern)` rows.

These are the highest-value because (a) they're add-only — most edits
are "I keep seeing CAFE FOO, please call it Cafe Foo", and (b) their
shape is trivial: a name and a pattern.

### Tier B — moved to DB but only seeded; UI editing deferred

The other five dictionaries:

4. **`prefix`** — 42 rows. Editable in v4 (the flag/capture matrix is wider).
5. **`suffix`** — 37 rows. Editable in v4.
6. **`expand`** — 102 rows. Editable in v4 (literal-pair, but very dense).
7. **`banking_ops`** — 10 ops × N patterns. Editable in v4.
8. **`locations`** (used by `suffix`) — 91-ish suburbs. Editable in v4.

Tier B still moves to the DB in this branch so the seed/migration
machinery only lands once. But the UI in v3 only exposes Tier A; Tier
B remains "edit the seed SQL or insert directly".

Rationale: Tier A covers 232 of 423 dictionary entries (≈ 55%) and is
where the user actually wants to act. The non-trivial UI work (regex
editor, capture-group help text, "test this rule" preview) is paid
once for Tier A and reused for Tier B in v4.

> Comment: the UI currently states "no normalisation" for transactions that don't get any prefix
> or suffix processing, but has a merchant identified. That's not quite accurate. We want to only
> flag "no normalisation rule" if the pipeline trace is completely empty

### Out of scope for v3 (deferred or rejected)

- **`payee_overrides`** (the per-`original_payee` escape hatch from
  v2 plan §3.1). Skip. The user already has per-payee control via
  `payee_normalisations.proposed_payee` and the Y/N/S workflow.
  Override patterns are the wrong unit anyway — if you keep wanting
  to override one specific raw string, that's evidence you want a
  *rule*, which is what this whole branch is about.
- **The "promote override → rule" nudge** (v2 plan §3.5). Falls out
  with payee_overrides.
- **The Review-tab "Rules" sub-pane** (v2 plan §3.4). The Review tab
  itself isn't built yet. v3 puts the rule UI on the **Normalise** tab
  (which already has the Y/N/S workflow over per-payee proposals), as
  a new sub-mode reachable from the active row.
- **Version history on rules.** Out. Adds schema overhead with no
  immediate payoff; the `_changes` infrastructure can capture it
  later if we want.
- **Auto-re-scan on rule change.** Not automatic. After adding a rule
  the user clicks a "re-scan" button (or runs `cargo run --bin
  normalise`). v3 doesn't try to invalidate-and-rebuild on every edit
  — too easy to make the UI feel laggy. The button is fast (≈ 100ms
  on the current ~2000-payee DB).

> Comment: Delete payee_overrides, promote override from the plan.
> Furthermore, v3 rule UI should not be done in the Normalise tab. It should be a new tab called
> "Pipeline". The queue will now have one entry for each stage of the normalisation pipeline,
> organised in chronological order. Clicking the queue item will bring up the detail panel for
> that pipeline stage. The detail panel is where patterns can be added, deleted, and modified.
> Provide mockups for this pipeline tab. I'm interested in the details panel. Should it simply
> show a list of regex patterns, or should it be more interactive - showing the impact of each
> modified pattern in real time. What's the best way to visualise this?  It would start with a
> regex tester, but also should show how many transactions it would impact, what the impact looks
> like, and whether there are any conflicting / overlapping patterns

## 4. Schema

Five tables, all with the same `_changes` / `with_operation` discipline
the rest of the schema uses. Timestamps are SQLite's
`strftime('%Y-%m-%dT%H:%M:%fZ','now')` to match existing conventions.

> Comment: `note` and `sort_order` correct. Ensure note is editable. Allow Pipeline tab to modify
> sort order for stages that have it. For person, merchants, and employers - use alphabetical
> order

```sql
-- Tier A
CREATE TABLE rule_persons (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    canonical   TEXT NOT NULL,
    pattern     TEXT NOT NULL,              -- literal substring (case-insensitive)
    note        TEXT,                       -- optional human note
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

-- Tier B (seeded only; no UI in v3)
CREATE TABLE rule_prefixes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern     TEXT NOT NULL UNIQUE,       -- regex source
    gateway     TEXT,                       -- optional
    operation   TEXT,                       -- one of BankingOperation::display_name() or NULL
    has_account INTEGER NOT NULL DEFAULT 0, -- 0/1
    has_date    INTEGER NOT NULL DEFAULT 0,
    sort_order  INTEGER NOT NULL,           -- preserves the existing in-code order
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
    sort_order        INTEGER NOT NULL,
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE rule_expansions (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern     TEXT NOT NULL UNIQUE,       -- regex source
    canonical   TEXT NOT NULL,              -- literal replacement
    sort_order  INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE rule_banking_ops (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    operation   TEXT NOT NULL,              -- BankingOperation::display_name()
    pattern     TEXT NOT NULL,              -- regex source
    has_account INTEGER NOT NULL DEFAULT 0,
    sort_order  INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (operation, pattern)
);

CREATE TABLE rule_locations (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    location    TEXT NOT NULL UNIQUE,       -- e.g. "NORTH STRATHFIELD"
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
```

Notes:

- `sort_order` lets the seeded data preserve the existing in-code order
  exactly. New rows the UI inserts get `MAX(sort_order)+1`, which puts
  them at the end. Order matters for `prefix`/`suffix`/`expand` where
  multiple patterns can match and "first wins".
- `persons` / `merchants` / `employers` order is **not** semantically
  meaningful — the existing code iterates and "first match wins" but
  the rules don't overlap, so we sort by `canonical` in the UI for
  scanability.
- Every table has `(created_at, updated_at)` so the UI can show "added
  3 days ago" / "edited yesterday" without a separate audit table.

## 5. Loading + caching

Compile-time pattern cost today is "compile once via `OnceLock`,
amortise across the whole process lifetime". We can't quite do that
post-migration (rules change while the server runs), but the cost we
actually want is:

- One `SELECT` per stage per pipeline-run is fine (cheap).
- Re-compiling 100+ regexes on every transaction is not (≈ 1 ms
  per regex × 423 rules × 2000 payees = 14 minutes).

So: **lazy + invalidatable cache** keyed on a generation counter.

```rust
// In normalise::cache (new module)
pub struct RuleCache {
    generation: AtomicU64,
    persons: RwLock<Option<(u64, Arc<Vec<CompiledPerson>>)>>,
    merchants: RwLock<Option<(u64, Arc<Vec<CompiledMerchant>>)>>,
    // … one slot per table.
}

impl RuleCache {
    pub fn merchants(&self, conn: &Connection) -> Result<Arc<Vec<CompiledMerchant>>> {
        // Fast path: cached + same generation as DB.
        // Slow path: SELECT + compile + store + return.
    }
    pub fn bump(&self) { self.generation.fetch_add(1, Ordering::SeqCst); }
}
```

Every rule mutation (`POST /normalise/rules/...`) calls `cache.bump()`
before responding. Reads inside `serve` go through a process-wide
`OnceLock<RuleCache>`; CLI binaries get a fresh cache per invocation
(which costs one extra round of compile, negligible at < 200 patterns
per stage).

The cache is intentionally *not* thread-local: `serve` is
single-threaded today (tiny_http one-thread-per-request handler in a
single loop), and `Arc<Vec<…>>` makes a future multi-thread switch
free.

> Comment: why is the generation counter needed? Shouldn't the cache just be regenerated on every
> rule edit, and nothing else? We could limit the cache generation to the edited pipeline stage
> only.

## 6. Pipeline integration

Each Tier A/B stage gains a sibling that takes `&Connection` and a
`&RuleCache`:

```rust
// merchants.rs
pub fn apply_with_db(result: &mut NormalisationResult, conn: &Connection, cache: &RuleCache) {
    let compiled = cache.merchants(conn).expect("merchants load");
    for cm in compiled.iter() {
        if cm.regex.is_match(&result.normalised) { … return; }
    }
}
```

`normalise()` (the public entry point) changes from:

```rust
pub fn normalise(original: &str) -> NormalisationResult { … }
```

to:

```rust
pub fn normalise(original: &str, ctx: &PipelineCtx) -> NormalisationResult { … }
```

where `PipelineCtx` bundles `&Connection + &RuleCache`. **Every call
site in the workspace gets touched** — but it's a small set:

- `scan::scan` (the bulk re-scanner)
- `transactions::views::render_active_detail` (per-row pipeline trace)
- `transactions::views::render_detail_fragment` (same)
- `normalise::views::render_page_shell` (active row pipeline trace)
- All `cargo test` sites that call `normalise()` directly

For tests and CLI, we ship a `PipelineCtx::with_seeded_in_memory()`
constructor that creates an in-memory DB and runs the seed. Production
code uses `PipelineCtx::new(&conn)`. Backward-compatibility shim:
`normalise_for_test(original: &str)` keeps the no-arg signature for
existing test files (forwards to the seeded ctx).

This is the most invasive part of the branch. ≈ 30 call sites, but
all mechanical.

## 7. Seed strategy

A `bin/dump_rules.rs` binary prints the current in-code tables as
`INSERT INTO rule_…` statements. The output is committed as
`src/db/seed_rules.sql` and loaded by `db::initialize` exactly once
when the corresponding table is empty (so existing DBs adopt the
seeds, and fresh DBs start with the same rule set).

The in-code constants are then **deleted** (the seed file is the
source of truth). The dump binary stays in the repo so future
regeneration is possible if we ever need to round-trip back to code.

Sanity check: a test loads the freshly seeded DB and runs every test
fixture from the pre-migration test suite through the new pipeline,
asserting bitwise-identical output. This is the fidelity gate before
we can delete the in-code constants.

> Comment: add a new rust module `rules`. It is the interface between the database used by `serve`, and
> a serialised set of rules. We need the serialised set because the database may be blown away from
> time to time, but we don't ever want to lose the rules. So the canonical rules will be written
> to `src/rules/[pipeline-stage].sql`. When `serve` starts, it will load rules into the pocketsmith
> database, initiate the cache, etc. When `serve` closes, it needs to re-dump the rules back to the
> SQL files during its cleanup path. Is this fine, or overengineering?

## 8. UI (Tier A only in v3)

The Normalise tab gains a third sub-mode on the active row's detail
panel. Today the detail shows:

```
[ Pipeline trace ]
[ N sibling transactions ]
[ Y/N/S buttons ]
```

After v3, when the active row's pipeline produced an entity_name via
merchants/persons/employers (visible in the trace's `+entity_name (…)`
chip), the detail panel grows a fourth section:

```
[ Rule that matched this payee ]
  merchants  +entity_name (Amazon)
  pattern:   (?i)AMAZON\b
  canonical: Amazon
  [ Edit ]  [ Delete ]
```

> Comment: Keep Normalise tab simple. If a rule matched the payee, it should only have Y/N/S as
> shortcuts

When **no** rule matched (the common "I want to add a rule" case), the
detail panel shows:

```
[ No rule matched ]
  Original: AMAZON MARKETPLACE
  After other stages: AMAZON MARKETPLACE
  [ Add merchant rule ]  [ Add person rule ]  [ Add employer rule ]
```

> Comment: This one makes sense. I don't want to have three buttons to add a rule though. How can
> we do this with a single button? No modal.

The Add buttons open a form pre-filled with a sensible default
pattern (literal-escape of the post-stage string) and a blank
canonical name. Submit writes the rule, calls `cache.bump()`, re-runs
the pipeline on the active row, and re-renders the detail panel. No
full page re-scan happens automatically — a yellow chip in the page
header says "12 payees might match new rules — re-scan?" with a
button.

A separate top-level URL `/normalise/rules/<stage>` lists the rule
table for browsing/editing:

- `GET /normalise/rules/merchants` — full table, sortable by canonical
  or by "how many txns currently match this rule" (computed by joining
  to `payee_normalisations.matched_pattern` once we persist it; see §9).
- `GET /normalise/rules/persons` and `…/employers` — same shape.
- `POST /normalise/rules/<stage>` — create.
- `POST /normalise/rules/<stage>/<id>/edit` — update.
- `POST /normalise/rules/<stage>/<id>/delete` — delete.

The list views are read-mostly; they reuse the existing queue/detail
shell so they pick up the keyboard nav for free.

## 9. Persisting matched-pattern in proposals

For the "delete this rule" / "edit this rule" actions on a transaction
to know *which* rule fired, we need to store the matched pattern on
the proposal. Today `last_matched_pattern` is computed at trace time
but discarded; the staging table doesn't keep it.

Add a column:

```sql
ALTER TABLE payee_normalisations
  ADD COLUMN matched_rule_id INTEGER,
  ADD COLUMN matched_stage   TEXT;
```

`scan::scan` writes both when the pipeline reports them. The UI then
joins to the rule tables to render the "rule that matched" card. On
rule delete, the matched-rule column is `NULL`-set in a follow-up
`UPDATE … WHERE matched_rule_id = ?` and the affected proposals get
re-staged on the next scan.

> Comment: Why is this needed? What would be the consequence of not doing it. If possible I would
> like to defer to limit scope. You can convince me otherwise

> Comment: What is matched_rule_id? Each rule table has its own id. How is this meant to uniquely
> identify a role? If using `matched_stage`, the problem is that a normalised payee results from 
> multiple matched rule and stages. Help me understand why this schema change is actually correct

## 10. Build order

Each step is a small commit; the whole thing ships as a single
`feature/editable-rules` branch with ~15 commits.

1. **Schema.** Create the eight tables. `db::initialize` adds them
   idempotently. No code reads them yet.
2. **`dump_rules` binary.** Generate `src/db/seed_rules.sql` from the
   current in-code constants. Commit the SQL file.
3. **Seed loader.** `db::initialize` runs `seed_rules.sql` when the
   tables are empty. Tests assert seed → table contents.
4. **`RuleCache`.** New module `normalise::cache`. Generation counter,
   per-stage RwLock slots, lazy compile. Unit tests on
   load/bump/reload.
5. **`PipelineCtx`.** Threads `&Connection + &RuleCache` through
   `normalise()`. All call sites updated; tests adapted via the
   `with_seeded_in_memory()` ctx. Pipeline still uses the in-code
   constants — this commit only changes signatures.
6. **Convert `merchants` stage** to read from `rule_merchants`. Drop
   the in-code `MERCHANTS` constant. Fidelity test: every existing
   merchant test passes unchanged.
7. **Convert `persons` stage** to read from `rule_persons`. Same drill.
8. **Convert `employers` stage** to read from `rule_employers`.
9. **Convert Tier B stages** (`prefix`, `suffix`, `expand`,
   `banking_ops`, `locations`) in five separate commits. No UI yet,
   purely an internal switch.
10. **Persist `matched_rule_id` + `matched_stage`** on
    `payee_normalisations`. Update `scan::scan`. UI not consuming yet.
11. **Detail-panel "rule that matched" card** (Normalise + Transactions
    tabs). Read-only.
12. **Detail-panel "add merchant/person/employer rule" buttons.** Forms
    submit to `POST /normalise/rules/<stage>` and re-render the active
    row.
13. **Rule list views** (`GET /normalise/rules/<stage>`). Sortable
    table. CRUD endpoints.
14. **Re-scan banner.** Page header detects "new rules since last scan"
    and offers a one-click re-scan.

Steps 1–10 are infrastructure (no user-visible change). Steps 11–14
are the new UI.

> Comment: make use of red greed TDD in each commit. Pick minimal implementations, reuse code where
> practical. Structure and name code consistently across the codebase.

## 11. Test strategy

- **Per-stage fidelity tests.** For each stage being converted: pick
  10 representative payees from `pocketsmith.db`, snapshot the
  pre-conversion `NormalisationResult` (string + features + class),
  and assert the post-conversion run produces the same result.
- **Pipeline-end-to-end fidelity.** Run the full pipeline against
  every distinct `original_payee` in the production DB, before and
  after conversion. Assert identical `payee_normalisations` rows
  would result (i.e. same `proposed_payee`, `class`, `features_json`).
  This is the gate for deleting the in-code constants.
- **Cache invalidation.** Insert a rule → assert next pipeline run
  picks it up. Delete a rule → assert next run no longer matches.
- **UI smoke tests** for the new endpoints, in the same shape as the
  existing serve smoke tests.
- **Migration test.** Fresh DB → `db::initialize` → assert rule tables
  contain the seed.

> Comment: Use test pyramid strategy. Rely on a large volume of unit tests that runs quickly
> and with few dependencies. Use a medium volume of integration tests that validates user flows
> and functionality across modules, potentially using mocks. Use a small number of end-to-end tests
> that hits the real API

## 12. Cost estimate

- Steps 1–5 (infra without behaviour change): ≈ 1 day.
- Steps 6–9 (per-stage conversion + fidelity tests): ≈ 1 day.
- Steps 10–14 (UI): ≈ 1.5 days.

Total: ~3.5 days. The big-bang fidelity test in step 9 is what makes
this safe — if it passes, we can delete the in-code constants without
fearing a regression.

> Comment: Do you recommend doing this in one stage, one PR; or multiple stages, one PR per
> stage? I would like to do code reviews and don't want to be overwhelmed by very long PRs. I also
> want to do user acceptance testing at appropriate times

## 13. Open questions for the user before coding starts

1. **Class assignment.** Today `merchants::apply` sets
   `PayeeClass::Merchant`. Should the rule_merchants table allow
   overriding the class (e.g. a row that sets class=Other)? Default
   answer: no — keep one stage = one class for simplicity.

> Comment: Correct. One class, one stage

2. **Rule notes vs commit messages.** v2 plan added a `note` column.
   Worth it, or is git history enough? Default answer: keep `note`;
   the UI is the audit log for the people who never read git logs.

> Comment: Keep note. This is important context for entities

3. **Re-scan scope.** When a rule changes, do we re-scan all payees
   or just the ones whose existing proposal's `matched_rule_id`
   equals the changed rule? Default answer: just the affected ones,
   for speed. The "12 payees might match" banner uses the count of
   payees with `matched_rule_id IS NULL AND payee_normalisations.status
   = 'pending'`.

> Comment: 

4. **Order semantics for persons/employers/merchants.** Today first
   match wins. Do we keep that, or switch to "most-specific wins"
   (longest pattern)? Default answer: keep first-match-wins, sort UI
   by canonical, and document that overlap is the user's responsibility.

> Comment: Keep first match wins. Order alphabetically. User ensures no overlap

5. **Tier B UI in v3 or v4?** Current plan says v4. If you want
   prefix/suffix/expand editable now, the cost roughly doubles
   (regex-capture form is fiddly).

> Comment: Needs to be in v3

Sign-off on these → start at step 1.
