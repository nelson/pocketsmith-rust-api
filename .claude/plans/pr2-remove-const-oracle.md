# PR 2 — `remove-const-oracle`

Branch: `remove-const-oracle` (depends on PR 1 `feat/pipeline-trace-two-line`, already landed).

## Context

The normalise pipeline historically carried two implementations of every
matcher/strip stage:

1. **Production path** — `apply_with_db()`, loads + compiles rules from the
   SQLite `rule_*` tables (the only path used at runtime).
2. **`#[cfg(test)]` const oracle** — `apply()` + hard-coded const
   dictionaries (`MERCHANTS`, `PREFIXES`, `SUFFIXES`, `EXPANSIONS`, person/
   employer/banking consts) + `compiled_*()` compilers, kept only so tests
   could assert the DB path reproduces the const path byte-for-byte
   (`db_apply_matches_const_oracle` per stage, plus the
   `--features fidelity` real-DB comparison in `mod.rs`).

Now that the DB tables are the single source of truth (mirrored to
`src/rules/*.sql`) and rules are about to become **editable**, the const
oracle is dead weight: it duplicates the rule corpus in Rust, must be kept
in lockstep with the `.sql` seeds, and would actively contradict any
user-edited rule. PR 2 retires it. This is a hard prerequisite before any
later PR can mutate the live DB.

Goal: **net-negative LOC**; the hermetic per-stage
`*_stage_reads_its_rules_from_the_db` tests remain the permanent coverage
of the load→compile→apply→capture machinery. Also adds `updated_at`
triggers to the `rule_*` tables so future edits are timestamped.

## Scope of the const oracle (what exists today)

Per-stage const oracle lives in (each has `#[cfg(test)] fn apply()`,
`#[cfg(test)]` const dict, `compiled_*()`, a `db_apply_matches_const_oracle`
test, and a body of behaviour `test_*` cases that call `apply()`):

| File | const dict | behaviour `test_*`/`assert_*` calling `apply()` |
|------|-----------|---------------------|
| `merchants.rs` | `MERCHANTS` | ~70 |
| `suffix.rs` | `SUFFIXES` | ~28 |
| `prefix.rs` | `PREFIXES` | ~21 |
| `expand.rs` | `EXPANSIONS` | ~21 |
| `banking_ops.rs` | banking consts | ~21 |
| `persons.rs` | `KNOWN_PERSONS` | ~5 |
| `employers.rs` | employer consts | ~5 |
| `locations.rs` | — (already DB-only, no oracle) | 0 |

`mod.rs`:
- `#[cfg(feature = "fidelity")] fn converted_stages_db_matches_const_on_real_payees` — real-DB const comparison.
- `#[cfg(feature = "fidelity")] fn location_extraction_coverage_on_real_payees` — coverage assertion (>4000 suburbs). Per §8 decision: **also deleted**, feature removed.
- Hermetic `*_stage_reads_its_rules_from_the_db` tests — **kept** (the replacement).

`Cargo.toml`: `[features] fidelity = []` — removed entirely (per §8 decision 1).

## Resolved decision — hybrid: convert a tricky subset, delete the rest

The ~150 behaviour `test_*` cases (e.g. `test_woolworths`,
`assert_merchant("woolworths", "Woolworths")`) all call the const-backed
`apply()`. **Decision (hybrid):** convert only the ~3–5 *genuinely tricky*
patterns per stage to run against a `seeded_in_memory()` pipeline via
`apply_with_db`; **delete the rest**.

Rationale: the hermetic `*_stage_reads_its_rules_from_the_db` tests already
prove the machinery (load→compile→apply→capture) against toy rules. The
bulk of the behaviour tests only assert *seed-data* facts — which become
low-signal or stale once rules are user-editable (PR 4+). But a few encode
non-obvious regex intent that a toy-rule hermetic test can't catch and that
a careless seed edit could break — those are worth keeping as DB-backed
guards. Keeps the PR net-negative LOC while preserving high-signal
coverage.

