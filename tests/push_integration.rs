//! End-to-end integration tests for the `push` pipeline.
//!
//! These complement the rich stubbed-API unit tests in `src/push/tests.rs`
//! by driving the **real** writers (`normalise::scan`/`apply_confirmed`,
//! `transfers`-style direct UPDATEs) into the `push::push` entry point
//! through the public `PushApi` trait. They cover seams the unit tests
//! deliberately don't touch:
//!
//! 1. The `reason='normalise-apply'` string (literally what
//!    `normalise::apply::apply_confirmed` writes) is recognised by the
//!    pending-push query. A rename in either place would be caught here.
//! 2. The full normalise scan→confirm→apply→push round-trip produces a
//!    payee-only PUT and stamps `pushed_at` on the apply's change row.
//! 3. Production timestamp drift: apply runs locally, the remote then
//!    changes (someone edits in the PocketSmith UI), and a subsequent
//!    push records `skipped_changed_upstream` and leaves the change row
//!    unstamped so the next try can revisit it.
//! 4. `--limit` truncates by transaction id and leaves the unpicked txns
//!    pending — important when bulk-pushing 1000+ rows in batches.
//! 5. `push::count_pending` agrees with `push::push`'s own outcome count
//!    and trends to zero after a successful drain.

use std::cell::RefCell;
use std::collections::HashMap;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use pocketsmith::db::{self, payee_normalisations as pn, with_operation};
use pocketsmith::models::{Category, Transaction, TransactionUpdate};
use pocketsmith::normalise::{apply as norm_apply, scan as norm_scan};
use pocketsmith::push::{self, PushApi, PushOpts};
use pocketsmith::review::Status;

// ---------------------------------------------------------------------------
// Stub PushApi — implements the public trait from outside the crate. Records
// every GET/PUT and lets each test pre-seed the remote view.
// ---------------------------------------------------------------------------

struct StubApi {
    remotes: RefCell<HashMap<i64, Transaction>>,
    next_updated_at: RefCell<String>,
    gets: RefCell<Vec<i64>>,
    puts: RefCell<Vec<(i64, TransactionUpdate)>>,
}

impl StubApi {
    fn new() -> Self {
        Self {
            remotes: RefCell::new(HashMap::new()),
            next_updated_at: RefCell::new("2026-02-01T00:00:00Z".into()),
            gets: RefCell::new(Vec::new()),
            puts: RefCell::new(Vec::new()),
        }
    }

    fn set_remote(&self, id: i64, payee: &str, updated_at: &str) {
        self.remotes
            .borrow_mut()
            .insert(id, make_remote(id, payee, updated_at));
    }
}

impl PushApi for StubApi {
    fn get_transaction(&self, id: i64) -> Result<Transaction> {
        self.gets.borrow_mut().push(id);
        self.remotes
            .borrow()
            .get(&id)
            .cloned()
            .with_context(|| format!("stub has no remote for txn {id}"))
    }

    fn update_transaction(&self, id: i64, update: &TransactionUpdate) -> Result<Transaction> {
        // TransactionUpdate doesn't impl Clone; spell it out so we can record
        // exactly what the wire body would have been.
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
        let base = self
            .remotes
            .borrow()
            .get(&id)
            .cloned()
            .with_context(|| format!("stub has no remote for txn {id} to update"))?;
        Ok(Transaction {
            payee: update.payee.clone().or(base.payee),
            is_transfer: update.is_transfer.or(base.is_transfer),
            updated_at: Some(self.next_updated_at.borrow().clone()),
            ..base
        })
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

fn make_remote(id: i64, payee: &str, updated_at: &str) -> Transaction {
    Transaction {
        id,
        transaction_type: Some("debit".into()),
        payee: Some(payee.into()),
        amount: Some(-50.0),
        amount_in_base_currency: Some(-50.0),
        date: Some("2026-01-01".into()),
        cheque_number: None,
        memo: None,
        is_transfer: Some(false),
        category: None,
        note: None,
        labels: None,
        original_payee: Some(payee.into()),
        upload_source: None,
        closing_balance: None,
        transaction_account: None,
        status: Some("posted".into()),
        needs_review: Some(false),
        created_at: Some("2026-01-01T00:00:00Z".into()),
        updated_at: Some(updated_at.into()),
    }
}

/// Seed a transaction account (synthesised — we don't go through the API)
/// and a synced transaction whose initial state mirrors what `sync` would
/// have produced. Crucially, the seed runs under reason='sync' so its
/// resulting `_transaction_changes` row is filtered out of the
/// pending-push query.
fn seed_synced_txn(conn: &Connection, id: i64, original_payee: &str) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO transaction_accounts (id, name) VALUES (1, 'Test')",
        [],
    )?;
    with_operation(conn, "sync", |conn| {
        conn.execute(
            "INSERT INTO transactions
                (id, transaction_account_id, date, amount, original_payee, payee, updated_at)
             VALUES (?1, 1, '2026-01-01', -50.0, ?2, ?2, '2026-01-01T00:00:00Z')",
            params![id, original_payee],
        )?;
        Ok(())
    })?;
    Ok(())
}

