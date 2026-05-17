# Writeback Stage 1 — Transfer-pair writeback (Learning MVP)

> See [writeback-overview.md](./writeback-overview.md) for the rollout context and stage table. This is the only stage committed to in full implementation detail; Stages 2–5 are sketched with explicit open questions for resolution at the time they begin.

## Scope

Push the local result of `transfers --apply` to Pocketsmith: for each confirmed transfer pair, PUT both `is_transfer=true` and `category_id=<transfer category>` on each of the two transactions. Nothing else. Driven entirely from DB state — no manual ids.

## Why these two fields, together

`transfers --apply` writes both in a single `UPDATE transactions SET category_id=?, is_transfer=1 WHERE id=?` (`src/bin/transfers.rs`), so the resulting `_transactions_history` row has `_mask = 18` (bits 2|16). Splitting the push would create a half-pushed state; pushing both together matches the source write exactly.

## Safety: timestamp guard

Before each PUT:
1. `GET /v2/transactions/{id}` (existing `PocketSmithClient::get_transaction`).
2. Compare remote `updated_at` against the value stored in local `transactions.updated_at` (this was last refreshed when we pulled).
3. If they differ → **abort this txn**. No PUT. No mark. Record a one-line log entry: `txn={id} skipped: remote updated_at {remote} != local {local}`.

This is intentionally blunt — it will sometimes refuse when the upstream change was to an unrelated field. That's fine: those false-positives are Stage 2's observation data. Real conflict detection is Stage 4.

## Data model changes

### 1. New column on `_transactions_history` (the "pushed" marker)

```sql
ALTER TABLE _transactions_history ADD COLUMN _pushed_at TEXT;
```

Migration: in `src/db/mod.rs`, after `conn.execute_batch(SCHEMA)`, run a `PRAGMA table_info('_transactions_history')` check and `ALTER TABLE … ADD COLUMN _pushed_at TEXT` if absent. Idempotent.

Update the `CREATE TABLE _transactions_history` in `src/db/schema.rs` so fresh DBs already have the column.

### 2. New table `_writeback_log` (the audit trail — one row per attempt, regardless of outcome)

```sql
CREATE TABLE IF NOT EXISTS _writeback_log (
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
CREATE INDEX IF NOT EXISTS idx_writeback_log_attempted_at ON _writeback_log(attempted_at);
CREATE INDEX IF NOT EXISTS idx_writeback_log_txn_id ON _writeback_log(txn_id);
```

This is the reason Stage 1 is called the Learning MVP — Stage 2 observation runs entirely off this table.

No `_writeback_conflicts` table yet (Stage 4). No new client functions — `PocketSmithClient::get_transaction` and `update_transaction` are already in `src/client.rs` on master.

## Pending query

```sql
SELECT DISTINCT h._rowid
FROM _transactions_history h
JOIN _transaction_change_log l ON h._version = l.version
WHERE (h._mask & 16) != 0          -- is_transfer bit
  AND l.reason = 'transfers'        -- only from /review-transfers --apply
  AND h._pushed_at IS NULL
  AND EXISTS (SELECT 1 FROM transactions t WHERE t.id = h._rowid);  -- skip locally-deleted
```

Then, per txn id: collect every history row's `_version` whose `(_mask & 18) != 0` and `_pushed_at IS NULL` — those are the rows we'll mark on success.

## Algorithm

```
wrap entire run in db::with_transaction_change_log(conn, "writeback", |conn| { ... })

for each txn_id in pending (optionally truncated to opts.limit):
    match run_one_txn(api, conn, txn_id, opts):
        Ok(Outcome::Pushed)                  => stats.pushed += 1
        Ok(Outcome::WouldPush)               => stats.would_push += 1
        Ok(Outcome::SkippedChangedUpstream)  => stats.skipped_changed_upstream += 1
        Ok(Outcome::DeletedUpstream)         => stats.deleted_upstream += 1
        Err(e)                               => { log "txn={id} failed: {e:#}"; stats.failed += 1 }
    // never `?` out — keep going so one batch yields one report
    // run_one_txn ALWAYS writes a _writeback_log row before returning (including in the Err arm)

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

UPDATE transactions SET updated_at = resp.updated_at WHERE id = txn_id  (no trigger fires)
UPDATE _transactions_history SET _pushed_at = strftime(...) WHERE _rowid = ? AND _version IN (...) AND _pushed_at IS NULL
return Ok(Pushed)
```

Order matters: PUT first, then mark. A crash between the two is harmless because the PUT is idempotent on these fields — a re-run will produce the same PUT and then mark.

## Files

