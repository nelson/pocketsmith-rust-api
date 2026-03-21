use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::*;

use super::institutions::upsert_institution;

pub fn upsert_transaction_account(
    conn: &Connection,
    ta: &TransactionAccount,
    account_id: Option<i64>,
) -> Result<()> {
    if let Some(ref inst) = ta.institution {
        upsert_institution(conn, inst)?;
    }

    conn.execute(
        "INSERT OR REPLACE INTO transaction_accounts (
            id, account_id, name, number, currency_code, account_type,
            current_balance, current_balance_date,
            current_balance_in_base_currency, current_balance_exchange_rate,
            safe_balance, safe_balance_in_base_currency,
            starting_balance, starting_balance_date,
            institution_id, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17
        )",
        params![
            ta.id,
            account_id,
            ta.name,
            ta.number,
            ta.currency_code,
            ta.account_type,
            ta.current_balance,
            ta.current_balance_date,
            ta.current_balance_in_base_currency,
            ta.current_balance_exchange_rate,
            ta.safe_balance,
            ta.safe_balance_in_base_currency,
            ta.starting_balance,
            ta.starting_balance_date,
            ta.institution.as_ref().map(|i| i.id),
            ta.created_at,
            ta.updated_at,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::*;

    #[test]
    fn test_upsert_transaction_account() {
        let conn = test_db();
        let ta = make_transaction_account(1, "Checking");
        upsert_transaction_account(&conn, &ta, None).unwrap();

        let name: String = conn
            .query_row(
                "SELECT name FROM transaction_accounts WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "Checking");
    }

    #[test]
    fn test_upsert_transaction_account_with_account_id() {
        let conn = test_db();
        let ta = make_transaction_account(1, "Checking");
        upsert_transaction_account(&conn, &ta, Some(100)).unwrap();

        let account_id: i64 = conn
            .query_row(
                "SELECT account_id FROM transaction_accounts WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(account_id, 100);
    }

    #[test]
    fn test_upsert_transaction_account_with_institution() {
        let conn = test_db();
        let mut ta = make_transaction_account(1, "Checking");
        ta.institution = Some(make_institution(50, "ANZ Bank"));
        upsert_transaction_account(&conn, &ta, None).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM institutions WHERE id = 50", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "ANZ Bank");

        let inst_id: i64 = conn
            .query_row(
                "SELECT institution_id FROM transaction_accounts WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(inst_id, 50);
    }
}
