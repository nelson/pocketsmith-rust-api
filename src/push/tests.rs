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

/// Insert a category, account, transaction (under reason "sync"),
/// then re-run `transfers --apply`-style write to mark is_transfer=1 +
/// category_id=99 (under reason "transfers"). Returns the txn id.
fn fixture_confirmed_transfer(conn: &Connection, id: i64) -> i64 {
    // Ensure the _Transfer category exists (FK target) and a baseline pull
    // exists with is_transfer=0 / category_id=NULL — both reason='sync'.
    with_operation(conn, "sync", |conn| {
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
fn pending_query_empty_when_only_sync_writes() {
    let conn = test_db();
    with_operation(&conn, "sync", |conn| {
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

    // Confirm push did NOT touch transactions.updated_at — push must not
    // write to the transactions table (Stage 1 invariant). The local row
    // still reflects the last sync's timestamp.
    let ts = local_updated_at(&conn, 1).unwrap();
    assert_eq!(ts.as_deref(), Some("2024-06-15T00:00:00Z"));
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

    // No further setup needed: the pending query filters out anything
    // with pushed_at IS NOT NULL, so the second run never even calls the
    // API. (The 2024-07-01 timestamp from next_updated_at lives only in
    // push_log.response_body.)
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

/// Regression test for the marking bug: the `_transaction_changes` row
/// created by the INSERT trigger on a sync upsert has mask=63 (every bit
/// set, the framework's "create" marker). Bit 16 (is_transfer) and bit 2
/// (category_id) are part of that 63, so the naive mark-as-pushed UPDATE
/// `mask & 18 != 0 AND pushed_at IS NULL` would stamp the sync-insert row
/// as if push had written it. It should not: the sync row reflects
/// incoming remote state, not a local edit waiting to be propagated.
/// The fix filters by reason='transfers' on the marking step too.
#[test]
fn push_does_not_stamp_mask63_sync_row() {
    let conn = test_db();
    fixture_confirmed_transfer(&conn, 1);
    let api = StubApi::new();
    api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));

    // Sanity: before push there's a mask=63 row from the sync insert AND
    // a mask=18 row from the transfers apply. The mask=18 row has
    // reason='transfers'; the mask=63 row has reason='sync'.
    let rows: Vec<(i64, i64, String, Option<String>)> = conn
        .prepare(
            "SELECT c.id, c.mask, o.reason, c.pushed_at
               FROM _transaction_changes c
               JOIN _operations o ON c.operation_id = o.id
              WHERE c.transaction_id = 1 ORDER BY c.id",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(
        rows.iter().any(|(_, m, r, _)| *m == 63 && r == "sync"),
        "missing sync mask=63 row in fixture: {rows:?}"
    );
    assert!(
        rows.iter().any(|(_, m, r, _)| *m == 18 && r == "transfers"),
        "missing transfers mask=18 row in fixture: {rows:?}"
    );

    let stats = push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(stats.pushed, 1);

    // After push: ONLY the transfers row should be stamped. The sync
    // mask=63 row stays pushed_at IS NULL.
    let stamped_sync: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _transaction_changes c
               JOIN _operations o ON c.operation_id = o.id
              WHERE c.transaction_id = 1 AND o.reason = 'sync'
                AND c.pushed_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        stamped_sync, 0,
        "sync rows (incl. the mask=63 create marker) must never be stamped pushed_at"
    );

    let stamped_transfers: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _transaction_changes c
               JOIN _operations o ON c.operation_id = o.id
              WHERE c.transaction_id = 1 AND o.reason = 'transfers'
                AND c.pushed_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stamped_transfers, 1, "the transfers row should be stamped");
}

/// Push must not write to the `transactions` table at all (Stage 1
/// invariant: `transactions` is owned by `sync` + un-pushed local edits;
/// push is neither). Verifies by snapshotting every column before and
/// after a successful push.
#[test]
fn push_does_not_modify_transactions_table() {
    let conn = test_db();
    fixture_confirmed_transfer(&conn, 1);
    let api = StubApi::new();
    api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
    // Server would normally bump updated_at; make sure we'd notice if we
    // copied it onto the local row.
    *api.next_updated_at.borrow_mut() = "2099-12-31T23:59:59Z".into();

    let before: Vec<(String, Option<String>)> = conn
        .prepare("SELECT name, '' FROM pragma_table_info('transactions')")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    let cols: Vec<String> = before.into_iter().map(|(c, _)| c).collect();
    let sql = format!(
        "SELECT {} FROM transactions WHERE id = 1",
        cols.iter()
            .map(|c| format!("COALESCE(CAST({c} AS TEXT), '<null>')"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let snap_before: Vec<String> = conn
        .query_row(&sql, [], |row| {
            (0..cols.len())
                .map(|i| row.get::<_, String>(i))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap();

    let stats = push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(stats.pushed, 1);

    let snap_after: Vec<String> = conn
        .query_row(&sql, [], |row| {
            (0..cols.len())
                .map(|i| row.get::<_, String>(i))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap();

    assert_eq!(
        snap_before, snap_after,
        "push must not modify any column of the transactions row"
    );
}

/// Push records its successful PUTs in `_operations.transactions_updated`
/// via `db::record_operation_writes`, since the default counter (which
/// derives from `_transaction_changes` trigger rows) would always be 0
/// for push.
#[test]
fn push_records_explicit_writes_on_operations_row() {
    let conn = test_db();
    fixture_confirmed_transfer(&conn, 1);
    fixture_confirmed_transfer(&conn, 2);
    let api = StubApi::new();
    api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
    api.set_remote(remote_matching(2, "2024-06-15T00:00:00Z"));

    let stats = push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(stats.pushed, 2);

    let (reason, count): (String, i64) = conn
        .query_row(
            "SELECT reason, transactions_updated
               FROM _operations ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(reason, "push");
    assert_eq!(count, 2);
}

/// End-to-end stub test: the full transfer lifecycle from raw
/// transactions through detection, confirmation, application, and push.
///
/// Rationale: `push_happy_path` jumps straight to the post-`apply`
/// state via a hand-crafted `_transaction_changes` row. This test wires
/// the upstream as well — `transfers::find_pairs` then
/// `db::transfer_pairs::update_status` then `transfers::apply_confirmed`
/// — so push's pending query is verified against output that
/// `transfers --apply` actually produces. The wire format of the PUT is
/// already pinned by `tests/api_integration::test_transaction_lifecycle`
/// against the live API, so no live call is needed here.
#[test]
fn end_to_end_pair_lifecycle_via_stub() {
    use crate::db::{upsert_category, upsert_transaction_account, upsert_transaction};
    use crate::transfers::{self, Confidence, Status};

    let conn = test_db();

    // --- Fixture: two opposite-sign transactions on different accounts,
    // same date, neither marked as transfer. Plus a `_Transfer` category
    // (apply_confirmed bails without it).
    with_operation(&conn, "sync", |conn| {
        upsert_category(conn, &make_category(99, "_Transfer"))?;
        upsert_transaction_account(conn, &make_transaction_account(10, "Checking"))?;
        upsert_transaction_account(conn, &make_transaction_account(20, "Savings"))?;

        let mut a = make_transaction(1, "Transfer to xx8005");
        a.amount = Some(-250.0);
        a.date = Some("2024-06-15".into());
        a.transaction_account = Some(make_transaction_account(10, "Checking"));
        a.is_transfer = Some(false);
        a.category = None;
        a.updated_at = Some("2024-06-15T00:00:00Z".into());
        upsert_transaction(conn, &a)?;

        let mut b = make_transaction(2, "Transfer from xx0001");
        b.amount = Some(250.0);
        b.date = Some("2024-06-15".into());
        b.transaction_account = Some(make_transaction_account(20, "Savings"));
        b.is_transfer = Some(false);
        b.category = None;
        b.updated_at = Some("2024-06-15T00:00:00Z".into());
        upsert_transaction(conn, &b)?;
        Ok(())
    })
    .unwrap();

    // --- Step 1: detect. Must find exactly one pair.
    let candidates = transfers::find_pairs(&conn).unwrap();
    assert_eq!(
        candidates.len(),
        1,
        "find_pairs should detect the seeded pair; got: {candidates:?}"
    );
    let cand = &candidates[0];
    assert_eq!(cand.confidence, Confidence::High);
    // Order isn't guaranteed; just check it's our two txns.
    let mut ids = [cand.txn_id_a, cand.txn_id_b];
    ids.sort();
    assert_eq!(ids, [1, 2]);

    // --- Step 2: insert as pending (mirrors what the auto-confirm path
    // would do without auto-confirming).
    crate::db::transfer_pairs::insert_pair(
        &conn,
        &crate::transfers::TransferPair {
            txn_id_a: cand.txn_id_a,
            txn_id_b: cand.txn_id_b,
            amount_cents: cand.amount_cents,
            confidence: cand.confidence,
            status: Status::Pending,
        },
    )
    .unwrap();

    // --- Step 3: review marks it confirmed.
    crate::db::transfer_pairs::update_status(
        &conn,
        cand.txn_id_a,
        cand.txn_id_b,
        Status::Confirmed,
    )
    .unwrap();
    let status_int: i32 = conn
        .query_row(
            "SELECT status FROM transfer_pairs WHERE txn_id_a = ?1 AND txn_id_b = ?2",
            rusqlite::params![cand.txn_id_a, cand.txn_id_b],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status_int, Status::Confirmed.to_i32());

    // --- Step 4: apply. Local UPDATEs set is_transfer=1 + category_id=99
    // and delete the confirmed pair row.
    let apply_stats = transfers::apply_confirmed(&conn).unwrap();
    assert_eq!(apply_stats.rows_drained, 1);
    assert_eq!(apply_stats.transactions_updated, 2);

    // --- Step 5: push picks both txns up.
    let api = StubApi::new();
    api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
    api.set_remote(remote_matching(2, "2024-06-15T00:00:00Z"));

    // Pending query sees both before push.
    let pending = pending_txn_ids(&conn, None).unwrap();
    let mut p = pending.clone();
    p.sort();
    assert_eq!(p, vec![1, 2], "pending query must see both legs of the pair");

    let stats = push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(stats.pushed, 2, "stats: {stats:?}");
    assert_eq!(stats.failed, 0);

    // Two PUTs, both carrying the transfer-side fields (is_transfer,
    // category_id, and the new `[paired:<other_id>]` memo marker) and
    // nothing else.
    let puts = api.puts.borrow();
    assert_eq!(puts.len(), 2);
    for (id, put) in puts.iter() {
        assert!(*id == 1 || *id == 2);
        assert_eq!(put.is_transfer, Some(true));
        assert_eq!(put.category_id, Some(99));
        // memo should reference the other leg of the pair.
        let other = if *id == 1 { 2 } else { 1 };
        assert_eq!(
            put.memo.as_deref(),
            Some(format!("[paired:{other}]").as_str()),
            "PUT memo should be the paired-marker for txn {id}"
        );
        assert!(
            put.payee.is_none()
                && put.note.is_none()
                && put.labels.is_none()
                && put.amount.is_none()
                && put.date.is_none(),
            "PUT carried fields outside transfer-push scope: {put:?}"
        );
    }

    // Both txns stamped on their transfers row; sync rows untouched.
    for id in [1i64, 2] {
        let stamped_transfers: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _transaction_changes c
                   JOIN _operations o ON c.operation_id = o.id
                  WHERE c.transaction_id = ?1 AND o.reason = 'transfers'
                    AND c.pushed_at IS NOT NULL",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stamped_transfers, 1, "txn {id} transfers row not stamped");

        let stamped_sync: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _transaction_changes c
                   JOIN _operations o ON c.operation_id = o.id
                  WHERE c.transaction_id = ?1 AND o.reason = 'sync'
                    AND c.pushed_at IS NOT NULL",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stamped_sync, 0, "txn {id} sync row was wrongly stamped");
    }

    // Push's operations row reports the right write count.
    let (reason, written): (String, i64) = conn
        .query_row(
            "SELECT reason, transactions_updated
               FROM _operations ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(reason, "push");
    assert_eq!(written, 2);

    // Re-running push is a no-op.
    let again = push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(again.pushed, 0);
    assert_eq!(again.would_push, 0);
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

// =============================================================================
// Stage 3 tests — generalised pending query, multi-reason fold, all six
// locally-mutated fields. See `.claude/plans/push-stage-3-expand-fields.md`.
// =============================================================================

/// Helper: insert a baseline txn under reason='sync', then mutate it under
/// the given reason. Returns the txn id.
fn fixture_local_edit(conn: &Connection, id: i64, reason: &str, mutate_sql: &str) -> i64 {
    with_operation(conn, "sync", |conn| {
        let mut t = make_transaction(id, "Initial Payee");
        t.is_transfer = Some(false);
        t.category = None;
        t.memo = None;
        t.note = None;
        t.payee = Some("Initial Payee".into());
        t.labels = None;
        t.updated_at = Some("2024-06-15T00:00:00Z".into());
        upsert_transaction(conn, &t)?;
        Ok(())
    })
    .unwrap();
    with_operation(conn, reason, |conn| {
        conn.execute(mutate_sql, [])?;
        Ok(())
    })
    .unwrap();
    id
}

/// Stage 3 test 14: pending query picks up `reason='normalisation'` rows.
#[test]
fn pending_query_picks_up_normalisation_rows() {
    let conn = test_db();
    fixture_local_edit(
        &conn,
        1,
        "normalisation",
        "UPDATE transactions SET payee = 'Cleaned Payee' WHERE id = 1",
    );
    assert_eq!(pending_txn_ids(&conn, None).unwrap(), vec![1]);
}

/// Stage 3 test 15a: pending query ignores `reason='sync'` rows.
#[test]
fn pending_query_ignores_sync_rows() {
    let conn = test_db();
    // Sync-only insert, no local edits afterwards.
    with_operation(&conn, "sync", |conn| {
        upsert_transaction(conn, &make_transaction(1, "Anything"))
    })
    .unwrap();
    assert!(pending_txn_ids(&conn, None).unwrap().is_empty());
}

/// Stage 3 test 15b: pending query ignores `reason='push'` rows. (Push
/// itself doesn't currently write to `transactions`, but a future writer
/// under reason='push' must not generate self-pushable rows.)
#[test]
fn pending_query_ignores_push_rows() {
    let conn = test_db();
    with_operation(&conn, "sync", |conn| {
        upsert_transaction(conn, &make_transaction(1, "X"))
    })
    .unwrap();
    // Force a row under reason='push' with a dirty bit. We can't go through
    // the trigger because push doesn't UPDATE transactions, so we synthesise
    // a change row directly.
    with_operation(&conn, "push", |conn| {
        conn.execute(
            "INSERT INTO _transaction_changes
               (transaction_id, payee, operation_id, mask)
               VALUES (1, 'fake', (SELECT id FROM _current_operation), 1)",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    assert!(pending_txn_ids(&conn, None).unwrap().is_empty());
}

/// Stage 3 test 16: two unpushed history rows on the same txn (one
/// `normalisation`, one `transfers`) → a single PUT carrying both fields.
#[test]
fn multi_reason_dirty_bits_fold_into_one_put() {
    let conn = test_db();

    // Sync seed.
    with_operation(&conn, "sync", |conn| {
        crate::db::upsert_category(conn, &make_category(99, "_Transfer"))?;
        let mut t = make_transaction(1, "Initial Payee");
        t.is_transfer = Some(false);
        t.category = None;
        t.payee = Some("Initial Payee".into());
        t.updated_at = Some("2024-06-15T00:00:00Z".into());
        upsert_transaction(conn, &t)?;
        Ok(())
    })
    .unwrap();

    // Normalisation: clean the payee.
    with_operation(&conn, "normalisation", |conn| {
        conn.execute(
            "UPDATE transactions SET payee = 'Cleaned Payee' WHERE id = 1",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    // Transfers --apply: flip is_transfer + category_id.
    with_operation(&conn, "transfers", |conn| {
        conn.execute(
            "UPDATE transactions SET is_transfer = 1, category_id = 99 WHERE id = 1",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    // Push.
    let api = StubApi::new();
    api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
    let stats = push(&api, &conn, &PushOpts::default()).unwrap();

    assert_eq!(stats.pushed, 1, "stats: {stats:?}");
    let puts = api.puts.borrow();
    assert_eq!(puts.len(), 1, "a multi-reason fold must produce exactly one PUT");
    let (id, put) = &puts[0];
    assert_eq!(*id, 1);
    assert_eq!(put.payee.as_deref(), Some("Cleaned Payee"));
    assert_eq!(put.is_transfer, Some(true));
    assert_eq!(put.category_id, Some(99));
    // Untouched fields stay None.
    assert!(put.memo.is_none() && put.note.is_none() && put.labels.is_none());
}

/// Stage 3 test 17: after a successful Stage-3 PUT, all involved
/// `_transaction_changes` rows (across reasons) have `pushed_at` stamped;
/// the sync-create marker stays unstamped.
#[test]
fn pushed_at_stamped_on_all_local_writer_rows() {
    let conn = test_db();
    with_operation(&conn, "sync", |conn| {
        crate::db::upsert_category(conn, &make_category(99, "_Transfer"))?;
        let mut t = make_transaction(1, "Initial Payee");
        t.is_transfer = Some(false);
        t.category = None;
        t.payee = Some("Initial Payee".into());
        t.updated_at = Some("2024-06-15T00:00:00Z".into());
        upsert_transaction(conn, &t)?;
        Ok(())
    })
    .unwrap();
    with_operation(&conn, "normalisation", |conn| {
        conn.execute(
            "UPDATE transactions SET payee = 'Cleaned' WHERE id = 1",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    with_operation(&conn, "transfers", |conn| {
        conn.execute(
            "UPDATE transactions SET is_transfer = 1, category_id = 99 WHERE id = 1",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    let api = StubApi::new();
    api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
    push(&api, &conn, &PushOpts::default()).unwrap();

    let stamped: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _transaction_changes c
               JOIN _operations o ON c.operation_id = o.id
              WHERE c.transaction_id = 1
                AND o.reason IN ('normalisation','transfers')
                AND c.pushed_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stamped, 2, "both local-writer rows should be stamped");

    let unstamped_sync: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM _transaction_changes c
               JOIN _operations o ON c.operation_id = o.id
              WHERE c.transaction_id = 1 AND o.reason = 'sync'
                AND c.pushed_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(unstamped_sync, 0, "sync row must remain unstamped");
}

/// Stage 3 test 18: labels stored as a JSON array locally are PUT as CSV
/// on the wire. Pinned here so a future serialisation tweak doesn't drift.
#[test]
fn labels_serialise_as_csv_on_wire() {
    let conn = test_db();
    fixture_local_edit(
        &conn,
        1,
        "normalisation",
        "UPDATE transactions SET labels = '[\"food\",\"weekly\"]' WHERE id = 1",
    );

    let api = StubApi::new();
    api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
    let stats = push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(stats.pushed, 1);

    let puts = api.puts.borrow();
    assert_eq!(puts.len(), 1);
    let put = &puts[0].1;
    assert_eq!(put.labels.as_deref(), Some("food,weekly"));

    // And the on-wire JSON pins the shape directly (not a Vec, not an array).
    let body = serde_json::to_string(put).unwrap();
    assert!(
        body.contains("\"labels\":\"food,weekly\""),
        "unexpected serialisation: {body}"
    );
}

/// Module-level safety: `build_update` populates exactly the dirty bits.
#[test]
fn build_update_only_sets_dirty_bits() {
    let local = LocalSnapshot {
        payee: Some("P".into()),
        category_id: Some(7),
        note: Some("N".into()),
        labels: Some("[\"a\"]".into()),
        is_transfer: 1,
        memo: Some("M".into()),
        updated_at: Some("2024-06-15T00:00:00Z".into()),
    };
    // Only payee + memo dirty.
    let put = build_update(&local, MASK_PAYEE | MASK_MEMO);
    assert_eq!(put.payee.as_deref(), Some("P"));
    assert_eq!(put.memo.as_deref(), Some("M"));
    assert!(put.category_id.is_none());
    assert!(put.note.is_none());
    assert!(put.labels.is_none());
    assert!(put.is_transfer.is_none());
}

/// Each mask bit drives exactly one `TransactionUpdate` field; setting a
/// bit on its own must populate that field and leave the other five `None`.
/// Pinned so that adding a new locally-mutated field can't silently break
/// the bit→field mapping.
#[test]
fn build_update_each_bit_in_isolation() {
    let local = LocalSnapshot {
        payee: Some("P".into()),
        category_id: Some(7),
        note: Some("N".into()),
        labels: Some("[\"a\",\"b\"]".into()),
        is_transfer: 1,
        memo: Some("M".into()),
        updated_at: Some("2024-06-15T00:00:00Z".into()),
    };

    let put = build_update(&local, MASK_PAYEE);
    assert_eq!(put.payee.as_deref(), Some("P"));
    assert!(put.category_id.is_none() && put.note.is_none() && put.labels.is_none()
        && put.is_transfer.is_none() && put.memo.is_none());

    let put = build_update(&local, MASK_CATEGORY_ID);
    assert_eq!(put.category_id, Some(7));
    assert!(put.payee.is_none() && put.note.is_none() && put.labels.is_none()
        && put.is_transfer.is_none() && put.memo.is_none());

    let put = build_update(&local, MASK_NOTE);
    assert_eq!(put.note.as_deref(), Some("N"));

    let put = build_update(&local, MASK_LABELS);
    assert_eq!(put.labels.as_deref(), Some("a,b"));

    let put = build_update(&local, MASK_IS_TRANSFER);
    assert_eq!(put.is_transfer, Some(true));

    let put = build_update(&local, MASK_MEMO);
    assert_eq!(put.memo.as_deref(), Some("M"));

    // Empty mask → fully empty body.
    let put = build_update(&local, 0);
    assert_eq!(serde_json::to_string(&put).unwrap(), "{}");
}

/// Direct coverage of the JSON→CSV labels transform. Production behaviour
/// is pinned indirectly through `labels_serialise_as_csv_on_wire`, but the
/// helper has edge cases (empty array, invalid JSON, single element) that
/// don't appear there.
#[test]
fn labels_for_put_shapes() {
    assert_eq!(labels_for_put(None), None);
    assert_eq!(labels_for_put(Some("[]")).as_deref(), Some(""));
    assert_eq!(labels_for_put(Some("[\"only\"]")).as_deref(), Some("only"));
    assert_eq!(
        labels_for_put(Some("[\"a\",\"b\",\"c\"]")).as_deref(),
        Some("a,b,c")
    );
    // Invalid JSON: pass through unchanged so the user sees what's stored
    // rather than a silent drop. Inline-checked with a deliberately weird
    // value (CSV-ish, missing brackets).
    assert_eq!(
        labels_for_put(Some("already,csv")).as_deref(),
        Some("already,csv")
    );
}

/// Reproduces the live Stage 3 smoke test: a single normalisation
/// edit (payee only) on an otherwise-clean txn produces a PUT body of
/// exactly `{"payee":"<new>"}` — no other fields, not even is_transfer.
/// Pinned because Stage 1's transfer-path tests would never catch a
/// regression that accidentally sent is_transfer:false on payee-only
/// pushes (which would silently overwrite a server-side classification).
#[test]
fn payee_only_push_sends_only_payee() {
    let conn = test_db();
    fixture_local_edit(
        &conn,
        1,
        "normalisation",
        "UPDATE transactions SET payee = 'Cleaned Payee' WHERE id = 1",
    );

    let api = StubApi::new();
    api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
    let stats = push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(stats.pushed, 1);

    let puts = api.puts.borrow();
    assert_eq!(puts.len(), 1);
    let (id, put) = &puts[0];
    assert_eq!(*id, 1);
    assert_eq!(put.payee.as_deref(), Some("Cleaned Payee"));
    // Critical: no other field set, including is_transfer (which has a
    // dedicated bool field on TransactionUpdate and is the easiest
    // accidental-default to trip).
    assert!(
        put.is_transfer.is_none()
            && put.category_id.is_none()
            && put.note.is_none()
            && put.labels.is_none()
            && put.memo.is_none()
            && put.amount.is_none()
            && put.date.is_none()
            && put.cheque_number.is_none()
            && put.needs_review.is_none(),
        "PUT carried unexpected fields: {put:?}"
    );

    // And on the wire the body is exactly the one-key object.
    let body = serde_json::to_string(put).unwrap();
    assert_eq!(body, r#"{"payee":"Cleaned Payee"}"#);

    // push_log captures the same body verbatim.
    let logged: String = conn
        .query_row(
            "SELECT request_body FROM push_log WHERE outcome = 'pushed' AND txn_id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(logged, r#"{"payee":"Cleaned Payee"}"#);
}

/// Lifecycle: edit → push → edit-again → push must pick up only the
/// second edit. Mirrors the live behaviour we saw with the six
/// originally-pushed transfer pairs that got re-pushed after
/// `--annotate-existing` added their `[paired:<id>]` memos: the first
/// batch's rows stay stamped, the new mask=32 row is fresh and gets
/// picked up. Generalised here for the normalisation reason.
#[test]
fn re_edit_after_push_picks_up_only_new_change() {
    let conn = test_db();
    fixture_local_edit(
        &conn,
        1,
        "normalisation",
        "UPDATE transactions SET payee = 'First Pass' WHERE id = 1",
    );

    let api = StubApi::new();
    api.set_remote(remote_matching(1, "2024-06-15T00:00:00Z"));
    let first = push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(first.pushed, 1);

    // Simulate sync absorbing the server's bumped updated_at — production
    // does this on the next `pocketsmith sync`. We need it because the
    // stub stamped a new updated_at into its remote view.
    *api.next_updated_at.borrow_mut() = "2024-07-01T00:00:00Z".into();
    api.set_remote(remote_matching(1, "2024-07-01T00:00:00Z"));
    with_operation(&conn, "sync", |conn| {
        conn.execute(
            "UPDATE transactions SET updated_at = '2024-07-01T00:00:00Z' WHERE id = 1",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    // Second normalisation edit — e.g. a rule was tightened.
    with_operation(&conn, "normalisation", |conn| {
        conn.execute(
            "UPDATE transactions SET payee = 'Second Pass' WHERE id = 1",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    // Pending query sees exactly the new edit, not the stamped one.
    assert_eq!(pending_txn_ids(&conn, None).unwrap(), vec![1]);

    let second = push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(second.pushed, 1);
    let puts = api.puts.borrow();
    assert_eq!(puts.len(), 2, "first + second push together");
    assert_eq!(puts[1].1.payee.as_deref(), Some("Second Pass"));

    // Both normalisation rows are now stamped. SQLite timestamps have
    // millisecond precision, so two fast pushes may legitimately share the
    // same value; row order, not timestamp uniqueness, identifies the runs.
    let stamps: Vec<Option<String>> = conn
        .prepare(
            "SELECT c.pushed_at FROM _transaction_changes c
               JOIN _operations o ON c.operation_id = o.id
              WHERE c.transaction_id = 1 AND o.reason = 'normalisation'
              ORDER BY c.id",
        )
        .unwrap()
        .query_map([], |r| r.get::<_, Option<String>>(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(stamps.len(), 2);
    assert!(stamps[0].is_some() && stamps[1].is_some());
    assert!(stamps[0] <= stamps[1], "push stamps must not go backwards");
}
