// Tests for PR 1 schema conventions: lookup tables, FK references, renames,
// AUTOINCREMENT, and the documented underscore prefix convention (Convention C).
//
// Convention C (also documented at the top of `src/db/schema.rs`):
//   The underscore prefix marks tables and columns that are managed by triggers
//   or the operation framework. Application-readable, application-writable
//   tables (whether locally sourced like `transfer_pairs` or remotely sourced
//   like `transactions`) have no prefix.

use pocketsmith_sync::db;
use rusqlite::Connection;

fn open() -> Connection {
    db::initialize_in_memory().unwrap()
}

// ---- Lookup tables ----

#[test]
fn statuses_lookup_seeded() {
    let conn = open();
    let rows: Vec<(i64, String)> = conn
        .prepare("SELECT id, name FROM statuses ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (0, "pending".to_string()),
            (1, "confirmed".to_string()),
            (2, "rejected".to_string()),
        ]
    );
}

#[test]
fn confidences_lookup_seeded() {
    let conn = open();
    let rows: Vec<(i64, String)> = conn
        .prepare("SELECT id, name FROM confidences ORDER BY id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (0, "low".to_string()),
            (1, "medium".to_string()),
            (2, "high".to_string()),
        ]
    );
}

#[test]
fn field_masks_lookup_seeded() {
    let conn = open();
    let rows: Vec<(i64, String)> = conn
        .prepare("SELECT mask, name FROM field_masks ORDER BY mask")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    // Seed only the 9 masks actually produced today (Option A).
    // FK on _transaction_changes.mask will fail loudly if a new code path
    // emits an un-enumerated combination -- that is the intended alarm.
    assert_eq!(
        rows,
        vec![
            (0, "none".to_string()),
            (1, "payee".to_string()),
            (2, "category_id".to_string()),
            (4, "note".to_string()),
            (8, "labels".to_string()),
            (16, "is_transfer".to_string()),
            (18, "category_id, is_transfer".to_string()),
            (32, "memo".to_string()),
            (63, "create".to_string()),
        ]
    );
}

// ---- Helpers for FK tests ----

/// Create a single transactions row so transfer_pairs FK can attach to it.
/// Wrapped in an operation so the _transactions_history INSERT trigger
/// finds a current operation id.
fn seed_txn(conn: &Connection, id: i64) {
    db::with_transaction_change_log(conn, "test-seed", |conn| {
        conn.execute(
            "INSERT INTO transactions (id, amount, date, transaction_account_id) VALUES (?1, 100.0, '2026-01-01', NULL)",
            rusqlite::params![id],
        )?;
        Ok(())
    })
    .unwrap();
}

// ---- FK enforcement on transfer_pairs ----

// ---- Operation framework renames ----

#[test]
fn operations_table_exists_with_autoincrement_id() {
    let conn = open();
    // Insert two operations, delete the second, insert a third.
    // With AUTOINCREMENT the third must NOT reuse id 2.
    conn.execute(
        "INSERT INTO _operations (reason) VALUES ('test1')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO _operations (reason) VALUES ('test2')",
        [],
    )
    .unwrap();
    let id2 = conn.last_insert_rowid();
    conn.execute("DELETE FROM _operations WHERE id = ?1", [id2])
        .unwrap();
    conn.execute(
        "INSERT INTO _operations (reason) VALUES ('test3')",
        [],
    )
    .unwrap();
    let id3 = conn.last_insert_rowid();
    assert!(
        id3 > id2,
        "AUTOINCREMENT should not reuse id {id2}, got {id3}"
    );
}

#[test]
fn transaction_changes_table_exists_with_renamed_columns() {
    let conn = open();
    // Smoke test: SELECT all expected columns from the renamed table.
    // - _version -> operation_id
    // - _updated -> updated_at
    // - _mask    -> mask
    conn.prepare(
        "SELECT id, transaction_id, operation_id, updated_at, mask, \
         payee, category_id, note, labels, is_transfer, memo, \
         old_payee, old_category_id, old_note, old_labels, old_is_transfer, old_memo \
         FROM _transaction_changes",
    )
    .unwrap();
}

#[test]
fn transaction_changes_id_autoincrement_after_delete() {
    let conn = open();
    seed_txn(&conn, 20);
    // Seeding a txn fires the INSERT trigger, creating a _transaction_changes
    // row with mask = 63. Grab its id.
    let id_a: i64 = conn
        .query_row(
            "SELECT id FROM _transaction_changes WHERE transaction_id = 20",
            [],
            |r| r.get(0),
        )
        .unwrap();
    // Delete that row, then trigger another by updating the txn under an op.
    conn.execute("DELETE FROM _transaction_changes WHERE id = ?1", [id_a])
        .unwrap();
    db::with_transaction_change_log(&conn, "test", |conn| {
        conn.execute(
            "UPDATE transactions SET payee = 'updated' WHERE id = 20",
            [],
        )?;
        Ok(())
    })
    .unwrap();
    let id_b: i64 = conn
        .query_row(
            "SELECT id FROM _transaction_changes WHERE transaction_id = 20",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        id_b > id_a,
        "AUTOINCREMENT should not reuse id {id_a}, got {id_b}"
    );
}

