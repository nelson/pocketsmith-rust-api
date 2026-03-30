use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::transfers::{Confidence, Status, TransferPair};

#[derive(Debug)]
pub struct TransferPairRow {
    pub txn_id_a: i64,
    pub txn_id_b: i64,
    pub amount_cents: i64,
    pub confidence: Confidence,
    pub status: Status,
    pub date_a: String,
    pub date_b: String,
    pub payee_a: String,
    pub payee_b: String,
    pub account_name_a: String,
    pub account_name_b: String,
}

pub fn insert_pair(conn: &Connection, pair: &TransferPair) -> Result<()> {
    conn.execute(
        "INSERT INTO transfer_pairs (txn_id_a, txn_id_b, amount_cents, confidence, status)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            pair.txn_id_a,
            pair.txn_id_b,
            pair.amount_cents,
            pair.confidence.as_str(),
            pair.status.as_str(),
        ],
    )
    .context("Failed to insert transfer pair")?;
    Ok(())
}

fn row_to_pair_row(row: &rusqlite::Row) -> rusqlite::Result<TransferPairRow> {
    let confidence_str: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    Ok(TransferPairRow {
        txn_id_a: row.get(0)?,
        txn_id_b: row.get(1)?,
        amount_cents: row.get(2)?,
        confidence: Confidence::from_str(&confidence_str).unwrap_or(Confidence::Low),
        status: Status::from_str(&status_str).unwrap_or(Status::Pending),
        date_a: row.get(5)?,
        date_b: row.get(6)?,
        payee_a: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
        payee_b: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
        account_name_a: row.get::<_, Option<String>>(9)?.unwrap_or_default(),
        account_name_b: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
    })
}

const PAIR_ROW_QUERY: &str = "
    SELECT tp.txn_id_a, tp.txn_id_b, tp.amount_cents, tp.confidence, tp.status,
           ta.date, tb.date,
           COALESCE(ta.original_payee, ta.payee), COALESCE(tb.original_payee, tb.payee),
           aa.name, ab.name
    FROM transfer_pairs tp
    JOIN transactions ta ON ta.id = tp.txn_id_a
    JOIN transactions tb ON tb.id = tp.txn_id_b
    LEFT JOIN transaction_accounts aa ON aa.id = ta.transaction_account_id
    LEFT JOIN transaction_accounts ab ON ab.id = tb.transaction_account_id
";

pub fn get_pending_pairs(conn: &Connection, limit: usize) -> Result<Vec<TransferPairRow>> {
    let query = format!(
        "{} WHERE tp.status = 'pending' ORDER BY
            CASE tp.confidence WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END,
            tp.amount_cents DESC
         LIMIT ?1",
        PAIR_ROW_QUERY
    );
    let mut stmt = conn.prepare(&query)?;
    let rows = stmt
        .query_map([limit], |row| row_to_pair_row(row))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn get_confirmed_pairs(conn: &Connection) -> Result<Vec<TransferPairRow>> {
    let query = format!("{} WHERE tp.status = 'confirmed'", PAIR_ROW_QUERY);
    let mut stmt = conn.prepare(&query)?;
    let rows = stmt
        .query_map([], |row| row_to_pair_row(row))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::*;
    use crate::db::{upsert_transaction, upsert_transaction_account, with_transaction_change_log};

    fn setup_pair(conn: &Connection) {
        let acct1 = make_transaction_account(100, "Savings");
        let acct2 = make_transaction_account(200, "Everyday");
        upsert_transaction_account(conn, &acct1).unwrap();
        upsert_transaction_account(conn, &acct2).unwrap();

        with_transaction_change_log(conn, "test", |conn| {
            let mut t1 = make_transaction(1, "Transfer to xx8005");
            t1.amount = Some(1000.0);
            t1.date = Some("2026-03-01".into());
            t1.original_payee = Some("Transfer to xx8005".into());
            t1.transaction_account = Some(crate::models::TransactionAccount {
                id: 100,
                ..make_transaction_account(100, "Savings")
            });
            upsert_transaction(conn, &t1)?;

            let mut t2 = make_transaction(2, "Transfer from xx8820");
            t2.amount = Some(-1000.0);
            t2.date = Some("2026-03-01".into());
            t2.original_payee = Some("Transfer from xx8820".into());
            t2.transaction_account = Some(crate::models::TransactionAccount {
                id: 200,
                ..make_transaction_account(200, "Everyday")
            });
            upsert_transaction(conn, &t2)?;
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_insert_and_get_pending() {
        let conn = test_db();
        setup_pair(&conn);

        let pair = TransferPair {
            txn_id_a: 1,
            txn_id_b: 2,
            amount_cents: 100000,
            confidence: Confidence::High,
            status: Status::Pending,
        };
        insert_pair(&conn, &pair).unwrap();

        let pending = get_pending_pairs(&conn, 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].txn_id_a, 1);
        assert_eq!(pending[0].txn_id_b, 2);
        assert_eq!(pending[0].amount_cents, 100000);
        assert_eq!(pending[0].confidence, Confidence::High);
        assert_eq!(pending[0].account_name_a, "Savings");
        assert_eq!(pending[0].account_name_b, "Everyday");
    }

    #[test]
    fn test_get_confirmed_pairs() {
        let conn = test_db();
        setup_pair(&conn);

        let pair = TransferPair {
            txn_id_a: 1,
            txn_id_b: 2,
            amount_cents: 100000,
            confidence: Confidence::High,
            status: Status::Confirmed,
        };
        insert_pair(&conn, &pair).unwrap();

        let confirmed = get_confirmed_pairs(&conn).unwrap();
        assert_eq!(confirmed.len(), 1);

        let pending = get_pending_pairs(&conn, 10).unwrap();
        assert_eq!(pending.len(), 0);
    }

    #[test]
    fn test_unique_constraint_prevents_duplicate() {
        let conn = test_db();
        setup_pair(&conn);

        // Add a third transaction for the second pair attempt
        with_transaction_change_log(&conn, "test", |conn| {
            let mut t3 = make_transaction(3, "Another transfer");
            t3.amount = Some(-1000.0);
            t3.date = Some("2026-03-01".into());
            t3.transaction_account = Some(crate::models::TransactionAccount {
                id: 200,
                ..make_transaction_account(200, "Everyday")
            });
            upsert_transaction(conn, &t3)?;
            Ok(())
        })
        .unwrap();

        let pair1 = TransferPair {
            txn_id_a: 1,
            txn_id_b: 2,
            amount_cents: 100000,
            confidence: Confidence::High,
            status: Status::Pending,
        };
        insert_pair(&conn, &pair1).unwrap();

        // Try to pair txn 1 again — should fail (UNIQUE on txn_id_a)
        let pair2 = TransferPair {
            txn_id_a: 1,
            txn_id_b: 3,
            amount_cents: 100000,
            confidence: Confidence::High,
            status: Status::Pending,
        };
        assert!(insert_pair(&conn, &pair2).is_err());
    }
}
