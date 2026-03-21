use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::*;

use super::scenarios::upsert_scenario;
use super::transaction_accounts::upsert_transaction_account;

pub fn upsert_account(conn: &Connection, a: &Account) -> Result<()> {
    // Upsert nested transaction accounts first
    if let Some(ref tas) = a.transaction_accounts {
        for ta in tas {
            upsert_transaction_account(conn, ta, Some(a.id))?;
        }
    }
    if let Some(ref ta) = a.primary_transaction_account {
        upsert_transaction_account(conn, ta, Some(a.id))?;
    }

    // Upsert nested scenarios
    if let Some(ref scenarios) = a.scenarios {
        for s in scenarios {
            upsert_scenario(conn, s)?;
        }
    }
    if let Some(ref s) = a.primary_scenario {
        upsert_scenario(conn, s)?;
    }

    conn.execute(
        "INSERT OR REPLACE INTO accounts (
            id, title, currency_code, account_type, is_net_worth,
            primary_transaction_account_id, primary_scenario_id,
            current_balance, current_balance_date,
            current_balance_in_base_currency, current_balance_exchange_rate,
            safe_balance, safe_balance_in_base_currency,
            created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15
        )",
        params![
            a.id,
            a.title,
            a.currency_code,
            a.account_type,
            a.is_net_worth,
            a.primary_transaction_account.as_ref().map(|ta| ta.id),
            a.primary_scenario.as_ref().map(|s| s.id),
            a.current_balance,
            a.current_balance_date,
            a.current_balance_in_base_currency,
            a.current_balance_exchange_rate,
            a.safe_balance,
            a.safe_balance_in_base_currency,
            a.created_at,
            a.updated_at,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::*;

    #[test]
    fn test_upsert_account_simple() {
        let conn = test_db();
        let account = Account {
            id: 100,
            title: Some("Savings".into()),
            currency_code: Some("NZD".into()),
            account_type: Some("bank".into()),
            is_net_worth: Some(true),
            primary_transaction_account: None,
            primary_scenario: None,
            transaction_accounts: None,
            scenarios: None,
            current_balance: Some(5000.0),
            current_balance_date: Some("2024-01-01".into()),
            current_balance_in_base_currency: Some(5000.0),
            current_balance_exchange_rate: Some(1.0),
            safe_balance: Some(4500.0),
            safe_balance_in_base_currency: Some(4500.0),
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        };
        upsert_account(&conn, &account).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM accounts WHERE id = 100", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "Savings");
    }

    #[test]
    fn test_upsert_account_with_nested_transaction_accounts() {
        let conn = test_db();
        let account = Account {
            id: 100,
            title: Some("Main".into()),
            currency_code: Some("NZD".into()),
            account_type: Some("bank".into()),
            is_net_worth: Some(true),
            primary_transaction_account: Some(make_transaction_account(200, "Primary")),
            primary_scenario: None,
            transaction_accounts: Some(vec![
                make_transaction_account(200, "Primary"),
                make_transaction_account(201, "Secondary"),
            ]),
            scenarios: None,
            current_balance: Some(5000.0),
            current_balance_date: None,
            current_balance_in_base_currency: Some(5000.0),
            current_balance_exchange_rate: None,
            safe_balance: None,
            safe_balance_in_base_currency: None,
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        };
        upsert_account(&conn, &account).unwrap();

        let ta_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transaction_accounts WHERE account_id = 100",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(ta_count, 2);

        let primary_ta_id: i64 = conn
            .query_row(
                "SELECT primary_transaction_account_id FROM accounts WHERE id = 100",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(primary_ta_id, 200);
    }
}
