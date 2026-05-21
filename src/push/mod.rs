//! Stage 1 push — narrow MVP: push `is_transfer` + `category_id` for confirmed
//! transfer pairs that `transfers --apply` has already applied locally.
//!
//! See [`.claude/plans/push-stage-1-transfer-pair-mvp.md`] for the design
//! rationale. This module is intentionally minimal so we can observe how it
//! behaves in anger (Stage 2) before generalising to other fields (Stage 3)
//! or adding per-field conflict detection (Stage 4).
//!
//! Out of scope for Stage 1 (documented here so reviewers don't add them):
//! - `payee`, `note`, `labels`, `memo`        → Stage 3.
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
//!   4. PUT { is_transfer, category_id } only.
//!   5. Update local `transactions.updated_at` to the response's value
//!      (under reason `push` so the resulting `_transaction_changes` row is
//!      attributable).
//!   6. Stamp `pushed_at` on every `_transaction_changes` row for this txn
//!      whose mask intersects bits 2|16 (category_id, is_transfer) and which
//!      is still `pushed_at IS NULL`.
//!
//! A 404 from the GET is recorded as `deleted_upstream` (the txn was removed
//! on the server). Any other error path records `failed` with the error
//! message; the loop never short-circuits — one batch yields one report.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::client::PocketSmithClient;
use crate::db;
use crate::models::{Transaction, TransactionUpdate};

/// Bits we touch in Stage 1: `category_id` (1<<1 = 2) | `is_transfer` (1<<4 = 16) = 18.
const STAGE1_MASK: i64 = 18;

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
        Ok(())
    })?;

    Ok(stats)
}

/// Pending query — see plan §"Pending query". Only Stage-1 transfer pushes.
fn pending_txn_ids(conn: &Connection, limit: Option<usize>) -> Result<Vec<i64>> {
    let sql = "
        SELECT DISTINCT c.transaction_id
        FROM _transaction_changes c
        JOIN _operations o ON c.operation_id = o.id
        WHERE (c.mask & 16) != 0
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
    updated_at: Option<String>,
}