fn fresh_db() -> Connection {
    let conn = db::initialize_in_memory().unwrap();
    // The normalisation pipeline reads its rules from the DB; seed them so
    // norm_scan produces the same proposals it did with const rules.
    pocketsmith::rules::load_into_db(&conn).unwrap();
    conn
}

fn unstamped_change_count(conn: &Connection, txn_id: i64, reason: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM _transaction_changes c
           JOIN _operations o ON c.operation_id = o.id
          WHERE c.transaction_id = ?1
            AND o.reason = ?2
            AND c.pushed_at IS NULL",
        params![txn_id, reason],
        |r| r.get::<_, i64>(0),
    )
    .unwrap()
}

// ---------------------------------------------------------------------------
// 1. Full normalise scan→confirm→apply→push round-trip
// ---------------------------------------------------------------------------

#[test]
fn normalise_scan_confirm_apply_push_round_trip() {
    let conn = fresh_db();
    // Three transactions sharing one bank-supplied original payee — typical
    // case: the user wants the same clean-up applied to all of them.
    for id in 1..=3 {
        seed_synced_txn(&conn, id, "WOOLWORTHS 1624 STRATHF").unwrap();
    }

    // Stage 1: scan → one pending proposal covering all three rows.
    let scan = norm_scan::scan(&conn).unwrap();
    assert_eq!(scan.inserted, 1);

    // Stage 2: user confirms via the serve UI (we shortcut to the DB op).
    pn::update_status(&conn, "WOOLWORTHS 1624 STRATHF", Status::Confirmed).unwrap();

    // Stage 3: apply drains the staging row into transactions.payee. This
    // is what produces the `reason='normalise-apply'` change rows that
    // push must then pick up.
    let apply = norm_apply::apply_confirmed(&conn).unwrap();
    assert_eq!(apply.transactions_updated, 3);
    assert_eq!(apply.rows_drained, 1);

    // The pending-push query must now see all three txns. If the literal
    // reason string ever drifts between `apply.rs` and `push.rs`, this
    // assertion catches it.
    assert_eq!(push::count_pending(&conn).unwrap(), 3);

    // Stage 4: push to a stub API.
    let api = StubApi::new();
    for id in 1..=3 {
        api.set_remote(id, "WOOLWORTHS 1624 STRATHF", "2026-01-01T00:00:00Z");
    }
    let stats = push::push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(stats.pushed, 3);
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.skipped_changed_upstream, 0);

    // Each PUT carries only the payee — payee normalisations must not bleed
    // into other fields. (mask=1 only.)
    let puts = api.puts.borrow();
    assert_eq!(puts.len(), 3);
    for (_id, put) in puts.iter() {
        let normalised = put.payee.as_deref().expect("payee must be set");
        assert_ne!(normalised, "WOOLWORTHS 1624 STRATHF",
            "payee should be the cleaned form, not the raw original");
        assert!(
            put.is_transfer.is_none()
                && put.category_id.is_none()
                && put.note.is_none()
                && put.memo.is_none()
                && put.labels.is_none(),
            "normalise-apply must produce a payee-only PUT, got: {put:?}"
        );
    }

    // All three normalise-apply change rows now have pushed_at stamped.
    for id in 1..=3 {
        assert_eq!(unstamped_change_count(&conn, id, "normalise-apply"), 0);
    }

    // And the queue is drained.
    assert_eq!(push::count_pending(&conn).unwrap(), 0);

    // Idempotent re-run (matches the manual verification step in the
    // walk-through: a second push must be a no-op).
    let again = push::push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(again.pushed, 0);
    assert_eq!(again.would_push, 0);
}

