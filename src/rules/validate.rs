//! One validator + stage schema, shared by the CLI and the GUI
//! (rule-cli §3.2). [`validate_draft`] is the single place rule
//! semantics are checked; [`StageSchema`] declares which field flags a
//! stage accepts and which feature → capture-group conditions apply.

use regex::Regex;

use super::model::{RuleData, RuleError};
use super::Stage;
use crate::normalise::BankingOperation;

/// A `has_*` feature and the named capture group its pattern must
/// contain when the feature is on (rule-cli §2.2).
pub struct FeatureSpec {
    /// CLI flag stem without the `has-` prefix, e.g. `account` for
    /// `--has-account`. Used in error messages and arg parsing.
    pub flag: &'static str,
    /// Required `(?P<group>…)` name in the pattern.
    pub group: &'static str,
}

/// Static descriptor of a stage's editable surface: the value flags it
/// accepts and the feature → capture-group conditions it enforces.
/// Shared verbatim by the CLI arg-parser and the GUI form.
pub struct StageSchema {
    pub stage: Stage,
    /// Value flags (without leading `--`) valid for this stage.
    pub value_flags: &'static [&'static str],
    /// Feature toggles valid for this stage.
    pub features: &'static [FeatureSpec],
}

const ACCOUNT: FeatureSpec = FeatureSpec { flag: "account", group: "account" };
const DATE: FeatureSpec = FeatureSpec { flag: "date", group: "date" };
const LOCATION: FeatureSpec = FeatureSpec { flag: "location", group: "location" };
const CURRENCY: FeatureSpec = FeatureSpec { flag: "currency-code", group: "currency_code" };
const AMOUNT: FeatureSpec = FeatureSpec { flag: "amount", group: "amount_in_cents" };

impl StageSchema {
    /// The schema for `stage`.
    pub fn for_stage(stage: Stage) -> StageSchema {
        match stage {
            Stage::Prefixes => StageSchema {
                stage,
                value_flags: &["pattern", "gateway", "operation", "note"],
                features: &[ACCOUNT, DATE],
            },
            Stage::Suffixes => StageSchema {
                stage,
                value_flags: &["pattern", "gateway", "operation", "institution", "note"],
                features: &[ACCOUNT, DATE, LOCATION, CURRENCY, AMOUNT],
            },
            Stage::Expansions => StageSchema {
                stage,
                value_flags: &["pattern", "canonical", "note"],
                features: &[],
            },
            Stage::Persons | Stage::Employers | Stage::Merchants => StageSchema {
                stage,
                value_flags: &["pattern", "canonical", "note"],
                features: &[],
            },
            Stage::BankingOps => StageSchema {
                stage,
                value_flags: &["pattern", "operation", "note"],
                features: &[ACCOUNT],
            },
            // Locations carry their text in `--canonical` and the
            // location/region split in `--kind` (rule-cli §2.2 flag set).
            Stage::Locations => StageSchema {
                stage,
                value_flags: &["canonical", "kind", "note"],
                features: &[],
            },
        }
    }

    /// Whether `flag` (a value or feature flag, without `--`/`--has-`)
    /// is accepted by this stage.
    pub fn allows_value(&self, flag: &str) -> bool {
        self.value_flags.contains(&flag)
    }

    /// Whether `feature` (e.g. `account`) is a valid feature here.
    pub fn allows_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f.flag == feature)
    }
}

