# editable-rules-v3 — autonomous implementation progress & decisions

> Written by the agent while you were away. Please review the **Decisions
> needing your sign-off** section when you return. Everything else is
> FYI / status.

## Branch layout

Each PR is its own branch, stacked on the previous one (you said you'll
rebase). Base of the stack is `plan/editable-rules` (so the plan docs +
mockups are in-tree); functionally that's `master` + the plan commits.

```
plan/editable-rules
  └─ feat/editable-rules-pr0   (PR 0: trace-empty norm state)
       └─ feat/editable-rules-pr1   (PR 1: schema + rules module + dump_rules + freshness)
            └─ feat/editable-rules-pr2   (...continues while you're away)
```

All tests green at each commit (`cargo test --features web`).

## Status by PR

- **PR 0 — DONE.** `transactions: only flag "no normalisation" when trace is empty`.
  Added `NormState::Clean` (no staging row but non-empty pipeline trace —
  the steady state after confirm+apply, or an already-normalised import).
  Renders a benign label glyph + "Payee normalised" pillar instead of the
  red "No normalisation rule". Queue derivation memoises the pipeline
  trace per distinct payee, only for rows lacking a staging row (~tens of
  ms on a full 1000-row queue; benchmarked normalise at ~38µs/call).

- **PR 1 — DONE (with one carve-out, see Decisions #1).**
  - 8 `rule_*` tables + `_meta(key,value)` added to `schema.rs`.
  - New `src/rules/` module: `Stage` enum, `load_into_db`, `dump_stage`,
    `dump_all`, `schedule_dump`, `bootstrap_from_constants`,
    `dump_stage_to_string`. Canonical store is `src/rules/*.sql`.
  - `seed_rows()` accessors added to each `normalise::<stage>` module
    (the bridge from the in-code constants to the DB tables). Stage
    modules made `pub(crate)`.
  - `src/bin/dump_rules.rs` binary — regenerates the 8 SQL files from the
    constants. Committed the generated files.
  - `load_into_db` wired into serve startup (seeds empty tables from the
    SQL files, falls back to constants on first-ever boot).
  - `freshness` module (header chip data + markup) implemented and tested
    — see Decisions #1 for why it isn't wired into a header yet.

## Decisions needing your sign-off

### 1. The `pushed N ago` / `synced N ago` header chip has no header to live in on `master`

The plan (§4.2) says "the header strip already has a `synced 3h ago`
chip; add a sibling `pushed 3d ago` chip." That header strip
(`render_header`, `last_sync_info`, `sync_chip_class`, …) only exists on
the **unmerged `feature/dashboard-mvp` branch**, not on `master`. On this
base, `render_page` only renders the tab bar — there is no header strip.

**What I did:** implemented `src/bin/serve/freshness.rs` — a generalised,
fully-unit-tested data+markup helper that computes either chip
(`synced`/`pushed`) from `_operations` filtered by `reason`, with the same
fresh/stale/old buckets. It is **not yet wired into a page** (marked
`#![allow(dead_code)]`), because forking the whole dashboard header into
this branch would be speculative and would conflict badly on rebase.

**What I need from you:** confirm the plan: should PR 1 land the header
chip against the dashboard branch (i.e. this stack should be rebased onto
`feature/dashboard-mvp` rather than `master`)? Or keep it on `master` and
wire the chips when dashboard merges? The `freshness` helper is ready to
drop into dashboard's `render_header` either way.

### 2. Persons stage has no `sort_order`, but its apply() is order-sensitive

The plan's schema for `rule_persons` has no `sort_order`, and §2 lists
persons as "order doesn't matter (alphabetical UI)". But the in-code
`KNOWN_PERSONS` table **is** order-sensitive: generic title patterns
(`MR`/`MISS`/`MRS` → "Unknown Person") and single-token fallbacks
(`TAM`,`LOK` → "Nelson Tam") are declared **last** so specific patterns
win first under first-match-wins.

For PR 1 (infra only — pipeline still uses constants) I preserved
declaration order via the autoincrement `id` (dump/load round-trips by
`id`), so nothing is lost yet. **But PR 6 (persons conversion) will need a
decision:** if the persons rule list is queried `ORDER BY canonical`
(alphabetical, per plan), the generic fallbacks will no longer be last and
behaviour will change for ambiguous inputs. Options: (a) add `sort_order`
to `rule_persons` after all; (b) special-case the generic catch-alls; (c)
accept the behavioural change. I lean (a). Flagging now so it's not a
surprise in PR 6.

### 3. `dump_rules` regenerates from constants, not from the live DB

`cargo run --bin dump_rules` builds an in-memory DB, seeds from the
in-code constants, and dumps. It is a **bootstrap/recovery** tool. The
live `src/rules/*.sql` files are kept current by serve's per-mutation
background dumps (`schedule_dump`), not by this binary. Running it after
UI edits would overwrite them with the constants. I documented this in the
binary's header comment. If you'd prefer `dump_rules` to dump the
*current* DB by default, say so.

### 4. Seed counts differ from the plan's table

Plan §2 lists banking_ops = 38 and locations = 91; the actual code has 26
banking-op patterns and 95 locations (and 118 person *patterns* across 82
canonicals, 5 employer patterns across 4 canonicals). I seeded faithfully
from the code (the source of truth), so the counts reflect reality, not
the plan's prose. No action needed unless the plan's numbers were a target.

## Notes / smaller choices

- `rule_*` tables: app-owned, no underscore prefix (Convention C). `_meta`
  is framework-ish so it keeps the underscore. Passes `schema_conventions`.
- Dumps omit `id`/`created_at`/`updated_at` (ids re-assigned on load to
  preserve order; timestamps fall back to DEFAULT) — keeps git diffs clean
  and makes dump→load→dump byte-identical (there's a round-trip test).
- No `updated_at` auto-update triggers on the rule tables yet (nothing
  updates them until the Pipeline tab lands). The UI mutation code (PR 4+)
  will set `updated_at` explicitly or I'll add triggers then.
- `POCKETSMITH_RULES_DIR` env var overrides the `src/rules` location (used
  by tests / isolated runs).
