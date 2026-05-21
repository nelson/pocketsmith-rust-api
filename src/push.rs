//! Stage 1+ push — push `is_transfer` + `category_id` + `memo` for confirmed
//! transfer pairs that `transfers --apply` (or `transfers --annotate-existing`)
//! has already written locally.
//!
//! See [`.claude/plans/push-stage-1-transfer-pair-mvp.md`] for the original
//! design rationale and [`.claude/plans/push-stage-3-expand-fields.md`] for
//! the broader generalisation. This module is still scoped to the transfers
//! reason — normalise / other writers are deferred to a later Stage-3 pass.
//!
//! Out of scope here (documented so reviewers don't add them):
//! - per-field conflict detection / table     → Stage 4.
//! - conflict review UX                       → Stage 5.
//! - rate limiting / 429 retry                → not yet needed for the volumes
//!                                              transfer pushes produce.
//! - settle window                            → not needed for transfer pairs.
//!
//! Algorithm (per txn):
//!   1. GET /transactions/{id} (timestamp guard).
//!   2. If remote.updated_at != local.updated_at → record
//!      `skipped_changed_upstream`, do not PUT.
//!   3. If dry-run → record `would_push`, do not PUT.
//!   4. PUT { is_transfer, category_id, memo } only.
//!   5. Stamp `pushed_at` on every `_transaction_changes` row for this txn
//!      whose mask intersects bits 2|16|32 (category_id, is_transfer, memo)
//!      and which is still `pushed_at IS NULL`, scoped to operations whose
//!      reason='transfers'.
//!
//! A 404 from the GET is recorded as `deleted_upstream` (the txn was removed
//! on the server). Any other error path records `failed` with the error
//! message; the loop never short-circuits — one batch yields one report.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::client::PocketSmithClient;
use crate::db;
use crate::models::{Transaction, TransactionUpdate};

/// Bits we touch for transfer pushes: `category_id` (1<<1 = 2) | `is_transfer`
/// (1<<4 = 16) | `memo` (1<<5 = 32) = 50.
const TRANSFERS_PUSH_MASK: i64 = 50;

#[derive(Debug, Clone, Copy, Default)]
pub struct PushOpts {
    pub dry_run: bool,
    pub limit: Option<usize>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PushStats {
    pub pushed: u32,
    pub would_push: u32,
    pub skipped_changed_upstream: u32,
    pub deleted_upstream: u32,
    pub failed: u32,
}

/// Minimal API surface we need from the PocketSmith client. Existing in the
/// trait form so unit tests can substitute a stub without touching
/// `src/client.rs`.
pub trait PushApi {
    fn get_transaction(&self, id: i64) -> Result<Transaction>;
    fn update_transaction(&self, id: i64, update: &TransactionUpdate) -> Result<Transaction>;
}

impl PushApi for PocketSmithClient {
    fn get_transaction(&self, id: i64) -> Result<Transaction> {
        PocketSmithClient::get_transaction(self, id)
    }

