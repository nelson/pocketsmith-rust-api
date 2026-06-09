//! Typed CRUD over the `rule_*` tables (rule-cli §3.3).
//!
//! These are **pure storage ops**: each performs exactly one row
//! mutation and does *not* open an operation or dump. The single-change
//! orchestration (one `with_operation`, one dump, one activity line)
//! lives in [`commit`](super::commit). They are split out only for
//! testability — callers always go through `commit`, which admits
//! exactly one [`Mutation`].

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};

use super::model::{LocationKind, MoveTarget, Rule, RuleData, RuleError};
use super::Stage;

/// Stages with a `sort_order` column (NOT NULL): the three loop stages
/// plus banking_ops. New rows append at `MAX(sort_order)+1`.
fn has_sort_order(stage: Stage) -> bool {
    matches!(
        stage,
        Stage::Prefixes | Stage::Suffixes | Stage::Expansions | Stage::BankingOps
    )
}

/// Stages whose order is user-controllable via `move` (rule-cli §3.3):
/// the three loop stages only. banking_ops keeps a `sort_order` for a
/// stable dump but is auto-ordered, so it is not movable.
pub fn is_movable(stage: Stage) -> bool {
    matches!(stage, Stage::Prefixes | Stage::Suffixes | Stage::Expansions)
}

/// Data columns for a stage, in the order [`map_row`] reads them.
fn data_columns(stage: Stage) -> &'static [&'static str] {
    match stage {
        Stage::Prefixes => &["pattern", "gateway", "operation", "has_account", "has_date", "note"],
        Stage::Suffixes => &[
            "pattern", "gateway", "operation", "institution", "has_account", "has_date",
            "has_location", "has_currency_code", "has_amount", "note",
        ],
        Stage::Expansions => &["pattern", "canonical", "note"],
        Stage::Persons | Stage::Employers | Stage::Merchants => &["canonical", "pattern", "note"],
        Stage::BankingOps => &["operation", "pattern", "has_account", "note"],
        Stage::Locations => &["location", "kind", "note"],
    }
}

/// `ORDER BY` for reads: ordered stages by (sort_order, id); the rest by
/// the auto-sort that matches the pipeline (id = declaration order).
fn order_by(stage: Stage) -> &'static str {
    if has_sort_order(stage) {
        "sort_order, id"
    } else {
        "id"
    }
}

/// Build the `SELECT id, <sort_order|NULL>, <data cols> FROM table` head.
fn select_sql(stage: Stage) -> String {
    let sort_expr = if has_sort_order(stage) { "sort_order" } else { "NULL" };
    format!(
        "SELECT id, {sort_expr}, {} FROM {}",
        data_columns(stage).join(", "),
        stage.table()
    )
}

/// Read one row (id, sort_order, then data cols at offset 2) into a [`Rule`].
fn map_row(stage: Stage, row: &rusqlite::Row<'_>) -> rusqlite::Result<Rule> {
    let id: i64 = row.get(0)?;
    let sort_order: Option<i64> = row.get(1)?;
    // Data columns start at index 2.
    let s = |i: usize| -> rusqlite::Result<Option<String>> { row.get(i) };
    let req = |i: usize| -> rusqlite::Result<String> { row.get(i) };
    let b = |i: usize| -> rusqlite::Result<bool> { Ok(row.get::<_, i64>(i)? != 0) };
    let data = match stage {
        Stage::Prefixes => RuleData::Prefix {
            pattern: req(2)?,
            gateway: s(3)?,
            operation: s(4)?,
            has_account: b(5)?,
            has_date: b(6)?,
            note: s(7)?,
        },
        Stage::Suffixes => RuleData::Suffix {
            pattern: req(2)?,
            gateway: s(3)?,
            operation: s(4)?,
            institution: s(5)?,
            has_account: b(6)?,
            has_date: b(7)?,
            has_location: b(8)?,
            has_currency_code: b(9)?,
            has_amount: b(10)?,
            note: s(11)?,
        },
        Stage::Expansions => RuleData::Expansion {
            pattern: req(2)?,
            canonical: req(3)?,
            note: s(4)?,
        },
        Stage::Persons => RuleData::Person {
            canonical: req(2)?,
            pattern: req(3)?,
            note: s(4)?,
        },
        Stage::Employers => RuleData::Employer {
            canonical: req(2)?,
            pattern: req(3)?,
            note: s(4)?,
        },
        Stage::Merchants => RuleData::Merchant {
            canonical: req(2)?,
            pattern: req(3)?,
            note: s(4)?,
        },
        Stage::BankingOps => RuleData::BankingOp {
            operation: req(2)?,
            pattern: req(3)?,
            has_account: b(4)?,
            note: s(5)?,
        },
        Stage::Locations => RuleData::Location {
            location: req(2)?,
            // CHECK constraint guarantees a valid kind; default to Location.
            kind: LocationKind::from_str(&req(3)?).unwrap_or(LocationKind::Location),
            note: s(4)?,
        },
    };
    Ok(Rule { id, sort_order, data })
}

