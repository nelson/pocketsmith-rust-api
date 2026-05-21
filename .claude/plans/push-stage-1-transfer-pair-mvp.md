# Push Stage 1 — Transfer-pair push (Learning MVP)

> See [push-overview.md](./push-overview.md) for the rollout context and stage table. This is the only stage committed to in full implementation detail; Stages 2–5 are sketched with explicit open questions for resolution at the time they begin.

> **Naming note (2026-05-21):** what earlier drafts called "writeback" is now consistently called "push" — the binary, the module, the table, the column, the log entries, the outcomes. Several DB objects were also renamed since the original draft: `_transactions_history` → `_transaction_changes`, `_transaction_change_log` → `_operations`, `_version` → `operation_id`, `_rowid` → `transaction_id`, `_mask` → `mask`. The plan below uses the current names.

## Scope

Push the local result of `transfers --apply` to Pocketsmith: for each confirmed transfer pair, PUT both `is_transfer=true` and `category_id=<transfer category>` on each of the two transactions. Nothing else. Driven entirely from DB state — no manual ids.

## Why these two fields, together

`transfers --apply` writes both in a single `UPDATE transactions SET category_id=?, is_transfer=1 WHERE id=?` (`src/bin/transfers.rs`), so the resulting `_transaction_changes` row has `mask = 18` (bits 2|16). Splitting the push would create a half-pushed state; pushing both together matches the source write exactly.

## Safety: timestamp guard

Before each PUT:
1. `GET /v2/transactions/{id}` (existing `PocketSmithClient::get_transaction`).
2. Compare remote `updated_at` against the value stored in local `transactions.updated_at` (this was last refreshed when we pulled).
3. If they differ → **abort this txn**. No PUT. No mark. Record a one-line log entry: `txn={id} skipped: remote updated_at {remote} != local {local}`.

This is intentionally blunt — it will sometimes refuse when the upstream change was to an unrelated field. That's fine: those false-positives are Stage 2's observation data. Real conflict detection is Stage 4.

## Data model changes

### 1. New column on `_transaction_changes` (the "pushed" marker)

```sql
ALTER TABLE _transaction_changes ADD COLUMN pushed_at TEXT;
```

(No underscore prefix on the column itself — `_transaction_changes` is already a framework table by Convention C, and inner columns follow the same `created_at` / `updated_at` style.)

Migration: in `src/db/mod.rs`, after `conn.execute_batch(SCHEMA)`, run a `PRAGMA table_info('_transaction_changes')` check and `ALTER TABLE … ADD COLUMN pushed_at TEXT` if absent. Idempotent.

Update the `CREATE TABLE _transaction_changes` in `src/db/schema.rs` so fresh DBs already have the column.

### 2. New table `push_log` (the audit trail — one row per attempt, regardless of outcome)

By Convention C this is application-readable and not driven by triggers, so it gets no underscore prefix (mirrors `transfer_pairs`).

```sql
CREATE TABLE IF NOT EXISTS push_log (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    txn_id                   INTEGER NOT NULL,
    attempted_at             TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    outcome                  TEXT NOT NULL CHECK(outcome IN
                               ('pushed','would_push','skipped_changed_upstream','deleted_upstream','failed')),
    local_updated_at_before  TEXT,
    remote_updated_at_seen   TEXT,
    request_body             TEXT,   -- JSON serialisation of TransactionUpdate; NULL when no PUT issued
    response_body            TEXT,   -- JSON of returned Transaction; NULL unless outcome='pushed'
    error_message            TEXT    -- populated when outcome='failed'
);
CREATE INDEX IF NOT EXISTS idx_push_log_attempted_at ON push_log(attempted_at);
CREATE INDEX IF NOT EXISTS idx_push_log_txn_id ON push_log(txn_id);
```

This is the reason Stage 1 is called the Learning MVP — Stage 2 observation runs entirely off this table.

No `push_conflicts` table yet (Stage 4). No new client functions — `PocketSmithClient::get_transaction` and `update_transaction` are already in `src/client.rs` on master.

## Pending query

```sql
SELECT DISTINCT c.transaction_id
FROM _transaction_changes c
JOIN _operations o ON c.operation_id = o.id
WHERE (c.mask & 16) != 0          -- is_transfer bit
  AND o.reason = 'transfers'       -- only from /review-transfers --apply
  AND c.pushed_at IS NULL
  AND EXISTS (SELECT 1 FROM transactions t WHERE t.id = c.transaction_id);  -- skip locally-deleted
```

