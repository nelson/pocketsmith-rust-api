use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::*;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::*;

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
}