/// List a stage's rules in apply order.
pub fn list(conn: &Connection, stage: Stage) -> Result<Vec<Rule>> {
    let sql = format!("{} ORDER BY {}", select_sql(stage), order_by(stage));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| map_row(stage, row))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Fetch one rule by id, or `None` if it isn't in that stage.
pub fn get(conn: &Connection, stage: Stage, id: i64) -> Result<Option<Rule>> {
    let sql = format!("{} WHERE id = ?1", select_sql(stage));
    let mut stmt = conn.prepare(&sql)?;
    Ok(stmt.query_row([id], |row| map_row(stage, row)).optional()?)
}

/// Load a stage's rules in apply order as bare [`RuleData`] for the
/// pipeline compilers — the single typed read that replaces the
/// per-stage hand-written `SELECT`s (rule-cli §3.1).
pub fn load_for_compile(conn: &Connection, stage: Stage) -> Result<Vec<RuleData>> {
    Ok(list(conn, stage)?.into_iter().map(|r| r.data).collect())
}

/// Next append position for an ordered stage.
fn next_sort_order(conn: &Connection, stage: Stage) -> Result<i64> {
    let sql = format!("SELECT COALESCE(MAX(sort_order) + 1, 0) FROM {}", stage.table());
    Ok(conn.query_row(&sql, [], |r| r.get(0))?)
}

/// Bound parameter values for a rule's data columns, in `data_columns`
/// order. Booleans become 0/1; `None` becomes SQL NULL.
fn data_params(data: &RuleData) -> Vec<rusqlite::types::Value> {
    use rusqlite::types::Value;
    fn t(s: &str) -> Value {
        Value::Text(s.to_string())
    }
    fn opt(s: &Option<String>) -> Value {
        match s {
            Some(v) => Value::Text(v.clone()),
            None => Value::Null,
        }
    }
    fn flag(b: bool) -> Value {
        Value::Integer(if b { 1 } else { 0 })
    }
    match data {
        RuleData::Prefix { pattern, gateway, operation, has_account, has_date, note } => {
            vec![t(pattern), opt(gateway), opt(operation), flag(*has_account), flag(*has_date), opt(note)]
        }
        RuleData::Suffix {
            pattern, gateway, operation, institution, has_account, has_date, has_location,
            has_currency_code, has_amount, note,
        } => vec![
            t(pattern), opt(gateway), opt(operation), opt(institution), flag(*has_account),
            flag(*has_date), flag(*has_location), flag(*has_currency_code), flag(*has_amount), opt(note),
        ],
        RuleData::Expansion { pattern, canonical, note } => vec![t(pattern), t(canonical), opt(note)],
        RuleData::Person { canonical, pattern, note }
        | RuleData::Employer { canonical, pattern, note }
        | RuleData::Merchant { canonical, pattern, note } => {
            vec![t(canonical), t(pattern), opt(note)]
        }
        RuleData::BankingOp { operation, pattern, has_account, note } => {
            vec![t(operation), t(pattern), flag(*has_account), opt(note)]
        }
        RuleData::Location { location, kind, note } => {
            vec![t(location), t(kind.as_str()), opt(note)]
        }
    }
}

/// Insert a new rule, returning its assigned id. New ordered-stage rows
/// append at `MAX(sort_order)+1`. A `UNIQUE` violation maps to
/// [`RuleError::Duplicate`].
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