/// Validate a draft rule the same way both shells need (rule-cli §3.2):
/// regex compiles, required text non-empty, operation/kind parse, and
/// every enabled `has_*` feature has its named capture group.
pub fn validate_draft(data: &RuleData) -> Result<(), RuleError> {
    match data {
        RuleData::Prefix { pattern, operation, has_account, has_date, .. } => {
            non_empty("pattern", pattern)?;
            let re = compile_raw(pattern)?;
            check_operation_opt(operation)?;
            require_capture(*has_account, &re, pattern, &ACCOUNT)?;
            require_capture(*has_date, &re, pattern, &DATE)?;
        }
        RuleData::Suffix {
            pattern,
            operation,
            has_account,
            has_date,
            has_location,
            has_currency_code,
            has_amount,
            ..
        } => {
            non_empty("pattern", pattern)?;
            let re = compile_raw(pattern)?;
            check_operation_opt(operation)?;
            require_capture(*has_account, &re, pattern, &ACCOUNT)?;
            require_capture(*has_date, &re, pattern, &DATE)?;
            require_capture(*has_location, &re, pattern, &LOCATION)?;
            require_capture(*has_currency_code, &re, pattern, &CURRENCY)?;
            require_capture(*has_amount, &re, pattern, &AMOUNT)?;
        }
        RuleData::Expansion { pattern, canonical, .. } => {
            non_empty("pattern", pattern)?;
            non_empty("canonical", canonical)?;
            // Expansion patterns are escaped to literals at compile time,
            // so any non-empty string is valid — nothing more to check.
        }
        RuleData::Person { canonical, pattern, .. }
        | RuleData::Employer { canonical, pattern, .. } => {
            non_empty("canonical", canonical)?;
            non_empty("pattern", pattern)?;
            // Person/employer patterns are escaped literals; always valid.
        }
        RuleData::Merchant { canonical, pattern, .. } => {
            non_empty("canonical", canonical)?;
            non_empty("pattern", pattern)?;
            compile_raw(pattern)?; // merchants compile the raw regex
        }
        RuleData::BankingOp { operation, pattern, has_account, .. } => {
            non_empty("operation", operation)?;
            non_empty("pattern", pattern)?;
            let re = compile_raw(pattern)?;
            check_operation_required(operation)?;
            require_capture(*has_account, &re, pattern, &ACCOUNT)?;
        }
        RuleData::Location { location, .. } => {
            non_empty("location", location)?;
            // `kind` is already a typed LocationKind, so it can't be bad.
        }
    }
    Ok(())
}

fn non_empty(field: &'static str, v: &str) -> Result<(), RuleError> {
    if v.trim().is_empty() {
        Err(RuleError::Missing(field))
    } else {
        Ok(())
    }
}

/// Compile a raw-regex stage's pattern, mapping a syntax error to
/// [`RuleError::BadRegex`].
fn compile_raw(pattern: &str) -> Result<Regex, RuleError> {
    Regex::new(pattern).map_err(|e| RuleError::BadRegex {
        pattern: pattern.to_string(),
        msg: e.to_string(),
    })
}

fn check_operation_opt(op: &Option<String>) -> Result<(), RuleError> {
    if let Some(op) = op {
        if BankingOperation::from_display_name(op).is_none() {
            return Err(RuleError::BadOperation(op.clone()));
        }
    }
    Ok(())
}

fn check_operation_required(op: &str) -> Result<(), RuleError> {
    if BankingOperation::from_display_name(op).is_none() {
        Err(RuleError::BadOperation(op.to_string()))
    } else {
        Ok(())
    }
}

/// If `enabled`, the compiled regex must declare the feature's named
/// capture group, else [`RuleError::MissingCapture`].
fn require_capture(
    enabled: bool,
    re: &Regex,
    pattern: &str,
    spec: &FeatureSpec,
) -> Result<(), RuleError> {
    if !enabled {
        return Ok(());
    }
    let has_group = re.capture_names().flatten().any(|n| n == spec.group);
    if has_group {
        Ok(())
    } else {
        Err(RuleError::MissingCapture {
            feature: leak_feature_flag(spec.flag),
            group: spec.group,
            pattern: pattern.to_string(),
        })
    }
}

