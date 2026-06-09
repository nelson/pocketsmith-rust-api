//! The single authority on rule data structures (rule-cli §3.1).
//!
//! [`RuleData`] is the editable payload of a rule — one variant per
//! pipeline [`Stage`]. The DB columns in `db/schema.rs` and
//! [`Stage::dump_columns`](super::Stage) mirror *this*, not the reverse.
//! Both the CLI (`src/bin/rule.rs`) and the future editor GUI build a
//! [`Mutation`] over `RuleData` and hand it to the shared library
//! (`validate` → `impact` → `commit`); neither shell re-declares a
//! rule's fields.

use super::Stage;

/// How a location rule is used: a suburb/city (`Location`) or a
/// country/state region (`Region`). Replaces the stringly-typed `kind`
/// column match in `normalise::locations`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationKind {
    Location,
    Region,
}

impl LocationKind {
    /// The stored `kind` string (`rule_locations.kind`).
    pub fn as_str(&self) -> &'static str {
        match self {
            LocationKind::Location => "location",
            LocationKind::Region => "region",
        }
    }

    /// Parse a stored `kind` string; `None` for anything unrecognised.
    pub fn from_str(s: &str) -> Option<LocationKind> {
        match s {
            "location" => Some(LocationKind::Location),
            "region" => Some(LocationKind::Region),
            _ => None,
        }
    }
}

/// The editable payload of one rule, one variant per [`Stage`]. The
/// `has_*` booleans are feature toggles (see rule-cli §2.2); the
/// capture-extracting ones require a matching named group in `pattern`,
/// enforced by [`validate`](super::validate).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleData {
    Prefix {
        pattern: String,
        gateway: Option<String>,
        operation: Option<String>,
        has_account: bool,
        has_date: bool,
        note: Option<String>,
    },
    Suffix {
        pattern: String,
        gateway: Option<String>,
        operation: Option<String>,
        institution: Option<String>,
        has_account: bool,
        has_date: bool,
        has_location: bool,
        has_currency_code: bool,
        has_amount: bool,
        note: Option<String>,
    },
    Expansion {
        pattern: String,
        canonical: String,
        note: Option<String>,
    },
    Person {
        canonical: String,
        pattern: String,
        note: Option<String>,
    },
    Employer {
        canonical: String,
        pattern: String,
        note: Option<String>,
    },
    Merchant {
        canonical: String,
        pattern: String,
        note: Option<String>,
    },
    BankingOp {
        operation: String,
        pattern: String,
        has_account: bool,
        note: Option<String>,
    },
    Location {
        location: String,
        kind: LocationKind,
        note: Option<String>,
    },
}

impl RuleData {
    /// Which [`Stage`] this payload belongs to.
    pub fn stage(&self) -> Stage {
        match self {
            RuleData::Prefix { .. } => Stage::Prefixes,
            RuleData::Suffix { .. } => Stage::Suffixes,
            RuleData::Expansion { .. } => Stage::Expansions,
            RuleData::Person { .. } => Stage::Persons,
            RuleData::Employer { .. } => Stage::Employers,
            RuleData::Merchant { .. } => Stage::Merchants,
            RuleData::BankingOp { .. } => Stage::BankingOps,
            RuleData::Location { .. } => Stage::Locations,
        }
    }

    /// The rule's regex/pattern source where it has one. `None` only for
    /// locations (which match on `location` text, not a pattern).
    pub fn pattern(&self) -> Option<&str> {
        match self {
            RuleData::Prefix { pattern, .. }
            | RuleData::Suffix { pattern, .. }
            | RuleData::Expansion { pattern, .. }
            | RuleData::Person { pattern, .. }
            | RuleData::Employer { pattern, .. }
            | RuleData::Merchant { pattern, .. }
            | RuleData::BankingOp { pattern, .. } => Some(pattern),
            RuleData::Location { .. } => None,
        }
    }

    /// The rule's canonical / display string where it has one: the
    /// canonical name (entity stages), the operation (banking_ops), or
    /// the location text. Used for activity lines and list rendering.
    pub fn canonical(&self) -> Option<&str> {
        match self {
            RuleData::Expansion { canonical, .. }
            | RuleData::Person { canonical, .. }
            | RuleData::Employer { canonical, .. }
            | RuleData::Merchant { canonical, .. } => Some(canonical),
            RuleData::BankingOp { operation, .. } => Some(operation),
            RuleData::Location { location, .. } => Some(location),
            RuleData::Prefix { .. } | RuleData::Suffix { .. } => None,
        }
    }

    /// The rule's free-text note, if any.
    pub fn note(&self) -> Option<&str> {
        match self {
            RuleData::Prefix { note, .. }
            | RuleData::Suffix { note, .. }
            | RuleData::Expansion { note, .. }
            | RuleData::Person { note, .. }
            | RuleData::Employer { note, .. }
            | RuleData::Merchant { note, .. }
            | RuleData::BankingOp { note, .. }
            | RuleData::Location { note, .. } => note.as_deref(),
        }
    }
}

/// A saved rule as read back from the DB: the typed [`RuleData`] plus its
/// row id and (for loop stages) its apply-order position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: i64,
    pub sort_order: Option<i64>,
    pub data: RuleData,
}

/// Where to drop a moved loop-stage rule: relative to a neighbour, never
/// an absolute slot (rule-cli §3.3). Maps onto the CLI's
/// `--before <id>` / `--after <id>` and the GUI's `Alt+↑/↓` + drag-drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveTarget {
    Before(i64),
    After(i64),
}

impl MoveTarget {
    /// The anchor rule id this move is relative to.
    pub fn anchor(&self) -> i64 {
        match self {
            MoveTarget::Before(id) | MoveTarget::After(id) => *id,
        }
    }
}

