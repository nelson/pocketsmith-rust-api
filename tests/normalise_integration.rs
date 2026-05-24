//! End-to-end integration tests for the normalise scan/apply paradigm.
//!
//! These cover the full pipeline against an in-memory DB:
//!   1. scan → populate payee_normalisations from transactions
//!   2. confirm + apply → drain confirmed rows into transactions.payee
//!   3. reject persistence → a rejected row stays rejected across rescans
//!      until the underlying proposal changes.

use rusqlite::{params, Connection};

use pocketsmith_sync::db::{self, payee_normalisations as pn};
use pocketsmith_sync::normalise::{apply as norm_apply, scan as norm_scan};
use pocketsmith_sync::review::Status;
use pocketsmith_sync::test_support::{seed_account, seed_txn};

fn fresh_db() -> Connection {
    let conn = db::initialize_in_memory().unwrap();
    seed_account(&conn, 1, "Test").unwrap();
    conn
}

fn insert_txn(conn: &Connection, id: i64, original_payee: &str, payee: &str) {
    seed_txn(conn, id, 1, original_payee, payee).unwrap();
}

fn payee_of(conn: &Connection, id: i64) -> Option<String> {
    conn.query_row(
        "SELECT payee FROM transactions WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )
    .ok()
}

#[test]
fn scan_then_apply_round_trip_writes_payee_and_drains_staging() {
    let conn = fresh_db();
    // Three transactions sharing one bank-supplied original payee.
    for id in 1..=3 {
        insert_txn(&conn, id, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF");
    }

    // Step 1: scan stages a single pending proposal covering all three rows.
    let scan_stats = norm_scan::scan(&conn).unwrap();
    assert_eq!(scan_stats.inserted, 1);
    let staged = pn::get_by_original(&conn, "WOOLWORTHS 1624 STRATHF")
        .unwrap()
        .unwrap();
    assert_eq!(staged.status, Status::Pending);
    assert_eq!(staged.txn_count, 3);

    // Step 2: confirm + apply writes payee to every transaction in the
    // group and removes the staging row.
    pn::update_status(&conn, "WOOLWORTHS 1624 STRATHF", Status::Confirmed).unwrap();
    let apply_stats = norm_apply::apply_confirmed(&conn).unwrap();
    assert_eq!(apply_stats.transactions_updated, 3);
    assert_eq!(apply_stats.rows_drained, 1);

    for id in 1..=3 {
        assert_eq!(payee_of(&conn, id).as_deref(), Some(staged.proposed_payee.as_str()));
    }
    assert!(pn::get_by_original(&conn, "WOOLWORTHS 1624 STRATHF")
        .unwrap()
        .is_none());
}

#[test]
fn rescan_after_apply_skips_already_normalised_rows() {
    let conn = fresh_db();
    insert_txn(&conn, 1, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF");

    // First scan + confirm + apply.
    norm_scan::scan(&conn).unwrap();
    pn::update_status(&conn, "WOOLWORTHS 1624 STRATHF", Status::Confirmed).unwrap();
    norm_apply::apply_confirmed(&conn).unwrap();

    // The transaction now carries the normalised payee. A second scan
    // sees the current payee already equals what the pipeline would
    // propose, and so skips (rule a) without re-inserting a staging row.
    let stats = norm_scan::scan(&conn).unwrap();
    assert_eq!(stats.skipped_no_change, 1);
    assert_eq!(stats.inserted, 0);
    assert!(pn::get_by_original(&conn, "WOOLWORTHS 1624 STRATHF")
        .unwrap()
        .is_none());
}

#[test]
fn rejected_row_persists_across_rescans_and_apply_leaves_payee_untouched() {
    let conn = fresh_db();
    insert_txn(&conn, 1, "COLES 0042", "COLES 0042");

    // Scan stages a pending proposal.
    norm_scan::scan(&conn).unwrap();
    pn::update_status(&conn, "COLES 0042", Status::Rejected).unwrap();

    // Apply does not touch transactions with a rejected staging row.
    let stats = norm_apply::apply_confirmed(&conn).unwrap();
    assert_eq!(stats.transactions_updated, 0);
    assert_eq!(stats.rows_drained, 0);
    assert_eq!(payee_of(&conn, 1).as_deref(), Some("COLES 0042"));

    // A second scan sees the existing row whose proposal still matches
    // what the pipeline produces → only txn_count is touched, status
    // stays Rejected (no re-prompt).
    let stats = norm_scan::scan(&conn).unwrap();
    assert_eq!(stats.txn_count_updated, 1);
    assert_eq!(stats.inserted, 0);
    assert_eq!(stats.overwritten, 0);

    let row = pn::get_by_original(&conn, "COLES 0042").unwrap().unwrap();
    assert_eq!(row.status, Status::Rejected);
    assert_eq!(payee_of(&conn, 1).as_deref(), Some("COLES 0042"));
}