/// Update an existing rule's data columns by id (sort_order untouched).
pub fn update_rule(conn: &Connection, id: i64, data: &RuleData) -> Result<()> {
    let stage = data.stage();
    let cols = data_columns(stage);
    let assignments: Vec<String> =
        cols.iter().enumerate().map(|(i, c)| format!("{c} = ?{}", i + 1)).collect();
    let mut vals = data_params(data);
    let id_idx = vals.len() + 1;
    vals.push(rusqlite::types::Value::Integer(id));
    let sql = format!(
        "UPDATE {} SET {} WHERE id = ?{id_idx}",
        stage.table(),
        assignments.join(", ")
    );
    match conn.execute(&sql, rusqlite::params_from_iter(vals.iter())) {
        Ok(0) => Err(RuleError::NotFound { stage, id }.into()),
        Ok(_) => Ok(()),
        Err(e) => Err(map_unique(conn, data, e)),
    }
}

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

/// Reposition one loop-stage rule relative to a neighbour, then
/// renumber `sort_order` dense (rule-cli §3.3).
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

/// Map a rusqlite UNIQUE-constraint error to a friendly
/// [`RuleError::Duplicate`]; pass other errors through as anyhow.
fn map_unique(conn: &Connection, data: &RuleData, e: rusqlite::Error) -> anyhow::Error {
    if let rusqlite::Error::SqliteFailure(f, _) = &e {
        if f.code == rusqlite::ErrorCode::ConstraintViolation {
            return RuleError::Duplicate(duplicate_message(conn, data)).into();
        }
    }
    e.into()
}

/// Build "a {stage} rule with {field} {value:?} already exists (#id)".
fn duplicate_message(conn: &Connection, data: &RuleData) -> String {
    let stage = data.stage();
    let name = stage.name();
    let (clause, descr): (String, String) = match data {
        RuleData::Prefix { pattern, .. }
        | RuleData::Suffix { pattern, .. }
        | RuleData::Expansion { pattern, .. }
        | RuleData::Merchant { pattern, .. }
        | RuleData::Employer { pattern, .. } => {
            ("pattern = ?1".into(), format!("pattern {pattern:?}"))
        }
        RuleData::Person { canonical, pattern, .. } => (
            "canonical = ?1 AND pattern = ?2".into(),
            format!("canonical {canonical:?} + pattern {pattern:?}"),
        ),
        RuleData::BankingOp { operation, pattern, .. } => (
            "operation = ?1 AND pattern = ?2".into(),
            format!("operation {operation:?} + pattern {pattern:?}"),
        ),
        RuleData::Location { location, .. } => {
            ("location = ?1".into(), format!("location {location:?}"))
        }
    };
    let id = find_conflict_id(conn, stage, &clause, data);
    match id {
        Some(id) => format!("a {name} rule with {descr} already exists (#{id})"),
        None => format!("a {name} rule with {descr} already exists"),
    }
}

