use crate::normalise::{meta, Metadata};
use crate::normalise::rules::CompiledClassifyRules;
use serde_json::Value;

/// Stage 2: Type classification.
/// Classifies the payee based on the ORIGINAL payee (before stripping)
/// for reliable pattern matching. First match wins; default is "merchant".
pub fn apply(original_payee: &str, metadata: &mut Metadata, rules: &CompiledClassifyRules) {
    for rule in &rules.rules {
        if rule.re.is_match(original_payee) {
            metadata.insert(meta::TYPE.into(), Value::String(rule.payee_type.clone()));

            if let Some(ref extraction) = rule.extraction {
                if let Some(caps) = extraction.re.captures(original_payee) {
                    if let Some(m) = caps.get(1) {
                        let entity = m.as_str().trim();
                        if !entity.is_empty() {
                            metadata.insert(
                                meta::EXTRACTED_ENTITY.into(),
                                Value::String(entity.to_string()),
                            );
                            metadata.insert(
                                meta::EXTRACT_KIND.into(),
                                Value::String(extraction.kind.clone()),
                            );
                        }
                    }
                }
            }

            return;
        }
    }

    metadata.insert(meta::TYPE.into(), Value::String("merchant".into()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::rules::CompiledClassifyRules;
    use std::path::Path;

    fn rules() -> CompiledClassifyRules {
        CompiledClassifyRules::load(Path::new("rules")).unwrap()
    }

    #[test]
    fn test_salary_from() {
        let r = rules();
        let mut meta = Metadata::new();
        apply("Salary from APPLE PTY LTD", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "salary");
        assert_eq!(meta[meta::EXTRACTED_ENTITY], "APPLE PTY LTD");
        assert_eq!(meta[meta::EXTRACT_KIND], "employer");
    }

    #[test]
    fn test_salary_from_with_suffix() {
        let r = rules();
        let mut meta = Metadata::new();
        apply("Salary from APPLE PTY LTD - Salary", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "salary");
        assert_eq!(meta[meta::EXTRACTED_ENTITY], "APPLE PTY LTD");
    }

    #[test]
    fn test_osko_incoming() {
        let r = rules();
        let mut meta = Metadata::new();
        apply("John Smith 12345678 - Osko Payment - Receipt 12345", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "transfer_in");
        assert_eq!(meta[meta::EXTRACTED_ENTITY], "John Smith");
        assert_eq!(meta[meta::EXTRACT_KIND], "person");
    }

    #[test]
    fn test_osko_outgoing() {
        let r = rules();
        let mut meta = Metadata::new();
        apply("John Smith - Osko Payment to 12345678", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "transfer_out");
        assert_eq!(meta[meta::EXTRACTED_ENTITY], "John Smith");
    }

    #[test]
    fn test_transfer_in() {
        let r = rules();
        let mut meta = Metadata::new();
        apply("Fast Transfer From Jane Doe, BSB 123-456", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "transfer_in");
        assert_eq!(meta[meta::EXTRACTED_ENTITY], "Jane Doe");
    }

    #[test]
    fn test_transfer_out() {
        let r = rules();
        let mut meta = Metadata::new();
        apply("Transfer To Savings CommBank App", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "transfer_out");
        assert_eq!(meta[meta::EXTRACTED_ENTITY], "Savings");
    }

    #[test]
    fn test_banking_operation_direct_credit() {
        let r = rules();
        let mut meta = Metadata::new();
        apply("Direct Credit 12345 ACME CORP", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "banking_operation");
        // Non-greedy extraction stops at first whitespace due to optional (?:[\s,].*)? suffix
        assert_eq!(meta[meta::EXTRACTED_ENTITY], "ACME");
    }

    #[test]
    fn test_banking_operation_no_extract() {
        let r = rules();
        let mut meta = Metadata::new();
        apply("Wdl ATM CBA Sydney", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "banking_operation");
        assert!(!meta.contains_key(meta::EXTRACTED_ENTITY));
    }

    #[test]
    fn test_default_merchant() {
        let r = rules();
        let mut meta = Metadata::new();
        apply("WOOLWORTHS 1234 STRATHFIELD", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "merchant");
        assert!(!meta.contains_key(meta::EXTRACTED_ENTITY));
    }

    #[test]
    fn test_bpay() {
        let r = rules();
        let mut meta = Metadata::new();
        apply("BPAY PAYMENT TO AGL ENERGY", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "banking_operation");
    }

    #[test]
    fn test_preserves_existing_metadata() {
        let r = rules();
        let mut meta = Metadata::new();
        meta.insert(meta::PREFIX_STRIPPED.into(), Value::String("Square".into()));
        apply("WOOLWORTHS 1234", &mut meta, &r);
        assert_eq!(meta[meta::TYPE], "merchant");
        assert_eq!(meta[meta::PREFIX_STRIPPED], "Square");
    }
}
