//! Activity-line formatting shared by the CLI committed-change line and
//! the GUI activity log (rule-cli §3.5). Pure: never touches the DB.

use super::model::{Mutation, MoveTarget, Rule, RuleData};
use super::Stage;

/// One-line description of a committed [`Mutation`], using the activity
/// vocabulary (`+ added` / `~ edited` / `− deleted` / `moved`). For
/// edits and deletes the caller passes the `before` rule so the line can
/// show the old → new pattern.
pub struct RuleChange;

impl RuleChange {
    pub fn describe(mutation: &Mutation, before: Option<&Rule>) -> String {
        match mutation {
            Mutation::Add(data) => format!("+ added {}", label_and_pattern(data)),
            Mutation::Edit { data, .. } => {
                let label = primary_label(data);
                match (before.and_then(|b| b.data.pattern()), data.pattern()) {
                    (Some(old), Some(new)) if old != new => {
                        format!("~ edited {label}  {old} → {new}")
                    }
                    _ => format!("~ edited {label}"),
                }
            }
            Mutation::Delete { .. } => match before {
                Some(b) => format!("− deleted {}", label_and_pattern(&b.data)),
                None => "− deleted rule".to_string(),
            },
            Mutation::Move { stage, id, target } => {
                let (dir, anchor) = match target {
                    MoveTarget::Before(a) => ("before", a),
                    MoveTarget::After(a) => ("after", a),
                };
                format!("moved {} #{id} {dir} #{anchor}", singular(*stage))
            }
        }
    }
}

/// "{canonical} {pattern}", or just one of them when the other is absent.
fn label_and_pattern(data: &RuleData) -> String {
    match (data.canonical(), data.pattern()) {
        (Some(c), Some(p)) => format!("{c} {p}"),
        (Some(c), None) => c.to_string(),
        (None, Some(p)) => p.to_string(),
        (None, None) => String::new(),
    }
}

/// The single most identifying string for a rule (canonical, else pattern).
fn primary_label(data: &RuleData) -> String {
    data.canonical().or_else(|| data.pattern()).unwrap_or("").to_string()
}

/// Singular stage noun for move lines ("prefixes" → "prefix").
fn singular(stage: Stage) -> &'static str {
    match stage {
        Stage::Prefixes => "prefix",
        Stage::Suffixes => "suffix",
        Stage::Expansions => "expansion",
        Stage::Persons => "person",
        Stage::Employers => "employer",
        Stage::Merchants => "merchant",
        Stage::BankingOps => "banking_op",
        Stage::Locations => "location",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::model::Rule;

    fn merchant(canonical: &str, pattern: &str) -> RuleData {
        RuleData::Merchant { canonical: canonical.into(), pattern: pattern.into(), note: None }
    }

    #[test]
    fn add_line() {
        let m = Mutation::Add(merchant("Bunnings", "(?i)BUNNINGS"));
        assert_eq!(RuleChange::describe(&m, None), "+ added Bunnings (?i)BUNNINGS");
    }

    #[test]
    fn edit_line_shows_pattern_diff() {
        let before = Rule { id: 9, sort_order: None, data: merchant("Uber", "(?i)UBER") };
        let m = Mutation::Edit { id: 9, data: merchant("Uber", "(?i)UBER TRIP") };
        assert_eq!(
            RuleChange::describe(&m, Some(&before)),
            "~ edited Uber  (?i)UBER → (?i)UBER TRIP"
        );
    }

    #[test]
    fn edit_line_without_pattern_change() {
        let before = Rule { id: 9, sort_order: None, data: merchant("Uber", "(?i)UBER") };
        let m = Mutation::Edit { id: 9, data: merchant("Uber Inc", "(?i)UBER") };
        assert_eq!(RuleChange::describe(&m, Some(&before)), "~ edited Uber Inc");
    }

    #[test]
    fn delete_line() {
        let before = Rule { id: 4, sort_order: None, data: merchant("Amazon", "(?i)AMAZON") };
        let m = Mutation::Delete { stage: Stage::Merchants, id: 4 };
        assert_eq!(RuleChange::describe(&m, Some(&before)), "− deleted Amazon (?i)AMAZON");
    }

    #[test]
    fn move_line() {
        let m = Mutation::Move { stage: Stage::Prefixes, id: 7, target: MoveTarget::Before(3) };
        assert_eq!(RuleChange::describe(&m, None), "moved prefix #7 before #3");
    }
}
