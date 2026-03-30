mod schema;

pub mod categories;
pub mod transaction_accounts;
pub mod transactions;
pub mod users;

pub use categories::upsert_category;
pub use transaction_accounts::upsert_transaction_account;
pub use transactions::upsert_transaction;
pub use users::upsert_user;

use anyhow::{Context, Result};
use rusqlite::Connection;

pub fn initialize(path: &str) -> Result<Connection> {
    let conn = Connection::open(path).context("Failed to open database")?;

    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(schema::SCHEMA).context("Failed to create tables")?;

    Ok(conn)
}

pub fn initialize_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory().context("Failed to open in-memory database")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(schema::SCHEMA)?;
    Ok(conn)
}

pub fn get_last_change(conn: &Connection, reason: &str) -> Result<Option<(i64, String)>> {
    let mut stmt = conn.prepare(
        "SELECT version, created_at FROM _transaction_change_log WHERE reason = ?1 ORDER BY version DESC LIMIT 1",
    )?;
    Ok(stmt.query_row([reason], |row| Ok((row.get(0)?, row.get(1)?))).ok())
}

pub fn with_transaction_change_log<F, T>(conn: &Connection, reason: &str, f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T>,
{
    conn.execute(
        "INSERT INTO _transaction_change_log (reason) VALUES (?1)",
        [reason],
    )?;
    let version = conn.last_insert_rowid();

    conn.execute("DELETE FROM _transaction_change_log_context", [])?;
    conn.execute(
        "INSERT INTO _transaction_change_log_context (_version) VALUES (?1)",
        [version],
    )?;

    let result = f(conn);

    let count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT _rowid) FROM _transactions_history WHERE _version = ?1",
        [version],
        |row| row.get(0),
    )?;
    conn.execute(
        "UPDATE _transaction_change_log SET transactions_updated = ?1 WHERE version = ?2",
        rusqlite::params![count, version],
    )?;

    conn.execute("DELETE FROM _transaction_change_log_context", [])?;

    result
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::*;
    use crate::models::*;

    pub fn test_db() -> Connection {
        initialize_in_memory().unwrap()
    }

    pub fn make_user(id: i64, name: &str) -> User {
        User {
            id,
            login: Some("testlogin".into()),
            name: Some(name.into()),
            email: Some("test@example.com".into()),
            avatar_url: None,
            beta_user: Some(false),
            time_zone: Some("UTC".into()),
            week_start_day: Some(1),
            is_reviewing_transactions: Some(false),
            base_currency_code: Some("NZD".into()),
            always_show_base_currency: Some(false),
            using_multiple_currencies: Some(false),
            available_accounts: Some(10),
            available_budgets: Some(5),
            forecast_last_updated_at: None,
            forecast_last_accessed_at: None,
            forecast_start_date: None,
            forecast_end_date: None,
            forecast_defer_recalculate: Some(false),
            forecast_needs_recalculate: Some(false),
            last_logged_in_at: None,
            last_activity_at: None,
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        }
    }

    pub fn make_transaction_account(id: i64, name: &str) -> TransactionAccount {
        TransactionAccount {
            id,
            name: Some(name.into()),
            number: Some("12-3456-7890".into()),
            currency_code: Some("NZD".into()),
            account_type: Some("bank".into()),
            current_balance: Some(1000.0),
            current_balance_date: Some("2024-01-01".into()),
            current_balance_in_base_currency: Some(1000.0),
            current_balance_exchange_rate: Some(1.0),
            safe_balance: Some(900.0),
            safe_balance_in_base_currency: Some(900.0),
            starting_balance: Some(0.0),
            starting_balance_date: Some("2020-01-01".into()),
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        }
    }

    pub fn make_category(id: i64, title: &str) -> Category {
        Category {
            id,
            title: Some(title.into()),
            colour: Some("#ff0000".into()),
            children: None,
            parent_id: None,
            is_transfer: Some(false),
            is_bill: Some(false),
            roll_up: Some(false),
            refund_behaviour: None,
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        }
    }

    pub fn make_transaction(id: i64, payee: &str) -> Transaction {
        Transaction {
            id,
            transaction_type: Some("debit".into()),
            payee: Some(payee.into()),
            amount: Some(-50.0),
            amount_in_base_currency: Some(-50.0),
            date: Some("2024-06-15".into()),
            cheque_number: None,
            memo: None,
            is_transfer: Some(false),
            category: None,
            note: None,
            labels: None,
            original_payee: None,
            upload_source: None,
            closing_balance: None,
            transaction_account: None,
            status: Some("posted".into()),
            needs_review: Some(false),
            created_at: Some("2024-06-15T00:00:00Z".into()),
            updated_at: Some("2024-06-15T00:00:00Z".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_helpers::*;

    #[test]
    fn test_get_last_change_returns_none_when_empty() {
        let conn = test_db();
        assert_eq!(get_last_change(&conn, "pocketsmith").unwrap(), None);
    }

    #[test]
    fn test_with_transaction_change_log_creates_entry() {
        let conn = test_db();
        with_transaction_change_log(&conn, "pocketsmith", |_| Ok(())).unwrap();
        let (version, _) = get_last_change(&conn, "pocketsmith").unwrap().unwrap();
        assert_eq!(version, 1);
    }

    #[test]
    fn test_get_last_change_filters_by_reason() {
        let conn = test_db();
        with_transaction_change_log(&conn, "pocketsmith", |_| Ok(())).unwrap();
        with_transaction_change_log(&conn, "rules", |_| Ok(())).unwrap();
        let (version, _) = get_last_change(&conn, "pocketsmith").unwrap().unwrap();
        assert_eq!(version, 1);
        let (version, _) = get_last_change(&conn, "rules").unwrap().unwrap();
        assert_eq!(version, 2);
    }

    #[test]
    fn test_with_transaction_change_log_increments_version() {
        let conn = test_db();
        with_transaction_change_log(&conn, "test", |_| Ok(())).unwrap();
        with_transaction_change_log(&conn, "test", |_| Ok(())).unwrap();
        with_transaction_change_log(&conn, "test", |_| Ok(())).unwrap();
        let (version, _) = get_last_change(&conn, "test").unwrap().unwrap();
        assert_eq!(version, 3);
    }

    #[test]
    fn test_with_transaction_change_log_counts_transactions() {
        let conn = test_db();
        with_transaction_change_log(&conn, "test", |conn| {
            upsert_transaction(conn, &make_transaction(1, "A"))?;
            upsert_transaction(conn, &make_transaction(2, "B"))?;
            Ok(())
        }).unwrap();
        let count: i64 = conn.query_row(
            "SELECT transactions_updated FROM _transaction_change_log WHERE version = 1",
            [], |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_initialize_creates_all_tables() {
        let conn = test_db();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(tables.contains(&"users".to_string()));
        assert!(tables.contains(&"transaction_accounts".to_string()));
        assert!(tables.contains(&"categories".to_string()));
        assert!(tables.contains(&"transactions".to_string()));
    }

    #[test]
    fn test_initialize_creates_transfer_pairs_table() {
        let conn = test_db();
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='transfer_pairs'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists);
    }
}
