//! Form decoding for the Pipeline editor (editable-rules-ui §3.1).
//!
//! Two pure pieces, both HTTP-free so they unit-test without a server:
//!   * [`parse_urlencoded`] decodes an `application/x-www-form-urlencoded`
//!     request body into a field map.
//!   * [`build_rule_data`] turns that field map into the typed
//!     [`RuleData`] for a stage, mirroring `Stage::dump_columns` /
//!     `crud::data_columns`. The web layer's single source for "form →
//!     rule"; the handlers then hand the `RuleData` to the shared
//!     `validate` → `impact` → `commit` core.

use std::collections::HashMap;

use pocketsmith::rules::model::{LocationKind, RuleData};
use pocketsmith::rules::Stage;

/// A decoded form: field name → value. Repeated keys keep the last value
/// (checkboxes only ever submit once).
pub type Form = HashMap<String, String>;

/// Decode an `application/x-www-form-urlencoded` body into a field map.
/// `+` becomes a space and `%XX` escapes are decoded as UTF-8 bytes;
/// malformed escapes are passed through literally.
pub fn parse_urlencoded(body: &str) -> Form {
    let mut map = HashMap::new();
    for pair in body.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        map.insert(decode_component(k), decode_component(v));
    }
    map
}

/// Percent-decode one component, turning `+` into a space. Bytes are
/// collected then interpreted as UTF-8 (lossy) so multi-byte payees
/// round-trip.
fn decode_component(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = hex_val(bytes[i + 1]);
                let lo = hex_val(bytes[i + 2]);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        out.push((h << 4) | l);
                        i += 3;
                    }
                    // Malformed escape: keep the literal '%'.
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Required text, stored **verbatim** (`""` when absent — `validate_draft`
/// rejects an empty one). Deliberately not trimmed: a loop-stage pattern
/// like `^POS ` relies on its trailing space, so the editor must round-trip
/// the field exactly as typed.
fn text(f: &Form, key: &str) -> String {
    f.get(key).cloned().unwrap_or_default()
}

/// Optional text: `None` when missing or blank (whitespace-only counts as
/// blank), else the raw value verbatim.
fn opt(f: &Form, key: &str) -> Option<String> {
    match f.get(key) {
        Some(s) if !s.trim().is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// A checkbox: true when the field is present with a truthy value. HTML
/// checkboxes submit `on` when checked and are absent when unchecked.
fn flag(f: &Form, key: &str) -> bool {
    matches!(f.get(key).map(|s| s.as_str()), Some("on" | "1" | "true" | "yes"))
}

/// Build the typed [`RuleData`] for `stage` from a decoded form. The
/// field names match the editor inputs (and the DB columns); no
/// validation happens here — that's `validate_draft`'s job.
pub fn build_rule_data(stage: Stage, f: &Form) -> RuleData {
    match stage {
        Stage::Prefixes => RuleData::Prefix {
            pattern: text(f, "pattern"),
            gateway: opt(f, "gateway"),
            operation: opt(f, "operation"),
            has_account: flag(f, "has_account"),
            has_date: flag(f, "has_date"),
            note: opt(f, "note"),
        },
        Stage::Suffixes => RuleData::Suffix {
            pattern: text(f, "pattern"),
            gateway: opt(f, "gateway"),
            operation: opt(f, "operation"),
            institution: opt(f, "institution"),
            has_account: flag(f, "has_account"),
            has_date: flag(f, "has_date"),
            has_location: flag(f, "has_location"),
            has_currency_code: flag(f, "has_currency_code"),
            has_amount: flag(f, "has_amount"),
            note: opt(f, "note"),
        },
        Stage::Expansions => RuleData::Expansion {
            pattern: text(f, "pattern"),
            canonical: text(f, "canonical"),
            note: opt(f, "note"),
        },
        Stage::Persons => RuleData::Person {
            canonical: text(f, "canonical"),
            pattern: text(f, "pattern"),
            note: opt(f, "note"),
        },
        Stage::Employers => RuleData::Employer {
            canonical: text(f, "canonical"),
            pattern: text(f, "pattern"),
            note: opt(f, "note"),
        },
        Stage::Merchants => RuleData::Merchant {
            canonical: text(f, "canonical"),
            pattern: text(f, "pattern"),
            note: opt(f, "note"),
        },
        Stage::BankingOps => RuleData::BankingOp {
            operation: text(f, "operation"),
            pattern: text(f, "pattern"),
            has_account: flag(f, "has_account"),
            note: opt(f, "note"),
        },
        Stage::Locations => RuleData::Location {
            location: text(f, "location"),
            kind: f
                .get("kind")
                .and_then(|s| LocationKind::from_str(s))
                .unwrap_or(LocationKind::Location),
            note: opt(f, "note"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pairs_plus_and_percent() {
        let f = parse_urlencoded("pattern=%28%3Fi%29UBER&canonical=Uber+Eats&note=");
        assert_eq!(f.get("pattern").unwrap(), "(?i)UBER");
        assert_eq!(f.get("canonical").unwrap(), "Uber Eats");
        assert_eq!(f.get("note").unwrap(), "");
    }

    #[test]
    fn empty_body_is_empty_map() {
        assert!(parse_urlencoded("").is_empty());
    }

    #[test]
    fn key_without_value_decodes_to_blank() {
        let f = parse_urlencoded("has_account");
        assert_eq!(f.get("has_account").unwrap(), "");
    }

    #[test]
    fn multibyte_percent_escapes_round_trip() {
        // "Café" → UTF-8 C3 A9 for é.
        let f = parse_urlencoded("canonical=Caf%C3%A9");
        assert_eq!(f.get("canonical").unwrap(), "Café");
    }

    #[test]
    fn malformed_escape_kept_literally() {
        let f = parse_urlencoded("pattern=50%25+off");
        assert_eq!(f.get("pattern").unwrap(), "50% off");
    }

    #[test]
    fn builds_merchant() {
        let f = parse_urlencoded("canonical=Uber&pattern=%28%3Fi%29UBER");
        let d = build_rule_data(Stage::Merchants, &f);
        assert_eq!(
            d,
            RuleData::Merchant {
                canonical: "Uber".into(),
                pattern: "(?i)UBER".into(),
                note: None
            }
        );
    }

    #[test]
    fn builds_prefix_with_flags_and_optional_text() {
        // has_account checked, has_date absent; gateway blank → None.
        let f = parse_urlencoded("pattern=%5EPOS+&gateway=&operation=Purchase&has_account=on");
        let d = build_rule_data(Stage::Prefixes, &f);
        assert_eq!(
            d,
            RuleData::Prefix {
                pattern: "^POS ".into(),
                gateway: None,
                operation: Some("Purchase".into()),
                has_account: true,
                has_date: false,
                note: None,
            }
        );
    }

    #[test]
    fn builds_suffix_all_features() {
        let f = parse_urlencoded(
            "pattern=x&has_account=on&has_date=on&has_location=on&has_currency_code=on&has_amount=on",
        );
        match build_rule_data(Stage::Suffixes, &f) {
            RuleData::Suffix {
                has_account,
                has_date,
                has_location,
                has_currency_code,
                has_amount,
                ..
            } => {
                assert!(has_account && has_date && has_location && has_currency_code && has_amount);
            }
            other => panic!("expected suffix, got {other:?}"),
        }
    }

    #[test]
    fn builds_location_kind_default_and_region() {
        let f = parse_urlencoded("location=Ultimo");
        match build_rule_data(Stage::Locations, &f) {
            RuleData::Location { location, kind, .. } => {
                assert_eq!(location, "Ultimo");
                assert_eq!(kind, LocationKind::Location);
            }
            other => panic!("expected location, got {other:?}"),
        }
        let f = parse_urlencoded("location=NSW&kind=region");
        match build_rule_data(Stage::Locations, &f) {
            RuleData::Location { kind, .. } => assert_eq!(kind, LocationKind::Region),
            other => panic!("expected location, got {other:?}"),
        }
    }
}
