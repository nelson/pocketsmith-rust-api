use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::models::*;

pub fn initialize(path: &str) -> Result<Connection> {
    let conn = Connection::open(path).context("Failed to open database")?;

    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS users (
            id                          INTEGER PRIMARY KEY,
            login                       TEXT,
            name                        TEXT,
            email                       TEXT,
            avatar_url                  TEXT,
            beta_user                   INTEGER,
            time_zone                   TEXT,
            week_start_day              INTEGER,
            is_reviewing_transactions   INTEGER,
            base_currency_code          TEXT,
            always_show_base_currency   INTEGER,
            using_multiple_currencies   INTEGER,
            available_accounts          INTEGER,
            available_budgets           INTEGER,
            forecast_last_updated_at    TEXT,
            forecast_last_accessed_at   TEXT,
            forecast_start_date         TEXT,
            forecast_end_date           TEXT,
            forecast_defer_recalculate  INTEGER,
            forecast_needs_recalculate  INTEGER,
            last_logged_in_at           TEXT,
            last_activity_at            TEXT,
            created_at                  TEXT,
            updated_at                  TEXT
        );

        CREATE TABLE IF NOT EXISTS institutions (
            id              INTEGER PRIMARY KEY,
            title           TEXT,
            currency_code   TEXT,
            created_at      TEXT,
            updated_at      TEXT
        );

        CREATE TABLE IF NOT EXISTS scenarios (
            id                                  INTEGER PRIMARY KEY,
            title                               TEXT,
            scenario_type                       TEXT,
            minimum_value                       REAL,
            maximum_value                       REAL,
            achieve_date                        TEXT,
            starting_balance                    REAL,
            starting_balance_date               TEXT,
            closing_balance                     REAL,
            closing_balance_date                TEXT,
            current_balance                     REAL,
            current_balance_date                TEXT,
            current_balance_in_base_currency    REAL,
            current_balance_exchange_rate       REAL,
            safe_balance                        REAL,
            safe_balance_in_base_currency       REAL,
            interest_rate                       REAL,
            interest_rate_repeat_id             INTEGER,
            created_at                          TEXT,
            updated_at                          TEXT
        );

        CREATE TABLE IF NOT EXISTS transaction_accounts (
            id                                  INTEGER PRIMARY KEY,
            account_id                          INTEGER,
            name                                TEXT,
            number                              TEXT,
            currency_code                       TEXT,
            account_type                        TEXT,
            current_balance                     REAL,
            current_balance_date                TEXT,
            current_balance_in_base_currency    REAL,
            current_balance_exchange_rate       REAL,
            safe_balance                        REAL,
            safe_balance_in_base_currency       REAL,
            starting_balance                    REAL,
            starting_balance_date               TEXT,
            institution_id                      INTEGER,
            created_at                          TEXT,
            updated_at                          TEXT,
            FOREIGN KEY (institution_id) REFERENCES institutions(id)
        );

        CREATE TABLE IF NOT EXISTS accounts (
            id                                  INTEGER PRIMARY KEY,
            title                               TEXT,
            currency_code                       TEXT,
            account_type                        TEXT,
            is_net_worth                        INTEGER,
            primary_transaction_account_id      INTEGER,
            primary_scenario_id                 INTEGER,
            current_balance                     REAL,
            current_balance_date                TEXT,
            current_balance_in_base_currency    REAL,
            current_balance_exchange_rate       REAL,
            safe_balance                        REAL,
            safe_balance_in_base_currency       REAL,
            created_at                          TEXT,
            updated_at                          TEXT,
            FOREIGN KEY (primary_transaction_account_id) REFERENCES transaction_accounts(id),
            FOREIGN KEY (primary_scenario_id) REFERENCES scenarios(id)
        );

        CREATE TABLE IF NOT EXISTS categories (
            id              INTEGER PRIMARY KEY,
            title           TEXT,
            colour          TEXT,
            parent_id       INTEGER,
            is_transfer     INTEGER,
            is_bill         INTEGER,
            roll_up         INTEGER,
            refund_behaviour TEXT,
            created_at      TEXT,
            updated_at      TEXT,
            FOREIGN KEY (parent_id) REFERENCES categories(id)
        );

        CREATE TABLE IF NOT EXISTS transactions (
            id                          INTEGER PRIMARY KEY,
            transaction_type            TEXT,
            payee                       TEXT,
            amount                      REAL,
            amount_in_base_currency     REAL,
            date                        TEXT,
            cheque_number               TEXT,
            memo                        TEXT,
            is_transfer                 INTEGER,
            category_id                 INTEGER,
            note                        TEXT,
            labels                      TEXT,
            original_payee              TEXT,
            upload_source               TEXT,
            closing_balance             REAL,
            transaction_account_id      INTEGER,
            status                      TEXT,
            needs_review                INTEGER,
            created_at                  TEXT,
            updated_at                  TEXT,
            FOREIGN KEY (category_id) REFERENCES categories(id),
            FOREIGN KEY (transaction_account_id) REFERENCES transaction_accounts(id)
        );
        ",
    )
    .context("Failed to create tables")?;

    Ok(conn)
}