fn find_conflict_id(conn: &Connection, stage: Stage, clause: &str, data: &RuleData) -> Option<i64> {
    let sql = format!("SELECT id FROM {} WHERE {clause}", stage.table());
    let res = match data {
        RuleData::Person { canonical, pattern, .. } => {
            conn.query_row(&sql, params![canonical, pattern], |r| r.get(0))
        }
        RuleData::BankingOp { operation, pattern, .. } => {
            conn.query_row(&sql, params![operation, pattern], |r| r.get(0))
        }
        RuleData::Location { location, .. } => conn.query_row(&sql, params![location], |r| r.get(0)),
        other => {
            let pattern = other.pattern().unwrap_or("");
            conn.query_row(&sql, params![pattern], |r| r.get(0))
        }
    };
    res.optional().ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;
    use crate::rules::model::MoveTarget;

    fn merchant(canonical: &str, pattern: &str) -> RuleData {
        RuleData::Merchant { canonical: canonical.into(), pattern: pattern.into(), note: None }
    }

    fn prefix(pattern: &str) -> RuleData {
        RuleData::Prefix {
            pattern: pattern.into(),
            gateway: None,
            operation: None,
            has_account: false,
            has_date: false,
            note: None,
        }
    }

    #[test]
    fn insert_get_update_delete_roundtrip() {
        let conn = initialize_in_memory().unwrap();
        let id = insert_rule(&conn, &merchant("Uber", "(?i)UBER")).unwrap();
        let got = get(&conn, Stage::Merchants, id).unwrap().unwrap();
        assert_eq!(got.data, merchant("Uber", "(?i)UBER"));
        assert_eq!(got.sort_order, None);

        update_rule(&conn, id, &merchant("Uber", "(?i)UBER\\b")).unwrap();
        assert_eq!(get(&conn, Stage::Merchants, id).unwrap().unwrap().data.pattern(), Some("(?i)UBER\\b"));

        delete_rule(&conn, Stage::Merchants, id).unwrap();
        assert!(get(&conn, Stage::Merchants, id).unwrap().is_none());
    }

    #[test]
    fn update_missing_is_not_found() {
        let conn = initialize_in_memory().unwrap();
        let err = update_rule(&conn, 999, &merchant("X", "X")).unwrap_err();
        assert!(matches!(err.downcast_ref::<RuleError>(), Some(RuleError::NotFound { id: 999, .. })));
    }

    #[test]
    fn unique_violation_maps_to_duplicate() {
        let conn = initialize_in_memory().unwrap();
        let first = insert_rule(&conn, &merchant("Amazon", "(?i)AMAZON")).unwrap();
        let err = insert_rule(&conn, &merchant("Amazon dup", "(?i)AMAZON")).unwrap_err();
        match err.downcast_ref::<RuleError>() {
            Some(RuleError::Duplicate(msg)) => {
                assert!(msg.contains("(?i)AMAZON"), "msg: {msg}");
                assert!(msg.contains(&format!("#{first}")), "msg should name conflict id: {msg}");
            }
            other => panic!("expected Duplicate, got {other:?}"),
        }
    }

    #[test]
    fn loop_stage_appends_and_is_dense() {
        let conn = initialize_in_memory().unwrap();
        let a = insert_rule(&conn, &prefix("^A ")).unwrap();
        let b = insert_rule(&conn, &prefix("^B ")).unwrap();
        let c = insert_rule(&conn, &prefix("^C ")).unwrap();
        let order: Vec<i64> = list(&conn, Stage::Prefixes).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(order, vec![a, b, c]);
        let sorts: Vec<i64> =
            list(&conn, Stage::Prefixes).unwrap().iter().map(|r| r.sort_order.unwrap()).collect();
        assert_eq!(sorts, vec![0, 1, 2]);
    }

    #[test]
    fn move_before_and_after_keep_dense() {
        let conn = initialize_in_memory().unwrap();
        let a = insert_rule(&conn, &prefix("^A ")).unwrap();
        let b = insert_rule(&conn, &prefix("^B ")).unwrap();
        let c = insert_rule(&conn, &prefix("^C ")).unwrap();
        // Move C before A → [C, A, B]
        move_rule(&conn, Stage::Prefixes, c, MoveTarget::Before(a)).unwrap();
        let order: Vec<i64> = list(&conn, Stage::Prefixes).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(order, vec![c, a, b]);
        // Move C after B → [A, B, C]
        move_rule(&conn, Stage::Prefixes, c, MoveTarget::After(b)).unwrap();
        let order: Vec<i64> = list(&conn, Stage::Prefixes).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(order, vec![a, b, c]);
        // Dense 0..N-1
        let sorts: Vec<i64> =
            list(&conn, Stage::Prefixes).unwrap().iter().map(|r| r.sort_order.unwrap()).collect();
        assert_eq!(sorts, vec![0, 1, 2]);
    }

    #[test]
    fn delete_renumbers_loop_stage_dense() {
        let conn = initialize_in_memory().unwrap();
        let a = insert_rule(&conn, &prefix("^A ")).unwrap();
        let b = insert_rule(&conn, &prefix("^B ")).unwrap();
        let c = insert_rule(&conn, &prefix("^C ")).unwrap();
        delete_rule(&conn, Stage::Prefixes, b).unwrap();
        let rows = list(&conn, Stage::Prefixes).unwrap();
        let ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        let sorts: Vec<i64> = rows.iter().map(|r| r.sort_order.unwrap()).collect();
        assert_eq!(ids, vec![a, c]);
        assert_eq!(sorts, vec![0, 1], "sort_order must stay dense after delete");
    }

    #[test]
    fn move_rejects_non_movable_stage() {
        let conn = initialize_in_memory().unwrap();
        let id = insert_rule(&conn, &merchant("Uber", "(?i)UBER")).unwrap();
        let other = insert_rule(&conn, &merchant("Lyft", "(?i)LYFT")).unwrap();
        let err = move_rule(&conn, Stage::Merchants, id, MoveTarget::Before(other)).unwrap_err();
        assert!(matches!(err.downcast_ref::<RuleError>(), Some(RuleError::NotOrdered(Stage::Merchants))));
    }
}
