//! Small shared helpers: opening the DB, number/text formatting, and
//! building a typed `RuleData` from the parsed flags.

use rusqlite::Connection;

use pocketsmith_sync::db;
use pocketsmith_sync::rules::model::{LocationKind, RuleData, RuleError};
use pocketsmith_sync::rules::validate::StageSchema;
use pocketsmith_sync::rules::Stage;

use crate::args::Flags;
use crate::AppError;

pub(crate) fn open_db() -> Result<Connection, AppError> {
    db::open_app_db().map_err(AppError::from)
}

/// Format cents as "$8.4k" / "$980" (magnitude, one-dp k for ≥ $1000).
pub(crate) fn money(cents: i64) -> String {
    let dollars = cents as f64 / 100.0;
    if dollars >= 1000.0 {
        format!("${:.1}k", dollars / 1000.0)
    } else {
        format!("${:.0}", dollars)
    }
}

/// Group an integer with thousands separators: 1204 → "1,204".
pub(crate) fn commas(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    let bytes = s.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*b as char);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Strip control characters (newlines, tabs, etc.) from a payee and
/// collapse runs of whitespace, so a multi-line bank payee stays on one
/// aligned table row.
pub(crate) fn sanitize(s: &str) -> String {
    let cleaned: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Best-effort created_at/updated_at lookup (not part of the typed Rule).
pub(crate) fn timestamps(
    conn: &Connection,
    stage: Stage,
    id: i64,
) -> (Option<String>, Option<String>) {
    let sql = format!("SELECT created_at, updated_at FROM {} WHERE id = ?1", stage.table());
    conn.query_row(&sql, [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or((None, None))
}

/// Build a `RuleData` for `stage` from the flags, inheriting from `base`
/// (the saved rule, for `edit`) when a value/feature isn't given.
pub(crate) fn build_rule_data(
    stage: Stage,
    flags: &Flags,
    base: Option<&RuleData>,
) -> Result<RuleData, AppError> {
    let schema = StageSchema::for_stage(stage);
    // Reject out-of-schema flags up front (rule-cli §2.2).
    for k in flags.values.keys() {
        if !schema.allows_value(k) {
            return Err(RuleError::UnknownFlag { stage, flag: k.clone() }.into());
        }
    }
    for k in flags.features.keys() {
        if !schema.allows_feature(k) {
            return Err(RuleError::UnknownFlag { stage, flag: format!("has-{k}") }.into());
        }
    }

    // Value getter: explicit flag, else the saved value, else None.
    let val = |name: &str, base_val: Option<&str>| -> Option<String> {
        flags.values.get(name).cloned().or_else(|| base_val.map(|s| s.to_string()))
    };
    let req = |name: &str, base_val: Option<&str>| -> String {
        val(name, base_val).unwrap_or_default()
    };
    // Feature getter: explicit toggle, else inherited (add → false).
    let feat = |name: &str, base_default: bool| -> bool {
        flags.features.get(name).copied().unwrap_or(base_default)
    };

    macro_rules! base_field {
        ($variant:path { $field:ident }) => {
            match base {
                Some($variant { $field, .. }) => Some($field.as_str()),
                _ => None,
            }
        };
    }
    macro_rules! base_opt {
        ($variant:path { $field:ident }) => {
            match base {
                Some($variant { $field: Some(v), .. }) => Some(v.as_str()),
                _ => None,
            }
        };
    }
    macro_rules! base_flag {
        ($variant:path { $field:ident }) => {
            matches!(base, Some($variant { $field: true, .. }))
        };
    }

    let data = match stage {
        Stage::Prefixes => RuleData::Prefix {
            pattern: req("pattern", base_field!(RuleData::Prefix { pattern })),
            gateway: val("gateway", base_opt!(RuleData::Prefix { gateway })),
            operation: val("operation", base_opt!(RuleData::Prefix { operation })),
            has_account: feat("account", base_flag!(RuleData::Prefix { has_account })),
            has_date: feat("date", base_flag!(RuleData::Prefix { has_date })),
            note: val("note", base_opt!(RuleData::Prefix { note })),
        },
        Stage::Suffixes => RuleData::Suffix {
            pattern: req("pattern", base_field!(RuleData::Suffix { pattern })),
            gateway: val("gateway", base_opt!(RuleData::Suffix { gateway })),
            operation: val("operation", base_opt!(RuleData::Suffix { operation })),
            institution: val("institution", base_opt!(RuleData::Suffix { institution })),
            has_account: feat("account", base_flag!(RuleData::Suffix { has_account })),
            has_date: feat("date", base_flag!(RuleData::Suffix { has_date })),
            has_location: feat("location", base_flag!(RuleData::Suffix { has_location })),
            has_currency_code: feat(
                "currency-code",
                base_flag!(RuleData::Suffix { has_currency_code }),
            ),
            has_amount: feat("amount", base_flag!(RuleData::Suffix { has_amount })),
            note: val("note", base_opt!(RuleData::Suffix { note })),
        },
        Stage::Expansions => RuleData::Expansion {
            pattern: req("pattern", base_field!(RuleData::Expansion { pattern })),
            canonical: req("canonical", base_field!(RuleData::Expansion { canonical })),
            note: val("note", base_opt!(RuleData::Expansion { note })),
        },
        Stage::Persons => RuleData::Person {
            canonical: req("canonical", base_field!(RuleData::Person { canonical })),
            pattern: req("pattern", base_field!(RuleData::Person { pattern })),
            note: val("note", base_opt!(RuleData::Person { note })),
        },
        Stage::Employers => RuleData::Employer {
            canonical: req("canonical", base_field!(RuleData::Employer { canonical })),
            pattern: req("pattern", base_field!(RuleData::Employer { pattern })),
            note: val("note", base_opt!(RuleData::Employer { note })),
        },
        Stage::Merchants => RuleData::Merchant {
            canonical: req("canonical", base_field!(RuleData::Merchant { canonical })),
            pattern: req("pattern", base_field!(RuleData::Merchant { pattern })),
            note: val("note", base_opt!(RuleData::Merchant { note })),
        },
        Stage::BankingOps => RuleData::BankingOp {
            operation: req("operation", base_field!(RuleData::BankingOp { operation })),
            pattern: req("pattern", base_field!(RuleData::BankingOp { pattern })),
            has_account: feat("account", base_flag!(RuleData::BankingOp { has_account })),
            note: val("note", base_opt!(RuleData::BankingOp { note })),
        },
        Stage::Locations => {
            let kind_str = val(
                "kind",
                base.and_then(|b| match b {
                    RuleData::Location { kind, .. } => Some(kind.as_str()),
                    _ => None,
                }),
            )
            .unwrap_or_else(|| "location".to_string());
            let kind = LocationKind::from_str(&kind_str)
                .ok_or_else(|| RuleError::BadKind(kind_str.clone()))?;
            RuleData::Location {
                location: req("canonical", base_field!(RuleData::Location { location })),
                kind,
                note: val("note", base_opt!(RuleData::Location { note })),
            }
        }
    };
    Ok(data)
}