pub fn upsert_user(conn: &Connection, u: &User) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO users (
            id, login, name, email, avatar_url, beta_user, time_zone,
            week_start_day, is_reviewing_transactions, base_currency_code,
            always_show_base_currency, using_multiple_currencies,
            available_accounts, available_budgets,
            forecast_last_updated_at, forecast_last_accessed_at,
            forecast_start_date, forecast_end_date,
            forecast_defer_recalculate, forecast_needs_recalculate,
            last_logged_in_at, last_activity_at, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24
        )",
        params![
            u.id,
            u.login,
            u.name,
            u.email,
            u.avatar_url,
            u.beta_user,
            u.time_zone,
            u.week_start_day,
            u.is_reviewing_transactions,
            u.base_currency_code,
            u.always_show_base_currency,
            u.using_multiple_currencies,
            u.available_accounts,
            u.available_budgets,
            u.forecast_last_updated_at,
            u.forecast_last_accessed_at,
            u.forecast_start_date,
            u.forecast_end_date,
            u.forecast_defer_recalculate,
            u.forecast_needs_recalculate,
            u.last_logged_in_at,
            u.last_activity_at,
            u.created_at,
            u.updated_at,
        ],
    )?;
    Ok(())
}

pub fn upsert_institution(conn: &Connection, i: &Institution) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO institutions (id, title, currency_code, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![i.id, i.title, i.currency_code, i.created_at, i.updated_at],
    )?;
    Ok(())
}

pub fn upsert_scenario(conn: &Connection, s: &Scenario) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO scenarios (
            id, title, scenario_type, minimum_value, maximum_value,
            achieve_date, starting_balance, starting_balance_date,
            closing_balance, closing_balance_date,
            current_balance, current_balance_date,
            current_balance_in_base_currency, current_balance_exchange_rate,
            safe_balance, safe_balance_in_base_currency,
            interest_rate, interest_rate_repeat_id,
            created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
            ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20
        )",
        params![
            s.id,
            s.title,
            s.scenario_type,
            s.minimum_value,
            s.maximum_value,
            s.achieve_date,
            s.starting_balance,
            s.starting_balance_date,
            s.closing_balance,
            s.closing_balance_date,
            s.current_balance,
            s.current_balance_date,
            s.current_balance_in_base_currency,
            s.current_balance_exchange_rate,
            s.safe_balance,
            s.safe_balance_in_base_currency,
            s.interest_rate,
            s.interest_rate_repeat_id,
            s.created_at,
            s.updated_at,
        ],
    )?;
    Ok(())
}

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

pub fn upsert_category(conn: &Connection, c: &Category) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO categories (
            id, title, colour, parent_id, is_transfer, is_bill,
            roll_up, refund_behaviour, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            c.id,
            c.title,
            c.colour,
            c.parent_id,
            c.is_transfer,
            c.is_bill,
            c.roll_up,
            c.refund_behaviour,
            c.created_at,
            c.updated_at,
        ],
    )?;

    // Recursively upsert children
    if let Some(ref children) = c.children {
        for child in children {
            upsert_category(conn, child)?;
        }
    }

    Ok(())
}

