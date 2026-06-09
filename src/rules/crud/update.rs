//! Update: rewrite an existing rule's data columns by id.

use anyhow::Result;
use rusqlite::Connection;

use super::super::model::{RuleData, RuleError};
use super::{data_columns, data_params, map_unique};

/// Update an existing rule's data columns by id (sort_order untouched).
pub fn update_rule(conn: &Connection, id: i64, data: &RuleData) -> Result<()> {
    let stage = data.stage();
    let cols = data_columns(stage);
    let assignments: Vec<String> =
        cols.iter().enumerate().map(|(i, c)| format!("{c} = ?{}", i + 1)).collect();
    let mut vals = data_params(data);
    let id_idx = vals.len() + 1;
    vals.push(rusqlite::types::Value::Integer(id));
    let sql = format!("UPDATE {} SET {} WHERE id = ?{id_idx}", stage.table(), assignments.join(", "));
    match conn.execute(&sql, rusqlite::params_from_iter(vals.iter())) {
        Ok(0) => Err(RuleError::NotFound { stage, id }.into()),
        Ok(_) => Ok(()),
        Err(e) => Err(map_unique(conn, data, e)),
    }
}
