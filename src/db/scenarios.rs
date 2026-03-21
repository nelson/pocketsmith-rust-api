use anyhow::Result;
use rusqlite::{params, Connection};

use crate::models::*;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::test_helpers::*;

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
}
