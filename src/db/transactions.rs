use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::*;

use super::categories::upsert_category;
use super::transaction_accounts::upsert_transaction_account;

pub fn upsert_transaction(conn: &Connection, t: &Transaction) -> Result<()> {
    // Upsert embedded category first
    if let Some(ref cat) = t.category {
        upsert_category(conn, cat)?;
    }

    // Upsert embedded transaction account
    if let Some(ref ta) = t.transaction_account {
        upsert_transaction_account(conn, ta, None)?;
    }

    let labels_json = t
        .labels
        .as_ref()
        .map(|l| serde_json::to_string(l).unwrap_or_default());

    conn.execute(
        "INSERT OR REPLACE INTO transactions (
            id, transaction_type, payee, amount, amount_in_base_currency,
            date, cheque_number, memo, is_transfer, category_id,
            note, labels, original_payee, upload_source,
            closing_balance, transaction_account_id, status,
            needs_review, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20
        )",
        params![
            t.id,
            t.transaction_type,
            t.payee,
            t.amount,
            t.amount_in_base_currency,
            t.date,
            t.cheque_number,
            t.memo,
            t.is_transfer,
            t.category.as_ref().map(|c| c.id),
            t.note,
            labels_json,
            t.original_payee,
            t.upload_source,
            t.closing_balance,
            t.transaction_account.as_ref().map(|ta| ta.id),
            t.status,
            t.needs_review,
            t.created_at,
            t.updated_at,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::*;

    #[test]
    fn test_upsert_transaction_simple() {
        let conn = test_db();
        let txn = make_transaction(1, "Supermarket");
        upsert_transaction(&conn, &txn).unwrap();

        let payee: String = conn
            .query_row("SELECT payee FROM transactions WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(payee, "Supermarket");
    }

    #[test]
    fn test_upsert_transaction_with_category() {
        let conn = test_db();
        let mut txn = make_transaction(1, "Supermarket");
        txn.category = Some(make_category(10, "Food"));
        upsert_transaction(&conn, &txn).unwrap();

        let cat_title: String = conn
            .query_row("SELECT title FROM categories WHERE id = 10", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(cat_title, "Food");

        let cat_id: i64 = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cat_id, 10);
    }

    #[test]
    fn test_upsert_transaction_with_transaction_account() {
        let conn = test_db();
        let mut txn = make_transaction(1, "Supermarket");
        txn.transaction_account = Some(make_transaction_account(300, "Daily"));
        upsert_transaction(&conn, &txn).unwrap();

        let ta_name: String = conn
            .query_row(
                "SELECT name FROM transaction_accounts WHERE id = 300",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ta_name, "Daily");

        let ta_id: i64 = conn
            .query_row(
                "SELECT transaction_account_id FROM transactions WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ta_id, 300);
    }

    #[test]
    fn test_upsert_transaction_with_labels() {
        let conn = test_db();
        let mut txn = make_transaction(1, "Store");
        txn.labels = Some(vec!["food".into(), "weekly".into()]);
        upsert_transaction(&conn, &txn).unwrap();

        let labels: String = conn
            .query_row("SELECT labels FROM transactions WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        let parsed: Vec<String> = serde_json::from_str(&labels).unwrap();
        assert_eq!(parsed, vec!["food", "weekly"]);
    }

    #[test]
    fn test_upsert_transaction_replace() {
        let conn = test_db();
        upsert_transaction(&conn, &make_transaction(1, "Store A")).unwrap();
        upsert_transaction(&conn, &make_transaction(1, "Store B")).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let payee: String = conn
            .query_row("SELECT payee FROM transactions WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(payee, "Store B");
    }

    #[test]
    fn test_upsert_transaction_full_with_nested() {
        let conn = test_db();
        let mut ta = make_transaction_account(300, "Daily");
        ta.institution = Some(make_institution(50, "ANZ"));

        let mut txn = make_transaction(1, "Supermarket");
        txn.category = Some(make_category(10, "Food"));
        txn.transaction_account = Some(ta);
        txn.labels = Some(vec!["groceries".into()]);
        txn.note = Some("Weekly shop".into());

        upsert_transaction(&conn, &txn).unwrap();

        let inst_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM institutions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(inst_count, 1);

        let cat_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))
            .unwrap();
        assert_eq!(cat_count, 1);

        let ta_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transaction_accounts",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ta_count, 1);

        let txn_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(txn_count, 1);
    }

    #[test]
    fn test_batch_upsert_transactions() {
        let conn = test_db();
        let txns: Vec<Transaction> = (1..=100)
            .map(|i| make_transaction(i, &format!("Store {}", i)))
            .collect();

        for txn in &txns {
            upsert_transaction(&conn, txn).unwrap();
        }

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 100);
    }
}
