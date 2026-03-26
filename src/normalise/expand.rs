use crate::normalise::{meta, Metadata};
use crate::normalise::rules::CompiledExpandRules;
use serde_json::Value;

/// Stage 3: Expand truncated words in payee.
/// Returns the expanded payee string, updating metadata with expansions and detected location.
pub fn apply(payee: &str, metadata: &mut Metadata, rules: &CompiledExpandRules) -> String {
    let mut result = payee.to_string();
    let mut expansions: Vec<String> = Vec::new();

    for pat in &rules.patterns {
        if pat.re.is_match(&result) {
            result = pat.re.replace_all(&result, pat.replacement.as_str()).into_owned();
            expansions.push(format!("{}->{}", pat.truncated, pat.replacement));
        }
    }

    if !expansions.is_empty() {
        metadata.insert(
            meta::TRUNCATIONS_EXPANDED.into(),
            Value::Array(expansions.into_iter().map(Value::String).collect()),
        );
    }

    // Detect known locations in the (possibly expanded) payee
    for loc in &rules.locations {
        if loc.re.is_match(&result) {
            metadata.insert(meta::DETECTED_LOCATION.into(), Value::String(loc.name.clone()));
            break;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::rules::CompiledExpandRules;
    use std::path::Path;

    fn rules() -> CompiledExpandRules {
        CompiledExpandRules::load(Path::new("rules")).unwrap()
    }

    #[test]
    fn test_suburb_expansion() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("WOOLWORTHS STRATHFIEL", &mut meta, &r);
        assert_eq!(result, "WOOLWORTHS STRATHFIELD");
        assert!(meta.contains_key(meta::TRUNCATIONS_EXPANDED));
    }

    #[test]
    fn test_word_expansion() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("ACME PHARMCY", &mut meta, &r);
        assert_eq!(result, "ACME PHARMACY");
    }

    #[test]
    fn test_merchant_expansion() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("STARBUC STRATHFIEL", &mut meta, &r);
        assert_eq!(result, "STARBUCKS STRATHFIELD");
        let expansions = meta[meta::TRUNCATIONS_EXPANDED].as_array().unwrap();
        assert_eq!(expansions.len(), 2);
    }

    #[test]
    fn test_no_expansion() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("WOOLWORTHS STRATHFIELD", &mut meta, &r);
        assert_eq!(result, "WOOLWORTHS STRATHFIELD");
        assert!(!meta.contains_key(meta::TRUNCATIONS_EXPANDED));
    }

    #[test]
    fn test_location_detection() {
        let r = rules();
        let mut meta = Metadata::new();
        let _result = apply("WOOLWORTHS STRATHFIELD", &mut meta, &r);
        assert_eq!(meta[meta::DETECTED_LOCATION], "Strathfield");
    }

    #[test]
    fn test_location_detection_after_expansion() {
        let r = rules();
        let mut meta = Metadata::new();
        let _result = apply("WOOLWORTHS STRATHFIEL", &mut meta, &r);
        // Location should be detected after expansion
        assert_eq!(meta[meta::DETECTED_LOCATION], "Strathfield");
    }

    #[test]
    fn test_longest_match_first() {
        let r = rules();
        let mut meta = Metadata::new();
        // "NORTH STRATH" should expand to "NORTH STRATHFIELD", not "NORTH STRATHFIELD"
        // (i.e., the multi-word "NORTH STRATH" pattern should match before single "STRATH")
        let result = apply("WOOLWORTHS NORTH STRATH", &mut meta, &r);
        assert_eq!(result, "WOOLWORTHS NORTH STRATHFIELD");
        assert_eq!(meta[meta::DETECTED_LOCATION], "North Strathfield");
    }

    #[test]
    fn test_preserves_existing_metadata() {
        let r = rules();
        let mut meta = Metadata::new();
        meta.insert("type".into(), Value::String("merchant".into()));
        let _result = apply("WOOLWORTHS STRATHFIELD", &mut meta, &r);
        assert_eq!(meta["type"], "merchant");
    }
}