    fn update_transaction(&self, id: i64, update: &TransactionUpdate) -> Result<Transaction> {
        PocketSmithClient::update_transaction(self, id, update)
    }
}

/// 404 sniffer. The client's error path produces `"GET ... returned 404 Not Found: ..."`,
/// so a substring check on the rendered error chain is sufficient for the
/// only thing we care about. Kept local to this module so we don't refactor
/// the client just for this.
fn is_not_found(err: &anyhow::Error) -> bool {
    let s = format!("{:#}", err);
    s.contains("returned 404")
}

#[derive(Debug)]
enum Outcome {
    Pushed,
    WouldPush,
    SkippedChangedUpstream,
    DeletedUpstream,
}

/// Entry point. Wraps the whole batch in `with_operation("push", ...)` so the
/// (one) `transactions.updated_at` UPDATE per successful PUT lands as a
/// `_transaction_changes` row with `reason='push'` — easy to filter out of
/// the Stage 3 pending query.
pub fn push<A: PushApi>(api: &A, conn: &Connection, opts: &PushOpts) -> Result<PushStats> {
    let mut stats = PushStats::default();

    let pending = pending_txn_ids(conn, opts.limit)?;

    db::with_operation(conn, "push", |conn| {
        for txn_id in pending {
            match run_one_txn(api, conn, txn_id, opts) {
                Ok(Outcome::Pushed) => stats.pushed += 1,
                Ok(Outcome::WouldPush) => stats.would_push += 1,
                Ok(Outcome::SkippedChangedUpstream) => stats.skipped_changed_upstream += 1,
                Ok(Outcome::DeletedUpstream) => stats.deleted_upstream += 1,
                Err(e) => {
                    eprintln!("txn={txn_id} failed: {e:#}");
                    stats.failed += 1;
                }
            }
        }
        // Record how many txns this push actually wrote to Pocketsmith.
        // `with_operation`'s default count is derived from
        // `_transaction_changes` rows, but push doesn't write to
        // `transactions` (and therefore doesn't fire the change trigger), so
        // without this override `_operations.transactions_updated` for push
        // would always be 0. Semantics: "upstream writes successfully
        // performed" — the next sync should see roughly the same number of
        // transaction updates, modulo unrelated remote changes.
        db::record_operation_writes(conn, stats.pushed as i64)?;
        Ok(())
    })?;

    Ok(stats)
}

/// Pending query — transfer-side edits awaiting push. Picks up any
/// `reason='transfers'` change row whose mask intersects any of
/// `(category_id | is_transfer | memo)` and which hasn't yet been stamped
/// `pushed_at`. The memo bit is what lets `annotate_existing_pairs` produce
/// a memo-only change that still gets caught here.
fn pending_txn_ids(conn: &Connection, limit: Option<usize>) -> Result<Vec<i64>> {
    let sql = "
        SELECT DISTINCT c.transaction_id
        FROM _transaction_changes c
        JOIN _operations o ON c.operation_id = o.id
        WHERE (c.mask & 50) != 0
          AND o.reason = 'transfers'
          AND c.pushed_at IS NULL
          AND EXISTS (SELECT 1 FROM transactions t WHERE t.id = c.transaction_id)
        ORDER BY c.transaction_id";
    let mut stmt = conn.prepare(sql)?;
    let ids: Vec<i64> = stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(match limit {
        Some(n) => ids.into_iter().take(n).collect(),
        None => ids,
    })
}

struct LocalSnapshot {
    is_transfer: i64,
    category_id: Option<i64>,
    memo: Option<String>,
    updated_at: Option<String>,
}

fn load_local(conn: &Connection, txn_id: i64) -> Result<LocalSnapshot> {
    conn.query_row(
        "SELECT is_transfer, category_id, memo, updated_at FROM transactions WHERE id = ?1",
        [txn_id],
        |row| {
            Ok(LocalSnapshot {
                is_transfer: row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                category_id: row.get(1)?,
                memo: row.get(2)?,
                updated_at: row.get(3)?,
            })
        },
    )
    .with_context(|| format!("local row missing for txn {txn_id}"))
}

fn run_one_txn<A: PushApi>(
    api: &A,
    conn: &Connection,
    txn_id: i64,
    opts: &PushOpts,
) -> Result<Outcome> {
    let local = load_local(conn, txn_id)?;

    // Step 1: timestamp guard via GET.
    let remote = match api.get_transaction(txn_id) {
        Ok(r) => r,
        Err(e) if is_not_found(&e) => {
            log_attempt(
                conn,
                txn_id,
                "deleted_upstream",
                local.updated_at.as_deref(),
                None,
                None,
                None,
                None,
            )?;
            return Ok(Outcome::DeletedUpstream);
        }
        Err(e) => {
            // Non-404: record `failed` here so we still get exactly one log
            // row per attempt, then bubble up so the outer loop counts it.
            let msg = format!("{e:#}");
            log_attempt(
                conn,
                txn_id,
                "failed",
                local.updated_at.as_deref(),
                None,
                None,
                None,
                Some(&msg),
            )?;
            return Err(e);
        }
    };

    if remote.updated_at != local.updated_at {
        log_attempt(
            conn,
            txn_id,
            "skipped_changed_upstream",
            local.updated_at.as_deref(),
            remote.updated_at.as_deref(),
            None,
            None,
            None,
        )?;
        return Ok(Outcome::SkippedChangedUpstream);
    }

    let put = TransactionUpdate {
        is_transfer: Some(local.is_transfer != 0),
        category_id: local.category_id,
        memo: local.memo.clone(),
        ..Default::default()
    };

    if opts.dry_run {
        let request_body = serde_json::to_string(&put).ok();
        log_attempt(
            conn,
            txn_id,
            "would_push",
            local.updated_at.as_deref(),
            remote.updated_at.as_deref(),
            request_body.as_deref(),
            None,
            None,
        )?;
        return Ok(Outcome::WouldPush);
    }

    let request_body = serde_json::to_string(&put).ok();
    let resp = match api.update_transaction(txn_id, &put) {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("{e:#}");
            log_attempt(
                conn,
                txn_id,
                "failed",
                local.updated_at.as_deref(),
                remote.updated_at.as_deref(),
                request_body.as_deref(),
                None,
                Some(&msg),
            )?;
            return Err(e);
        }
    };

