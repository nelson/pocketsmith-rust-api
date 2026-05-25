# Plan: Editable normalisation rules (v2)

Status: **deferred — not part of the Dashboard/Transactions/Review v1.**
Branch: same `plan/transactions-and-dashboard-tabs`, but no code on this
plan lands until v1 is done and we've felt the pain of "wanting to fix a
rule from the UI".

This is the follow-up to §6 of `PLAN-transactions-and-dashboard.md`,
where we compared four options (A: DB dictionaries, B: override list,
C: hybrid, D: scripted) and chose **C** for v2. This file is the
implementation plan for C.

---

## 1. What "rule changes" means today (v1)

Today, when a payee normalises wrong, the workflow is:

1. Edit `src/normalise/*.rs` (add a merchant alias, extend a suffix
   list, fix a stage's regex, etc).
2. `cargo build`.
3. Restart `serve`.
4. Re-run the normalisation pipeline (`cargo run --bin normalise`).
5. Review the freshly-staged proposals in the Normalise / Transactions
   / Review tabs.

This works. It's a code edit, not a data edit. For a single-user tool
on a tight feedback loop, it's not much friction — but it does mean
"the rule store is the Git history of `src/normalise/`", which is
honest but means rule changes can't happen from the web UI.

**v1 deliberately does not change this.** The Transactions and Review
tabs surface *where* rules need to change. Actually changing them is
still a Rust edit.

## 2. When v2 should trigger

Build v2 when one of these is true:

- You're editing `src/normalise/` more than ~once a week to add merchant
  aliases or strip-list entries (i.e., the data is changing more than
  the structure).
- You want a non-developer (future you on a phone, a partner, etc.) to
  be able to fix a normalisation mistake.
- Multiple developers want to add merchant aliases without merge
  conflicts on a single Rust source file.

Not before. The migration is non-trivial and only pays off if you'd
actually use it.

## 3. Design summary (Option C)

Two storage layers, each consumed by an existing pipeline stage:

```
                      transactions.original_payee
                                  |
                                  v
   +-----------------------+ pipeline (Rust) +------------------------+
   |  pos_prefixes (DB)    |    stage 1: strip_pos_prefix             |
   |  country_suffixes(DB) |    stage 2: strip_country                |
   |  suburb_suffixes(DB)  |    stage 3: strip_suburb_suffix          |
   |  -- titlecase (code)--|    stage 4: titlecase  [pure Rust]       |
   |  merchant_aliases(DB) |    stage 5: classify_known_merchants     |
   |  payee_overrides(DB)  |    stage 6: apply_override (LAST WIN)    |
   +-----------------------+                                          |
                                  v                                   |
                      payee_normalisations (staging) <----------------+
                                  v
                              review tabs
                                  v
                         transactions.payee (on apply)
```

Stages whose *behaviour* is data-driven (1, 2, 3, 5, 6) load their
dictionaries from SQLite. Stages whose behaviour is pure logic (4 —
titlecase) stay in Rust unchanged.

### 3.1 Schema

```sql
-- Each row is one entry the corresponding pipeline stage will use.
CREATE TABLE pos_prefixes      (prefix TEXT PRIMARY KEY,
                                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
CREATE TABLE country_suffixes  (suffix TEXT PRIMARY KEY,
                                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));
CREATE TABLE suburb_suffixes   (suffix TEXT PRIMARY KEY,
                                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')));

-- merchant_aliases is the fuzzy-match dictionary used by classify_known_merchants.
-- pattern is matched against the post-titlecase normalised string.
CREATE TABLE merchant_aliases (
    pattern     TEXT PRIMARY KEY,    -- substring match (case-insensitive)
    replacement TEXT NOT NULL,        -- canonical merchant name
    class       TEXT,                 -- e.g. 'cafe', 'shopping', 'subscription'
    note        TEXT,                 -- optional human note
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- payee_overrides is the last-resort escape hatch. Keyed on the RAW
-- original_payee — sidesteps the pipeline entirely for one specific string.
CREATE TABLE payee_overrides (
    original_payee TEXT PRIMARY KEY,
    forced_payee   TEXT NOT NULL,
    class          TEXT,
    note           TEXT,              -- "why did I override this?"
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
```

All five tables get an `updated_at` trigger like `payee_normalisations`
already has.

### 3.2 Pipeline changes

In `src/normalise/`:

- Each data-driven stage gets a `Stage::run(conn, input) -> Result`
  signature (currently they may take a `&Connection` already; if not,
  thread one through).
- Stages cache the dictionary in memory per pipeline run (single
  `SELECT *` at the top), so cost is one query per stage per run.
- A new last stage `apply_override` does
  `SELECT forced_payee, class FROM payee_overrides WHERE original_payee = ?`.
  If hit, it replaces the current state and emits a `TraceEntry`
  with `stage = "override"` so the pipeline trace makes the override
  visible to the reviewer.

The trace already supports per-stage transformation logging (see
`src/normalise/`'s `TraceEntry`). Override hits show up as their own
entry, marked with a `stage = "override"` label, so the reviewer never
loses context for *why* a payee normalised the way it did.

### 3.3 Migration / seed

One-shot migration (run once on the dev's machine, then committed as
a SQL fixture for fresh DBs):

```bash
# Dump current in-code dictionaries
cargo run --bin dump-rules > rules.sql
# Apply
sqlite3 pocketsmith.db < rules.sql
```

Add a `bin/dump-rules.rs` that prints `INSERT INTO ...` statements for
every dictionary entry currently hardcoded in `src/normalise/`. After
this lands, the in-code defaults are deleted.

`db::initialize` gets an idempotent block that creates the new tables
if missing. Existing seed data ships either as bundled SQL or as a
small `seed_rules` function called only when the tables are empty.

### 3.4 UI surfaces

A new sub-pane on the **Review** tab. Three tabs within Review:

- **Leverage** — the existing Review queue (what to clean next).
- **Rules** — CRUD on the five dictionary tables. Looks like the
  Normalise tab's queue/detail layout: list of entries in `#queue`,
  edit form in `#detail`. Same keyboard shortcuts.
- **Overrides** — the `payee_overrides` table, listed by use-count
  (how many transactions match each override). The "promote" nudge
  lives here: any override whose pattern appears in 3+ entries gets a
  badge prompting "promote to a `merchant_aliases` rule".

When the user clicks `[N] Reject` on a normalisation proposal in the
Normalise/Transactions tab, the detail panel offers a follow-up:
"want to override?" — opens the override editor pre-filled with the
original_payee. This is the on-ramp from "the pipeline got it wrong"
to "fix it now from the UI".

### 3.5 The "promote override → rule" nudge

When 3+ rows in `payee_overrides` share a substring (case-insensitive,
of length ≥ 4), surface a nudge:

```
3 overrides match "AMAZON":
  AMAZON MARKETPLACE       → "Amazon Marketplace"
  AMAZON.COM*MK4F2         → "Amazon"
  AMAZON PRIME*MEMBERSHIP  → "Amazon Prime"

Promote to a merchant_aliases rule?
  pattern: "amazon"  →  "Amazon" / class: shopping
  [ Promote ]   [ Dismiss ]
```

Promoting writes a `merchant_aliases` row and (at the user's choice)
deletes the now-redundant overrides. This keeps the override table
small and turns repeated manual fixes into general rules over time.

The matching algorithm is deliberately simple — longest common
substring across the override patterns, capped at top N matches per
session. We'd surface at most ~5 nudges at a time so they're
suggestions, not nags.

## 4. Testing strategy

### 4.1 Unit tests

- Each pipeline stage gets a test that loads a known-state DB
  (in-memory SQLite) with fixture rows and asserts the transformation.
- The override stage gets a test that asserts: pipeline output ==
  override target, *regardless* of what the prior stages produced
  (last-write-wins semantics).
- The "promote" matcher gets fuzz-style tests for substring
  detection over 100 random override sets.

### 4.2 Integration tests

- Round-trip: `INSERT INTO merchant_aliases ...` → run pipeline → assert
  `payee_normalisations` row has the expected proposed_payee.
- Round-trip: `INSERT INTO payee_overrides ...` → run pipeline → assert
  override wins regardless of upstream stages.
- Promotion flow: insert 3 matching overrides → call promote endpoint
  → assert merchant_aliases row created and overrides deleted.

### 4.3 Migration test

A test that runs `seed_rules` on a fresh DB, then asserts the pipeline
on a fixture transaction produces the same result as the v1 in-code
pipeline did. Belt-and-braces guarantee the migration didn't lose
behaviour.

## 5. Build order (when v2 starts)

1. Schema migration: create the five tables. Add `seed_rules` that
   populates them from the in-code defaults. Tests assert seed →
   table contents.
2. Convert one pipeline stage to read from its table (start with the
   smallest one — probably `pos_prefixes`). Existing tests must still
   pass (this is the fidelity check).
3. Convert remaining stages, one per commit.
4. Add the override stage. Override-wins tests.
5. Delete the in-code defaults. The seed function is now the source
   of truth for fresh DBs.
6. Review tab: Rules sub-pane (read-only listing first).
7. Review tab: Rules sub-pane editing (CRUD endpoints + form).
8. Review tab: Overrides sub-pane.
9. Override editor on-ramp from the Normalise/Transactions reject
   action.
10. Promote-override nudge.

Each step is a small commit; each ships independently. Steps 1–5 are
infrastructure (no UI change visible). Steps 6–10 are the UI work.

## 6. Risks and what we're explicitly *not* building

- **No scripted/expressive rules** (Option D from the original plan).
  Rules remain declarative dictionary entries plus regex-free string
  match. If you find yourself wanting "give me a rule that does X for
  every payee on Tuesdays where the amount is even", revisit.
- **No version history on rules.** SQLite's `_changes` mechanism could
  capture it, but isn't worth the schema overhead for v2. Add later
  if you want to know "when did Amazon start mapping to that?".
- **No multi-tenant rule sharing.** This is a single-user tool.
- **No rule-change preview** ("if I add this alias, what would
  re-normalise?"). Might be a v3 feature; for v2 you just add the
  rule and re-run the pipeline to see.

## 7. Cost estimate

Steps 1–5 (infrastructure, no UI): roughly the same effort as the
Transactions tab on its own. ~2 days of focused work.

Steps 6–10 (UI): roughly half a Transactions tab. ~1 day.

Total: ~3 days of work when triggered. Worth keeping in mind so we
don't accidentally pay for it during v1.
