//! Reads: list / get / load_for_compile, plus the row-mapping plumbing
//! they alone use.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

use super::super::model::{LocationKind, Rule, RuleData};
use super::super::Stage;
use super::{data_columns, has_sort_order};

/// List a stage's rules in apply order.
pub fn list(conn: &Connection, stage: Stage) -> Result<Vec<Rule>> {
    let sql = format!("{} ORDER BY {}", select_sql(stage), order_by(stage));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| map_row(stage, row))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    // First-match entity stages are ordered by the shared alphabetical /
    // longer-substring-first comparator (editable-rules-ui §0) for both
    // display and apply, so first-match is correct without a manual
    // sort_order. Ordering is by the **pattern** (the text matched against
    // the payee) so the more specific rule wins — e.g. `JANE CRICKET`
    // before `CRICKET`. Locations have no pattern, so fall back to the
    // location text (canonical). The comparator runs in Rust (SQL can't
    // express the substring tie-break).
    if super::super::is_entity_ordered(stage) {
        let key = |r: &Rule| {
            let raw = r.data.pattern().or_else(|| r.data.canonical()).unwrap_or("");
            super::super::literal_key(raw)
        };
        out.sort_by(|a, b| super::super::entity_cmp(&key(a), &key(b)));
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
    format!("SELECT id, {sort_expr}, {} FROM {}", data_columns(stage).join(", "), stage.table())
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