Tricky patterns to KEEP (convert to `apply_with_db`), per stage — pick the
ones whose regex does real work, e.g.:
- `merchants.rs`: no-space `TRANSPORTFORNSWTRAVEL` → Transport for NSW;
  apostrophe-optional `DIGGY DOO'S`/`DIGGY DOOS`; `MAMAKSMLC`/`MAMAK
  VILLAGE` alternation; `UBER *TRIP` vs `UBER EATS` vs bare `UBER`
  ordering; `AMAZON PRIME` vs `AMAZON` ordering.
- `suffix.rs` / `prefix.rs` / `expand.rs` / `banking_ops.rs`: 3–5 each that
  exercise named-capture extraction, ordering, or multi-alternation —
  delete plain single-literal cases.
- `persons.rs` / `employers.rs`: keep 1–2 representative each.

Delete: plain `assert_*("woolworths", "Woolworths")`-style single-literal
cases and near-duplicates.

Conversion mechanics (keep it cheap — seed once per stage, per thread):

```rust
#[cfg(test)]
thread_local! {
    static PIPELINE: OwnedPipeline = OwnedPipeline::seeded_in_memory().unwrap();
}
```

- Each stage module keeps a small `#[cfg(test)]` helper that seeds the
  thread-local pipeline, runs `apply_with_db`, and returns the result (or,
  for `merchants.rs`, rewrite the existing `assert_merchant` helper once).
  Only the KEPT tricky tests call it; everything else is deleted.
- Using a `thread_local!` `OwnedPipeline` avoids re-parsing every
  `src/rules/*.sql` once per test while staying `Send`-free (the
  `Connection` never crosses threads).
- Net result is strongly negative LOC (whole const dicts + ~140 tests
  removed; a handful of tests rewired).

## Files to modify

- `src/normalise/merchants.rs`, `suffix.rs`, `prefix.rs`, `expand.rs`,
  `banking_ops.rs`, `persons.rs`, `employers.rs` — remove oracle.
- `src/normalise/mod.rs` — remove the two `fidelity`-gated tests.
- `src/db/schema.rs` — add `rule_*_updated_at` triggers.
- `Cargo.toml` — remove `fidelity` feature.
- `.github/workflows/*` — drop any `--features fidelity` invocation if present.

## Reuse

- `OwnedPipeline::seeded_in_memory()` (`src/normalise/cache.rs`, re-exported
  in `mod.rs`) — builds an in-memory DB seeded from `src/rules/*.sql`;
  already used by the kept tests. The basis for any test conversion.
- Existing `payee_normalisations_updated_at` / `transfer_pairs_updated_at`
  triggers in `schema.rs` — the template for the new `rule_*` triggers
  (`CREATE TRIGGER IF NOT EXISTS`, `WHEN NEW.updated_at = OLD.updated_at`).

## Steps

- [ ] Per stage file: delete `#[cfg(test)] fn apply()`, the const dict
      struct + array, `compiled_*()`, and `db_apply_matches_const_oracle`.
- [ ] Add a thread-local seeded-pipeline test helper per stage module;
      convert ~3–5 tricky `test_*` per stage to `apply_with_db`; **delete**
      the remaining single-literal / duplicate behaviour tests.
- [ ] Remove now-unused `#[cfg(test)] use` imports (e.g. `OnceLock`) and
      `#[cfg(test)]` helper structs.
- [ ] `mod.rs`: delete both `fidelity`-gated tests.
- [ ] `Cargo.toml`: remove `[features] fidelity = []`.
- [ ] `schema.rs`: add `CREATE TRIGGER IF NOT EXISTS rule_<t>_updated_at`
      for each of the 8 `rule_*` tables (keyed on `id`). No
      `RULES_SCHEMA_VERSION` bump (trigger-only, no re-seed).
- [ ] Add a schema test: `UPDATE rule_merchants SET canonical=…` bumps
      `updated_at`; a fresh `INSERT` leaves `created_at == updated_at`.

## Verification

- [ ] `cargo build --features web` — no unused-import / dead-code warnings.
- [ ] `cargo test --features web` — all lib + serve tests pass.
- [ ] `cargo test --features fidelity` — **fails to compile** (feature gone);
      confirm no remaining references.
- [ ] `grep -rn "oracle\|fidelity\|fn apply(" src/normalise` returns only
      doc-comment mentions that were intentionally reworded (or nothing).
- [ ] Net diff is negative (deleted >> added).
