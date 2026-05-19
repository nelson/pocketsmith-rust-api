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
