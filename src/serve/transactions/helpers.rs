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
    /// Display payee — prefer `transactions.payee` if non-null, otherwise
    /// fall back to `original_payee` so the row is never blank.
    pub payee: String,
    /// Amount in cents (signed). Positive = inflow, negative = outflow.
    /// Stored as cents to keep totals exact in the UI.
    pub amount_cents: i64,
    /// Account display name; may be missing if the row's
    /// `transaction_account_id` is unknown to the join.
    pub account_name: Option<String>,
    /// Original payee string — the input to the normalise pipeline.
    /// Needed by the queue row to derive normalisation state without a
    /// second round-trip.
    pub original_payee: Option<String>,
    /// The transaction's `category_id`. None means uncategorised —
    /// drives the categorise slot in the three-emoji status stack.
    pub category_id: Option<i64>,
    /// The category's `title`, joined from `categories`. Populated when
    /// `category_id` is Some and the join hits; the queue's category
    /// tag renders this string (e.g. "Eating Out").
    pub category_title: Option<String>,
    /// `is_transfer` flag from the source row. Drives orphan detection
    /// (this is_transfer=1 + no `transfer_pairs` row ⇒ orphan).
    pub is_transfer: bool,
    /// Status code from `transfer_pairs` (0=pending, 1=confirmed,
    /// 2=rejected) if this txn is part of any pair, else `None`.
    /// Pre-fetched here via a LEFT JOIN so the queue render does not
    /// have to do per-row SQL lookups (eliminates the N+1 that
    /// dominated render time on a 22k-row DB).
    pub pair_status: Option<i32>,
    /// Status code from `payee_normalisations` for this txn's
    /// original_payee, or `None` if no row exists. Same N+1
    /// motivation as `pair_status`.
    pub norm_status: Option<i32>,
}

/// Fetch the most recent transactions (date DESC, id DESC as tiebreak)
/// joined with their account name. `limit` caps the result; pass a
/// large value to fetch everything.
///
/// Test-only now: production code paths use `filtered_transactions`
/// (with a TxnFilter::All argument when no filter is needed) so the
/// queue panel and detail-fragment paths share a single SQL query
/// shape. Kept here so the per-row tests for join/order/limit/cents
/// keep working without rewrites.
#[cfg(test)]
pub fn recent_transactions(conn: &Connection, limit: i64) -> Result<Vec<TxnQueueRow>> {
    let mut stmt = conn.prepare(
        "SELECT t.id,
                t.date,
                t.payee,
                t.amount,
                ta.name,
                t.original_payee,
                t.category_id,
                c.title,
                COALESCE(t.is_transfer, 0)
         FROM transactions t
         LEFT JOIN transaction_accounts ta
           ON ta.id = t.transaction_account_id
         LEFT JOIN categories c
           ON c.id = t.category_id
         ORDER BY t.date DESC, t.id DESC
         LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![limit], |row| {
            let amount: f64 = row.get(3)?;
            let is_transfer_int: i64 = row.get(8)?;
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
                category_title: row.get(7)?,
                is_transfer: is_transfer_int != 0,
                pair_status: None,
                norm_status: None,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Fetch one transaction by id, materialising the same row shape as
/// `recent_transactions`. Returns `None` if no such row exists.
///
/// Splitting this out matters for performance: the previous detail-
/// panel code path called `recent_transactions(100_000)` and then
/// linear-scanned the result for a single id, which is ~50ms per
/// detail GET on a 22k-row DB. A direct lookup by primary key is
/// ~1ms.
pub fn fetch_by_id(conn: &Connection, txn_id: i64) -> Result<Option<TxnQueueRow>> {
    let mut stmt = conn.prepare(
        "SELECT t.id,
                t.date,
                t.payee,
                t.amount,
                ta.name,
                t.original_payee,
                t.category_id,
                c.title,
                COALESCE(t.is_transfer, 0)
         FROM transactions t
         LEFT JOIN transaction_accounts ta
           ON ta.id = t.transaction_account_id
         LEFT JOIN categories c
           ON c.id = t.category_id
         WHERE t.id = ?1",
    )?;
    let row = stmt
        .query_row(rusqlite::params![txn_id], |row| {
            let amount: f64 = row.get(3)?;
            let is_transfer_int: i64 = row.get(8)?;
            let payee: Option<String> = row.get(2)?;
            let original_payee: Option<String> = row.get(5)?;
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
                category_title: row.get(7)?,
                is_transfer: is_transfer_int != 0,
                pair_status: None,
                norm_status: None,
            })
        })
        .ok();
    Ok(row)
}

/// One of the queue's filter chips. Drives the WHERE clause on top of
/// the base query in `recent_transactions`. The string forms travel in
/// the URL (`?filter=needs-rule`) and on the wire to HTMX swaps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnFilter {
    All,
    /// Rows whose `original_payee` has no row in `payee_normalisations`
    /// at all. The pipeline has nothing to say about this payee — you
    /// either teach it a rule, or accept the noise.
    NeedsRule,
    /// Rows whose `original_payee` has a `payee_normalisations` row
    /// with status = pending. The pipeline produced a proposal that
    /// hasn't been reviewed yet.
    RulePending,
    /// Rows with `is_transfer = 1` and no row in `transfer_pairs`
    /// (either side). "Looks like a transfer; no counterpart found."
    OrphanTransfer,
    /// Rows with `category_id IS NULL`.
    Uncategorised,
}

