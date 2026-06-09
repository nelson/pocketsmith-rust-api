# Plan: `rule-cli` — scriptable rule editing + the shared rule-editing library core

> Branch: `rule-cli`. Order 4 in `editable-rules-ui.md` §7.
> Depends on: `remove-const-oracle` (done — commit `b92bb7b`).
> Prereq sibling: `pipeline-trace-cli` (Order 3) is a *separate* small PR;
> this plan does **not** assume it has landed and shares nothing with it.
>
> **This PR introduces the rule-editing _library core_** (typed CRUD +
> `compute_buckets` + validation + activity/dirty helpers) that the GUI
> (`editor-gui-framework`, Order 5) later *consumes unchanged*. The CLI is
> a thin presentation layer over that core. Nothing here is HTTP.

---

## 0. Codebase reality check (verified against current tree)

Verified before execution; the plan is adjusted to match:
- **§1.0 relocation is already done.** `rules/` exists at the repo root with
  all eight `.sql` files, `rules::rules_dir()` already defaults to `"rules"`,
  and the `load_committed` test helper + `dump_reproduces_committed_files`
  already read from `rules/`. **Remaining §1.0 work is only the stale doc
  comments** in `src/rules/mod.rs` that still say `src/rules/<stage>.sql`
  (module header + `Stage::name`/`dump_columns` comments) and any
  `README.md` references. No `git mv` and no `rules_dir()` change needed.
- **No `clap`, no colour crate.** Arg parsing is hand-rolled (a `match` over
  `env::args()`, see `src/bin/normalise.rs`). The `rule` binary follows the
  same style. The colour/bold layer (§4.0) is a **hand-rolled ANSI helper in
  the binary, no new dependency**. The crate sets `warnings = "deny"`, so the
  new code must be warning-clean.
- **Reuse points confirmed:** `db::with_operation(conn, reason, f)` (single
  transaction; reason = a short tag like `"rule-edit"`),
  `db::open_app_db()` (seeds rule tables for the CLI),
  `normalise::{normalise, OwnedPipeline, PipelineCtx, RuleCache}` for the
  dry-run engine, `RuleCache::invalidate(stage)`, `dump_stage` /
  `dump_stage_to_string` / `schedule_dump`, `BankingOperation::{display_name,
  from_display_name}`, and per-stage `UNIQUE` constraints (merchants/
  employers/prefixes/suffixes/expansions on `pattern`; persons on
  `(canonical,pattern)`; banking_ops on `(operation,pattern)`; locations on
  `location`) for `RuleError::Duplicate` mapping.
- **§12 lives in one place:** the serve table is driven by
  `rules::display_columns` → `list_display` → `render_rule_table`
  (`src/bin/serve/pipeline/views.rs`); dropping `note` from `display_columns`
  removes it everywhere at once. (Locations' `display_columns` already omits
  `kind`.)

---

## 1. Design tenets (from the brief)

### 1.0 Library logic and rule data live in separate places
The **logic** (the Rust rules library) and the **data** (the canonical
`.sql` seed files) are two different things and are stored separately:

- `src/rules/` — **logic only**: `Stage`, the typed rule model, validation,
  CRUD, impact, commit, activity, dirty, dump/load. No data files.
- `rules/` (repo root) — **data only**: the eight `rules/<stage>.sql`
  canonical seeds (relocated out of `src/rules/`). This is what `git diff`
  shows when a rule changes and what re-seeds a blown-away DB.

The `POCKETSMITH_RULES_DIR` default **already** points at `rules` and the
eight `.sql` files **already** live there (see §0). The only residue is
stale doc-comment text under `src/rules/` and `README.md`. (See §9.)

### 1.1 All business logic lives in the rules library
The CLI and the future GUI are both thin shells that call the same
library functions; **neither shell contains rule semantics**:
   - validation (`rules::validate::validate_draft`),
   - typed CRUD (`insert_rule` / `update_rule` / `delete_rule` / `move_rule`),
   - dry-run impact (`impact::compute_buckets`),
   - single-string tester (`impact::test_one`),
   - activity-line formatting (`RuleChange::describe`),
   - dirty derivation (`dirty::would_restage`, see §1.2).
   The web handlers in `editor-gui-framework` will call these exact
   functions; the only thing each shell owns is *presentation* (text/JSON
   vs HTML) and *dump policy* (see §1.2 — proven not to cause divergence).

### 1.2 Why CLI and GUI cannot diverge despite different cache/dump policy
Concern: the CLI dumps `.sql` synchronously and runs with a cold cache,
while serve dumps in the background and holds a warm `RuleCache` — could
they behave differently?

**No, and here is the argument.** There are three artefacts:
1. the `rule_*` tables in SQLite — **the single source of truth**;
2. the `RuleCache` — a *memoization* of compiled regexes derived purely
   from (1);
3. the `rules/*.sql` files — a *git mirror* of (1), never read while a
   process is running (only on a cold re-seed of an empty table).

Reads always resolve against (1) (via (2) when warm). The invariant both
shells maintain is **"compiled form == current committed DB rows"**:
- **CLI:** starts cold → reads committed rows → mutates inside
  `with_operation` → exits. No warm cache can ever be stale because the
  process ends; the next `rule`/`normalise` invocation re-reads (1).
- **serve:** holds a warm cache → on every commit it calls
  `cache.invalidate(stage)`, so the next read recompiles from the same
  committed (1). Identical end state.

The dump (3) is the *only* policy difference, and it feeds **nothing**
while the process runs — it is purely the human-reviewable export. So
it cannot change pipeline behaviour. `rules::commit` (§3.6) centralises
this: it *always* mutates (1) under `with_operation` and *always*
invalidates a cache if one is supplied; `DumpPolicy` only chooses *when*
(3) is written. A unit test asserts cache invalidation happens for both
policies, and an integration test asserts a CLI commit + a serve commit
leave byte-identical `rule_*` rows and `.sql` output.

