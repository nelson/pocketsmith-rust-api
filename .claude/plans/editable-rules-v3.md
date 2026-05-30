# Plan: Editable normalisation rules (v3)

> Branch: `feature/editable-rules` (to be created from `master` after this
> plan is signed off). Supersedes [`editable-rules-v2.md`](./editable-rules-v2.md).
> Status: **plan only — code starts after sign-off.**
>
> Mockups for the new Pipeline tab:
> - [`pipeline-A-merchants.html`](../../mockups/pipeline-A-merchants.html) — merchants stage, edit mode
> - [`pipeline-A2-merchants-eval.html`](../../mockups/pipeline-A2-merchants-eval.html) — merchants stage, evaluate mode (categorical impact)
> - [`pipeline-B-prefix.html`](../../mockups/pipeline-B-prefix.html) — prefix stage, captures + sort_order
> - [`pipeline-C-add-from-transaction.html`](../../mockups/pipeline-C-add-from-transaction.html) — "Add rule for this payee" entry from Transactions

## 1. Trigger and goal

The trigger described in the v2 plan has fired: rule edits are happening
weekly, and the user wants to fix a mis-normalisation from the web UI
without re-building.

**Goal:** every dictionary that today lives as `const …: &[…] = &[…]`
in `src/normalise/` becomes a row set in SQLite, editable from a new
Pipeline tab. The compiled pipeline keeps doing what it does today;
only the source of its tables changes.

**Non-goal:** scripted/expressive rules. Rules stay declarative: a
pattern, a few flags, a few captures.

## 2. What's actually in code today

Eight pipeline stages, in this order ([`src/normalise/mod.rs`](../../src/normalise/mod.rs)):

| # | Stage          | Shape                                          | Count | Order  |
|---|----------------|------------------------------------------------|------:|--------|
| 1 | `prefix`       | regex + optional gateway/operation + captures  |    42 | matters (loop) |
| 2 | `suffix`       | regex + optional gateway/operation/institution + captures | 37 | matters (loop) |
| 3 | `expand`       | regex → canonical (literal substitution)       |   102 | matters (loop) |
| 4 | `persons`      | canonical + literal-substring patterns         |    82 | doesn't matter (alphabetical UI) |
| 5 | `employers`    | canonical + regex patterns                     |     4 | doesn't matter (alphabetical UI) |
| 6 | `merchants`    | regex → canonical, sets `class=Merchant`       |   146 | doesn't matter (alphabetical UI) |
| 7 | `banking_ops`  | regex + operation + optional account capture   |    38 | matters within an op |
| — | `locations`    | literal list, used by `suffix`                 |    91 | n/a |

`run_traced` in `mod.rs` is the only call site for stages today.
`scan::scan(conn)` walks distinct payees and writes proposals into
`payee_normalisations`. Both are reused as-is — the change is purely
inside the per-payee `normalise()` call.

## 3. Scope of v3

**Every stage is editable from the UI.** No Tier A / Tier B split.
Stages do still differ in how their detail panels look (capture flags,
sort-order semantics), but the framework is one piece of code
parameterised over the stage's schema.

### Build order — by pipeline data flow

Per user constraint: tiers work hand-in-hand; downstream stages need
upstream cleaning to be in place to be usefully edited. So we build in
the order data flows. Each stage is its own PR.

