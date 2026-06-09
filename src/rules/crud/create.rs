//! Create: insert a new rule, returning its id.

use anyhow::Result;
use rusqlite::Connection;

use super::super::model::RuleData;
use super::super::Stage;
use super::{data_columns, data_params, has_sort_order, map_unique};

/// Insert a new rule, returning its assigned id. New ordered-stage rows
/// append at `MAX(sort_order)+1`. A `UNIQUE` violation maps to
/// [`RuleError::Duplicate`](super::super::model::RuleError).
pub fn insert_rule(conn: &Connection, data: &RuleData) -> Result<i64> {
    let stage = data.stage();
    let cols = data_columns(stage);
    let mut vals = data_params(data);
    let mut collist: Vec<&str> = cols.to_vec();
    if has_sort_order(stage) {
        collist.push("sort_order");
        vals.push(rusqlite::types::Value::Integer(next_sort_order(conn, stage)?));
    }
    let placeholders: Vec<String> = (1..=vals.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        stage.table(),
        collist.join(", "),
        placeholders.join(", ")
    );
    match conn.execute(&sql, rusqlite::params_from_iter(vals.iter())) {
        Ok(_) => Ok(conn.last_insert_rowid()),
        Err(e) => Err(map_unique(conn, data, e)),
    }
}

/// Next append position for an ordered stage.
fn next_sort_order(conn: &Connection, stage: Stage) -> Result<i64> {
    let sql = format!("SELECT COALESCE(MAX(sort_order) + 1, 0) FROM {}", stage.table());
    Ok(conn.query_row(&sql, [], |r| r.get(0))?)
}
