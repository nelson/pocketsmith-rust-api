//! Read-only DB queries that power the Dashboard tab. Pure functions
//! over [`rusqlite::Connection`]; views consume the returned structs
//! without touching SQL directly.
//!
//! All money is in `amount_in_base_currency` (currently AUD on the
//! production DB). Transfers (`is_transfer = 1`) are excluded from
//! every aggregate so internal moves don't inflate the totals.

use anyhow::Result;
use rusqlite::Connection;

/// One row on the months queue.
#[derive(Debug, Clone)]
#[allow(dead_code)] // txn_count surfaces in the detail header today; promoted to the row in a follow-up
pub struct MonthRow {
    /// `YYYY-MM`.
    pub month: String,
    /// Sum of positive base-currency amounts in the month, excluding
    /// transfers. Dollars (the schema stores REAL dollars, not cents).
    pub total_in: f64,
    /// Sum of negative base-currency amounts in the month (also in
    /// dollars, but expressed as a *positive* number so the renderer
    /// can label it as "out $X"). Excludes transfers.
    pub total_out: f64,
    /// `total_in - total_out`. Positive = surplus, negative = deficit.
    pub net: f64,
    /// Number of non-transfer transactions in the month.
    pub txn_count: i64,
    /// Fraction of non-transfer txns with a category set, 0.0\u20131.0.
    /// Used by the queue's hygiene meter.
    pub frac_categorised: f64,
    /// Fraction of non-transfer txns whose `original_payee` has a
    /// confirmed `payee_normalisations` row, 0.0\u20131.0.
    pub frac_normalised: f64,
}

/// One source/sink row in the per-month category breakdown table and
/// the Sankey. `signed_total` is in dollars and keeps the original
/// sign so the table can show inflow vs outflow.
#[derive(Debug, Clone)]
#[allow(dead_code)] // category_id retained so a future click-through can deep-link
pub struct CategoryBreakdownRow {
    pub category_id: Option<i64>,
    pub category_title: String,
    pub signed_total: f64,
    pub txn_count: i64,
}

