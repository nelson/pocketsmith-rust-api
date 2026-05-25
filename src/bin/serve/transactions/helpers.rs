//! Helpers for the `/transactions/*` tab.
//!
//! The Transactions tab is, in code terms, mostly a *view* over data the
//! existing handlers already manage. The helpers in this file are pure
//! read-side: they query the database, derive per-transaction cleaning
//! state, and produce shapes the views can render. No mutation here.

use anyhow::Result;
use rusqlite::Connection;

/// One row of the Transactions queue panel. Carries everything the
/// queue list needs to render a single line plus the data the detail
/// fragment endpoint will load for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxnQueueRow {
    pub id: i64,
    /// Calendar date as stored (`YYYY-MM-DD`).
    pub date: String,
    /// Display payee â prefer `transactions.payee` if non-null, otherwise
    /// fall back to `original_payee` so the row is never blank.
    pub payee: String,
    /// Amount in cents (signed). Positive = inflow, negative = outflow.
    /// Stored as cents to keep totals exact in the UI.
    pub amount_cents: i64,
    /// Account display name; may be missing if the row's
    /// `transaction_account_id` is unknown to the join.
    pub account_name: Option<String>,
    /// Original payee string â the input to the normalise pipeline.
    /// Needed by the queue row to derive normalisation state without a
    /// second round-trip.
    pub original_payee: Option<String>,
    /// The transaction's `category_id`. None means uncategorised â
    /// drives the categorise slot in the three-emoji status stack.
    pub category_id: Option<i64>,
    /// `is_transfer` flag from the source row. Drives orphan detection
    /// (this is_transfer=1 + no `transfer_pairs` row â orphan).
    pub is_transfer: bool,
}

