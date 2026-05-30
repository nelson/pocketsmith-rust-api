# editable-rules-v3 — autonomous implementation progress & decisions

> Written by the agent while you were away. Please review the **Decisions
> needing your sign-off** section when you return. Everything else is
> FYI / status.

## Branch layout

PR 0 and PR 1 are **merged to `master`**. Remaining PRs continue as
their own branches, each stacked on the previous (rebase as you go).

```
master  (← PR 0 + PR 1 merged)
     └─ feat/editable-rules-pr2   (PipelineCtx + RuleCache)
          └─ … PRs 3–10
```

All tests green at each commit (`cargo test --features web`).

## Status by PR

- **PR 0 — ✅ MERGED.** `transactions: only flag "no normalisation" when trace is empty`.
  Added `NormState::Clean` (no staging row but non-empty pipeline trace —
  the steady state after confirm+apply, or an already-normalised import).
  Renders a benign label glyph + "Payee normalised" pillar instead of the
  red "No normalisation rule". Queue derivation memoises the pipeline
  trace per distinct payee, only for rows lacking a staging row (~tens of
  ms on a full 1000-row queue; benchmarked normalise at ~38µs/call).

- **PR 1 — ✅ MERGED.** (Revised per review — see "Review feedback" below.)
  - 8 `rule_*` tables + `_meta(key,value)` added to `schema.rs`.
  - New `src/rules/` module: `Stage` enum (`name()` + derived `table()`),
    `load_into_db`, `dump_stage`, `dump_all`, `schedule_dump`,
    `dump_stage_to_string`. Canonical store is `src/rules/*.sql` — the
    sole source of truth for the seed (decoupled from the in-code consts).
  - `src/bin/dump.rs` binary (renamed from `dump_rules`) — dumps the
    **live DB** to the 8 SQL files (export / recovery). Committed files
    carry a fixed historical `created_at`/`updated_at`
    (`2026-04-03T12:40:21Z`, the git-blame date of the original rules) so
    rule age is stable across re-seeds; the columns round-trip on dump.
  - `load_into_db` wired into serve startup (seeds empty tables from the
    SQL files; errors clearly if a file is missing).
  - `freshness` module wired into the header strip: `synced` + `pushed`
    chips, each with a shape-distinct glyph (fresh leaf / fallen leaf /
    bare tree / question-mark) so state reads without colour
    (accessibility). The old bespoke `sync-chip` in `render.rs` was
    removed in favour of it.
  - REMOVED in review: `src/rules/seed.rs`, the 8 `seed_rows()`
    accessors, and `bootstrap_from_constants`/`bootstrap_stage` — the
    `*.sql` files replace the constants→DB bridge.

- **PR 2 — ✅ MERGED.** `normalise: PipelineCtx + RuleCache plumbing (no
  behaviour change)`. Threaded a `PipelineCtx { conn, cache }` through
  `normalise()`, `scan`, and the serve views; `RuleCache` is an empty
  skeleton (no slots yet). Stages still read constants (`let _ = ctx`),
  so output is byte-identical — proven by scanning the real DB on PR2 vs
  master (7,236 staged rows identical). `OwnedPipeline::seeded_in_memory`
  seeds tests from `src/rules/*.sql` via `load_into_db`.

- **PR 3 — ✅ MERGED.** `serve: Pipeline tab shell (queue + stub detail +
  activity)`. New **Pipeline** nav tab (3rd slot). Read-only shell:
  `/pipeline/` lists the 8 stages in execution order with live rule
  counts + semantic tags; `/pipeline/stage/<slug>` is the HTMX detail
  fragment (records the active stage). Added `rules::count` +
  `Stage::from_name`. GET-only — editing lands per stage.