Then, per txn id: collect every change row's `id` whose `(mask & 18) != 0` and `pushed_at IS NULL` — those are the rows we'll mark on success.

## Algorithm

```
wrap entire run in db::with_operation(conn, "push", |conn| { ... })

for each txn_id in pending (optionally truncated to opts.limit):
    match run_one_txn(api, conn, txn_id, opts):
        Ok(Outcome::Pushed)                  => stats.pushed += 1
        Ok(Outcome::WouldPush)               => stats.would_push += 1
        Ok(Outcome::SkippedChangedUpstream)  => stats.skipped_changed_upstream += 1
        Ok(Outcome::DeletedUpstream)         => stats.deleted_upstream += 1
        Err(e)                               => { log "txn={id} failed: {e:#}"; stats.failed += 1 }
    // never `?` out — keep going so one batch yields one report
    // run_one_txn ALWAYS writes a push_log row before returning (including in the Err arm)

// inside run_one_txn:
local  = SELECT is_transfer, category_id, updated_at FROM transactions WHERE id = ?
remote = api.get_transaction(txn_id)?    // 404 → Ok(DeletedUpstream), other err → bubble to outer
if remote.updated_at != local.updated_at: return Ok(SkippedChangedUpstream)
if opts.dry_run:                         return Ok(WouldPush)

let put = TransactionUpdate {
    is_transfer: Some(local.is_transfer == 1),
    category_id: local.category_id,
    ..Default::default()
}
let resp = api.update_transaction(txn_id, &put)?

UPDATE transactions SET updated_at = resp.updated_at WHERE id = txn_id  (trigger fires under reason='push')
UPDATE _transaction_changes SET pushed_at = strftime(...) WHERE transaction_id = ? AND id IN (...) AND pushed_at IS NULL
return Ok(Pushed)
```

Order matters: PUT first, then mark. A crash between the two is harmless because the PUT is idempotent on these fields — a re-run will produce the same PUT and then mark.

Note: bumping `transactions.updated_at` after the PUT will fire the `_transaction_changes_update` trigger. That's fine — it produces a new change row with `mask=0` (none of the tracked fields actually changed), which is harmless and won't be re-picked-up by the pending query (`mask & 16 = 0`). We tolerate the extra row in Stage 1 rather than add a guard; revisit if it shows up as noise in Stage 2.

## Files

