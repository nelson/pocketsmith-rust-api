use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::normalise::rules::{compile_icase, load_yaml, Re};

use super::{confidence, CategoriseResult, CategoriseSource};

#[derive(Deserialize)]
struct CategoriseRulesYaml {
    #[serde(default)]
    type_rules: HashMap<String, String>,
    #[serde(default)]
    payee_overrides: Vec<PayeeOverrideYaml>,
    #[serde(default)]
    google_places_mappings: HashMap<String, String>,
}

#[derive(Deserialize)]
struct PayeeOverrideYaml {
    pattern: String,
    category: String,
}

pub struct CompiledPayeeOverride {
    pub re: Re,
    pub category: String,
}

pub struct CategoriseRules {
    pub type_rules: HashMap<String, String>,
    pub payee_overrides: Vec<CompiledPayeeOverride>,
    pub google_places_mappings: HashMap<String, String>,
}

impl CategoriseRules {
    pub fn load(rules_dir: &Path) -> Result<Self> {
        let yaml: CategoriseRulesYaml = load_yaml(rules_dir, "categorise.yaml")?;
        let overrides = yaml
            .payee_overrides
            .into_iter()
            .map(|o| {
                Ok(CompiledPayeeOverride {
                    re: compile_icase(&o.pattern, "categorise payee override")?,
                    category: o.category,
                })
            })
            .collect::<Result<_>>()?;
        Ok(Self {
            type_rules: yaml.type_rules,
            payee_overrides: overrides,
            google_places_mappings: yaml.google_places_mappings,
        })
    }
}

/// Try rule-based categorisation using transaction type and payee overrides.
/// Returns Some(CategoriseResult) if a rule matched, None if it falls through.
pub fn try_rules(
    normalised_payee: &str,
    txn_type: Option<&str>,
    transaction_count: usize,
    rules: &CategoriseRules,
) -> Option<CategoriseResult> {
    // 1. Check payee overrides first (most specific)
    for ovr in &rules.payee_overrides {
        if ovr.re.is_match(normalised_payee) {
            return Some(CategoriseResult {
                normalised_payee: normalised_payee.to_string(),
                category: Some(ovr.category.clone()),
                source: CategoriseSource::Rule,
                reason: format!("payee_override:{}", ovr.re.as_str()),
                confidence: confidence::PAYEE_OVERRIDE,
                transaction_count,
            });
        }
    }

    // 2. Check type rules
    if let Some(typ) = txn_type {
        if let Some(cat) = rules.type_rules.get(typ) {
            let conf = match typ {
                "salary" | "transfer_in" | "transfer_out" => confidence::TYPE_HIGH,
                "banking_operation" => confidence::TYPE_BANKING,
                _ => confidence::TYPE_DEFAULT,
            };
            return Some(CategoriseResult {
                normalised_payee: normalised_payee.to_string(),
                category: Some(cat.clone()),
                source: CategoriseSource::Rule,
                reason: format!("type:{}→{}", typ, cat),
                confidence: conf,
                transaction_count,
            });
        }
    }

    // merchant type or unknown type -> falls through to API/LLM
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn rules() -> CategoriseRules {
        CategoriseRules::load(Path::new("rules")).unwrap()
    }

    #[test]
    fn test_salary_maps_to_income() {
        let r = rules();
        let result = try_rules("Apple (Salary)", Some("salary"), 10, &r).unwrap();
        assert_eq!(result.category, Some("_Income".into()));
        assert_eq!(result.source, CategoriseSource::Rule);
        assert!(result.confidence >= 0.99);
    }

    #[test]
    fn test_transfer_out_maps_to_transfer() {
        let r = rules();
        let result = try_rules("Internal Account Transfer", Some("transfer_out"), 5, &r).unwrap();
        assert_eq!(result.category, Some("_Transfer".into()));
    }

    #[test]
    fn test_transfer_in_maps_to_transfer() {
        let r = rules();
        let result = try_rules("Nelson Tam", Some("transfer_in"), 3, &r).unwrap();
        assert_eq!(result.category, Some("_Transfer".into()));
    }

    #[test]
    fn test_banking_operation_maps_to_bills() {
        let r = rules();
        let result = try_rules("Some Bank Operation", Some("banking_operation"), 2, &r).unwrap();
        assert_eq!(result.category, Some("_Bills".into()));
        assert!(result.confidence <= 0.80);
    }

    #[test]
    fn test_merchant_falls_through() {
        let r = rules();
        let result = try_rules("Woolworths Strathfield", Some("merchant"), 20, &r);
        assert!(result.is_none());
    }

    #[test]
    fn test_afes_override_giving() {
        let r = rules();
        let result = try_rules("AFES (Donation)", Some("banking_operation"), 4, &r).unwrap();
        assert_eq!(result.category, Some("_Giving".into()));
        assert!(result.reason.contains("payee_override"));
    }

    #[test]
    fn test_donation_label_override() {
        let r = rules();
        let result = try_rules("Some Charity (Donation)", Some("transfer_out"), 1, &r).unwrap();
        assert_eq!(result.category, Some("_Giving".into()));
    }

    #[test]
    fn test_mortgage_override() {
        let r = rules();
        let result = try_rules("Loan Repayment", Some("banking_operation"), 12, &r).unwrap();
        assert_eq!(result.category, Some("_Mortgage".into()));
    }

    #[test]
    fn test_transport_nsw_override() {
        let r = rules();
        let result = try_rules("Transport NSW", Some("merchant"), 30, &r).unwrap();
        assert_eq!(result.category, Some("_Transport".into()));
    }

    #[test]
    fn test_no_type_merchant_falls_through() {
        let r = rules();
        let result = try_rules("Random Coffee Shop", None, 5, &r);
        assert!(result.is_none());
    }
}