/// A single rule change. There is exactly one per [`commit`](super::commit)
/// — rule edits are singular and atomic (rule-cli §3.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mutation {
    Add(RuleData),
    Edit { id: i64, data: RuleData },
    Delete { stage: Stage, id: i64 },
    Move { stage: Stage, id: i64, target: MoveTarget },
}

impl Mutation {
    /// The [`Stage`] this mutation operates on.
    pub fn stage(&self) -> Stage {
        match self {
            Mutation::Add(data) => data.stage(),
            Mutation::Edit { data, .. } => data.stage(),
            Mutation::Delete { stage, .. } => *stage,
            Mutation::Move { stage, .. } => *stage,
        }
    }
}

/// One typed error set shared by the CLI and the GUI (rule-cli §3.2,
/// §7). [`exit_code`](Self::exit_code) drives the scriptable exit-code
/// contract; [`is_syntax`](Self::is_syntax) selects the `syntax error:`
/// vs `error:` prefix in the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleError {
    /// The `pattern` failed to compile as a regex.
    BadRegex { pattern: String, msg: String },
    /// A required text field was empty.
    Missing(&'static str),
    /// `operation` didn't parse to a known `BankingOperation`.
    BadOperation(String),
    /// `kind` wasn't one of `location` / `region`.
    BadKind(String),
    /// A `has_*` feature is on but its named capture group is absent.
    MissingCapture { feature: &'static str, group: &'static str, pattern: String },
    /// A `UNIQUE` constraint was violated on commit.
    Duplicate(String),
    /// No rule with that id in that stage.
    NotFound { stage: Stage, id: i64 },
    /// `move` attempted on a stage with no manual order.
    NotOrdered(Stage),
    /// `move` anchor lives in a different stage than the moved rule.
    CrossStage,
    /// `--stage` named an unknown stage.
    UnknownStage(String),
    /// A field flag isn't valid for this stage.
    UnknownFlag { stage: Stage, flag: String },
}

impl RuleError {
    /// `true` for input that can't even be evaluated (regex syntax, bad
    /// operation/kind, missing capture group, unknown stage) → exit 2.
    /// Everything else is a usage / not-found / duplicate error → exit 1.
    pub fn is_syntax(&self) -> bool {
        matches!(
            self,
            RuleError::BadRegex { .. }
                | RuleError::BadOperation(_)
                | RuleError::BadKind(_)
                | RuleError::MissingCapture { .. }
                | RuleError::UnknownStage(_)
        )
    }

    /// Exit code for the scriptable contract (rule-cli §7).
    pub fn exit_code(&self) -> i32 {
        if self.is_syntax() {
            2
        } else {
            1
        }
    }
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleError::BadRegex { pattern, msg } => {
                write!(f, "{msg} (pattern {pattern:?})")
            }
            RuleError::Missing(field) => write!(f, "{field} is required and must not be empty"),
            RuleError::BadOperation(op) => write!(f, "unknown --operation {op:?}"),
            RuleError::BadKind(kind) => {
                write!(f, "--kind must be 'location' or 'region', got {kind:?}")
            }
            RuleError::MissingCapture { feature, group, pattern } => write!(
                f,
                "--{feature} requires the pattern to capture a named group \
                 (?P<{group}>...), but {pattern:?} has none."
            ),
            RuleError::Duplicate(msg) => write!(f, "{msg}"),
            RuleError::NotFound { stage, id } => {
                write!(f, "no {} rule with id {id}", stage.name())
            }
            RuleError::NotOrdered(stage) => write!(
                f,
                "the {} stage is auto-ordered; only loop stages \
                 (prefixes/suffixes/expansions) can be moved",
                stage.name()
            ),
            RuleError::CrossStage => {
                write!(f, "the move anchor must be in the same stage as the moved rule")
            }
            RuleError::UnknownStage(s) => write!(f, "unknown --stage {s:?}"),
            RuleError::UnknownFlag { stage, flag } => {
                write!(f, "--{flag} is not a valid field for the {} stage", stage.name())
            }
        }
    }
}

impl std::error::Error for RuleError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_maps_each_variant() {
        let m = RuleData::Merchant {
            canonical: "Uber".into(),
            pattern: "(?i)UBER".into(),
            note: None,
        };
        assert_eq!(m.stage(), Stage::Merchants);
        assert_eq!(m.pattern(), Some("(?i)UBER"));
        assert_eq!(m.canonical(), Some("Uber"));
        assert_eq!(m.note(), None);

        let loc = RuleData::Location {
            location: "Ultimo".into(),
            kind: LocationKind::Location,
            note: Some("inner west".into()),
        };
        assert_eq!(loc.stage(), Stage::Locations);
        assert_eq!(loc.pattern(), None);
        assert_eq!(loc.canonical(), Some("Ultimo"));
        assert_eq!(loc.note(), Some("inner west"));
    }

    #[test]
    fn location_kind_round_trips() {
        for k in [LocationKind::Location, LocationKind::Region] {
            assert_eq!(LocationKind::from_str(k.as_str()), Some(k));
        }
        assert_eq!(LocationKind::from_str("nope"), None);
    }

    #[test]
    fn error_exit_codes_match_the_contract() {
        assert_eq!(
            RuleError::BadRegex { pattern: "(".into(), msg: "x".into() }.exit_code(),
            2
        );
        assert_eq!(RuleError::UnknownStage("x".into()).exit_code(), 2);
        assert_eq!(RuleError::NotFound { stage: Stage::Merchants, id: 1 }.exit_code(), 1);
        assert_eq!(RuleError::Duplicate("dup".into()).exit_code(), 1);
    }
}
