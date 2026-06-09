//! Reorder a loop-stage rule relative to a neighbour (rule-cli §3.3).

use anyhow::Result;
use rusqlite::{params, Connection};

use super::super::model::{MoveTarget, RuleError};
use super::super::Stage;
use super::is_movable;

/// Reposition one loop-stage rule relative to a neighbour, then renumber
/// `sort_order` dense.
pub fn move_rule(conn: &Connection, stage: Stage, id: i64, target: MoveTarget) -> Result<()> {
    if !is_movable(stage) {
        return Err(RuleError::NotOrdered(stage).into());
    }
    let anchor = target.anchor();
    if id == anchor {
        return Err(RuleError::CrossStage.into());
    }
    // Current order of ids.
    let sql = format!("SELECT id FROM {} ORDER BY sort_order, id", stage.table());
    let mut stmt = conn.prepare(&sql)?;
    let mut ids: Vec<i64> = stmt.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
    if !ids.contains(&id) {
        return Err(RuleError::NotFound { stage, id }.into());
    }
    if !ids.contains(&anchor) {
        return Err(RuleError::NotFound { stage, id: anchor }.into());
    }
    ids.retain(|&x| x != id);
    let pos = ids.iter().position(|&x| x == anchor).expect("anchor present");
    let insert_at = match target {
        MoveTarget::Before(_) => pos,
        MoveTarget::After(_) => pos + 1,
    };
    ids.insert(insert_at, id);
    // Reassign sort_order = index.
    let upd = format!("UPDATE {} SET sort_order = ?1 WHERE id = ?2", stage.table());
    for (i, rid) in ids.iter().enumerate() {
        conn.execute(&upd, params![i as i64, rid])?;
    }
    Ok(())
}
