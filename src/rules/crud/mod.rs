//! Typed CRUD over the `rule_*` tables (rule-cli §3.3), split one verb
//! per file (`create`/`read`/`update`/`delete`/`reorder`).
//!
//! These are **pure storage ops**: each performs exactly one row
//! mutation and does *not* open an operation or dump. The single-change
//! orchestration (one `with_operation`, one dump, one activity line)
//! lives in [`commit`](super::commit). They are split out only for
//! testability — callers always go through `commit`, which admits
//! exactly one [`Mutation`].
//!
//! This `mod.rs` holds the shared SQL-shape plumbing (column lists, row
//! mapping, UNIQUE-conflict messaging) used by more than one verb;
//! single-verb helpers live next to their verb. (Child modules may use
//! these private helpers — descendants see their ancestors' items.)

use rusqlite::{params_from_iter, Connection, OptionalExtension};

use super::model::{RuleData, RuleError};
use super::Stage;

mod create;
mod delete;
mod read;
mod reorder;
mod update;

pub use create::insert_rule;
pub use delete::delete_rule;
pub use read::{get, list, load_for_compile};
pub use reorder::move_rule;
pub use update::update_rule;

/// Stages with a `sort_order` column (NOT NULL): the three loop stages
/// plus banking_ops. New rows append at `MAX(sort_order)+1`.
fn has_sort_order(stage: Stage) -> bool {
    matches!(stage, Stage::Prefixes | Stage::Suffixes | Stage::Expansions | Stage::BankingOps)
}

/// Stages whose order is user-controllable via `move` (rule-cli §3.3):
/// the three loop stages only. banking_ops keeps a `sort_order` for a
/// stable dump but is auto-ordered, so it is not movable.
pub fn is_movable(stage: Stage) -> bool {
    matches!(stage, Stage::Prefixes | Stage::Suffixes | Stage::Expansions)
}

/// Data columns for a stage, in the order [`read::map_row`] reads them.
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

/// Map a rusqlite UNIQUE-constraint error to a friendly
/// [`RuleError::Duplicate`]; pass other errors through as anyhow. Shared
/// by `create` and `update`.
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
    let key = conflict_key(data);
    let descr = key.iter().map(|(c, v)| format!("{c} {v:?}")).collect::<Vec<_>>().join(" + ");
    match find_conflict_id(conn, stage, &key) {
        Some(id) => format!("a {} rule with {descr} already exists (#{id})", stage.name()),
        None => format!("a {} rule with {descr} already exists", stage.name()),
    }
}

/// The columns + values of the UNIQUE key a duplicate collides on, so the
/// conflict message and the id lookup are derived from one definition.
fn conflict_key(data: &RuleData) -> Vec<(&'static str, &str)> {
    match data {
        RuleData::Person { canonical, pattern, .. } => {
            vec![("canonical", canonical), ("pattern", pattern)]
        }
        RuleData::BankingOp { operation, pattern, .. } => {
            vec![("operation", operation), ("pattern", pattern)]
        }
        RuleData::Location { location, .. } => vec![("location", location)],
        // Prefix / Suffix / Expansion / Merchant / Employer are UNIQUE on pattern.
        other => vec![("pattern", other.pattern().unwrap_or(""))],
    }
}

fn find_conflict_id(conn: &Connection, stage: Stage, key: &[(&str, &str)]) -> Option<i64> {
    let clause = key
        .iter()
        .enumerate()
        .map(|(i, (col, _))| format!("{col} = ?{}", i + 1))
        .collect::<Vec<_>>()
        .join(" AND ");
    let sql = format!("SELECT id FROM {} WHERE {clause}", stage.table());
    let values = key.iter().map(|(_, v)| *v);
    conn.query_row(&sql, params_from_iter(values), |r| r.get(0)).optional().ok().flatten()
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