- **new** `src/push/mod.rs` (~150 LOC target):
  - `pub struct PushOpts { pub dry_run: bool, pub limit: Option<usize> }`
  - `pub struct PushStats { pub pushed, would_push, skipped_changed_upstream, deleted_upstream, failed: u32 }`
  - `pub trait PushApi { fn get_transaction(..); fn update_transaction(..); }` plus `impl PushApi for PocketSmithClient`. Lets the orchestrator be unit-tested with a stub.
  - `pub fn push<A: PushApi>(api: &A, conn: &Connection, opts: &PushOpts) -> Result<PushStats>` — body above.
  - 404 handling: if `get_transaction` errors and the error chain contains `"returned 404"` → `stats.deleted_upstream += 1`, don't push, don't mark. (Tiny `fn is_not_found(&anyhow::Error) -> bool` helper, local to this module — don't refactor `client.rs`.)
- **new** `src/bin/push.rs` (~50 LOC) — `--dry-run`, `--limit N`. No `--settle-days`, no other flags. Pattern after `src/bin/normalise.rs`. Exit code 1 if `stats.failed > 0`, else 0 (even if `skipped_changed_upstream > 0` — those are expected).
- **edit** `src/lib.rs` — `pub mod push;`
- **edit** `Cargo.toml` — `[[bin]] name = "push" path = "src/bin/push.rs"`
- **edit** `src/db/schema.rs` — add `pushed_at TEXT` column to `_transaction_changes`, plus `push_log` CREATE + indexes.
- **edit** `src/db/mod.rs` — guarded migration after `SCHEMA` exec.

## Reused (do not re-implement)

- `db::with_operation` — wrap the run, gives us `reason='push'` for the (lone) `transactions.updated_at` UPDATE.
- `db::initialize` / `initialize_in_memory` — fixtures.
- `PocketSmithClient::get_transaction` (`src/client.rs`) and `update_transaction` — already present on master, signatures match.
- `models::TransactionUpdate` — already has both `is_transfer: Option<bool>` and `category_id: Option<i64>` with `skip_serializing_if = "Option::is_none"`.

## Tests (write each before the code; one commit per pair)

In `src/push/mod.rs` `#[cfg(test)] mod tests` using an in-memory DB + `StubApi`:

1. **Schema migration:** fresh DB → `pushed_at` column exists on `_transaction_changes`.
2. **Pending query empty:** pull a txn under reason="sync", no `transfers --apply` ran → pending list is empty.
3. **Pending query finds confirmed transfer:** simulate `transfers --apply` writing `is_transfer=1, category_id=99` → pending list has the txn.
4. **Push happy path:** stub remote has same `updated_at` as local; run → exactly one PUT with `{is_transfer: Some(true), category_id: Some(99)}` and nothing else; `pushed_at` set; `transactions.updated_at` refreshed; stats `pushed=1`.
5. **Timestamp guard aborts:** stub remote has different `updated_at` → no PUT, no `pushed_at`, stats `skipped_changed_upstream=1`.
6. **Dry-run:** `dry_run=true` → no PUT, no `pushed_at`, `would_push=1`.
7. **Limit:** 3 pending, `limit=Some(2)` → 2 PUTs.
8. **404 → deleted upstream:** stub `get_transaction` returns error containing "returned 404" → no PUT, no `pushed_at`, stats `deleted_upstream=1`. (No conflict table yet — just count it.)
9. **Idempotent re-run:** run twice in a row → second run is a no-op (`pushed=0`) because `pushed_at` is set.
10. **Non-404 error is per-txn:** stub `update_transaction` returns `Err` (e.g. simulated 500) on txn A but succeeds on txn B → stats `pushed=1, failed=1`; A's `pushed_at` stays NULL (re-runnable), B's is set.
11. **Locally-deleted txn excluded:** insert a `_transaction_changes` row with `mask=18, reason='transfers', pushed_at=NULL` for a `transaction_id` with no matching `transactions` row → pending list is empty (EXISTS filter).
12. **Push log row written for every outcome:** run a fixture with one of each outcome (pushed, would_push, skipped_changed_upstream, deleted_upstream, failed) → `push_log` has 5 rows with the right `outcome` and the right `request_body`/`response_body`/`error_message` shape per row.
13. **CLI parse:** `parse_args(&["--dry-run","--limit","5"])` returns `PushOpts { dry_run: true, limit: Some(5) }`.

No integration test against the live API yet — that's a Stage 1 manual smoke step, not in CI.

## Manual smoke (Stage 1 acceptance)

1. `cargo run --bin transfers -- --apply` on a real DB with one or two confirmed pairs.
2. `cargo run --bin push -- --dry-run` → see `would_push: 2` (or however many).
3. `cargo run --bin push` → `pushed: 2`.
4. Inspect on Pocketsmith web UI — pair is marked.
5. `cargo run` (main pull) — no new conflicts, txns come back with `is_transfer=true`.
6. `cargo run --bin push` again → `pushed: 0` (idempotent).

If any of steps 1–6 surprise the user, **stop and debrief** — that's Stage 2.

## Stage 1 out of scope (document in module-level comment)

- `payee`, `note`, `labels`, `memo` (Stage 3).
- per-field conflict detection / `push_conflicts` table (Stage 4).
- conflict review UX (Stage 5).
- rate limiting / 429 retry.
- settle window — kept simple; not needed for transfer pairs.

## Verification

- `cargo test` — green, including the 13 new tests.
- `cargo build --bins` — `push` binary compiles.
- Manual smoke as above.
- `sqlite3 pocketsmith.db "SELECT transaction_id, mask, pushed_at FROM _transaction_changes WHERE mask & 16 != 0 ORDER BY id DESC LIMIT 20;"` — confirm `pushed_at` set on the rows we expect.
- `sqlite3 pocketsmith.db "SELECT outcome, COUNT(*) FROM push_log GROUP BY outcome;"` — confirm one log row per attempt.

End of Stage 1 → stop and run [Stage 2](./push-stage-2-observation.md). Do **not** start Stage 3 until the Stage 2 debrief is written.