- **new** `src/writeback/mod.rs` (~150 LOC target — much smaller than the archived version):
  - `pub struct WritebackOpts { pub dry_run: bool, pub limit: Option<usize> }`
  - `pub struct WritebackStats { pub pushed, would_push, skipped_changed_upstream, deleted_upstream, failed: u32 }`
  - `pub trait WritebackApi { fn get_transaction(..); fn update_transaction(..); }` plus `impl WritebackApi for PocketSmithClient`. Lets the orchestrator be unit-tested with a stub.
  - `pub fn writeback<A: WritebackApi>(api: &A, conn: &Connection, opts: &WritebackOpts) -> Result<WritebackStats>` — body above.
  - 404 handling: if `get_transaction` errors and the error chain contains `"returned 404"` → `stats.deleted_upstream += 1`, don't push, don't mark. (Tiny `fn is_not_found(&anyhow::Error) -> bool` helper, local to this module — don't refactor `client.rs`.)
- **new** `src/bin/writeback.rs` (~50 LOC) — `--dry-run`, `--limit N`. No `--settle-days`, no other flags. Pattern after `src/bin/normalise.rs`. Exit code 1 if `stats.failed > 0`, else 0 (even if `skipped_changed_upstream > 0` — those are expected).
- **edit** `src/lib.rs` — `pub mod writeback;`
- **edit** `Cargo.toml` — `[[bin]] name = "writeback" path = "src/bin/writeback.rs"`
- **edit** `src/db/schema.rs` — add `_pushed_at TEXT` column to `_transactions_history`, plus `_writeback_log` CREATE.
- **edit** `src/db/mod.rs` — guarded migration after `SCHEMA` exec.

## Reused (do not re-implement)

- `db::with_transaction_change_log` — wrap the run, gives us `reason='writeback'` for the (lone) `transactions.updated_at` UPDATE.
- `db::initialize` / `initialize_in_memory` — fixtures.
- `PocketSmithClient::get_transaction` (`src/client.rs`) and `update_transaction` — already present on master, signatures match.
- `models::TransactionUpdate` — already has both `is_transfer: Option<bool>` and `category_id: Option<i64>` with `skip_serializing_if = "Option::is_none"`.

## Tests (write each before the code; one commit per pair)

In `src/writeback/mod.rs` `#[cfg(test)] mod tests` using an in-memory DB + `StubApi`:

1. **Schema migration:** fresh DB → `_pushed_at` column exists on `_transactions_history`.
2. **Pending query empty:** pull a txn under reason="pocketsmith", no `transfers --apply` ran → pending list is empty.
3. **Pending query finds confirmed transfer:** simulate `transfers --apply` writing `is_transfer=1, category_id=99` → pending list has the txn.
4. **Push happy path:** stub remote has same `updated_at` as local; run → exactly one PUT with `{is_transfer: Some(true), category_id: Some(99)}` and nothing else; `_pushed_at` set; `transactions.updated_at` refreshed; stats `pushed=1`.
5. **Timestamp guard aborts:** stub remote has different `updated_at` → no PUT, no `_pushed_at`, stats `skipped_changed_upstream=1`.
6. **Dry-run:** `dry_run=true` → no PUT, no `_pushed_at`, `would_push=1`.
7. **Limit:** 3 pending, `limit=Some(2)` → 2 PUTs.
8. **404 → deleted upstream:** stub `get_transaction` returns error containing "returned 404" → no PUT, no `_pushed_at`, stats `deleted_upstream=1`. (No conflict table yet — just count it.)
9. **Idempotent re-run:** run twice in a row → second run is a no-op (`pushed=0`) because `_pushed_at` is set.
10. **Non-404 error is per-txn:** stub `update_transaction` returns `Err` (e.g. simulated 500) on txn A but succeeds on txn B → stats `pushed=1, failed=1`; A's `_pushed_at` stays NULL (re-runnable), B's is set.
11. **Locally-deleted txn excluded:** insert a `_transactions_history` row with `_mask=18, reason='transfers', _pushed_at=NULL` for a `_rowid` with no matching `transactions` row → pending list is empty (EXISTS filter).
12. **Writeback log row written for every outcome:** run a fixture with one of each outcome (pushed, would_push, skipped_changed_upstream, deleted_upstream, failed) → `_writeback_log` has 5 rows with the right `outcome` and the right `request_body`/`response_body`/`error_message` shape per row.
13. **CLI parse:** `parse_args(&["--dry-run","--limit","5"])` returns `WritebackOpts { dry_run: true, limit: Some(5) }`.

No integration test against the live API yet — that's a Stage 1 manual smoke step, not in CI.

## Manual smoke (Stage 1 acceptance)

1. `cargo run --bin transfers -- --apply` on a real DB with one or two confirmed pairs.
2. `cargo run --bin writeback -- --dry-run` → see `would_push: 2` (or however many).
3. `cargo run --bin writeback` → `pushed: 2`.
4. Inspect on Pocketsmith web UI — pair is marked.
5. `cargo run` (main pull) — no new conflicts, txns come back with `is_transfer=true`.
6. `cargo run --bin writeback` again → `pushed: 0` (idempotent).

If any of steps 1–6 surprise the user, **stop and debrief** — that's Stage 2.

## Stage 1 out of scope (document in module-level comment)

- `payee`, `note`, `labels`, `memo` (Stage 3).
- per-field conflict detection / `_writeback_conflicts` table (Stage 4).
- conflict review UX (Stage 5).
- rate limiting / 429 retry.
- settle window — kept simple; not needed for transfer pairs.

## Verification

- `cargo test` — green, including the 13 new tests.
- `cargo build --bins` — `writeback` binary compiles.
- Manual smoke as above.
- `sqlite3 pocketsmith.db "SELECT _rowid, _mask, _pushed_at FROM _transactions_history WHERE _mask & 16 != 0 ORDER BY _version DESC LIMIT 20;"` — confirm `_pushed_at` set on the rows we expect.
- `sqlite3 pocketsmith.db "SELECT outcome, COUNT(*) FROM _writeback_log GROUP BY outcome;"` — confirm one log row per attempt.

End of Stage 1 → stop and run [Stage 2](./writeback-stage-2-observation.md). Do **not** start Stage 3 until the Stage 2 debrief is written.