**`dirty::would_restage(conn) -> usize`** counts the distinct
`original_payee`s whose freshly-computed pipeline proposal differs from
the stored `payee_normalisations.proposed_payee`, when a rule edit
post-dates the last `normalise-scan`. It is the headless equivalent of
the GUI's "⚠ N payees would re-stage" banner; the CLI prints the count
and tells the user to run `normalise` to refresh. Shared by both shells.

### 1.3 CLI = non-interactive, scriptable, testable
   - **Non-interactive:** no TTY prompts in the default path. Destructive
     ops require an explicit flag (`--apply`, plus `--force` for delete),
     never an interactive "are you sure?".
   - **Scriptable:** `--json` on every read/dry-run/commit; stable schema;
     meaningful exit codes (§7).
   - **Testable:** every command is a pure-ish function over a
     `Connection`; integration tests drive them against an in-memory DB
     seeded from `rules/*.sql`, asserting buckets, the `rule_*` row, and
     the re-dumped `.sql`. `--json` golden files for output contracts.
     The keystone is a **single hermetic test** that encapsulates the
     whole lifecycle in one function (old rules → modification → new
     rules), with no on-disk fixtures — see §10.0.

### 1.4 CLI ⇄ GUI workflow parity
The mockups below mirror the Edit→Evaluate→Save split from the GUI mockups
(`pipeline-A*.html`): `add`/`edit` are **evaluate-by-default (dry-run)**;
`--apply` is the "Save". Same four/two impact buckets, same activity
vocabulary, same dirty-banner signal — rendered as text/JSON, not HTML.

### 1.5 Testing pyramid
Many cheap unit tests on the pure library core; a smaller band of
integration tests driving the CLI command functions end-to-end against an
in-memory DB; no E2E (rule editing is local-only, no network).

---

## 2. Surface shape — `rule` is its own CLI binary

Rule editing ships as a **standalone `rule` binary** (`src/bin/rule.rs`),
not a subcommand of `normalise`. `normalise` keeps its current flat
`--apply` / `--help` parsing untouched. (`pipeline-trace-cli` is likewise
its own concern; nothing here depends on it.)

```
rule <verb> [flags]            # NEW binary, this PR
normalise / normalise --apply  # unchanged
```

`rule` verbs:

| Verb | Purpose | Mutates? |
|------|---------|----------|
| `list`   | list a stage's rules (apply order) | no |
| `show`   | show one rule by id | no |
| `test`   | single-string tester against saved rules or a candidate | no |
| `add`    | create a rule — **evaluate by default**, `--apply` commits | only with `--apply` |
| `edit`   | update a rule by id — evaluate by default, `--apply` commits | only with `--apply` |
| `rm`     | delete a rule by id — evaluate by default, `--apply --force` commits | only with `--apply --force` |
| `move`   | reorder a loop-stage rule — evaluate by default, `--apply` commits | only with `--apply` |

### 2.1 Common flags
`--stage <name>` is **always required** (no "globally unique id" mode).

