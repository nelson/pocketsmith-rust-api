use crate::normalise::{meta, Metadata};
use crate::normalise::rules::CompiledStripRules;
use serde_json::Value;

/// Stage 1: Strip prefixes and suffixes.
/// Returns (cleaned_payee, metadata).
pub fn apply(payee: &str, rules: &CompiledStripRules) -> (String, Metadata) {
    let mut metadata = Metadata::new();
    let mut result = payee.to_uppercase();

    // Apply prefixes (first match wins)
    for rule in &rules.prefixes {
        if let Some((_start, end)) = rule.re.find(&result) {
            metadata.insert(meta::PREFIX_STRIPPED.into(), Value::String(rule.name.clone()));
            if let Some(ref flag) = rule.set_flag {
                metadata.insert(flag.clone(), Value::Bool(true));
            }
            result = result[end..].to_string();
            break;
        }
    }

    // Apply suffixes (all matching — truncate at match start)
    for rule in &rules.suffixes {
        if let Some((start, _end)) = rule.re.find(&result) {
            let stripped = metadata
                .entry(meta::SUFFIXES_STRIPPED.into())
                .or_insert_with(|| Value::Array(Vec::new()));
            if let Value::Array(ref mut arr) = stripped {
                arr.push(Value::String(rule.name.clone()));
            }
            result = result[..start].to_string();
        }
    }

    result = result.trim().to_string();
    (result, metadata)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::rules::CompiledStripRules;
    use std::path::Path;

    fn rules() -> CompiledStripRules {
        CompiledStripRules::load(Path::new("rules")).unwrap()
    }

    #[test]
    fn test_prefix_stripping() {
        let r = rules();
        let (result, meta) = apply("SQ *SOME COFFEE SHOP", &r);
        assert_eq!(result, "SOME COFFEE SHOP");
        assert_eq!(meta["prefix_stripped"], "Square");
    }

    #[test]
    fn test_suffix_stripping() {
        let r = rules();
        let (result, _meta) = apply("WOOLWORTHS Card xx1234 Value Date 01/01/2024", &r);
        assert_eq!(result, "WOOLWORTHS");
    }

    #[test]
    fn test_prefix_and_suffix() {
        let r = rules();
        let (result, meta) = apply("SQ *MY SHOP Card xx5678 Value Date 02/03/2024", &r);
        assert_eq!(result, "MY SHOP");
        assert_eq!(meta["prefix_stripped"], "Square");
    }

    #[test]
    fn test_no_match_uppercases() {
        let r = rules();
        let (result, meta) = apply("Plain Merchant Name", &r);
        assert_eq!(result, "PLAIN MERCHANT NAME");
        assert!(!meta.contains_key("prefix_stripped"));
    }

    #[test]
    fn test_set_flag() {
        let r = rules();
        let (result, meta) = apply("Return SOME STORE Card xx1234 Value Date 01/01/2024", &r);
        assert_eq!(result, "SOME STORE");
        assert_eq!(meta.get("is_return"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_multiple_suffixes_with_repeat() {
        let r = rules();
        let (result, _meta) = apply("ACME PTY LTD NSW", &r);
        assert_eq!(result, "ACME PTY LTD");
        let (result, _meta) = apply(&result, &r);
        assert_eq!(result, "ACME");
    }
}