impl TxnFilter {
    pub fn parse(s: &str) -> Self {
        match s {
            "needs-rule" => Self::NeedsRule,
            "rule-pending" => Self::RulePending,
            "orphan-transfer" => Self::OrphanTransfer,
            "uncategorised" => Self::Uncategorised,
            _ => Self::All,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::NeedsRule => "needs-rule",
            Self::RulePending => "rule-pending",
            Self::OrphanTransfer => "orphan-transfer",
            Self::Uncategorised => "uncategorised",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::NeedsRule => "Needs rule",
            Self::RulePending => "Rule pending",
            Self::OrphanTransfer => "Orphan transfer",
            Self::Uncategorised => "Uncategorised",
        }
    }

    pub const ALL: [TxnFilter; 5] = [
        Self::All,
        Self::NeedsRule,
        Self::RulePending,
        Self::OrphanTransfer,
        Self::Uncategorised,
    ];
}

/// Filtered version of [`recent_transactions`]. Same join + ordering;
/// the `filter` adds a tab-specific WHERE clause that narrows the row
/// set without touching the base query.
///
/// Performance note: this query also LEFT JOINs `transfer_pairs`
/// (twice, once per side) and `payee_normalisations` to pre-fetch
/// `pair_status` and `norm_status` for each row. Folding the per-
/// pillar lookups into the main query eliminates the N+1 the per-row
/// state-derivation loop in `render_page_shell` would otherwise pay
/// (~2000 prepared queries on a 1000-row queue, 10-20ms total). The
/// unique indexes on `transfer_pairs.txn_id_a` / `txn_id_b` and the
/// PK index on `payee_normalisations.original_payee` make the joins
/// constant-time per row.
pub fn filtered_transactions(
    conn: &Connection,
    filter: TxnFilter,
    limit: i64,
) -> Result<Vec<TxnQueueRow>> {
    // The where_clause is interpolated literally into the SQL: it does
    // not contain any user input, only the canonical clause for each
    // filter variant. Filters reuse the joined `pn`, `tpa`, `tpb`
    // aliases instead of correlated subqueries -- one less query plan
    // for the optimiser to think about.
    let where_clause: &str = match filter {
        TxnFilter::All => "1 = 1",
        TxnFilter::NeedsRule => {
            // No payee_normalisations row exists for this original_payee.
            "t.original_payee IS NOT NULL AND pn.original_payee IS NULL"
        }
        TxnFilter::RulePending => {
            // payee_normalisations row exists with status = 0 (pending).
            "pn.status = 0"
        }
        TxnFilter::OrphanTransfer => {
            // is_transfer = 1 and no row in transfer_pairs (either side).
            "COALESCE(t.is_transfer, 0) = 1
             AND tpa.txn_id_a IS NULL
             AND tpb.txn_id_b IS NULL"
        }
        TxnFilter::Uncategorised => "t.category_id IS NULL",
    };
    let sql = format!(
        "SELECT t.id,
                t.date,
                t.payee,
                t.amount,
                ta.name,
                t.original_payee,
                t.category_id,
                c.title,
                COALESCE(t.is_transfer, 0),
                COALESCE(tpa.status, tpb.status) AS pair_status,
                pn.status AS norm_status
         FROM transactions t
         LEFT JOIN transaction_accounts ta
           ON ta.id = t.transaction_account_id
         LEFT JOIN categories c
           ON c.id = t.category_id
         LEFT JOIN transfer_pairs tpa
           ON tpa.txn_id_a = t.id
         LEFT JOIN transfer_pairs tpb
           ON tpb.txn_id_b = t.id
         LEFT JOIN payee_normalisations pn
           ON pn.original_payee = t.original_payee
         WHERE {where_clause}
         ORDER BY t.date DESC, t.id DESC
         LIMIT ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(rusqlite::params![limit], |row| {
            let amount: f64 = row.get(3)?;
            let is_transfer_int: i64 = row.get(8)?;
            let payee: Option<String> = row.get(2)?;
            let original_payee: Option<String> = row.get(5)?;
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
                category_title: row.get(7)?,
                is_transfer: is_transfer_int != 0,
                pair_status: row.get(9)?,
                norm_status: row.get(10)?,
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
        // Same date, different ids — highest id should come first.
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
        // category_id set + is_transfer=true — both should round-trip.
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
        assert_eq!(rows[0].category_title, None);
        assert!(!rows[0].is_transfer);
        assert_eq!(rows[1].category_id, Some(42));
        assert_eq!(rows[1].category_title.as_deref(), Some("_Bills"));
        assert!(rows[1].is_transfer);
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use pocketsmith_sync::db::{initialize_in_memory, transfer_pairs, with_operation};
    use pocketsmith_sync::review::Status;
    use pocketsmith_sync::test_support::{seed_account, seed_pn};
    use pocketsmith_sync::transfers::{Confidence, TransferPair};

    fn insert_txn(
        conn: &Connection,
        id: i64,
        account_id: i64,
        date: &str,
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
                 VALUES (?1, ?2, ?3, -1.0, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    id,
                    account_id,
                    date,
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

    fn seed_category(conn: &Connection, id: i64, title: &str) {
        conn.execute(
            "INSERT INTO categories (id, title) VALUES (?1, ?2)",
            rusqlite::params![id, title],
        )
        .unwrap();
    }

    fn fixture() -> Connection {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Cheque").unwrap();
        seed_account(&conn, 2, "Savings").unwrap();
        seed_category(&conn, 100, "_Bills");

        // 1: needs rule (no pn row, no category, not a transfer)
        insert_txn(&conn, 1, 1, "2026-01-01", "WILD UNKNOWN", None, None, false);

        // 2: rule pending (pn row pending, has category)
        insert_txn(&conn, 2, 1, "2026-01-02", "AMAZON", Some("Amazon"), Some(100), false);
        seed_pn(&conn, "AMAZON", "Amazon", Status::Pending, 1).unwrap();

        // 3: rule confirmed, categorised, not orphan
        insert_txn(&conn, 3, 1, "2026-01-03", "WOOLIES", Some("Woolworths"), Some(100), false);
        seed_pn(&conn, "WOOLIES", "Woolworths", Status::Confirmed, 1).unwrap();

        // 4: orphan transfer (is_transfer=1, no pair, no rule, uncategorised)
        insert_txn(&conn, 4, 1, "2026-01-04", "TRF TO X", None, None, true);

        // 5: paired transfer (is_transfer=1, pair confirmed, has category)
        insert_txn(&conn, 5, 1, "2026-01-05", "TRF FROM Y", Some("From Savings"), Some(100), true);
        insert_txn(&conn, 6, 2, "2026-01-05", "TRF TO Y", Some("To Cheque"), Some(100), true);
        with_operation(&conn, "test-seed", |c| {
            transfer_pairs::insert_pair(
                c,
                &TransferPair {
                    txn_id_a: 5,
                    txn_id_b: 6,
                    amount_cents: 100,
                    confidence: Confidence::High,
                    status: Status::Confirmed,
                },
            )
        })
        .unwrap();

        // 7: uncategorised but has rule and not a transfer
        insert_txn(&conn, 7, 1, "2026-01-07", "STARBUCKS", Some("Starbucks"), None, false);
        seed_pn(&conn, "STARBUCKS", "Starbucks", Status::Confirmed, 1).unwrap();

        conn
    }

    fn ids(conn: &Connection, filter: TxnFilter) -> Vec<i64> {
        let mut ids: Vec<i64> = filtered_transactions(conn, filter, 1000)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn filter_all_returns_every_row() {
        let conn = fixture();
        // 7 inserted (ids 1..=7).
        assert_eq!(ids(&conn, TxnFilter::All), vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn filter_needs_rule_excludes_rows_with_any_pn_row() {
        let conn = fixture();
        // Rows lacking pn rows: id 1 (WILD UNKNOWN), id 4 (TRF TO X),
        // id 5 (TRF FROM Y), id 6 (TRF TO Y). Note that 5 and 6 are
        // paired but pairing and normalisation are independent --
        // having a confirmed pair does not exempt a row from needing
        // a normalisation rule.
        assert_eq!(ids(&conn, TxnFilter::NeedsRule), vec![1, 4, 5, 6]);
    }

    #[test]
    fn filter_rule_pending_picks_only_pending_pn_rows() {
        let conn = fixture();
        // id 2 (AMAZON) is the only pn=pending row.
        assert_eq!(ids(&conn, TxnFilter::RulePending), vec![2]);
    }

    #[test]
    fn filter_orphan_transfer_excludes_paired_transfers() {
        let conn = fixture();
        // id 4 is is_transfer=1 with no pair row.
        // ids 5, 6 are is_transfer=1 but paired.
        assert_eq!(ids(&conn, TxnFilter::OrphanTransfer), vec![4]);
    }

    #[test]
    fn filter_uncategorised_picks_only_null_category_id() {
        let conn = fixture();
        // ids 1, 4, 7 have category_id IS NULL.
        assert_eq!(ids(&conn, TxnFilter::Uncategorised), vec![1, 4, 7]);
    }

    #[test]
    fn filter_parse_round_trips_through_as_str() {
        for f in TxnFilter::ALL {
            assert_eq!(TxnFilter::parse(f.as_str()), f);
        }
        // Unknown strings fall back to All (URL-tampering robustness).
        assert_eq!(TxnFilter::parse("not-a-filter"), TxnFilter::All);
    }
}
