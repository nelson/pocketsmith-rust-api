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
        upsert_transaction_account(conn, ta)?;
    }

    let labels_json = t
        .labels
        .as_ref()
        .map(|l| serde_json::to_string(l).unwrap_or_default());

    conn.execute(
        "INSERT INTO transactions (
            id, transaction_type, payee, amount, amount_in_base_currency,
            date, cheque_number, memo, is_transfer, category_id,
            note, labels, original_payee, upload_source,
            closing_balance, transaction_account_id, status,
            needs_review, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20
        )
        ON CONFLICT(id) DO UPDATE SET
            transaction_type = excluded.transaction_type,
            payee = excluded.payee,
            amount = excluded.amount,
            amount_in_base_currency = excluded.amount_in_base_currency,
            date = excluded.date,
            cheque_number = excluded.cheque_number,
            memo = excluded.memo,
            is_transfer = excluded.is_transfer,
            category_id = excluded.category_id,
            note = excluded.note,
            labels = excluded.labels,
            original_payee = excluded.original_payee,
            upload_source = excluded.upload_source,
            closing_balance = excluded.closing_balance,
            transaction_account_id = excluded.transaction_account_id,
            status = excluded.status,
            needs_review = excluded.needs_review,
            created_at = excluded.created_at,
            updated_at = excluded.updated_at",
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
    use crate::db::with_change_context;

    #[test]
    fn test_upsert_transaction_simple() {
        let conn = test_db();
        with_change_context(&conn, "test", None, |conn| {
            upsert_transaction(conn, &make_transaction(1, "Supermarket"))
        })
        .unwrap();

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
        with_change_context(&conn, "test", None, |conn| upsert_transaction(conn, &txn)).unwrap();

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
        with_change_context(&conn, "test", None, |conn| upsert_transaction(conn, &txn)).unwrap();

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
        with_change_context(&conn, "test", None, |conn| upsert_transaction(conn, &txn)).unwrap();

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
        with_change_context(&conn, "test", None, |conn| {
            upsert_transaction(conn, &make_transaction(1, "Store A"))?;
            upsert_transaction(conn, &make_transaction(1, "Store B"))
        })
        .unwrap();

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
        let ta = make_transaction_account(300, "Daily");

        let mut txn = make_transaction(1, "Supermarket");
        txn.category = Some(make_category(10, "Food"));
        txn.transaction_account = Some(ta);
        txn.labels = Some(vec!["groceries".into()]);
        txn.note = Some("Weekly shop".into());

        with_change_context(&conn, "test", None, |conn| upsert_transaction(conn, &txn)).unwrap();

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
        with_change_context(&conn, "test", None, |conn| {
            let txns: Vec<Transaction> = (1..=100)
                .map(|i| make_transaction(i, &format!("Store {}", i)))
                .collect();
            for txn in &txns {
                upsert_transaction(conn, txn)?;
            }
            Ok(())
        })
        .unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 100);
    }

    // --- History tracking tests ---

    #[test]
    fn test_history_initial_insert() {
        let conn = test_db();
        with_change_context(&conn, "test", None, |conn| {
            upsert_transaction(conn, &make_transaction(1, "Store A"))
        })
        .unwrap();

        let (version, mask, payee, reason): (i64, i64, String, String) = conn
            .query_row(
                "SELECT _version, _mask, payee, reason FROM _transactions_history WHERE _rowid = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(version, 1);
        assert_eq!(mask, 63);
        assert_eq!(payee, "Store A");
        assert_eq!(reason, "test");
    }

    #[test]
    fn test_history_no_change_no_new_row() {
        let conn = test_db();
        let txn = make_transaction(1, "Store A");
        with_change_context(&conn, "test", None, |conn| {
            upsert_transaction(conn, &txn)?;
            upsert_transaction(conn, &txn)
        })
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM _transactions_history WHERE _rowid = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_history_tracked_field_change() {
        let conn = test_db();
        with_change_context(&conn, "test", None, |conn| {
            upsert_transaction(conn, &make_transaction(1, "Store A"))?;
            upsert_transaction(conn, &make_transaction(1, "Store B"))
        })
        .unwrap();

        let (version, mask, payee): (i64, i64, String) = conn
            .query_row(
                "SELECT _version, _mask, payee FROM _transactions_history WHERE _rowid = 1 AND _version = 2",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(version, 2);
        assert_eq!(mask, 1); // only payee bit
        assert_eq!(payee, "Store B");
    }

    #[test]
    fn test_history_only_changed_fields_populated() {
        let conn = test_db();
        with_change_context(&conn, "test", None, |conn| {
            upsert_transaction(conn, &make_transaction(1, "Store A"))?;
            let mut txn = make_transaction(1, "Store B");
            txn.note = Some("a note".into());
            upsert_transaction(conn, &txn)
        })
        .unwrap();

        let memo_is_null: bool = conn
            .query_row(
                "SELECT memo IS NULL FROM _transactions_history WHERE _rowid = 1 AND _version = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(memo_is_null); // memo didn't change, should be NULL
    }

    #[test]
    fn test_history_multiple_field_changes_mask() {
        let conn = test_db();
        with_change_context(&conn, "test", None, |conn| {
            upsert_transaction(conn, &make_transaction(1, "Store A"))?;
            let mut txn = make_transaction(1, "Store B"); // payee bit 0 = 1
            txn.memo = Some("new memo".into()); // memo bit 5 = 32
            upsert_transaction(conn, &txn)
        })
        .unwrap();

        let mask: i64 = conn
            .query_row(
                "SELECT _mask FROM _transactions_history WHERE _rowid = 1 AND _version = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mask, 1 | 32); // 33
    }

    #[test]
    fn test_history_reason_recorded() {
        let conn = test_db();
        with_change_context(&conn, "pocketsmith", None, |conn| {
            upsert_transaction(conn, &make_transaction(1, "Store A"))
        })
        .unwrap();

        let reason: String = conn
            .query_row(
                "SELECT reason FROM _transactions_history WHERE _rowid = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(reason, "pocketsmith");
    }

    #[test]
    fn test_history_sync_version_recorded() {
        let conn = test_db();
        with_change_context(&conn, "pocketsmith", Some(3), |conn| {
            upsert_transaction(conn, &make_transaction(1, "Store A"))
        })
        .unwrap();

        let sync_version: i64 = conn
            .query_row(
                "SELECT _sync_version FROM _transactions_history WHERE _rowid = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(sync_version, 3);
    }

    #[test]
    fn test_history_sync_version_null_when_not_provided() {
        let conn = test_db();
        with_change_context(&conn, "test", None, |conn| {
            upsert_transaction(conn, &make_transaction(1, "Store A"))
        })
        .unwrap();

        let is_null: bool = conn
            .query_row(
                "SELECT _sync_version IS NULL FROM _transactions_history WHERE _rowid = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(is_null);
    }

    #[test]
    fn test_history_missing_context_fails() {
        let conn = test_db();
        let result = upsert_transaction(&conn, &make_transaction(1, "Store A"));
        assert!(result.is_err());
    }

    #[test]
    fn test_history_delete_creates_row() {
        let conn = test_db();
        with_change_context(&conn, "test", None, |conn| {
            upsert_transaction(conn, &make_transaction(1, "Store A"))?;
            conn.execute("DELETE FROM transactions WHERE id = 1", [])?;
            Ok(())
        })
        .unwrap();

        let (version, mask): (i64, i64) = conn
            .query_row(
                "SELECT _version, _mask FROM _transactions_history WHERE _rowid = 1 ORDER BY _version DESC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(version, 2);
        assert_eq!(mask, 63);

        let payee_is_null: bool = conn
            .query_row(
                "SELECT payee IS NULL FROM _transactions_history WHERE _rowid = 1 AND _version = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(payee_is_null);
    }
}