#[test]
fn current_operation_table_exists() {
    let conn = open();
    // Smoke test: column rename `_version` -> `id`. The table is internal but
    // we can still SELECT from it (empty result is fine -- structure is what
    // we are testing).
    conn.prepare("SELECT id FROM _current_operation").unwrap();
}

#[test]
fn operations_table_has_expected_columns() {
    let conn = open();
    // Smoke test: SELECT all expected columns; failure means a missing column.
    conn.prepare("SELECT id, reason, created_at, transactions_updated FROM _operations")
        .unwrap();
}

#[test]
fn transfer_pairs_status_fk_rejects_invalid() {
    let conn = open();
    seed_txn(&conn, 1);
    seed_txn(&conn, 2);
    // status = 99 must be rejected by FK to statuses(id)
    let err = conn
        .execute(
            "INSERT INTO transfer_pairs (txn_id_a, txn_id_b, amount_cents, confidence, status) \
             VALUES (1, 2, 100, 2, 99)",
            [],
        )
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("foreign key"),
        "expected FK violation, got: {msg}"
    );
}

#[test]
fn transfer_pairs_confidence_fk_rejects_invalid() {
    let conn = open();
    seed_txn(&conn, 3);
    seed_txn(&conn, 4);
    let err = conn
        .execute(
            "INSERT INTO transfer_pairs (txn_id_a, txn_id_b, amount_cents, confidence, status) \
             VALUES (3, 4, 100, 99, 0)",
            [],
        )
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("foreign key"),
        "expected FK violation, got: {msg}"
    );
}

// ---- transfer_pairs.updated_at ----

#[test]
fn transfer_pairs_updated_at_refreshes_on_update() {
    let conn = open();
    seed_txn(&conn, 12);
    seed_txn(&conn, 13);
    conn.execute(
        "INSERT INTO transfer_pairs (txn_id_a, txn_id_b, amount_cents, confidence, status, created_at, updated_at) \
         VALUES (12, 13, 100, 2, 0, '2020-01-01T00:00:00.000Z', '2020-01-01T00:00:00.000Z')",
        [],
    )
    .unwrap();
    // Status change should bump updated_at to a later value.
    conn.execute(
        "UPDATE transfer_pairs SET status = 1 WHERE txn_id_a = 12",
        [],
    )
    .unwrap();
    let updated_at: String = conn
        .query_row(
            "SELECT updated_at FROM transfer_pairs WHERE txn_id_a = 12",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        updated_at.as_str() > "2020-01-01T00:00:00.000Z",
        "updated_at should be refreshed on UPDATE, was {updated_at}"
    );
}

#[test]
fn transfer_pairs_has_updated_at_with_default() {
    let conn = open();
    seed_txn(&conn, 10);
    seed_txn(&conn, 11);
    conn.execute(
        "INSERT INTO transfer_pairs (txn_id_a, txn_id_b, amount_cents, confidence, status) \
         VALUES (10, 11, 100, 2, 0)",
        [],
    )
    .unwrap();
    let (created_at, updated_at): (String, String) = conn
        .query_row(
            "SELECT created_at, updated_at FROM transfer_pairs WHERE txn_id_a = 10",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    // Default is the same `strftime(...,'now')` as created_at -- they should
    // be either identical or within a millisecond of each other.
    assert!(!updated_at.is_empty(), "updated_at should default to a timestamp");
    assert_eq!(
        &created_at[..10],
        &updated_at[..10],
        "created_at and updated_at should share the date prefix on insert"
    );
}

#[test]
fn transfer_pairs_status_fk_accepts_valid() {
    let conn = open();
    seed_txn(&conn, 5);
    seed_txn(&conn, 6);
    // All three statuses (0/1/2) and confidences (0/1/2) must be accepted.
    for (status, confidence) in [(0i64, 0i64), (1, 1), (2, 2)] {
        conn.execute(
            "DELETE FROM transfer_pairs WHERE txn_id_a = 5 AND txn_id_b = 6",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transfer_pairs (txn_id_a, txn_id_b, amount_cents, confidence, status) \
             VALUES (5, 6, 100, ?1, ?2)",
            rusqlite::params![confidence, status],
        )
        .unwrap();
    }
}