pub fn initialize_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory().context("Failed to open in-memory database")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS users (
            id                          INTEGER PRIMARY KEY,
            login                       TEXT,
            name                        TEXT,
            email                       TEXT,
            avatar_url                  TEXT,
            beta_user                   INTEGER,
            time_zone                   TEXT,
            week_start_day              INTEGER,
            is_reviewing_transactions   INTEGER,
            base_currency_code          TEXT,
            always_show_base_currency   INTEGER,
            using_multiple_currencies   INTEGER,
            available_accounts          INTEGER,
            available_budgets           INTEGER,
            forecast_last_updated_at    TEXT,
            forecast_last_accessed_at   TEXT,
            forecast_start_date         TEXT,
            forecast_end_date           TEXT,
            forecast_defer_recalculate  INTEGER,
            forecast_needs_recalculate  INTEGER,
            last_logged_in_at           TEXT,
            last_activity_at            TEXT,
            created_at                  TEXT,
            updated_at                  TEXT
        );

        CREATE TABLE IF NOT EXISTS institutions (
            id              INTEGER PRIMARY KEY,
            title           TEXT,
            currency_code   TEXT,
            created_at      TEXT,
            updated_at      TEXT
        );

        CREATE TABLE IF NOT EXISTS scenarios (
            id                                  INTEGER PRIMARY KEY,
            title                               TEXT,
            scenario_type                       TEXT,
            minimum_value                       REAL,
            maximum_value                       REAL,
            achieve_date                        TEXT,
            starting_balance                    REAL,
            starting_balance_date               TEXT,
            closing_balance                     REAL,
            closing_balance_date                TEXT,
            current_balance                     REAL,
            current_balance_date                TEXT,
            current_balance_in_base_currency    REAL,
            current_balance_exchange_rate       REAL,
            safe_balance                        REAL,
            safe_balance_in_base_currency       REAL,
            interest_rate                       REAL,
            interest_rate_repeat_id             INTEGER,
            created_at                          TEXT,
            updated_at                          TEXT
        );

        CREATE TABLE IF NOT EXISTS transaction_accounts (
            id                                  INTEGER PRIMARY KEY,
            account_id                          INTEGER,
            name                                TEXT,
            number                              TEXT,
            currency_code                       TEXT,
            account_type                        TEXT,
            current_balance                     REAL,
            current_balance_date                TEXT,
            current_balance_in_base_currency    REAL,
            current_balance_exchange_rate       REAL,
            safe_balance                        REAL,
            safe_balance_in_base_currency       REAL,
            starting_balance                    REAL,
            starting_balance_date               TEXT,
            institution_id                      INTEGER,
            created_at                          TEXT,
            updated_at                          TEXT,
            FOREIGN KEY (institution_id) REFERENCES institutions(id)
        );

        CREATE TABLE IF NOT EXISTS accounts (
            id                                  INTEGER PRIMARY KEY,
            title                               TEXT,
            currency_code                       TEXT,
            account_type                        TEXT,
            is_net_worth                        INTEGER,
            primary_transaction_account_id      INTEGER,
            primary_scenario_id                 INTEGER,
            current_balance                     REAL,
            current_balance_date                TEXT,
            current_balance_in_base_currency    REAL,
            current_balance_exchange_rate       REAL,
            safe_balance                        REAL,
            safe_balance_in_base_currency       REAL,
            created_at                          TEXT,
            updated_at                          TEXT,
            FOREIGN KEY (primary_transaction_account_id) REFERENCES transaction_accounts(id),
            FOREIGN KEY (primary_scenario_id) REFERENCES scenarios(id)
        );

        CREATE TABLE IF NOT EXISTS categories (
            id              INTEGER PRIMARY KEY,
            title           TEXT,
            colour          TEXT,
            parent_id       INTEGER,
            is_transfer     INTEGER,
            is_bill         INTEGER,
            roll_up         INTEGER,
            refund_behaviour TEXT,
            created_at      TEXT,
            updated_at      TEXT,
            FOREIGN KEY (parent_id) REFERENCES categories(id)
        );

        CREATE TABLE IF NOT EXISTS transactions (
            id                          INTEGER PRIMARY KEY,
            transaction_type            TEXT,
            payee                       TEXT,
            amount                      REAL,
            amount_in_base_currency     REAL,
            date                        TEXT,
            cheque_number               TEXT,
            memo                        TEXT,
            is_transfer                 INTEGER,
            category_id                 INTEGER,
            note                        TEXT,
            labels                      TEXT,
            original_payee              TEXT,
            upload_source               TEXT,
            closing_balance             REAL,
            transaction_account_id      INTEGER,
            status                      TEXT,
            needs_review                INTEGER,
            created_at                  TEXT,
            updated_at                  TEXT,
            FOREIGN KEY (category_id) REFERENCES categories(id),
            FOREIGN KEY (transaction_account_id) REFERENCES transaction_accounts(id)
        );
        ",
    )?;
    Ok(conn)
}

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

    fn test_db() -> Connection {
        initialize_in_memory().unwrap()
    }

    fn make_user(id: i64, name: &str) -> User {
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

    fn make_institution(id: i64, title: &str) -> Institution {
        Institution {
            id,
            title: Some(title.into()),
            currency_code: Some("NZD".into()),
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        }
    }

    fn make_transaction_account(id: i64, name: &str) -> TransactionAccount {
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
            institution: None,
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        }
    }

    fn make_category(id: i64, title: &str) -> Category {
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

    fn make_transaction(id: i64, payee: &str) -> Transaction {
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

    // --- User tests ---

    #[test]
    fn test_upsert_user_insert() {
        let conn = test_db();
        let user = make_user(1, "Alice");
        upsert_user(&conn, &user).unwrap();

        let name: String = conn
            .query_row("SELECT name FROM users WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name, "Alice");
    }

    #[test]
    fn test_upsert_user_replace() {
        let conn = test_db();
        upsert_user(&conn, &make_user(1, "Alice")).unwrap();
        upsert_user(&conn, &make_user(1, "Bob")).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let name: String = conn
            .query_row("SELECT name FROM users WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(name, "Bob");
    }

    #[test]
    fn test_upsert_multiple_users() {
        let conn = test_db();
        upsert_user(&conn, &make_user(1, "Alice")).unwrap();
        upsert_user(&conn, &make_user(2, "Bob")).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    // --- Institution tests ---

    #[test]
    fn test_upsert_institution() {
        let conn = test_db();
        let inst = make_institution(1, "ANZ Bank");
        upsert_institution(&conn, &inst).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM institutions WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "ANZ Bank");
    }

    #[test]
    fn test_upsert_institution_replace() {
        let conn = test_db();
        upsert_institution(&conn, &make_institution(1, "ANZ")).unwrap();
        upsert_institution(&conn, &make_institution(1, "Westpac")).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM institutions WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "Westpac");
    }

    // --- Transaction Account tests ---

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

    // --- Scenario tests ---

    #[test]
    fn test_upsert_scenario() {
        let conn = test_db();
        let scenario = Scenario {
            id: 1,
            title: Some("Main".into()),
            scenario_type: Some("no_budget".into()),
            minimum_value: None,
            maximum_value: None,
            achieve_date: None,
            starting_balance: Some(0.0),
            starting_balance_date: None,
            closing_balance: None,
            closing_balance_date: None,
            current_balance: Some(1500.0),
            current_balance_date: None,
            current_balance_in_base_currency: Some(1500.0),
            current_balance_exchange_rate: None,
            safe_balance: None,
            safe_balance_in_base_currency: None,
            interest_rate: None,
            interest_rate_repeat_id: None,
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        };
        upsert_scenario(&conn, &scenario).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM scenarios WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "Main");
    }

    // --- Account tests ---

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

    // --- Category tests ---

    #[test]
    fn test_upsert_category_simple() {
        let conn = test_db();
        let cat = make_category(1, "Food");
        upsert_category(&conn, &cat).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM categories WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "Food");
    }

    #[test]
    fn test_upsert_category_with_children() {
        let conn = test_db();
        let cat = Category {
            id: 1,
            title: Some("Food".into()),
            colour: Some("#ff0000".into()),
            children: Some(vec![
                Category {
                    id: 2,
                    title: Some("Groceries".into()),
                    colour: None,
                    children: None,
                    parent_id: Some(1),
                    is_transfer: Some(false),
                    is_bill: Some(false),
                    roll_up: Some(false),
                    refund_behaviour: None,
                    created_at: Some("2020-01-01T00:00:00Z".into()),
                    updated_at: Some("2024-01-01T00:00:00Z".into()),
                },
                Category {
                    id: 3,
                    title: Some("Restaurants".into()),
                    colour: None,
                    children: None,
                    parent_id: Some(1),
                    is_transfer: Some(false),
                    is_bill: Some(false),
                    roll_up: Some(false),
                    refund_behaviour: None,
                    created_at: Some("2020-01-01T00:00:00Z".into()),
                    updated_at: Some("2024-01-01T00:00:00Z".into()),
                },
            ]),
            parent_id: None,
            is_transfer: Some(false),
            is_bill: Some(false),
            roll_up: Some(true),
            refund_behaviour: None,
            created_at: Some("2020-01-01T00:00:00Z".into()),
            updated_at: Some("2024-01-01T00:00:00Z".into()),
        };
        upsert_category(&conn, &cat).unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM categories", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);

        let parent_id: i64 = conn
            .query_row(
                "SELECT parent_id FROM categories WHERE id = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(parent_id, 1);
    }

    #[test]
    fn test_upsert_category_replace() {
        let conn = test_db();
        upsert_category(&conn, &make_category(1, "Food")).unwrap();
        upsert_category(&conn, &make_category(1, "Groceries")).unwrap();

        let title: String = conn
            .query_row("SELECT title FROM categories WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(title, "Groceries");
    }

    // --- Transaction tests ---

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
        assert!(tables.contains(&"institutions".to_string()));
        assert!(tables.contains(&"scenarios".to_string()));
        assert!(tables.contains(&"transaction_accounts".to_string()));
        assert!(tables.contains(&"accounts".to_string()));
        assert!(tables.contains(&"categories".to_string()));
        assert!(tables.contains(&"transactions".to_string()));
    }
}