    // We deliberately do NOT bump `transactions.updated_at` to the server's
    // post-PUT value. Architectural invariant: `transactions` is the local
    // mirror of remote state (managed by `sync`) overlaid with un-pushed
    // local edits; push is neither, so it must not write here. The next
    // `sync` will pull the bumped `updated_at` naturally. The server's
    // returned timestamp is preserved in `push_log.response_body` for audit.
    //
    // Stamp every still-unpushed Stage-1 change row for this txn whose
    // operation came from `transfers --apply`. The reason filter is the bug
    // fix: without it, the mask=63 row that the INSERT trigger emits during
    // sync (which intersects bits 2|16) would also be stamped, which is
    // semantically wrong — that row represents incoming remote state, not a
    // local change waiting to be propagated.
    conn.execute(
        "UPDATE _transaction_changes
            SET pushed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE transaction_id = ?1
            AND (mask & ?2) != 0
            AND pushed_at IS NULL
            AND operation_id IN (
                SELECT id FROM _operations WHERE reason = 'transfers'
            )",
        params![txn_id, TRANSFERS_PUSH_MASK],
    )?;

    let response_body = serde_json::to_string(&resp).ok();
    log_attempt(
        conn,
        txn_id,
        "pushed",
        local.updated_at.as_deref(),
        remote.updated_at.as_deref(),
        request_body.as_deref(),
        response_body.as_deref(),
        None,
    )?;

    Ok(Outcome::Pushed)
}

#[allow(clippy::too_many_arguments)]
fn log_attempt(
    conn: &Connection,
    txn_id: i64,
    outcome: &str,
    local_updated_at_before: Option<&str>,
    remote_updated_at_seen: Option<&str>,
    request_body: Option<&str>,
    response_body: Option<&str>,
    error_message: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO push_log (
            txn_id, outcome,
            local_updated_at_before, remote_updated_at_seen,
            request_body, response_body, error_message
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            txn_id,
            outcome,
            local_updated_at_before,
            remote_updated_at_seen,
            request_body,
            response_body,
            error_message,
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// CLI parser. Lives here so it's unit-testable without spinning up the
// `push` binary.
// ---------------------------------------------------------------------------

/// Parse the argv tail (everything after argv[0]). Returns `PushOpts` or an
/// error message suitable for printing to stderr.
pub fn parse_args(args: &[&str]) -> Result<PushOpts, String> {
    let mut opts = PushOpts::default();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "--dry-run" => opts.dry_run = true,
            "--limit" => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| "--limit requires a value".to_string())?;
                let n: usize = v
                    .parse()
                    .map_err(|_| format!("--limit must be a non-negative integer, got {v:?}"))?;
                opts.limit = Some(n);
                i += 1;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }
    Ok(opts)
}

/// Helper for ad-hoc reporting / tests.
#[allow(dead_code)]
pub fn count_pending(conn: &Connection) -> Result<usize> {
    Ok(pending_txn_ids(conn, None)?.len())
}

#[allow(dead_code)]
fn local_updated_at(conn: &Connection, txn_id: i64) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT updated_at FROM transactions WHERE id = ?1",
            [txn_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten())
}


#[cfg(test)]
mod tests;
