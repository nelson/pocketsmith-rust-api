//! Delete a rule by id, keeping ordered stages dense.

use anyhow::Result;
use rusqlite::{params, Connection};

use super::super::model::RuleError;
use super::super::Stage;
use super::has_sort_order;

/// Delete a rule by id. Ordered stages are renumbered dense afterwards.
pub fn delete_rule(conn: &Connection, stage: Stage, id: i64) -> Result<()> {
    let sql = format!("DELETE FROM {} WHERE id = ?1", stage.table());
    let n = conn.execute(&sql, [id])?;
    if n == 0 {
        return Err(RuleError::NotFound { stage, id }.into());
    }
    if has_sort_order(stage) {
        renumber_dense(conn, stage)?;
    }
    Ok(())
}

/// Rewrite `sort_order` to a dense `0..N-1` run preserving current order.
fn renumber_dense(conn: &Connection, stage: Stage) -> Result<()> {
    let sql = format!("SELECT id FROM {} ORDER BY sort_order, id", stage.table());
    let mut stmt = conn.prepare(&sql)?;
    let ids: Vec<i64> = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    let upd = format!("UPDATE {} SET sort_order = ?1 WHERE id = ?2", stage.table());
    for (i, id) in ids.iter().enumerate() {
        conn.execute(&upd, params![i as i64, id])?;
    }
    Ok(())
}