/// Pull one row per month-with-data, newest first. A single composite
/// query is used so the queue render is one round trip.
pub fn monthly_summary(conn: &Connection) -> Result<Vec<MonthRow>> {
    let sql = "
        WITH per_txn AS (
            SELECT
                substr(t.date, 1, 7) AS month,
                t.id,
                t.amount_in_base_currency AS amt,
                t.category_id,
                t.original_payee
            FROM transactions t
            WHERE COALESCE(t.is_transfer, 0) = 0
              AND t.date IS NOT NULL
              AND t.amount_in_base_currency IS NOT NULL
        )
        SELECT
            p.month,
            SUM(CASE WHEN p.amt > 0 THEN p.amt ELSE 0 END) AS total_in,
            SUM(CASE WHEN p.amt < 0 THEN -p.amt ELSE 0 END) AS total_out,
            COUNT(*) AS txn_count,
            SUM(CASE WHEN p.category_id IS NOT NULL THEN 1 ELSE 0 END) AS n_cat,
            SUM(CASE WHEN EXISTS (
                  SELECT 1 FROM payee_normalisations pn
                  WHERE pn.original_payee = p.original_payee
                    AND pn.status = 'confirmed'
                ) THEN 1 ELSE 0 END) AS n_norm
        FROM per_txn p
        GROUP BY p.month
        ORDER BY p.month DESC
    ";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([], |r| {
            let month: String = r.get(0)?;
            let total_in: f64 = r.get(1)?;
            let total_out: f64 = r.get(2)?;
            let txn_count: i64 = r.get(3)?;
            let n_cat: i64 = r.get(4)?;
            let n_norm: i64 = r.get(5)?;
            let denom = txn_count.max(1) as f64;
            Ok(MonthRow {
                month,
                total_in,
                total_out,
                net: total_in - total_out,
                txn_count,
                frac_categorised: n_cat as f64 / denom,
                frac_normalised: n_norm as f64 / denom,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Pull the per-category breakdown for a single month, split by
/// sign so an uncategorised bucket with both inflows and outflows
/// surfaces twice (once positive, once negative) rather than
/// collapsing into a net that hides the inflow side. Sorted by
/// absolute value descending. Inflow rows have `signed_total > 0`;
/// outflow rows < 0. Uncategorised rows surface under the synthetic
/// title `Uncategorised`. Transfers excluded.
pub fn month_category_breakdown(conn: &Connection, ym: &str) -> Result<Vec<CategoryBreakdownRow>> {
    let sql = "
        SELECT
            t.category_id,
            COALESCE(c.title, 'Uncategorised') AS title,
            CASE WHEN t.amount_in_base_currency >= 0 THEN 1 ELSE -1 END AS sign,
            SUM(t.amount_in_base_currency) AS total,
            COUNT(*) AS n
        FROM transactions t
        LEFT JOIN categories c ON c.id = t.category_id
        WHERE COALESCE(t.is_transfer, 0) = 0
          AND substr(t.date, 1, 7) = ?1
          AND t.amount_in_base_currency IS NOT NULL
        GROUP BY
            t.category_id,
            COALESCE(c.title, 'Uncategorised'),
            CASE WHEN t.amount_in_base_currency >= 0 THEN 1 ELSE -1 END
        ORDER BY ABS(SUM(t.amount_in_base_currency)) DESC
    ";
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([ym], |r| {
            Ok(CategoryBreakdownRow {
                category_id: r.get(0)?,
                category_title: r.get(1)?,
                signed_total: r.get(3)?,
                txn_count: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Choose the month to show: explicit user selection if it's still
/// present in the data, else the most recent month with data, else
/// `None` (empty DB).
pub fn pick_active_month(stash: &Option<String>, months: &[MonthRow]) -> Option<String> {
    if let Some(s) = stash {
        if months.iter().any(|m| &m.month == s) {
            return Some(s.clone());
        }
    }
    months.first().map(|m| m.month.clone())
}

/// Map a hygiene fraction (0.0–1.0) to the CSS class for its dot.
pub fn hyg_class(frac: f64) -> &'static str {
    if frac >= 0.9 {
        "hyg-on"
    } else if frac >= 0.5 {
        "hyg-warn"
    } else {
        "hyg-bad"
    }
}

/// Format a `YYYY-MM` string as `Month YYYY` (e.g. "April 2026").
/// Anything that doesn't parse passes through unchanged.
pub fn pretty_month(ym: &str) -> String {
    let mut parts = ym.split('-');
    let (Some(y), Some(m)) = (parts.next(), parts.next()) else { return ym.to_string() };
    let name = match m {
        "01" => "January", "02" => "February", "03" => "March", "04" => "April",
        "05" => "May", "06" => "June", "07" => "July", "08" => "August",
        "09" => "September", "10" => "October", "11" => "November", "12" => "December",
        _ => return ym.to_string(),
    };
    format!("{name} {y}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db;
    use rusqlite::params;

    #[test]
    fn pretty_month_handles_known_months() {
        assert_eq!(pretty_month("2026-04"), "April 2026");
        assert_eq!(pretty_month("2025-12"), "December 2025");
        // Unknown shape passes through.
        assert_eq!(pretty_month("garbage"), "garbage");
    }

    #[test]
    fn pick_active_month_falls_back_to_most_recent() {
        let months = vec![
            MonthRow { month: "2026-04".into(), total_in: 0.0, total_out: 0.0, net: 0.0,
                txn_count: 0, frac_categorised: 1.0, frac_normalised: 1.0 },
            MonthRow { month: "2026-03".into(), total_in: 0.0, total_out: 0.0, net: 0.0,
                txn_count: 0, frac_categorised: 1.0, frac_normalised: 1.0 },
        ];
        assert_eq!(pick_active_month(&None, &months), Some("2026-04".to_string()));
        assert_eq!(pick_active_month(&Some("2026-03".into()), &months), Some("2026-03".into()));
        // Stale stash falls back to newest.
        assert_eq!(pick_active_month(&Some("1999-01".into()), &months), Some("2026-04".into()));
        // Empty data => None.
        assert_eq!(pick_active_month(&None, &[]), None);
    }

    fn fresh_conn() -> rusqlite::Connection {
        db::initialize_in_memory().unwrap()
    }

    fn ins_account(conn: &rusqlite::Connection, id: i64, name: &str) {
        conn.execute(
            "INSERT INTO transaction_accounts (id, name) VALUES (?1, ?2)",
            params![id, name],
        )
        .unwrap();
    }
    fn ins_category(conn: &rusqlite::Connection, id: i64, title: &str) {
        conn.execute(
            "INSERT INTO categories (id, title) VALUES (?1, ?2)",
            params![id, title],
        )
        .unwrap();
    }
    fn ins_txn(
        conn: &rusqlite::Connection,
        id: i64,
        date: &str,
        amount: f64,
        is_transfer: bool,
        category_id: Option<i64>,
        original_payee: &str,
    ) {
        db::with_operation(conn, "test-seed", |c| {
            c.execute(
                "INSERT INTO transactions \
                 (id, date, amount, amount_in_base_currency, is_transfer, category_id, \
                  original_payee, payee, transaction_account_id) \
                 VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6, ?6, 1)",
                params![id, date, amount, is_transfer as i64, category_id, original_payee],
            )?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn monthly_summary_groups_by_yyyymm_excluding_transfers() {
        let conn = fresh_conn();
        ins_account(&conn, 1, "Cheque");
        ins_txn(&conn, 100, "2026-04-15", -50.00, false, None, "WOOLIES");
        ins_txn(&conn, 101, "2026-04-30", 5000.00, false, None, "SALARY");
        ins_txn(&conn, 102, "2026-04-30", -100.00, true, None, "XFER");
        ins_txn(&conn, 200, "2026-03-10", -25.00, false, None, "COFFEE");

        let rows = monthly_summary(&conn).unwrap();
        let april = rows.iter().find(|r| r.month == "2026-04").expect("april");
        assert_eq!(april.txn_count, 2, "transfer must be excluded from txn_count");
        assert!((april.total_in - 5000.0).abs() < 0.01, "april in = {}", april.total_in);
        assert!((april.total_out - 50.0).abs() < 0.01, "april out = {}", april.total_out);
        assert!((april.net - 4950.0).abs() < 0.01, "april net = {}", april.net);
        assert_eq!(rows[0].month, "2026-04");
        assert_eq!(rows[1].month, "2026-03");
    }

    #[test]
    fn month_category_breakdown_sorts_by_absolute_amount_desc() {
        let conn = fresh_conn();
        ins_account(&conn, 1, "Cheque");
        ins_category(&conn, 10, "Eating Out");
        ins_category(&conn, 11, "Salary");
        ins_txn(&conn, 1, "2026-04-01",  5000.0, false, Some(11), "BOSS");
        ins_txn(&conn, 2, "2026-04-02",  -120.0, false, Some(10), "CAFE");
        ins_txn(&conn, 3, "2026-04-03",   -30.0, false, None,     "MYSTERY");

        let rows = month_category_breakdown(&conn, "2026-04").unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].category_title, "Salary", "largest abs first");
        assert_eq!(rows[1].category_title, "Eating Out");
        assert_eq!(rows[2].category_title, "Uncategorised");
        assert!((rows[0].signed_total - 5000.0).abs() < 0.01);
        assert!((rows[1].signed_total - -120.0).abs() < 0.01);
    }
}