- **PR 4a — ✅ MERGED (this PR).** `pipeline(prefix+suffix): convert to
  DB-backed rules`. Converted the `prefix` and `suffix` stages to read
  from the DB via `RuleCache` (`apply_with_db` + `load_compiled`; cache
  slots + `invalidate`). `normalise()` drives those two stages from the
  rule tables; the other six still use constants. The const `apply()` /
  `compiled_*` for each converted stage is kept as a `#[cfg(test)]`
  fidelity oracle (and the `PREFIXES`/`SUFFIXES` const tables are now
  test-only). New `fidelity` cargo feature + real-DB test:
  `cargo test --features fidelity` confirms the DB path is byte-identical
  to the const path across **21,046 payee×stage pairs** (10,523 distinct
  `original_payee`s × 2 stages).
  - **4b (REMAINING):** the Pipeline-tab editor UI for prefix/suffix —
    rule list, Edit/Evaluate card, categorical impact buckets, dirty
    banner + re-scan, and create/edit/delete/reorder mutations with
    background dump. Deferred for review (see Decision #5).

## Review feedback — addressed in PR 1

- **`mod.rs` slimmed**: merged `table()`/`file_stem()` into one `name()`.

## Decisions

### 1. Freshness header chip — ✅ RESOLVED (PR 1)

Dashboard is merged, so the header strip exists. PR 1 wired both the
`synced` and `pushed` chips into `render_header` via
`freshness::header_chips`, replacing the old bespoke `sync-chip`. Chips
encode state with a shape-distinct glyph (not colour alone) for
accessibility, and tint the whole chip (border + bg + label).

### 2. Persons stage has no `sort_order`, but its apply() is order-sensitive — ⚠️ STILL OPEN (PR 6)

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

### 3. `dump` reads the live DB — ✅ RESOLVED (PR 1)

The binary was renamed `dump_rules` → `dump` and now dumps the **live
`POCKETSMITH_DB`** to `src/rules/*.sql` (export / recovery), not the
in-code constants. The constants→DB bridge (`seed.rs`, `seed_rows()`,
`bootstrap_*`) was removed; the `*.sql` files are the canonical seed.

### 4. Seed counts differ from the plan's table — ℹ️ FYI (no action)

Plan §2 lists banking_ops = 38 and locations = 91; the actual code has 26
banking-op patterns and 95 locations (and 118 person *patterns* across 82
canonicals, 5 employer patterns across 4 canonicals). I seeded faithfully
from the code (the source of truth), so the counts reflect reality, not
the plan's prose. No action needed unless the plan's numbers were a target.

### 5. PR 4 split: stage conversion done (4a), editor UI deferred (4b) — ⚠️ STILL OPEN

The plan bundles PR 4 as "convert prefix+suffix to DB **and** build the
Pipeline-tab editor UI (rule list, Edit/Evaluate, categorical impact,
dirty banner, re-scan, mutations)" — exactly the surface you wanted to
review carefully (§10). PR 4a landed the **conversion core** (highest-
value, fidelity-critical, behaviour-identical, proven against the real
DB). The editor UI (4b) is deferred so it can be built/steered under
review. The `RuleCache`/`apply_with_db`/`load_compiled` pattern from 4a
is the template for converting the remaining six stages (PRs 5–8).

## Notes / smaller choices

- `rule_*` tables: app-owned, no underscore prefix (Convention C). `_meta`
  is framework-ish so it keeps the underscore. Passes `schema_conventions`.
- Dumps omit only `id` (re-assigned on load to preserve order).
  `created_at`/`updated_at` **are** dumped — seed rows carry a baked
  historical timestamp (`2026-04-03T12:40:21Z`) so rule age is stable
  across re-seeds, and a `dump→load→dump` round-trip is byte-identical
  (there's a round-trip test + a `dump_reproduces_committed_files` test).
- No `updated_at` auto-update triggers on the rule tables yet (nothing
  updates them until the Pipeline tab lands). The UI mutation code (PR 4+)
  will set `updated_at` explicitly or I'll add triggers then.
- `POCKETSMITH_RULES_DIR` env var overrides the `src/rules` location (used
  by tests / isolated runs).