fn load_local(conn: &Connection, txn_id: i64) -> Result<LocalSnapshot> {
    conn.query_row(
        "SELECT is_transfer, category_id, updated_at FROM transactions WHERE id = ?1",
        [txn_id],
        |row| {
            Ok(LocalSnapshot {
                is_transfer: row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                category_id: row.get(1)?,
                updated_at: row.get(2)?,
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

    // Refresh local `updated_at` to match the server's post-PUT value, so the
    // next push run's timestamp guard doesn't trip on our own write. This
    // UPDATE fires the `_transaction_changes_update` trigger, producing a
    // row with mask=0 (none of the tracked fields changed) — harmless and
    // ignored by the pending query.
    if let Some(ref ts) = resp.updated_at {
        conn.execute(
            "UPDATE transactions SET updated_at = ?1 WHERE id = ?2",
            params![ts, txn_id],
        )?;
    }

    // Stamp every still-unpushed Stage-1 change row for this txn.
    conn.execute(
        "UPDATE _transaction_changes
            SET pushed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE transaction_id = ?1
            AND (mask & ?2) != 0
            AND pushed_at IS NULL",
        params![txn_id, STAGE1_MASK],
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

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::*;
    use crate::db::{upsert_transaction, with_operation};
    use crate::models::Transaction;
    use std::cell::RefCell;

    // ----- Stub API ---------------------------------------------------------

    /// Stub controllable per-id. By default `get_transaction` returns a Transaction
    /// echoing whatever was last `set_remote()`'d for that id; `update_transaction`
    /// returns the same with `updated_at` bumped to `next_updated_at`.
    struct StubApi {
        remotes: RefCell<std::collections::HashMap<i64, Transaction>>,
        get_errors: RefCell<std::collections::HashMap<i64, String>>,
        update_errors: RefCell<std::collections::HashMap<i64, String>>,
        next_updated_at: RefCell<String>,
        gets: RefCell<Vec<i64>>,
        puts: RefCell<Vec<(i64, TransactionUpdate)>>,
    }

    impl StubApi {
        fn new() -> Self {
            Self {
                remotes: RefCell::new(Default::default()),
                get_errors: RefCell::new(Default::default()),
                update_errors: RefCell::new(Default::default()),
                next_updated_at: RefCell::new("2024-07-01T00:00:00Z".into()),
                gets: RefCell::new(Vec::new()),
                puts: RefCell::new(Vec::new()),
            }
        }

        fn set_remote(&self, t: Transaction) {
            self.remotes.borrow_mut().insert(t.id, t);
        }

        fn set_get_error(&self, id: i64, msg: &str) {
            self.get_errors.borrow_mut().insert(id, msg.into());
        }

        fn set_update_error(&self, id: i64, msg: &str) {
            self.update_errors.borrow_mut().insert(id, msg.into());
        }
    }

    impl PushApi for StubApi {
        fn get_transaction(&self, id: i64) -> Result<Transaction> {
            self.gets.borrow_mut().push(id);
            if let Some(msg) = self.get_errors.borrow().get(&id) {
                anyhow::bail!("{msg}");
            }
            self.remotes
                .borrow()
                .get(&id)
                .cloned()
                .with_context(|| format!("stub has no remote for txn {id}"))
        }

        fn update_transaction(&self, id: i64, update: &TransactionUpdate) -> Result<Transaction> {
            self.puts.borrow_mut().push((
                id,
                TransactionUpdate {
                    memo: update.memo.clone(),
                    cheque_number: update.cheque_number.clone(),
                    payee: update.payee.clone(),
                    amount: update.amount,
                    date: update.date.clone(),
                    is_transfer: update.is_transfer,
                    category_id: update.category_id,
                    note: update.note.clone(),
                    needs_review: update.needs_review,
                    labels: update.labels.clone(),
                },
            ));
            if let Some(msg) = self.update_errors.borrow().get(&id) {
                anyhow::bail!("{msg}");
            }
            let base = self
                .remotes
                .borrow()
                .get(&id)
                .cloned()
                .with_context(|| format!("stub has no remote for txn {id} to update"))?;
            Ok(Transaction {
                is_transfer: update.is_transfer.or(base.is_transfer),
                updated_at: Some(self.next_updated_at.borrow().clone()),
                ..base
            })
        }
    }

    // ----- Fixture helpers --------------------------------------------------

    /// Insert a category, account, transaction (under reason "pocketsmith"),
    /// then re-run `transfers --apply`-style write to mark is_transfer=1 +
    /// category_id=99 (under reason "transfers"). Returns the txn id.
    fn fixture_confirmed_transfer(conn: &Connection, id: i64) -> i64 {
        // Ensure the _Transfer category exists (FK target) and a baseline pull
        // exists with is_transfer=0 / category_id=NULL — both reason='pocketsmith'.
        with_operation(conn, "pocketsmith", |conn| {
            crate::db::upsert_category(conn, &make_category(99, "_Transfer"))?;
            let mut t = make_transaction(id, "Internal Transfer");
            t.category = None;
            t.is_transfer = Some(false);
            t.updated_at = Some("2024-06-15T00:00:00Z".into());
            upsert_transaction(conn, &t)?;
            Ok(())
        })
        .unwrap();
        // Now simulate `transfers --apply`: a single UPDATE bumping both
        // fields together → resulting _transaction_changes row has mask=18.
        with_operation(conn, "transfers", |conn| {
            conn.execute(
                "UPDATE transactions SET category_id = 99, is_transfer = 1 WHERE id = ?1",
                [id],
            )?;
            Ok(())
        })
        .unwrap();
        id
    }

    fn remote_matching(id: i64, updated_at: &str) -> Transaction {
        let mut t = make_transaction(id, "Internal Transfer");
        t.updated_at = Some(updated_at.into());
        t.is_transfer = Some(false);
        t
    }

    // ----- Tests ------------------------------------------------------------

    #[test]
    fn schema_migration_adds_pushed_at_column() {
        let conn = test_db();
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info('_transaction_changes')")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(cols.contains(&"pushed_at".to_string()), "cols: {cols:?}");
    }

    #[test]
    fn pending_query_empty_when_only_pocketsmith_writes() {
        let conn = test_db();
        with_operation(&conn, "pocketsmith", |conn| {
            upsert_transaction(conn, &make_transaction(1, "Anything"))
        })
        .unwrap();
        assert_eq!(pending_txn_ids(&conn, None).unwrap(), Vec::<i64>::new());
    }

    #[test]
    fn pending_query_finds_confirmed_transfer() {
        let conn = test_db();
        fixture_confirmed_transfer(&conn, 1);
        assert_eq!(pending_txn_ids(&conn, None).unwrap(), vec![1]);
    }

    #[test]
    fn push_happy_path() {
        let conn = test_db();
        fixture_confirmed_transfer(&conn, 1);
        let api = StubApi::new();
        api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
        *api.next_updated_at.borrow_mut() = "2024-07-01T12:00:00Z".into();

        let stats = push(&api, &conn, &PushOpts::default()).unwrap();

        assert_eq!(stats.pushed, 1, "stats: {stats:?}");
        assert_eq!(api.puts.borrow().len(), 1);
        let (id, put) = &api.puts.borrow()[0];
        assert_eq!(*id, 1);
        assert_eq!(put.is_transfer, Some(true));
        assert_eq!(put.category_id, Some(99));
        // Nothing else set.
        assert!(put.payee.is_none() && put.note.is_none() && put.memo.is_none()
                && put.labels.is_none() && put.amount.is_none() && put.date.is_none());

        // pushed_at stamped on the transfers row.
        let stamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _transaction_changes
                   WHERE transaction_id = 1 AND (mask & 18) != 0 AND pushed_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(stamped >= 1, "expected at least one stamped row");

        // local updated_at refreshed.
        let ts = local_updated_at(&conn, 1).unwrap();
        assert_eq!(ts.as_deref(), Some("2024-07-01T12:00:00Z"));
    }

    #[test]
    fn timestamp_guard_aborts_when_remote_differs() {
        let conn = test_db();
        fixture_confirmed_transfer(&conn, 1);
        let api = StubApi::new();
        api.set_remote(remote_matching(1, "2099-01-01T00:00:00Z"));

        let stats = push(&api, &conn, &PushOpts::default()).unwrap();

        assert_eq!(stats.skipped_changed_upstream, 1);
        assert_eq!(stats.pushed, 0);
        assert_eq!(api.puts.borrow().len(), 0);
        let stamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _transaction_changes
                   WHERE transaction_id = 1 AND pushed_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stamped, 0);
    }

    #[test]
    fn dry_run_does_not_put_or_stamp() {
        let conn = test_db();
        fixture_confirmed_transfer(&conn, 1);
        let api = StubApi::new();
        api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));

        let stats = push(
            &api,
            &conn,
            &PushOpts { dry_run: true, limit: None },
        )
        .unwrap();

        assert_eq!(stats.would_push, 1);
        assert_eq!(stats.pushed, 0);
        assert_eq!(api.puts.borrow().len(), 0);
        let stamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _transaction_changes
                   WHERE transaction_id = 1 AND pushed_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stamped, 0);
    }

    #[test]
    fn limit_truncates_pending_set() {
        let conn = test_db();
        for id in 1..=3 {
            fixture_confirmed_transfer(&conn, id);
        }
        let api = StubApi::new();
        for id in 1..=3 {
            api.set_remote(remote_matching(id, "2024-06-15T00:00:00Z"));
        }

        let stats = push(
            &api,
            &conn,
            &PushOpts { dry_run: false, limit: Some(2) },
        )
        .unwrap();

        assert_eq!(stats.pushed, 2);
        assert_eq!(api.puts.borrow().len(), 2);
    }

    #[test]
    fn deleted_upstream_when_get_returns_404() {
        let conn = test_db();
        fixture_confirmed_transfer(&conn, 1);
        let api = StubApi::new();
        api.set_get_error(1, "GET https://api/x returned 404 Not Found: {}");

        let stats = push(&api, &conn, &PushOpts::default()).unwrap();

        assert_eq!(stats.deleted_upstream, 1);
        assert_eq!(stats.failed, 0);
        assert_eq!(api.puts.borrow().len(), 0);
        let stamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _transaction_changes
                   WHERE transaction_id = 1 AND pushed_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stamped, 0);
    }

    #[test]
    fn idempotent_rerun_does_nothing() {
        let conn = test_db();
        fixture_confirmed_transfer(&conn, 1);
        let api = StubApi::new();
        api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
        *api.next_updated_at.borrow_mut() = "2024-07-01T12:00:00Z".into();

        let first = push(&api, &conn, &PushOpts::default()).unwrap();
        assert_eq!(first.pushed, 1);

        // Second remote echo must match the new local updated_at to satisfy
        // the timestamp guard on a hypothetical second pass — but pending
        // query should now be empty, so it doesn't matter.
        api.set_remote(remote_matching(1, "2024-07-01T12:00:00Z"));
        let second = push(&api, &conn, &PushOpts::default()).unwrap();
        assert_eq!(second.pushed, 0);
        assert_eq!(second.would_push, 0);
        // Still only one PUT in total.
        assert_eq!(api.puts.borrow().len(), 1);
    }

    #[test]
    fn non_404_error_is_per_txn() {
        let conn = test_db();
        fixture_confirmed_transfer(&conn, 1); // will fail on update
        fixture_confirmed_transfer(&conn, 2); // will succeed
        let api = StubApi::new();
        api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
        api.set_remote(remote_matching(2, "2024-06-15T00:00:00Z"));
        api.set_update_error(1, "PUT https://api/x returned 500 Internal Server Error: boom");

        let stats = push(&api, &conn, &PushOpts::default()).unwrap();

        assert_eq!(stats.pushed, 1, "stats: {stats:?}");
        assert_eq!(stats.failed, 1);

        // txn 1: not stamped (re-runnable next time).
        let a_stamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _transaction_changes
                   WHERE transaction_id = 1 AND pushed_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(a_stamped, 0);
        // txn 2: stamped.
        let b_stamped: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _transaction_changes
                   WHERE transaction_id = 2 AND (mask & 18) != 0 AND pushed_at IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(b_stamped >= 1);
    }

    #[test]
    fn locally_deleted_txn_excluded_from_pending() {
        let conn = test_db();
        fixture_confirmed_transfer(&conn, 1);
        // Simulate a locally-deleted txn: insert an orphan _transaction_changes
        // row (mask=18, reason='transfers', pushed_at=NULL) whose transaction_id
        // has no matching transactions row. The plan's EXISTS filter should
        // exclude it. We turn FKs off briefly so the orphan insert is allowed
        // — we want to exercise the EXISTS guard, not the FK.
        conn.execute_batch("PRAGMA foreign_keys = OFF;").unwrap();
        with_operation(&conn, "transfers", |conn| {
            conn.execute(
                "INSERT INTO _transaction_changes
                   (transaction_id, is_transfer, category_id, operation_id, mask)
                   VALUES (9999, 1, 99, (SELECT id FROM _current_operation), 18)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        let ids = pending_txn_ids(&conn, None).unwrap();
        assert_eq!(ids, vec![1]);
    }

    #[test]
    fn push_log_row_per_outcome() {
        let conn = test_db();
        // Five fixtures: pushed, would_push, skipped, deleted, failed.
        fixture_confirmed_transfer(&conn, 1); // pushed
        fixture_confirmed_transfer(&conn, 2); // would_push  — run twice (dry then real)
        fixture_confirmed_transfer(&conn, 3); // skipped
        fixture_confirmed_transfer(&conn, 4); // deleted
        fixture_confirmed_transfer(&conn, 5); // failed
        let api = StubApi::new();
        api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
        api.set_remote(remote_matching(2, "2024-06-15T00:00:00Z"));
        api.set_remote(remote_matching(3, "2099-01-01T00:00:00Z")); // mismatch
        api.set_get_error(4, "GET https://api/x returned 404 Not Found");
        api.set_remote(remote_matching(5, "2024-06-15T00:00:00Z"));
        api.set_update_error(5, "PUT https://api/x returned 500 Internal Server Error");

        // Dry-run first against txn 2 only.
        let _ = push(
            &api,
            &conn,
            &PushOpts { dry_run: true, limit: Some(2) },
        )
        .unwrap();
        // Re-stamp the dry-run sub-batch logged would_push for txns 1 and 2;
        // we only care that the 5 outcomes appear across runs.
        // Now real run for everything.
        let stats = push(&api, &conn, &PushOpts::default()).unwrap();

        // Verify per-outcome counts exist in push_log.
        let counts: std::collections::HashMap<String, i64> = conn
            .prepare("SELECT outcome, COUNT(*) FROM push_log GROUP BY outcome")
            .unwrap()
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(counts.get("pushed").copied().unwrap_or(0) >= 1, "counts={counts:?}");
        assert!(counts.get("would_push").copied().unwrap_or(0) >= 1, "counts={counts:?}");
        assert!(
            counts
                .get("skipped_changed_upstream")
                .copied()
                .unwrap_or(0)
                >= 1,
            "counts={counts:?}"
        );
        assert!(counts.get("deleted_upstream").copied().unwrap_or(0) >= 1, "counts={counts:?}");
        assert!(counts.get("failed").copied().unwrap_or(0) >= 1, "counts={counts:?}");

        // Shape: pushed has response_body; failed has error_message.
        let (resp, err): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT response_body, error_message FROM push_log
                   WHERE outcome = 'pushed' LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(resp.is_some());
        assert!(err.is_none());

        let (resp, err): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT response_body, error_message FROM push_log
                   WHERE outcome = 'failed' LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(resp.is_none());
        assert!(err.is_some());

        // Sanity on stats from the second run.
        assert!(stats.pushed + stats.failed + stats.skipped_changed_upstream + stats.deleted_upstream >= 4);
    }

    #[test]
    fn cli_parse_basic() {
        let opts = parse_args(&["--dry-run", "--limit", "5"]).unwrap();
        assert!(opts.dry_run);
        assert_eq!(opts.limit, Some(5));
    }

    #[test]
    fn cli_parse_defaults() {
        let opts = parse_args(&[]).unwrap();
        assert!(!opts.dry_run);
        assert_eq!(opts.limit, None);
    }

    #[test]
    fn cli_parse_rejects_unknown() {
        assert!(parse_args(&["--nope"]).is_err());
        assert!(parse_args(&["--limit"]).is_err()); // missing value
        assert!(parse_args(&["--limit", "abc"]).is_err());
    }
}
