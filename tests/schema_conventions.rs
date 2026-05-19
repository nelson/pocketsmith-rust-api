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