/// Fetch the most recent transactions (date DESC, id DESC as tiebreak)
/// joined with their account name. `limit` caps the result; pass a
/// large value to fetch everything.
///
/// This is the queue panel's primary data source. Filters and search
/// will narrow this set later â for now it returns "the last N rows"
/// unconditionally so the very first version of the queue can render.
pub fn recent_transactions(conn: &Connection, limit: i64) -> Result<Vec<TxnQueueRow>> {
    let mut stmt = conn.prepare(
        "SELECT t.id,
                t.date,
                t.payee,
                t.amount,
                ta.name,
                t.original_payee,
                t.category_id,
                COALESCE(t.is_transfer, 0)
         FROM transactions t
         LEFT JOIN transaction_accounts ta
           ON ta.id = t.transaction_account_id
         ORDER BY t.date DESC, t.id DESC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![limit], |row| {
            let amount: f64 = row.get(3)?;
            let is_transfer_int: i64 = row.get(7)?;
            let payee: Option<String> = row.get(2)?;
            let original_payee: Option<String> = row.get(5)?;
            // Display payee falls back to original_payee so the queue
            // row is never blank for freshly-synced rows that haven't
            // had a normalisation applied yet.
            let display = payee
                .clone()
                .or_else(|| original_payee.clone())
                .unwrap_or_default();
            Ok(TxnQueueRow {
                id: row.get(0)?,
                date: row.get(1)?,
                payee: display,
                amount_cents: (amount * 100.0).round() as i64,
                account_name: row.get(4)?,
                original_payee,
                category_id: row.get(6)?,
                is_transfer: is_transfer_int != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::initialize_in_memory;
    use pocketsmith_sync::db::with_operation;
    use pocketsmith_sync::test_support::seed_account;

    /// Insert a transaction row with a custom date and amount. Wraps
    /// `with_operation` so the `_transaction_changes_insert` trigger
    /// has a current operation to attribute the change to.
    fn insert_txn(
        conn: &Connection,
        id: i64,
        account_id: i64,
        date: &str,
        amount: f64,
        original_payee: &str,
        payee: Option<&str>,
        category_id: Option<i64>,
        is_transfer: bool,
    ) {
        with_operation(conn, "test-seed", |c| {
            c.execute(
                "INSERT INTO transactions
                   (id, transaction_account_id, date, amount,
                    original_payee, payee, category_id, is_transfer)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![
                    id,
                    account_id,
                    date,
                    amount,
                    original_payee,
                    payee,
                    category_id,
                    if is_transfer { 1 } else { 0 },
                ],
            )?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn recent_transactions_returns_newest_first() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Cheque").unwrap();
        // Insert in mixed order so the test can't accidentally pass on
        // insertion order.
        insert_txn(&conn, 100, 1, "2026-01-15", -10.0, "WOOLIES", Some("Woolworths"), None, false);
        insert_txn(&conn, 101, 1, "2026-03-02", -25.0, "AMAZON", Some("Amazon"), None, false);
        insert_txn(&conn, 102, 1, "2026-02-10", -5.0, "STARBUCKS", None, None, false);

        let rows = recent_transactions(&conn, 100).unwrap();
        let dates: Vec<&str> = rows.iter().map(|r| r.date.as_str()).collect();
        assert_eq!(dates, vec!["2026-03-02", "2026-02-10", "2026-01-15"]);
    }

    #[test]
    fn recent_transactions_breaks_date_ties_by_id_desc() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Cheque").unwrap();
        // Same date, different ids â highest id should come first.
        insert_txn(&conn, 100, 1, "2026-04-01", -10.0, "A", None, None, false);
        insert_txn(&conn, 200, 1, "2026-04-01", -20.0, "B", None, None, false);
        insert_txn(&conn, 150, 1, "2026-04-01", -30.0, "C", None, None, false);

        let ids: Vec<i64> = recent_transactions(&conn, 100).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![200, 150, 100]);
    }

    #[test]
    fn recent_transactions_falls_back_to_original_payee_when_payee_null() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Cheque").unwrap();
        // payee=NULL should display original_payee instead.
        insert_txn(&conn, 1, 1, "2026-01-01", -5.0, "ORIG NAME", None, None, false);

        let rows = recent_transactions(&conn, 10).unwrap();
        assert_eq!(rows[0].payee, "ORIG NAME");
        assert_eq!(rows[0].original_payee.as_deref(), Some("ORIG NAME"));
    }

    #[test]
    fn recent_transactions_amount_to_cents_is_exact() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Cheque").unwrap();
        // 10.45 in dollars must round-trip to 1045 cents (no float drift).
        insert_txn(&conn, 1, 1, "2026-01-01", -10.45, "X", None, None, false);
        insert_txn(&conn, 2, 1, "2026-01-01", 0.07, "Y", None, None, false);

        let rows = recent_transactions(&conn, 10).unwrap();
        // Newest-first by id: 2, 1.
        assert_eq!(rows[0].amount_cents, 7);
        assert_eq!(rows[1].amount_cents, -1045);
    }

    #[test]
    fn recent_transactions_includes_account_name_when_join_hits() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 5, "Smart Access").unwrap();
        insert_txn(&conn, 1, 5, "2026-01-01", -10.0, "X", None, None, false);

        let rows = recent_transactions(&conn, 10).unwrap();
        assert_eq!(rows[0].account_name.as_deref(), Some("Smart Access"));
    }

    /// `account_name` is `Option<String>` rather than `String` purely
    /// as defensive type-level documentation. In practice the schema's
    /// FK on `transactions.transaction_account_id REFERENCES
    /// transaction_accounts(id)` makes "join misses" unreachable: an
    /// orphaned txn cannot exist. We don't bother testing that branch
    /// because there's no way to construct the scenario without
    /// disabling FK enforcement, which would test SQLite, not us.

    #[test]
    fn recent_transactions_respects_limit() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Cheque").unwrap();
        for i in 0..10 {
            insert_txn(
                &conn,
                i,
                1,
                &format!("2026-01-{:02}", i + 1),
                -1.0,
                "X",
                None,
                None,
                false,
            );
        }

        let rows = recent_transactions(&conn, 3).unwrap();
        assert_eq!(rows.len(), 3);
    }

    /// Insert a categories row so transactions can reference it via
    /// the `category_id` foreign key.
    fn seed_category(conn: &Connection, id: i64, title: &str) {
        conn.execute(
            "INSERT INTO categories (id, title) VALUES (?1, ?2)",
            rusqlite::params![id, title],
        )
        .unwrap();
    }

    #[test]
    fn recent_transactions_propagates_category_id_and_is_transfer() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Cheque").unwrap();
        seed_category(&conn, 42, "_Bills");
        // category_id set + is_transfer=true â both should round-trip.
        insert_txn(
            &conn,
            1,
            1,
            "2026-01-01",
            -10.0,
            "X",
            None,
            Some(42),
            true,
        );
        insert_txn(&conn, 2, 1, "2026-01-02", -10.0, "Y", None, None, false);

        let rows = recent_transactions(&conn, 10).unwrap();
        // Newest first: id=2 (no category, not transfer), then id=1.
        assert_eq!(rows[0].category_id, None);
        assert!(!rows[0].is_transfer);
        assert_eq!(rows[1].category_id, Some(42));
        assert!(rows[1].is_transfer);
    }
}