> Why `--stage` is mandatory and ids are per-stage (conceding the
> reviewer's point): the eight `rule_*` tables each own their own
> `AUTOINCREMENT` id space, so id `4` is ambiguous without a stage. A
> global id would mean either a synthetic cross-table id (extra mapping
> table, more to keep in sync) or scanning all eight tables per lookup —
> both are complexity with no payoff, since the user always knows which
> stage they are editing (the GUI is literally one tab per stage). So
> `--stage` stays required and `--id` is scoped to it.

Other common flags: `--json`, `--apply` / `-a`, `--force` / `-f`,
`--quiet`, `--all` (expand bucket samples). **Short flags combine:**
`-af` == `--apply --force` (for `rm`). Colour/bold is on for a TTY and
off when piped or `NO_COLOR` is set (§4.0).

### 2.2 Field flags and feature semantics
Value fields: `--pattern`, `--canonical`, `--operation`, `--gateway`,
`--institution`, `--kind`, `--note`. Feature toggles (boolean):
`--has-account`, `--has-date`, `--has-location`, `--has-currency-code`,
`--has-amount`, with negatives `--no-account`, `--no-date`, `--no-location`,
`--no-currency-code`, `--no-amount`. A `StageSchema` descriptor (shared
with the GUI) declares which flags are valid for each stage and rejects
the rest with a clear error.

**Feature on/off rules:**
- **`add` (new rule):** every feature defaults **off**; it is on only if
  its `--has-*` flag is given. The `--no-*` flags are accepted but do
  nothing (already off).
- **`edit` (existing rule):** every feature **inherits the saved rule's
  value** unless explicitly toggled. `--has-*` turns it on, `--no-*` turns
  it off; omitting both leaves it as-is. (This is exactly where the
  negative flags earn their keep.)

**Feature → pattern conditions (validated, with unit tests).** Turning a
capture-extracting feature on requires the pattern to contain the matching
named capture group; validation fails otherwise with a message that names
the required group:

| Feature flag | Required named group in `pattern` | Stages |
|--------------|-----------------------------------|--------|
| `--has-account`       | `(?P<account>…)`         | prefixes, suffixes, banking_ops |
| `--has-date`          | `(?P<date>…)`            | prefixes, suffixes |
| `--has-location`      | `(?P<location>…)`        | suffixes |
| `--has-currency-code` | `(?P<currency_code>…)`   | suffixes |
| `--has-amount`        | `(?P<amount_in_cents>…)` | suffixes |

`--gateway` / `--operation` / `--institution` set a value directly and
impose no pattern condition. Example error:
```
error: --has-account requires the pattern to capture a named group
       (?P<account>...), but "^DIRECT (CREDIT|DEBIT)" has none.
```
These conditions live in `rules::validate` (one place), are unit-tested
per feature, and are surfaced identically by the CLI and the GUI.

---

## 3. Library core (the reusable business logic)

New/extended modules under `src/rules/`:

### 3.1 `rules::model` — the single authority on rule data structures
```rust
/// A saved rule as read back from the DB (has id + timestamps + order).
pub struct Rule { pub id: i64, pub sort_order: Option<i64>, pub data: RuleData }

/// The editable payload, one variant per stage. The columns in
/// db/schema.rs and Stage::dump_columns mirror THIS, not the reverse.
pub enum RuleData {
    Prefix   { pattern: String, gateway: Option<String>, operation: Option<String>,
               has_account: bool, has_date: bool, note: Option<String> },
    Suffix   { pattern: String, gateway: Option<String>, operation: Option<String>,
               institution: Option<String>, has_account: bool, has_date: bool,
               has_location: bool, has_currency_code: bool, has_amount: bool,
               note: Option<String> },
    Expansion{ pattern: String, canonical: String, note: Option<String> },
    Person   { canonical: String, pattern: String, note: Option<String> },
    Employer { canonical: String, pattern: String, note: Option<String> },
    Merchant { canonical: String, pattern: String, note: Option<String> },
    BankingOp{ operation: String, pattern: String, has_account: bool, note: Option<String> },
    Location { location: String, kind: LocationKind, note: Option<String> },
}
impl RuleData { pub fn stage(&self) -> Stage; }
```

**`src/rules` becomes the authority; `src/normalise` only imports.**
Today each `src/normalise/<stage>.rs` owns an ad-hoc `SELECT`, its own
column list, and a `Compiled<Stage>` struct — the rule *data shape* is
re-declared in eight places. This PR makes `rules::model` the one
definition of a rule's fields, and refactors the normalise stages to:
- load rows via `rules::crud::list(conn, stage)` (or a shared
  `rules::crud::load_for_compile` returning `Vec<RuleData>` in apply
  order), instead of hand-written `SELECT`s with duplicated column lists;
- keep only the **compiled** form (`Compiled<Stage>` = regex + the
  already-typed fields from `RuleData`) and the matching logic, which is
  genuinely runtime-only and stays in `normalise`.

Net effect: column names and field semantics live **once** in
`src/rules`; `normalise` defines no new rule semantics, only how a
compiled rule *matches*. This removes the current duplication and the
risk of the SELECT column lists drifting from `dump_columns`. The
existing per-stage `*_stage_reads_its_rules_from_the_db` tests guard that
behaviour is unchanged. `LocationKind` also moves into `rules::model` and
is imported by `normalise::locations`, replacing its stringly-typed
`kind` match.

### 3.2 `rules::validate` — one validator, shared by CLI + GUI
`validate_draft(&RuleData) -> Result<(), RuleError>` checks:
- regex stages compile their `pattern` (`RuleError::BadRegex`);
- required text fields non-empty (`RuleError::Missing`);
- `operation` parses to a known `BankingOperation` where required;
- `kind ∈ {location, region}` for locations;
- **feature → capture-group conditions** (§2.2): a `has_*` feature that is
  on requires its `(?P<name>…)` group, else `RuleError::MissingCapture`.
Uniqueness (`UNIQUE` constraints) is caught at the SQL layer and mapped to
`RuleError::Duplicate` so the CLI/GUI get one typed error set.

### 3.3 `rules::crud` — typed CRUD (SQL only, no `with_operation`)
```rust
pub fn list(conn, stage) -> Result<Vec<Rule>>;        // apply order
pub fn get(conn, stage, id) -> Result<Option<Rule>>;
pub fn insert_rule(conn, &RuleData) -> Result<i64>;   // returns new id
pub fn update_rule(conn, id, &RuleData) -> Result<()>;
pub fn delete_rule(conn, stage, id) -> Result<()>;
pub fn move_rule(conn, stage, id, target: MoveTarget) -> Result<()>; // loop stages only
```
- New loop-stage rules append at `MAX(sort_order)+1`; `move_rule`
  renumbers densely (keeps `prefix_seed_sort_order_is_dense` happy).
- These are **pure storage ops**: they perform exactly one row mutation
  each and do *not* open an operation or dump. The single-change
  orchestration (one `with_operation`, one dump, one activity line) lives
  in `rules::commit` (§3.6). They are *not* a batch API — see §3.6.

**`MoveTarget` and `move_rule` semantics.** Only the three loop stages
(prefixes / suffixes / expansions) are ordered; first-match stages are
auto-sorted by the §0 comparator and have no manual order. A move is a
single recorded operation that repositions **one** rule within its stage
and renumbers `sort_order` to stay dense `0..N-1`.

I surveyed the usual options (full-list rewrite `[id,id,…]`; absolute
`to_index`; relative-to-anchor `before/after`). Findings:
- **Full-list rewrite** is the safest against concurrent edits but is the
  wrong contract here — our list is small, single-user, and the user moves
  *one* rule; sending the whole order invites the "the slice isn't the
  whole" class of bugs.
- **Absolute `to_index`** is brittle: position numbers shift as the list
  changes, so a scripted `--to 3` can mean something different next week.
- **Relative-to-anchor** (`move_before(id, anchor)` / `move_after`) is
  what GTK's `ListStore.move_before/after` and most drag-drop UIs use
  ("drop this row above/below that row") and survives list edits because
  it names a neighbour, not a slot.

Decision: `MoveTarget { Before(anchor_id), After(anchor_id) }`, exposed on
the CLI as `--before <id>` / `--after <id>` (drop `--to <pos>`). It maps
directly onto the GUI's `Alt+↑/↓` (= move before prev / after next) and
drag-drop. `move_rule` validates that `id` and `anchor_id` are in the
same stage and that the stage is ordered, else `RuleError`.

### 3.4 `rules::impact` — dry-run engine (pure, the heart of evaluate)
```rust
pub struct PayeeSample { pub original_payee: String, pub txn_count: i64,
                         pub total_cents: i64, pub account: Option<String> }

pub fn load_payees(conn) -> Result<Vec<PayeeSample>>;   // distinct original_payee + agg

pub enum Bucket { NewlyMatched, Stolen, NewFallthrough, Unchanged,  // first-match stages
                  NewlyAffected, NoLongerAffected }                 // loop stages
pub struct Buckets { /* counts + up-to-6 samples per bucket, "was: X" for Stolen */ }

/// Pure over (base rules, candidate mutation, payees). Builds a scratch
/// in-memory rule set = saved rules with the mutation applied, runs the
/// FULL pipeline for each payee on both, and attributes the diff at this
/// stage's true position. First-match stages → 4 buckets; loop stages
/// (prefix/suffix/expand) → 2 (NewlyAffected / NoLongerAffected).
pub fn compute_buckets(conn, stage, mutation: &Mutation, payees: &[PayeeSample])
    -> Result<Buckets>;

/// Single-string tester (the GUI evaluate card's inline tester).
pub fn test_one(conn, stage, candidate: &RuleData, input: &str) -> TestResult;
// TestResult = Matches { canonical } | Misses | SyntaxError(String)
```
`Mutation = Add(RuleData) | Edit{ id, RuleData } | Delete{ id } | Move{ id, MoveTarget }`.
`compute_buckets` is the same function the GUI Evaluate card calls; the CLI
renders it as text/JSON, the GUI as coloured bucket cards.

### 3.5 `rules::activity` + `rules::dirty` — shared signals
- `RuleChange::describe(&Mutation, before: Option<&Rule>) -> String` →
  `+ added {canonical} {pattern}` / `~ edited … → …` / `− deleted …`
  (the §3.6 activity vocabulary, shared verbatim with the GUI activity log).
- `dirty::would_restage(conn) -> Result<usize>` → N distinct payees whose
  pipeline output would change vs the last `normalise-scan` (§1.2; the
  headless form of the GUI dirty banner). Reused by both shells.

### 3.6 `rules::commit` — the shared orchestration seam (one change, atomic)
```rust
pub struct CommitResult { pub change: String, pub dirty_payees: usize, pub new_id: Option<i64> }
pub fn commit(conn, mutation: &Mutation, dump: DumpPolicy, cache: Option<&RuleCache>)
    -> Result<CommitResult>;
```
`commit` takes **exactly one `Mutation`** and is the only "save a rule"
entry point. It: opens one `db::with_operation("rule-edit", …)`, performs
the single CRUD call for that mutation, invalidates `cache` (if supplied),
returns the activity line (`RuleChange::describe`) + dirty count, and
writes the dump per `DumpPolicy::Sync` (CLI — inline `dump_stage`, process
exits) or `DumpPolicy::Background` (serve — `schedule_dump`).

**Rule changes are singular and atomic — there is no accumulation.**
Confirming the reviewer's stance: one `with_operation` wraps one CRUD
call, producing one `_operations` row and one activity line. The CLI
evaluates a single change or applies a single change; the serve UI edits
one rule at a time (one Evaluate → one Save). There is no pending-changes
buffer and no multi-edit commit. The `crud` functions are split out from
`commit` **only for testability** (you can unit-test the SQL without an
operation wrapper), not to enable batching — callers always go through
`commit`, which admits exactly one mutation. (If a batch ever becomes
necessary it would be a deliberate future API with its own design; this PR
does not build or imply one.)

So the intended flow is linear and singular:
`validate_draft → compute_buckets` (evaluate/dry-run) → *on `--apply`* →
`commit` (= `with_operation` + one `crud` call + `RuleChange::describe` +
dirty count + dump). `activity` and `dirty` are pure read-side helpers
`commit` calls to build its `CommitResult`; they never mutate.

---

## 4. CLI mockups — reads

### 4.0 Colour, bold, and TTY behaviour (applies to every command)
Output is styled for readability on a terminal and **plain when not**:
- **Bold:** section headers (`STAGE merchants …`), the `candidate:` line,
  column headers, and the committed-change line.
- **Colour:** bucket glyphs/labels — green = newly matched/affected,
  yellow = stolen, red = new fallthrough / no-longer-affected / deletes,
  dim grey = unchanged. `✓` green, `⚠` yellow, `error:`/`syntax error:`
  red, the matched span in `test` green, `→ now:` cyan.
- **Auto-off** when stdout is not a TTY, when `--json` is set, or when
  `NO_COLOR` is in the environment — so piped/scripted/golden-file output
  is deterministic plain text. (`--quiet` further drops the decorative
  header lines.) The colour layer is a thin helper in the binary; the
  library returns structured data, never ANSI.

*(The mockups below are shown as plain text; picture bold headers and the
per-bucket colours described above.)*

### 4.1 `rule list --stage merchants`
```
$ rule list --stage merchants
STAGE merchants — first-match-wins (auto-ordered: alphabetical, longer-substring-first)

  id  canonical           pattern
  ──  ──────────────────  ──────────────────────────────
  12  Amazon Prime        (?i)AMAZON ?PRIME
   4  Amazon              (?i)AMAZON
  31  Diggy Doo's Coffee  (?i)DIGGY ?DOOS? COFFEE
  18  Mamak               (?i)MAMAKS?MLC
   7  Transport for NSW   (?i)TRANSPORTFORNSW(TRAVEL)?
  22  Uber Eats           (?i)UBER ?\*?EATS
   9  Uber                (?i)UBER

7 rules.  `rule show --stage merchants --id <id>` for detail (incl. note).
```
*(The `note` column is intentionally omitted — see §12; `show` carries it.)*

### 4.2 `rule list --stage merchants --json`
```
$ rule list --stage merchants --json
{
  "stage": "merchants",
  "ordered": false,
  "rules": [
    { "id": 12, "canonical": "Amazon Prime", "pattern": "(?i)AMAZON ?PRIME", "note": null },
    { "id": 4,  "canonical": "Amazon",       "pattern": "(?i)AMAZON",        "note": null }
  ]
}
```
*(JSON keeps `note` for completeness; the human table drops it.)*

### 4.3 `rule list --stage prefixes` (loop stage — shows order)
```
$ rule list --stage prefixes
STAGE prefixes — loop (apply order; reorder with `rule move`)

  #   id  pattern                          gateway  operation  acct  date
  ──  ──  ───────────────────────────────  ───────  ─────────  ────  ────
   0   3  ^SP \*                            Stripe   —          no    no
   1   7  ^(?P<account>xx\d{4}) VISA-       —        —          yes   no
   2  11  ^DIRECT (CREDIT|DEBIT)            —        Transfer   no    no

3 rules.  `#` is the apply position; `rule move` changes it.
```

### 4.4 `rule show --stage merchants --id 22`
```
$ rule show --stage merchants --id 22
merchants #22
  canonical    Uber Eats
  pattern      (?i)UBER ?\*?EATS
  note         —
  created_at   2026-04-03T00:00:00.000Z
  updated_at   2026-04-03T00:00:00.000Z
  impact       412 txns · $8.4k   (cached; re-run `normalise` to refresh)
```
*(`show` is the one place `note` is surfaced.)*

### 4.5 `rule test` — single-string tester
```
$ rule test --stage merchants --pattern '(?i)UBER ?\*?EATS' --canonical 'Uber Eats' "UBER *EATS Sydney AU"
✓ matches  →  Uber Eats        (matched span: "UBER *EATS")

$ rule test --stage merchants --pattern '(?i)UBER ?\*?EATS' --canonical 'Uber Eats' "OPAL TRAVEL"
✗ no match

$ rule test --stage merchants --pattern '(?i)UBER (' --canonical 'x' "UBER"
syntax error: regex parse error: unclosed group        # exit 2 (stderr)
```

---

## 5. CLI mockups — evaluate (dry-run, the default for add/edit/rm/move)

### 5.1 `rule add` — first-match stage, evaluate (default, NO write)
```
$ rule add --stage merchants --pattern '(?i)BUNNINGS' --canonical 'Bunnings'
EVALUATE (dry-run — nothing written)
candidate: merchants  +add  "Bunnings"  (?i)BUNNINGS

  ● newly matched      18 payees · 1,204 txns · $61.2k
      BUNNINGS 391 KOTARA          (812 txns · $44.1k · CommBank Everyday)
      BUNNINGS WAREHOUSE NORTH     (210 txns · $9.8k)
      BUNNINGS 145 ASHFIELD        ( 96 txns · $3.4k)
      … +15 more (use --all)
  ◐ stolen from another rule        0 payees
  ○ new fallthrough                 0 payees
  · unchanged                   10,172 payees

Re-run with --apply (-a) to commit. 18 payees would re-stage — then run
`normalise` (scan) to refresh proposals.
```

### 5.2 `rule edit` — evaluate showing **stolen** + **fallthrough**
```
$ rule edit --stage merchants --id 9 --pattern '(?i)UBER(?! ?\*?EATS)'
EVALUATE (dry-run — nothing written)
candidate: merchants  ~edit #9  "Uber"
  pattern  (?i)UBER  →  (?i)UBER(?! ?\*?EATS)

  ● newly matched         0 payees
  ◐ stolen from another   3 payees · 51 txns · $980
      UBER *EATS LATE NIGHT        (40 txns · $760 · was: Uber)   → now: Uber Eats
      UBEREATS HELP                ( 8 txns · $190 · was: Uber)   → now: Uber Eats
      UBER* EATS SYDNEY            ( 3 txns · $30  · was: Uber)   → now: Uber Eats
  ○ new fallthrough       2 payees · 14 txns · $220
      UBER ONE MEMBERSHIP          (12 txns · $200 · was: Uber)   → now: (unmatched)
      UBER PASS                    ( 2 txns · $20  · was: Uber)   → now: (unmatched)
  · unchanged         10,185 payees

Re-run with --apply (-a) to commit. 5 payees would re-stage.
```

### 5.3 `rule add --stage prefixes` — loop stage, **2 buckets**, feature flag
```
$ rule add --stage prefixes --pattern '^POS (?P<account>\d+) ' --has-account
EVALUATE (dry-run — nothing written)
candidate: prefixes  +add  ^POS (?P<account>\d+)   +account   (appends at position #3)

  ● newly affected      327 payees · 2,041 txns · $88.0k
      POS 0241 WOOLWORTHS METRO    (88 txns · $4.1k)  →  WOOLWORTHS METRO
      POS 1180 7-ELEVEN 2031       (61 txns · $1.2k)  →  7-ELEVEN 2031
      … +325 more (use --all)
  ○ no longer affected    0 payees

Re-run with --apply (-a) to commit. 327 payees would re-stage.
```
*(With `--has-account` the pattern must capture `(?P<account>…)` — §2.2.
Without the group, evaluate fails: `error: --has-account requires …`.)*

### 5.4 `rule add --json` — evaluate, machine-readable
```
$ rule add --stage merchants --pattern '(?i)BUNNINGS' --canonical 'Bunnings' --json
{
  "mode": "evaluate",
  "committed": false,
  "stage": "merchants",
  "mutation": { "kind": "add", "canonical": "Bunnings", "pattern": "(?i)BUNNINGS" },
  "buckets": {
    "newly_matched":  { "payees": 18, "txns": 1204, "total_cents": 6120000,
      "samples": [ { "original_payee": "BUNNINGS 391 KOTARA", "txns": 812,
                     "total_cents": 4410000, "account": "CommBank Everyday" } ] },
    "stolen":          { "payees": 0, "txns": 0, "total_cents": 0, "samples": [] },
    "new_fallthrough": { "payees": 0, "txns": 0, "total_cents": 0, "samples": [] },
    "unchanged":       { "payees": 10172 }
  },
  "dirty_payees": 18
}
```

### 5.5 Invalid regex — evaluate refuses, exit 2 (stderr)
```
$ rule add --stage merchants --pattern '(?i)BUNNINGS(' --canonical 'Bunnings'
syntax error: regex parse error:
    (?i)BUNNINGS(
                ^
    unclosed group
                                                       # exit 2, nothing written
```

---

## 6. CLI mockups — apply (commit)

### 6.1 `rule add --apply`
```
$ rule add --stage merchants --pattern '(?i)BUNNINGS' --canonical 'Bunnings' --apply
EVALUATE → APPLY
  ● newly matched  18 payees · 1,204 txns · $61.2k   (see --json for samples)

✓ committed: + added Bunnings (?i)BUNNINGS   (merchants #57)
✓ re-dumped rules/merchants.sql
⚠ 18 payees would re-stage — run `normalise` to refresh proposals.
```

### 6.2 `rule edit --apply` / `rule move --apply`
```
$ rule edit --stage merchants --id 9 --pattern '(?i)UBER(?! ?\*?EATS)' -a
✓ committed: ~ edited Uber  (?i)UBER → (?i)UBER(?! ?\*?EATS)   (merchants #9)
✓ re-dumped rules/merchants.sql
⚠ 5 payees would re-stage — run `normalise` to refresh proposals.

$ rule move --stage prefixes --id 7 --before 3 -a
✓ committed: moved prefix #7 before #3   (now position #1)
✓ re-dumped rules/prefixes.sql
⚠ 0 payees would re-stage.
```
*(`move` uses `--before <id>` / `--after <id>` — anchored to a neighbour,
not an absolute slot; see §3.3.)*

### 6.3 `rule rm` — evaluate, then `--apply --force` (`-af`)
```
$ rule rm --stage merchants --id 4
EVALUATE (dry-run — nothing written)
candidate: merchants  −delete #4  "Amazon"  (?i)AMAZON

  ○ new fallthrough     6 payees · 88 txns · $1.9k
      AMAZON MARKETPLACE AU        (70 txns · $1.6k)   → now: (unmatched)
      AMAZON AWS                   (18 txns · $0.3k)   → now: (unmatched)
  · unchanged       10,184 payees

Deleting is irreversible. Re-run with --apply --force (-af) to commit.

$ rule rm --stage merchants --id 4 --apply
error: deleting a rule requires --force (-f)            # exit 1, nothing written

$ rule rm --stage merchants --id 4 -af
✓ committed: − deleted Amazon (?i)AMAZON   (merchants #4)
✓ re-dumped rules/merchants.sql
⚠ 6 payees would re-stage — run `normalise` to refresh proposals.
```

### 6.4 `--apply --json` (commit, machine-readable)
```
$ rule add --stage merchants --pattern '(?i)BUNNINGS' --canonical 'Bunnings' -a --json
{
  "mode": "apply", "committed": true, "new_id": 57,
  "stage": "merchants",
  "change": "+ added Bunnings (?i)BUNNINGS",
  "buckets": { "newly_matched": { "payees": 18, "txns": 1204, "total_cents": 6120000 } },
  "dumped": "rules/merchants.sql",
  "dirty_payees": 18
}
```

### 6.5 Conflict (UNIQUE) on apply — exit 1 (stderr)
```
$ rule add --stage merchants --pattern '(?i)AMAZON' --canonical 'Amazon (dup)' --apply
error: a merchants rule with pattern "(?i)AMAZON" already exists (#4)   # exit 1
```

---

## 7. Exit codes + error stream (the scriptable contract)

| Code | Meaning |
|-----:|---------|
| 0 | success (read, dry-run evaluate, or committed apply) |
| 1 | usage / not-found / duplicate / `--apply` without `--force` for delete |
| 2 | invalid input that can't even be evaluated (regex syntax error, bad `--operation`, missing required capture group, unknown `--stage`) |

**Errors go to stderr; stdout carries only success output (§13 decision).**
A failure prints `error: …` / `syntax error: …` to **stderr** and sets the
exit code. Under `--json`, the *success* JSON still goes to stdout; on
failure stdout stays empty and a JSON envelope
`{"error": "…", "code": N}` is written to **stderr**. Rationale: a script
can always `cmd --json > out.json` and trust that `out.json` is either
valid success JSON or empty, checking `$?` (and reading stderr) for errors
— the conventional Unix split.

---

## 8. Help text mockup
```
$ rule --help
rule — view and edit normalisation rules (scriptable; library-backed)

USAGE
  rule list   --stage <stage> [--json]
  rule show   --stage <stage> --id <id> [--json]
  rule test   --stage <stage> [candidate flags] "<string>"
  rule add    --stage <stage> [field flags] [--apply|-a] [--json]
  rule edit   --stage <stage> --id <id> [field flags] [--apply|-a] [--json]
  rule rm     --stage <stage> --id <id> [--apply --force | -af] [--json]
  rule move   --stage <stage> --id <id> (--before <id> | --after <id>) [--apply|-a]

STAGES
  prefixes suffixes expansions  (loop — ordered, reorder with `move`)
  persons employers merchants banking_ops  (first-match-wins, auto-ordered)
  locations  (additive)

DEFAULTS
  add/edit/rm/move are DRY-RUN (evaluate) unless --apply/-a is given.
  rm additionally requires --force/-f to commit (combine as -af).

FIELD FLAGS (validated per stage; --stage is always required)
  values:   --pattern --canonical --operation --gateway --institution --kind --note
  features: --has-account --has-date --has-location --has-currency-code --has-amount
            (negatives --no-* only meaningful on `edit`; a --has-* feature
             requires the matching (?P<name>...) capture group in --pattern)

Colour/bold on a TTY; plain when piped, --json, or NO_COLOR set.
See `.claude/plans/rule-cli.md` and `editable-rules-ui.md` for the model.
```

---

## 9. Module / file layout

```
src/rules/                 # LIBRARY (logic only — no .sql data)
  mod.rs        # existing dump/load/Stage; re-export model+crud+impact+commit
  model.rs      # NEW  RuleData / Rule / Mutation / MoveTarget / LocationKind / RuleError
  validate.rs   # NEW  validate_draft + StageSchema (allowed flags + feature→capture conditions)
  crud.rs       # NEW  list/get/insert/update/delete/move (SQL only); load_for_compile
  impact.rs     # NEW  PayeeSample, Buckets, compute_buckets, test_one, load_payees
  activity.rs   # NEW  RuleChange::describe
  dirty.rs      # NEW  would_restage
  commit.rs     # NEW  commit(conn, mutation, DumpPolicy, Option<&RuleCache>) -> CommitResult

rules/                     # DATA (relocated from src/rules/*.sql)
  prefixes.sql suffixes.sql expansions.sql persons.sql employers.sql
  merchants.sql banking_ops.sql locations.sql

src/bin/rule.rs            # NEW binary: hand-rolled arg-parse (match over
                          #   env::args, like bin/normalise.rs) + colour/text/JSON
                          #   render (thin shell; ANSI helper, no new dep)
src/normalise/<stage>.rs   # refactored to import rules::model + load via crud (§3.1)
```
Also update (relocation itself is already done — §0): stale doc comments
in `src/rules/mod.rs` (module header + `Stage` comments) and `README.md`
that still reference `src/rules/<stage>.sql`. `rules::rules_dir()`, the
`load_committed` test helper, and `dump_reproduces_committed_files`
already read from `rules/` — no change needed.
The binary stays thin: parse → `validate_draft` → build `Mutation` →
`compute_buckets` (always) → `commit` (on `--apply`) → render. All
correctness-bearing code is unit-testable without the binary.

---

## 10. Testing pyramid

### 10.0 The keystone hermetic test (one function: old → modify → new)
A single in-memory test that needs **no on-disk fixtures** and exercises
the whole library lifecycle, so the "rules in / modification / rules out"
contract is provable in one place:
```rust
#[test]
fn edit_a_rule_end_to_end_in_memory() {
    // 1. OLD RULES: schema-only DB, seed a tiny hermetic rule set in-code
    //    (not the production seed) — one merchant "Uber" + one payee txn.
    let conn = db::initialize_in_memory().unwrap();
    crud::insert_rule(&conn, &RuleData::Merchant {
        canonical: "Uber".into(), pattern: "(?i)UBER".into(), note: None }).unwrap();
    seed_txn(&conn, 1, 1, "UBER *EATS SYDNEY", "UBER *EATS SYDNEY");

    // 2. EVALUATE the modification (pure, writes nothing)
    let m = Mutation::Edit { id: 1, data: RuleData::Merchant {
        canonical: "Uber".into(), pattern: r"(?i)UBER(?! ?\*?EATS)".into(), note: None } };
    let buckets = impact::compute_buckets(&conn, Stage::Merchants, &m,
                     &impact::load_payees(&conn).unwrap()).unwrap();
    assert_eq!(buckets.new_fallthrough.payees, 1); // "UBER *EATS" no longer matches
    assert_eq!(crud::get(&conn, Stage::Merchants, 1).unwrap().unwrap()
                   .data.pattern(), "(?i)UBER"); // still the OLD pattern

    // 3. APPLY (single atomic commit) then READ THE NEW RULES back
    std::env::set_var("POCKETSMITH_RULES_DIR", tmpdir.path()); // dump isolation
    let res = commit::commit(&conn, &m, DumpPolicy::Sync, None).unwrap();
    assert_eq!(res.change, r"~ edited Uber (?i)UBER → (?i)UBER(?! ?\*?EATS)");
    assert_eq!(crud::get(&conn, Stage::Merchants, 1).unwrap().unwrap()
                   .data.pattern(), r"(?i)UBER(?! ?\*?EATS)"); // NEW pattern persisted
}
```
This is the template the integration tests parameterise (add / edit / rm /
move) — hermetic, fast, no HTTP, no production-seed dependence.

### 10.1 Unit (broad base — pure, in-memory, fast)
- `validate_draft`: good/empty/bad-regex/bad-operation/bad-kind per stage;
  **feature→capture-group conditions** (`--has-account` without
  `(?P<account>…)` fails with the named-group message), one test per
  feature flag.
- feature inheritance: `edit` keeps saved flags unless toggled; `--no-*`
  clears; `add` defaults all features off.
- `compute_buckets` first-match: newly-matched, **stolen** (`was: X`),
  **new-fallthrough**, unchanged; ≥6 cases incl. multi-byte payee.
- `compute_buckets` loop stage: newly-affected / no-longer-affected only.
- `test_one`: matches→canonical, misses, syntax-error.
- `crud`: insert returns id; update; delete; `move_rule` before/after keeps
  sort_order dense; loop-stage append position; UNIQUE → `RuleError::Duplicate`.
- `RuleChange::describe`: add/edit/delete wording.
- `dirty::would_restage`: count matches a hand-built fixture.
- arg-parse: `--stage` required; out-of-schema flag rejected; `-af`
  expands to `--apply --force`; evaluate-by-default; colour auto-off when
  not a TTY / `NO_COLOR`.

### 10.2 Integration (middle — drive `rule` command fns against in-mem DB)
Seeded from `rules/*.sql` + a handful of `transactions`, with
`POCKETSMITH_RULES_DIR` redirected to a temp dir so committed data is
never touched:
- `add` evaluate writes nothing; `add --apply` inserts the row, re-dumps
  `rules/merchants.sql` (assert file contents), bumps dirty count.
- `edit --apply` then re-read shows new pattern; activity line correct.
- `rm --apply` rejected without `--force`; with `-af` deletes + dumps.
- `move --before/--after --apply` reorders a prefix and a subsequent
  `normalise` scan changes a known payee's proposal (apply-order is live).
- **CLI vs serve parity:** a `commit` via `DumpPolicy::Sync` and the
  equivalent serve-style commit leave byte-identical `rule_*` rows and
  `.sql` output (the §1.2 no-divergence guarantee).
- `--json` golden files for list / evaluate / apply (stable schema);
  error JSON envelope on stderr for a duplicate / bad regex.
- exit-code assertions (0 / 1 / 2) for the §7 matrix.

### 10.3 E2E (tip — none)
Rule editing is local-only (no PocketSmith network), so no E2E band; the
integration tests are the top of the pyramid here.

### 10.4 Out-of-scope cleanup folded in (small)
Remove the `note` column from the serve Pipeline-tab rule table
(`src/bin/serve/pipeline/views.rs` `render_rule_table` + the `note`
entries in `display_columns` in `src/rules/mod.rs`), per §12 feedback;
update the affected render test. `note` remains in the DB, the dump, and
`rule show` / JSON — only the at-a-glance tables drop it.

---

## 11. Decisions — resolved in this revision

1. **`rule` is its own binary** (`src/bin/rule.rs`), not a `normalise`
   subcommand. (§2)
2. **Library logic (`src/rules/`) and rule data (`rules/*.sql`) live
   separately;** `POCKETSMITH_RULES_DIR` default **already** points at
   `rules` and the files are **already** relocated (§0 — only stale doc
   comments remain). (§1.0)
3. **`src/rules::model` is the authority** on rule data shape; `normalise`
   imports it and keeps only compiled/matching logic. (§3.1)
4. **Rule changes are singular + atomic** — one `commit` = one
   `with_operation` = one CRUD call = one activity line; no batching. (§3.6)
5. **Evaluate-by-default; `--apply`/`-a` commits; `rm` also needs
   `--force`/`-f`; `-af` combines.** No interactive prompts. (§2.1)
6. **Feature flags:** off-by-default on `add`, inherited on `edit` with
   `--no-*` to clear; each `--has-*` requires its `(?P<name>…)` capture
   group, validated with a clear message. (§2.2)
7. **`move` is anchored** (`--before <id>` / `--after <id>`), recorded as a
   single operation; no absolute `--to <pos>`. (§3.3)
8. **JSON errors → stderr** (success JSON → stdout). (§7, §13)
9. **Colour + bold on a TTY, auto-off when piped/`--json`/`NO_COLOR`.** (§4.0)
10. **Bucket sample size = 6 + `--all`** (matches GUI §14 default).
11. **Cache/dump policies provably do not diverge** between CLI and serve. (§1.2)

---

## 12. Serve note-column removal (folded in)
The current read-only Pipeline tab renders the `note` column in each
stage's rule table (`render_rule_table` driven by `display_columns`). Per
the brief, `note` is a minor field and is stripped from at-a-glance
tables: drop it from `display_columns` for every stage and from the CLI
`list`. It stays in the DB, the `.sql` dump, the JSON, and `rule show`.

## 13. JSON error stream — decided
Convention chosen: **stdout = success payload only; stderr = all errors**
(plain `error:` line, or a `{"error","code"}` envelope under `--json`).
This is the standard Unix split and keeps a redirected `--json` stream
unambiguous (valid JSON or empty). Exit code remains the primary signal.

---

Implement in red-green order: (§1.0 relocation is **done** — only stale
doc comments remain) → model+validate → crud (+ normalise refactor to
import model) → impact (compute_buckets/test_one) → activity/dirty →
commit → `rule` binary (hand-rolled arg-parse + ANSI colour + text/JSON) →
keystone + integration + golden JSON → serve `note`-column removal.

Also register the new binary in `Cargo.toml`:
```toml
[[bin]]
name = "rule"
path = "src/bin/rule.rs"
```

---

## 14. UAT-driven presentation refinements (implemented post-plan)

These landed during UAT and refine the CLI presentation layer only — no
library/semantic changes. **The GUI (Order 5) should mirror this exact
vocabulary** so the two shells stay congruent (tenet §1.4).

### 14.1 Aligned tables everywhere
`list`, `show`, and the evaluate output all render through one
`render_table` helper: bold headers, a `───` rule separator, per-column
left/right alignment. Column widths are computed from the **plain** text,
so ANSI colour codes never shift alignment. `show` is a `field | value`
table; `list` keeps the `#` apply-order column for loop stages only.

### 14.2 Regex syntax highlighting (`highlight_regex`)
Applied to every pattern shown (`list` pattern column, `show` pattern row,
evaluate `candidate:` line); returns the plain pattern when colour is off.

| element | colour |
|---|---|
| grouping brackets `( )` | dim grey (`90`) |
| group constructs `?i` / `?:` / `?P<name>` | blue (`34`) |
| every other special — escapes `\b \d`, anchors `^ $`, classes `[ ]`, quantifiers `* + ? { }`, `.`, `\|` | blue (`34`) |
| the literal match text | **bold green** (`1;32`) |

### 14.3 Bucket vocabulary (glyphs + labels)
The evaluate summary is a table (`outcome · payees · txns · value`); below
it, a single aligned **detail table** lists the changed payees.

| glyph | label | colour | old | new |
|---|---|---|---|---|
| `+` | newly matched / newly affected | green | — | canonical |
| `±` | moved from other | blue | canonical | canonical |
| `-` | new fallthrough / no longer affected | red | canonical | — |
| `·` | unchanged | dim | — | — |

The JSON key for the moved bucket stays `stolen` (stable machine
contract); only the **human label** is "moved from other".

### 14.4 Detail table columns
`payee · txns · value · old · new`. The outcome glyph (coloured) prefixes
the payee so the bucket reads with or without colour; the `(old,new)`
null-pattern alone disambiguates the bucket on first-match stages. Payees
are **sanitised** (control chars → space, whitespace collapsed) so a
multi-line bank payee stays on one row. `txns`/`value` use thousands
separators; samples cap at 6 unless `--all`.

### 14.5 Important UX consequence — `add` can't "move"
On first-match stages a new rule is appended at the **lowest priority**
(highest id), so `rule add` can only ever produce `newly matched` (it
picks up fall-through payees); it can never show `moved from other` (a
higher-priority existing rule still wins). To reassign payees between
rules you `edit` (broaden a higher-priority rule) or `move` (promote one).
The GUI must surface the same reality (e.g. "new rules apply last; reorder
to take precedence").

### 14.6 Test isolation — inject the dump dir, don't use env
`DumpPolicy::Sync(PathBuf)` carries the target directory (production
passes `rules::rules_dir()`); `dump_stage_to(conn, stage, dir)` is the
injected-dir form of `dump_stage`. Tests pass a unique temp dir instead
of mutating the process-global `POCKETSMITH_RULES_DIR`, so the suite runs
fully in parallel with no serialisation. (Temp-dir names include an
atomic counter to stay unique under concurrent calls.)
