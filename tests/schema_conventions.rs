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
