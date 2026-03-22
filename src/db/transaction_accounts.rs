use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::*;

pub fn upsert_transaction_account(
    conn: &Connection,
    ta: &TransactionAccount,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO transaction_accounts (
            id, name, number, currency_code, account_type,
            current_balance, current_balance_date,
            current_balance_in_base_currency, current_balance_exchange_rate,
            safe_balance, safe_balance_in_base_currency,
            starting_balance, starting_balance_date,
            created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15
        )",
        params![
            ta.id,
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
        upsert_transaction_account(&conn, &ta).unwrap();

        let name: String = conn
            .query_row(
                "SELECT name FROM transaction_accounts WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "Checking");
    }
}