1. **Schema + seed-loader infrastructure** (no stage converted)
2. **`prefix`** + **`suffix`** (paired — share loop shape and capture-flag matrix)
3. **`expand`**
4. **`persons`** + **`employers`** + **`merchants`** (the three first-match-wins entity stages — share UI)
5. **`locations`** (small literal list)
6. **`banking_ops`** (depends on others' cleanup)

### Out of scope

- **`payee_overrides`** and the **promote-override nudge** — both cut.
  Y/N/S on `payee_normalisations` already gives per-payee control.
- **The Review-tab Rules sub-pane** (from v2). Pipeline tab supersedes.
- **Version history on rules.** `_changes` infra can capture later.
- **Auto-re-scan on rule change.** User-triggered only (§4.5).
- **`matched_rule_id` schema change.** Deferred (§9).
- **Cross-stage conflict detection.** Deferred.

### One Transactions-tab fix in scope

The Transactions tab today renders "no normalisation rule" whenever
`norm_status` is missing, which is wrong: a payee that hit a merchant
rule but had no prefix/suffix to strip still has a non-empty pipeline
trace. Pillar should only flag "no normalisation" when the trace is
**completely empty** (no stage transformed the string and no features
were extracted). Land as PR 0.

## 4. The Pipeline tab

### 4.1 Nav placement

Tab order: **Dashboard / Transactions / Pipeline / Transfers /
Normalise**. (Third position, between Transactions and Transfers, per
user.) Rationale: Pipeline edits *cause* Transfers / Normalise data,
so it sits upstream in the nav.

### 4.2 Header — sync + push freshness chips

The header strip already has a `synced 3h ago` chip; add a sibling
`pushed 3d ago` chip beside it. Same visual vocabulary, different data
source — `_operations` filtered on `reason='push'` instead of
`reason='sync'`. Same fresh / stale / old buckets (≤ 24h / ≤ 7d /
older). Tooltip prompts `cargo run --bin push`. Lands as part of PR 1
since both pipeline-tab and other tabs benefit.

### 4.3 Layout — two columns inside the detail panel

Three-pane shell as on every other tab:

- **Queue** — one row per pipeline stage in execution order.
  - Two-line layout: name + rule count on the first line, attribute
    tags on the second (`loop` / `first match` / `order matters` /
    `captures` / `aux`). Tags consume less horizontal space than free
    text and let the eye scan the queue at a glance.
  - Rule count always reads "**N rules**" — including banking_ops,
    which we no longer split into "10 ops · 38 patterns" (just "38
    rules" for consistency with the others).
  - Selected via click or arrow keys.
- **Detail** — two-column inside:
  - **Left column (taller):** rule list + search box + `[A] Add rule`
    button. The two-column split makes this list noticeably taller
    than the single-column variant we had earlier.
  - **Right column:** the focused-rule editor card. Sticky to the top
    of the detail scroll area so it stays visible while scrolling
    through long rule lists.
- **Activity** — recent rule-change log + a "re-scan now" link
  surfaced **only when there are dirty rules** (described in §4.5).

#### 4.3.1 Rule list shape — same across all stages

The rule list uses the same row geometry on every stage. A list with
order-irrelevant rules (persons / merchants / employers / banking_ops)
looks visually identical to one with reorderable rules (prefix /
suffix / expand) except for an extra leading drag-handle column.
No dotted lines vs solid lines; no per-stage row gap; no sequence
numbers.

Column layout:

```
stage         | columns
--------------|-----------------------------------------------
merchants     | [drag spacer] Canonical  Pattern  Impact (right)
persons       | [drag spacer] Canonical  Pattern  Impact (right)
employers     | [drag spacer] Canonical  Pattern  Impact (right)
banking_ops   | [drag spacer] Operation  Pattern  Captures · Impact (right)
prefix        | [⋮⋮ handle]  Pattern  Captures · Impact (right)
suffix        | [⋮⋮ handle]  Pattern  Captures · Impact (right)
expand        | [⋮⋮ handle]  Pattern  Canonical  Impact (right)
locations     | [drag spacer] Location  Impact (right)
```

A column-header row sits above every list (`Canonical / Pattern /
Impact` or equivalent), styled as small-caps in `var(--fg-dark)` so
it reads as scaffold not data. The header row uses the same grid
template as the data rows so columns align without per-stage CSS.

The `Impact` and `Captures` columns are **right-aligned**. "412 txns
· $8.4k" reads as a number, so it hugs the right edge of the row;
same for capture-flag tags like `+op +acct`.

### 4.4 Editor card — two states

The editor has **two explicit modes**, swapped server-side via HTMX:

- **Edit mode** *(default when a rule is selected)*. Form fields are
  enabled. Action row: `[E] Evaluate`, `[N] Cancel`, `Delete`. **No
  Save button in this mode.**

- **Evaluate mode** *(after the user clicks Evaluate)*. Form fields
  are read-only. Adds:
  - Single-string tester (`Test against a string` row): paste a
    candidate, see `✓ matches → canonical: …` or `✗ misses`.
  - Categorical impact (§4.6) computed against every distinct
    `original_payee` in the DB.
  - Action row becomes: `[Y] Save`, `[B] Back to edit`, `Delete`.

Rationale (per user feedback): live "as-you-type" impact recomputation
across the whole DB is fine perf-wise (~1 ms per stage per keystroke
on 423 payees × 150 rules), but it's still computing things the user
doesn't want to think about *while typing*. Splitting Edit / Evaluate
gives the user explicit control over when impact is shown, and
removes the "save a half-finished pattern by accident" failure mode —
Save physically doesn't exist outside Evaluate mode.

**Notes textbox is hidden by default.** Most rules don't have notes,
so the textbox is replaced by a `+ add note` link. Clicking expands
the textbox; rules that already have a note render expanded. Same
shape on every stage's editor.

**No explicit `sort_order` field in the editor.** For loop stages
(prefix / suffix / expand / banking_ops) the rule's pipeline order is
implicit in its row position in the rule list. Reorder via the
drag-handle column at the left of each row; the persisted
`sort_order` is a server-side concern the UI never surfaces as a
number. Removes the "two sources of truth" failure where the user
edits both the input and the position and they disagree.

#### 4.4.1 Buttons + keyboard shortcuts

Every button in the Pipeline tab uses the `[X] Label` motif and binds
a single-key shortcut, consistent with the rest of the app:

| Button | Where | Shortcut |
|--------|-------|----------|
| `[A] Add rule` | rule-list header (every stage) | `A` |
| `[E] Evaluate` | edit mode action row | `E` |
| `[N] Cancel` | edit mode action row | `N` |
| `[Y] Save` | evaluate mode action row | `Y` |
| `[B] Back to edit` | evaluate mode action row | `B` |
| `Delete` | both modes | (no shortcut; mouse-only to avoid mishit) |

The global `?` overlay (already shipped) gains entries for `A`, `E`,
`B` when the active tab is Pipeline. `Y / N` already exist globally
for confirm/reject; in Pipeline-tab evaluate mode `Y` binds to Save
and `N` to Cancel.

### 4.5 Re-scan = user-triggered, surfaced only when dirty

User: "this will be too slow. Let's make this user-triggered. Add a
button that only appears when the rules are dirty, and triggers a
rescan."

Concretely:

- Every rule mutation (create / edit / delete / reorder) writes a row
  to `_operations` with `reason='rule-edit'` (already standard via
  `with_operation`).
- The activity panel computes `dirty_since = MAX(
  _operations.created_at WHERE reason='rule-edit') vs. MAX(
  _operations.created_at WHERE reason='normalise-scan')`. If the
  former is later, rules are dirty.
- When dirty, the activity panel shows a yellow chip:
  `⚠ N payees would re-stage since the last scan · re-scan now ↻`.
  The number is computed by simulating the pipeline on every distinct
  `original_payee` (cheap: ≈ 100 ms on 423 payees) and counting those
  whose proposed_payee would change.
- Clicking re-scan runs `scan::scan(conn)` and clears the chip.
- When clean (post-scan), the chip is hidden.

The "would re-stage" count is computed lazily — only when the
activity panel is rendered with dirty rules. No background polling.

### 4.6 Impact preview — categorical lists, not bars

User: "I don't think we need a bar; just listing the actual impact
will do." Replaced bars with explicit four-bucket categorisation.
The buckets (computed against every distinct `original_payee`):

| Bucket | What it means | Coloring |
|--------|---------------|----------|
| **Newly matched** (`unmatched → matched`) | Payee was unmatched by any rule in this stage; the edited rule catches it now. | green |
| **Stolen from another rule** (`matched_by_X → matched_by_this`) | Payee was matched by a different rule in the same stage; the edited rule's pattern now catches it first (alphabetical-order tie-break). | yellow |
| **New fallthrough** (`matched → unmatched`) | Saved version of this rule caught the payee; edited version doesn't. | red |
| **Unchanged** (`matched_by_this → matched_by_this`) | Payee was matched by this rule before and after the edit. | dim |

Each bucket is a collapsible card with a header (count of payees +
total txns + total $ when relevant) and a `<ul>` of up to ~6 sample
payees. Sample row format: `<original_payee>` + per-row context (txn
count, account name, dollar value, or the previous match for the
"stolen" bucket). Long lists truncate with `… N more`.

The mockup `pipeline-A2-merchants-eval.html` shows all four buckets
populated. The "Unchanged" bucket is collapsed-by-default to reduce
noise.

**No overlap-warning card.** The "stolen from another rule" bucket
already surfaces overlaps in a more useful form (with the donor rule
named per row). Cross-stage overlap detection is deferred.

### 4.7 "Add rule for this payee" affordance from Transactions

When the user is on a Transactions or Normalise detail panel and the
active row has no entity matched (the trace is "completely empty" per
PR 0's definition), the detail panel shows a single button:

```
[ + Add rule for this payee ]
```

Clicking it navigates to the Pipeline tab in **new-rule** state
(mockup C) with these fields prefilled:

- **Stage** — chosen by the heuristic below. Editable via dropdown.
- **Pattern** — the post-pipeline string, regex-escaped. Editable
  (and almost always wants generalising before save, e.g. dropping a
  store number).
- **Canonical** — empty, autofocused.
- **Source banner** — chip at the top of the editor card showing the
  source transaction id + raw payee + amount + date so the user
  remembers what they were looking at.

The user is dropped straight into Edit mode of the editor card. They
fill canonical, optionally tighten the pattern, click `[E] Evaluate`,
inspect impact, click `[Y] Save`. New rules slot into the list
alphabetically (or at the end for `sort_order` stages).

#### 4.7.1 Heuristic for default stage

Per user: "Most strings are merchants, so it's ok to use just that as
a default. Are there better heuristics than 'alphabetic-only, less
than 4 words' to identify a person?"

Empirical look at the existing 100-pattern person dictionary
(`src/normalise/persons.rs`):

| Signal | Hit rate |
|--------|---------:|
| Title prefix (`MR / MRS / MS / MISS / DR`) | 24% |
| Contains digit | 0% |
| ≤ 4 words | 97% |
| ≤ 2 words | 51% |

Title-prefix is the only **high-precision** signal — it's near zero
false positives because no merchant pattern in the existing 146-row
dictionary starts with `MR `, `MRS `, etc. The "short alphabetic"
signals overlap heavily with merchants (BUPA, COLES, ALDI, ATM, ATO
all match "≤ 2 words, alphabetic").

So the heuristic is, literally, four lines:

```rust
fn guess_stage(after_pipeline: &str) -> Stage {
    let lead = after_pipeline.split_whitespace().next().unwrap_or("");
    let title = matches!(lead.to_ascii_uppercase().as_str(),
        "MR" | "MRS" | "MS" | "MISS" | "DR");
    if title { Stage::Persons } else { Stage::Merchants }
}
```

Default to merchants; switch to persons only when there's a clear
title prefix. The user can always override via the stage dropdown
before saving — the heuristic is a starting point, not a lock-in.

The mockup includes a small **`heuristic ✓`** tag next to the stage
dropdown so it's visually obvious that the default was auto-chosen
(versus user-locked) and a one-paragraph explanation card at the
bottom of the editor explains why.

## 5. Schema

Eight tables, all with `note TEXT`, `created_at`, `updated_at`. Stages
where order matters add `sort_order INTEGER NOT NULL`. `sort_order` is
editable from the Pipeline tab for those stages.

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
    sort_order  INTEGER NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE (operation, pattern)
);

-- ===== aux =====
CREATE TABLE rule_locations (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    location    TEXT NOT NULL UNIQUE,
    note        TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
```

Notes:

- `note` is editable from the UI for all stages; UI hides the textbox
  unless the user expands it (§4.4).
- `sort_order` is editable for prefix / suffix / expand / banking_ops.
  Insertion appends with `MAX(sort_order)+1`.
- Persons / merchants / employers display alphabetically
  (`ORDER BY canonical COLLATE NOCASE, id`).

## 6. The `rules` module — file-system canonical store

User: "We need the serialised set because the database may be blown
away from time to time, but we don't ever want to lose the rules…
canonical rules will be written to `src/rules/[pipeline-stage].sql`."

Verdict: yes. A SQL file per stage is the smallest representation
that's both human-readable (git diffs are reviewable) and
machine-readable (`conn.execute_batch(&fs::read_to_string(...))`).

### 6.1 Lifecycle

User: "Pick the simplest that provides reasonable UI performance.
Every edit may be ok on bg thread."

Design:

```
on serve startup:
    if rule tables are empty (or rules-schema-version bumped):
        load src/rules/*.sql into the rule tables
    initialise RuleCache (lazy; first SELECT happens on first stage call)

on every committed rule mutation (create / edit / delete / reorder):
    1. Inside the same with_operation("rule-edit", …) txn:
         INSERT/UPDATE/DELETE the rule
         cache.invalidate(stage)
    2. After the HTTP response is queued:
         spawn a thread that re-dumps src/rules/<stage>.sql
         (the dump reads the DB independently, so doesn't block the
          response and can't see partial state)

on serve shutdown:
    nothing special — disk is always current
```

The mutation flow inside the request handler stays synchronous (the
DB insert + cache invalidation), but the file dump happens on a
detached thread so the user sees the response immediately. Worst case
on disk: file lags the DB by a few ms (negligible — the file is a
backup, not the source of truth while serve is running).

This avoids debounce logic. Per-stage dump is ~1 ms (one SELECT, ≤ 200
INSERT lines), so even with a "save canonical, edit pattern, save,
edit canonical, save" rapid sequence the background dumps just queue
up serially.

If multiple concurrent edits arrive (which can't happen today —
tiny_http is single-threaded — but planning ahead): a `Mutex<()>`
around the dump function serialises them. The last-write-wins
semantic is fine because the dump always reads the current DB
contents.

### 6.2 Module shape

`src/rules/mod.rs` exposes:

```rust
pub fn load_into_db(conn: &Connection) -> Result<()>;
pub fn dump_stage(conn: &Connection, stage: Stage) -> Result<()>;
pub fn dump_all(conn: &Connection) -> Result<()>;          // for the bootstrap binary
pub fn schedule_dump(stage: Stage);                        // spawns the bg thread
```

A tiny `bin/dump_rules.rs` wraps `dump_all` for the initial seed
bootstrap (and as an escape hatch).

### 6.3 Schema versioning

A `rules_schema_version` row in a new `_meta(key, value)` k/v table.
Bump it when the in-tree SQL files introduce a new column. Load on
startup only when the on-disk version > stored version.

## 7. Caching — per-stage Arc<Vec>, no generation counter

User: "why is the generation counter needed? Shouldn't the cache just
be regenerated on every rule edit, and nothing else?" Right, dropped:

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
    pub fn invalidate(&self, stage: Stage) { /* sets the slot to None */ }
}
```

Per-stage invalidation, no global counter, no generation-mismatch
checks. A rule edit on merchants only invalidates `merchants`; the
next read recompiles just that stage.

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

`PipelineCtx` bundles `&Connection + &RuleCache`. ≈ 30 call sites
(mostly tests) need touching, all mechanical.

For tests, `PipelineCtx::with_seeded_in_memory()` constructs an
in-memory DB and runs the seed. Tests that don't care about rules
share a process-wide `OnceLock<PipelineCtx>` so they don't re-seed
per test.

## 9. Persisting `matched_rule_id` on proposals — **deferred**

User: "Why is this needed? What would be the consequence of not doing
it. If possible I would like to defer to limit scope."

Cut from v3. The use case it bought (click-through "fix this rule"
from a Transactions detail) is reachable via:

- Pipeline trace already shows which stage and pattern fired (commit
  `87f2ba5`).
- Click-through from Transactions can string-match the trace's
  `+pattern` against the stage's rule list.
- "Re-apply this stage's rules" is just the user-triggered re-scan.

The schema change was also genuinely confusing — each rule table has
its own auto-increment id, so `matched_rule_id INTEGER` is ambiguous
without a stage column, and a payee proposal's normalisation result
is the *composite* of multiple stages, not a single rule's hit. If we
ever need this, the right schema is a `rule_hits (proposal_id, stage,
rule_id)` side-table per stage that fired, not a column.

## 10. Build order — one PR per step

User: "one PR per stage. I want to do code reviews and don't want to
be overwhelmed."

11 PRs. Each is independently reviewable and acceptance-testable.

| # | PR title | Touches | Acceptance test |
|---|----------|---------|-----------------|
| 0 | `transactions: only flag "no normalisation" when trace is empty` | Transactions tab views | Verify the pillar text changes for merchant-only payees. |
| 1 | `db+rules: schema for 8 rule tables + src/rules/*.sql seed loader + dump_all binary; serve header gains pushed-at chip` | new `src/rules/` module, schema migration, `dump_rules` binary, header chip | `cargo run --bin dump_rules` writes 8 SQL files; fresh DB seeds correctly; existing DB retains data. Header shows two freshness chips. |
| 2 | `normalise: PipelineCtx + RuleCache (no behaviour change)` | `normalise::cache`, `PipelineCtx`, every call site | All existing tests pass; pipeline still uses in-code constants. |
| 3 | `serve: Pipeline tab shell (queue + empty detail + activity panel)` | new `serve/pipeline/` module, route table, nav | Tab visible in third nav slot; queue lists 8 stages with 2-line tag layout; arrow keys navigate. |
| 4 | `pipeline(prefix+suffix): convert to DB; tab UI for these two stages incl. Edit/Eval modes + categorical impact` | `prefix.rs`, `suffix.rs`, `pipeline/views.rs` | Edit a prefix rule, click Evaluate, see impact buckets, save, see dirty banner, re-scan, banner clears. |
| 5 | `pipeline(expand): convert + UI` | `expand.rs`, `pipeline/views.rs` | Same drill for expand. |
| 6 | `pipeline(persons+employers+merchants): convert + UI for the three first-match-wins stages` | three `*.rs` files, shared UI partial | Add a merchant rule end-to-end. |
| 7 | `pipeline(locations): convert + UI` | `locations.rs`, `suffix.rs`, `pipeline/views.rs` | Add/remove a suburb, see effect. |
| 8 | `pipeline(banking_ops): convert + UI` | `banking_ops.rs`, `pipeline/views.rs` | Same. |
| 9 | `pipeline: dirty-rules banner + sample-impact preview polish` | `pipeline/views.rs`, `scan::would_change_count` | Edit→banner→re-scan→banner-clears flow tested end-to-end. |
| 10 | `transactions: "+ Add rule for this payee" affordance + heuristic stage selector` | Transactions detail | Click button on a trace-empty txn, land in Pipeline tab in new-rule state with merchants pre-selected and pattern prefilled. |

Each PR has a fidelity-test gate: every test that passed before the
PR still passes after. Steps 4–8 also gate on a "production-DB
fidelity" test that runs the pipeline against every distinct
`original_payee` in `pocketsmith.db` and asserts identical
`payee_normalisations` rows before vs. after the conversion. Run
manually via `cargo test --features fidelity --test pipeline_fidelity`
before merge.

Estimated cost: ~5–6 days total spread across 11 PRs. Each PR ≤ 1 day.

## 11. Test strategy — pyramid

User: "Use test pyramid strategy. Rely on a large volume of unit
tests that runs quickly and with few dependencies. Use a medium
volume of integration tests that validates user flows and
functionality across modules. Use a small number of end-to-end
tests."

### 11.1 Unit (broad base) — fast, isolated, lots

- One test per stage's `apply_with_db` against a hand-written fixture
  rule set + a single input payee. In-memory SQLite, schema only.
- One test per `RuleCache::<stage>` for load / invalidate / reload.
- One test per `dump_stage` round-trip: insert → dump → reload → diff.
- `guess_stage` heuristic: ≥ 10 cases (title prefixes, no prefix,
  edge cases like single word, mixed case).
- Categorical-impact helper (`compute_buckets(rule_set, payees)`)
  tested as a pure function over fixture data.
- Pipeline-tab Markup-only view tests (no HTTP).

### 11.2 Integration (medium tier) — user flows, multi-module

- Walk a fixture DB through:
  - Open Pipeline tab → click stage → select rule → Edit fields →
    Evaluate → see buckets → Save → activity-panel banner appears →
    re-scan → banner clears.
  - Add merchant rule from Transactions empty-state → assert rule
    landed in DB and dumped to `src/rules/merchants.sql`.
  - Reorder a prefix rule (drag-handle POST) → assert pipeline output
    changes for an affected payee.
- Big fidelity test (`cargo test --features fidelity --test
  pipeline_fidelity`): full pipeline against every distinct
  `original_payee` in `pocketsmith.db`, asserts
  `(proposed_payee, class, features_json)` matches a snapshot taken
  before the migration. Skipped if no real DB present.

### 11.3 End-to-end (narrow top) — few, real, slow

- Already covered by `cargo run --bin sync` against the real
  PocketSmith API. We don't add new e2e tests for editable rules
  (rule editing is local-only).

### 11.4 Red-green TDD per PR

User: "make use of red green TDD in each commit."

For every PR ≥ 200 LOC, the commit log within the PR shows a
red-green sequence: failing test commit → make-it-pass commit →
refactor commit. Reviewer can step through.

Naming + structure consistency:

- `src/normalise/<stage>.rs` keeps owning the stage's pipeline
  function (`apply_with_db`).
- `src/rules/<stage>.rs` owns the loader + dumper + Compiled type.
- `src/bin/serve/pipeline/<stage>.rs` only if the stage's UI is
  meaningfully different from the framework default — most stages
  share `pipeline/views.rs` parameterised by stage.
- Function naming mirrors existing: `<stage>::apply_with_db`,
  `<stage>::compiled`, `rules::<stage>::load`, `rules::<stage>::dump`.

## 12. Cost estimate

- PR 0 (Transactions tab fix): half a day.
- PR 1–3 (infra): 1.5 days.
- PR 4–8 (stage conversions, 5 stages with UI): 3 days.
- PR 9–10 (polish + Transactions affordance): 1 day.

Total: ~5–6 days.

## 13. Decisions (resolved from previous open questions)

All six previous open questions are resolved:

1. **Re-scan scope** → user-triggered, dirty-rules banner only (§4.5).
2. **Pipeline tab nav slot** → 3rd, between Transactions and Transfers (§4.1).
3. **Auto-dump cadence** → per-mutation on a background thread, no
   debounce (§6.1).
4. **"Add rule" UX** → single button, heuristic-chosen stage with
   user override; merchants by default; persons only when title
   prefix (§4.7).
5. **Invalid-regex states in live preview** → in evaluate mode, show
   `syntax error: …` inline and keep the impact preview at the last
   valid state. Save button disabled until pattern compiles.
6. **Cross-stage conflict warnings** → deferred.

## 14. New open questions

A few details surfaced during this revision that may need a call:

1. **Sample-list size in impact buckets.** Mockup A2 shows ~6 rows
   per bucket with truncation. Is 6 the right cap? 10 might be more
   useful when investigating a "stolen from another rule" surprise.
   Default: 6 with a "show all" expander on click.
2. **Stage selector in editor card — where does it live?** Mockup C
   puts it at the top of the editor for the new-rule flow. For an
   existing rule, the stage is fixed (you can't re-stage a saved
   rule — that's a delete-and-recreate). Confirm: hide the picker
   entirely once a rule is saved?
3. **Heuristic 'tag' click behaviour.** Mockup C shows a
   `heuristic ✓` chip next to the dropdown. Click does what? Either
   (a) dismisses the chip (purely visual) or (b) reverts the
   dropdown to the heuristic's default if the user changed it.
   Default: (a). Semantics-free.
4. **Keyboard reorder for sort_order stages.** Drag-and-drop covers
   the mouse case. For keyboard-only, `Alt+↑` / `Alt+↓` on the
   selected rule nudges it up/down one position and persists to
   `sort_order` on each nudge. No explicit number to type.
   Default: implement both — drag for mouse, Alt-arrow for keyboard.

Sign-off on these (or pick alternatives) → start at PR 0.