/// `MissingCapture.feature` is `&'static str` and is the *has-* flag the
/// user typed (e.g. `has-account`). The `FeatureSpec.flag` stems are
/// static, so we can map them to their static `has-…` form without
/// allocation.
fn leak_feature_flag(flag: &'static str) -> &'static str {
    match flag {
        "account" => "has-account",
        "date" => "has-date",
        "location" => "has-location",
        "currency-code" => "has-currency-code",
        "amount" => "has-amount",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::model::LocationKind;

    fn prefix(pattern: &str, has_account: bool, has_date: bool) -> RuleData {
        RuleData::Prefix {
            pattern: pattern.into(),
            gateway: None,
            operation: None,
            has_account,
            has_date,
            note: None,
        }
    }

    #[test]
    fn good_merchant_passes() {
        let d = RuleData::Merchant {
            canonical: "Uber".into(),
            pattern: "(?i)UBER".into(),
            note: None,
        };
        assert!(validate_draft(&d).is_ok());
    }

    #[test]
    fn empty_canonical_is_missing() {
        let d = RuleData::Merchant {
            canonical: "  ".into(),
            pattern: "(?i)UBER".into(),
            note: None,
        };
        assert_eq!(validate_draft(&d), Err(RuleError::Missing("canonical")));
    }

    #[test]
    fn bad_regex_is_reported() {
        let d = RuleData::Merchant {
            canonical: "Uber".into(),
            pattern: "(?i)UBER(".into(),
            note: None,
        };
        match validate_draft(&d) {
            Err(RuleError::BadRegex { pattern, .. }) => assert_eq!(pattern, "(?i)UBER("),
            other => panic!("expected BadRegex, got {other:?}"),
        }
    }

    #[test]
    fn has_account_requires_named_group() {
        // Without the group → MissingCapture naming has-account.
        let bad = prefix(r"^DIRECT (CREDIT|DEBIT)", true, false);
        assert_eq!(
            validate_draft(&bad),
            Err(RuleError::MissingCapture {
                feature: "has-account",
                group: "account",
                pattern: r"^DIRECT (CREDIT|DEBIT)".into(),
            })
        );
        // With the group → ok.
        let good = prefix(r"^POS (?P<account>\d+) ", true, false);
        assert!(validate_draft(&good).is_ok());
    }

    #[test]
    fn has_date_requires_named_group() {
        let bad = prefix(r"^(\d+) ", false, true);
        assert!(matches!(
            validate_draft(&bad),
            Err(RuleError::MissingCapture { feature: "has-date", .. })
        ));
        let good = prefix(r"^(?P<date>\d{2}/\d{2}) ", false, true);
        assert!(validate_draft(&good).is_ok());
    }

    #[test]
    fn suffix_features_each_check_their_group() {
        // amount feature without (?P<amount_in_cents>…)
        let d = RuleData::Suffix {
            pattern: r"\s+AUD$".into(),
            gateway: None,
            operation: None,
            institution: None,
            has_account: false,
            has_date: false,
            has_location: false,
            has_currency_code: false,
            has_amount: true,
            note: None,
        };
        assert!(matches!(
            validate_draft(&d),
            Err(RuleError::MissingCapture { group: "amount_in_cents", .. })
        ));
    }

    #[test]
    fn banking_op_operation_must_parse() {
        let d = RuleData::BankingOp {
            operation: "Not An Op".into(),
            pattern: "(?i)INTEREST".into(),
            has_account: false,
            note: None,
        };
        assert_eq!(validate_draft(&d), Err(RuleError::BadOperation("Not An Op".into())));
    }

    #[test]
    fn prefix_optional_operation_must_parse_when_present() {
        let d = RuleData::Prefix {
            pattern: "^X ".into(),
            gateway: None,
            operation: Some("Bogus".into()),
            has_account: false,
            has_date: false,
            note: None,
        };
        assert_eq!(validate_draft(&d), Err(RuleError::BadOperation("Bogus".into())));
    }

    #[test]
    fn location_only_needs_nonempty_text() {
        let d = RuleData::Location {
            location: "Ultimo".into(),
            kind: LocationKind::Location,
            note: None,
        };
        assert!(validate_draft(&d).is_ok());
        let empty = RuleData::Location {
            location: "".into(),
            kind: LocationKind::Region,
            note: None,
        };
        assert_eq!(validate_draft(&empty), Err(RuleError::Missing("location")));
    }

    #[test]
    fn schema_flag_membership() {
        let s = StageSchema::for_stage(Stage::Suffixes);
        assert!(s.allows_value("institution"));
        assert!(!s.allows_value("canonical"));
        assert!(s.allows_feature("amount"));
        assert!(!s.allows_feature("xyz"));

        let p = StageSchema::for_stage(Stage::Prefixes);
        assert!(p.allows_feature("account"));
        assert!(!p.allows_feature("location"));
    }
}