// ---------------------------------------------------------------------------
// 2. Timestamp drift after apply — production scenario
// ---------------------------------------------------------------------------

#[test]
fn timestamp_drift_after_apply_skips_and_leaves_change_row_pending() {
    let conn = fresh_db();
    seed_synced_txn(&conn, 42, "AMAZON MKTPLACE PMTS").unwrap();

    norm_scan::scan(&conn).unwrap();
    pn::update_status(&conn, "AMAZON MKTPLACE PMTS", Status::Confirmed).unwrap();
    norm_apply::apply_confirmed(&conn).unwrap();
    assert_eq!(push::count_pending(&conn).unwrap(), 1);

    // Simulate: between apply and push, the user edited the transaction in
    // the PocketSmith web UI. Remote updated_at advances; our local copy
    // still says 2026-01-01. The GET returns a different timestamp.
    let api = StubApi::new();
    api.set_remote(42, "AMAZON MKTPLACE PMTS", "2026-01-15T09:30:00Z");

    let stats = push::push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(stats.skipped_changed_upstream, 1);
    assert_eq!(stats.pushed, 0);
    assert_eq!(stats.failed, 0);

    // Critical: no PUT was sent.
    assert_eq!(api.puts.borrow().len(), 0);

    // Critical: the change row is NOT stamped — a future push (after a
    // sync that absorbs the upstream change, then a re-decision by the
    // user) must be able to revisit this txn.
    assert_eq!(unstamped_change_count(&conn, 42, "normalise-apply"), 1);
    assert_eq!(push::count_pending(&conn).unwrap(), 1);

    // And push_log records the skip with both timestamps for audit.
    let (outcome, local_ts, remote_ts): (String, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT outcome, local_updated_at_before, remote_updated_at_seen
               FROM push_log WHERE txn_id = 42",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(outcome, "skipped_changed_upstream");
    assert_eq!(local_ts.as_deref(), Some("2026-01-01T00:00:00Z"));
    assert_eq!(remote_ts.as_deref(), Some("2026-01-15T09:30:00Z"));
}

// ---------------------------------------------------------------------------
// 3. --limit truncates and leaves the rest pending (multi-txn batching)
// ---------------------------------------------------------------------------

#[test]
fn limit_pushes_a_prefix_and_leaves_the_remainder_pending() {
    let conn = fresh_db();
    for id in [10i64, 20, 30, 40, 50] {
        seed_synced_txn(&conn, id, "COLES 0042").unwrap();
    }
    norm_scan::scan(&conn).unwrap();
    pn::update_status(&conn, "COLES 0042", Status::Confirmed).unwrap();
    norm_apply::apply_confirmed(&conn).unwrap();
    assert_eq!(push::count_pending(&conn).unwrap(), 5);

    let api = StubApi::new();
    for id in [10i64, 20, 30, 40, 50] {
        api.set_remote(id, "COLES 0042", "2026-01-01T00:00:00Z");
    }

    // First batch of 2: pending-query orders by transaction_id ASC, so we
    // get {10, 20}.
    let stats = push::push(
        &api,
        &conn,
        &PushOpts {
            dry_run: false,
            limit: Some(2),
        },
    )
    .unwrap();
    assert_eq!(stats.pushed, 2);
    let pushed_ids: Vec<i64> = api.puts.borrow().iter().map(|(id, _)| *id).collect();
    assert_eq!(pushed_ids, vec![10, 20]);

    // The remaining three are still pending.
    assert_eq!(push::count_pending(&conn).unwrap(), 3);
    assert_eq!(unstamped_change_count(&conn, 10, "normalise-apply"), 0);
    assert_eq!(unstamped_change_count(&conn, 30, "normalise-apply"), 1);
    assert_eq!(unstamped_change_count(&conn, 50, "normalise-apply"), 1);

    // Drain the rest in one go.
    let stats = push::push(&api, &conn, &PushOpts::default()).unwrap();
    assert_eq!(stats.pushed, 3);
    assert_eq!(push::count_pending(&conn).unwrap(), 0);
}

// ---------------------------------------------------------------------------
// 4. count_pending tracks the real queue depth
// ---------------------------------------------------------------------------

#[test]
fn count_pending_agrees_with_push_outcome_and_drains_to_zero() {
    let conn = fresh_db();
    for id in 1..=4 {
        seed_synced_txn(&conn, id, "UBER *EATS").unwrap();
    }
    assert_eq!(push::count_pending(&conn).unwrap(), 0,
        "fresh sync rows must not appear as pending push work");

    norm_scan::scan(&conn).unwrap();
    pn::update_status(&conn, "UBER *EATS", Status::Confirmed).unwrap();
    norm_apply::apply_confirmed(&conn).unwrap();

    let before = push::count_pending(&conn).unwrap();
    assert_eq!(before, 4);

    let api = StubApi::new();
    for id in 1..=4 {
        api.set_remote(id, "UBER *EATS", "2026-01-01T00:00:00Z");
    }
    let stats = push::push(&api, &conn, &PushOpts::default()).unwrap();

    // Push outcome accounts for exactly the count_pending() we measured.
    assert_eq!(
        stats.pushed + stats.skipped_changed_upstream + stats.deleted_upstream + stats.failed,
        before as u32
    );
    assert_eq!(push::count_pending(&conn).unwrap(), 0);
}

// ---------------------------------------------------------------------------
// 5. Multi-writer: transfers + normalise on different txns, --limit picks
//    one, the other stays pending — and the bits don't cross-contaminate.
// ---------------------------------------------------------------------------

#[test]
fn multi_writer_limit_one_pushes_lowest_id_and_isolates_dirty_bits() {
    let conn = fresh_db();
    seed_synced_txn(&conn, 100, "INTERNAL TRANSFER").unwrap();
    seed_synced_txn(&conn, 200, "WOOLWORTHS 1624 STRATHF").unwrap();

    // txn 100: transfers-style edit (is_transfer + category_id together).
    with_operation(&conn, "sync", |conn| {
        db::upsert_category(
            conn,
            &Category {
                id: 99,
                title: Some("_Transfer".into()),
                colour: None,
                children: None,
                parent_id: None,
                is_transfer: Some(true),
                is_bill: None,
                roll_up: None,
                refund_behaviour: None,
                created_at: None,
                updated_at: None,
            },
        )
    })
    .unwrap();
    with_operation(&conn, "transfers", |conn| {
        conn.execute(
            "UPDATE transactions SET is_transfer = 1, category_id = 99 WHERE id = 100",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    // txn 200: real normalise pipeline.
    norm_scan::scan(&conn).unwrap();
    pn::update_status(&conn, "WOOLWORTHS 1624 STRATHF", Status::Confirmed).unwrap();
    norm_apply::apply_confirmed(&conn).unwrap();

    assert_eq!(push::count_pending(&conn).unwrap(), 2);

    let api = StubApi::new();
    api.set_remote(100, "INTERNAL TRANSFER", "2026-01-01T00:00:00Z");
    api.set_remote(200, "WOOLWORTHS 1624 STRATHF", "2026-01-01T00:00:00Z");

    let stats = push::push(
        &api,
        &conn,
        &PushOpts {
            dry_run: false,
            limit: Some(1),
        },
    )
    .unwrap();
    assert_eq!(stats.pushed, 1);

    // txn 100 (lowest id) goes first, with transfer bits only.
    let puts = api.puts.borrow();
    assert_eq!(puts.len(), 1);
    let (id, put) = &puts[0];
    assert_eq!(*id, 100);
    assert_eq!(put.is_transfer, Some(true));
    assert_eq!(put.category_id, Some(99));
    assert!(
        put.payee.is_none(),
        "transfers PUT must not carry the normalise payee from a different txn: {put:?}"
    );

    // txn 200 still pending with only its normalise bit dirty.
    assert_eq!(unstamped_change_count(&conn, 200, "normalise-apply"), 1);
    assert_eq!(push::count_pending(&conn).unwrap(), 1);
}
