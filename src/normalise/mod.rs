mod banking_ops;
mod employers;
mod expand;
pub use expand::expand;
mod extract;
mod locations;
mod merchants;
mod persons;
mod prefix;
mod suffix;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BankingOperation {
    Interest,
    CreditCard,
    Transfer,
    AccountServicing,
    Loan,
    Deposit,
    Withdrawal,
}

/// Listed in order of priority for classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayeeClass {
    Person,
    Employer, // a special case of Merchant, if it fits known past employers and money is incoming
    Merchant,
    Other,
    Unclassified,
}

#[derive(Debug, Clone, Default)]
pub struct Features {
    pub entity_name: Option<String>,
    pub location: Option<String>,
    pub operation: Option<BankingOperation>,
    pub reason: Option<String>,
    pub institution: Option<String>,
    pub gateway: Option<String>,
    pub account: Option<String>, // e.g. last 4 digits of card
    pub date: Option<String>,
    pub currency_code: Option<String>,
    pub amount_in_cents: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct NormalisationResult {
    pub original: String,
    pub normalised: String,
    pub class: PayeeClass,
    pub features: Features,
}

impl NormalisationResult {
    pub fn new(payee: &str) -> Self {
        Self {
            original: payee.to_string(),
            normalised: payee.to_string(),
            class: PayeeClass::Unclassified,
            features: Features::default(),
        }
    }
}

/// Run the full normalisation pipeline on a raw payee string.
pub fn normalise(original: &str) -> NormalisationResult {
    let mut result = NormalisationResult::new(original);
    prefix::apply(&mut result);
    suffix::apply(&mut result);
    // @cc reomve this line. trim strings after each step instead of at the end?
    result.normalised = result.normalised.trim().to_string();
    expand::expand(&mut result);
    extract::extract_entities(&mut result);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_features_default() {
        let f = Features::default();
        assert!(f.entity_name.is_none());
        assert!(f.location.is_none());
        assert!(f.operation.is_none());
        assert!(f.date.is_none());
        assert!(f.currency_code.is_none());
        assert!(f.amount_in_cents.is_none());
    }

    #[test]
    fn test_payee_class_equality() {
        assert_eq!(PayeeClass::Person, PayeeClass::Person);
        assert_ne!(PayeeClass::Person, PayeeClass::Merchant);
    }

    #[test]
    fn test_banking_operation_variants() {
        assert_eq!(BankingOperation::Transfer, BankingOperation::Transfer);
        assert_ne!(BankingOperation::Transfer, BankingOperation::Interest);
    }

    #[test]
    fn test_normalisation_result_construction() {
        let result = NormalisationResult {
            original: "TEST".to_string(),
            normalised: "Test".to_string(),
            class: PayeeClass::Unclassified,
            features: Features::default(),
        };
        assert_eq!(result.original, "TEST");
        assert_eq!(result.class, PayeeClass::Unclassified);
    }

    #[test]
    fn test_normalisation_result_new() {
        let result = NormalisationResult::new("TEST");
        assert_eq!(result.original, "TEST");
        assert_eq!(result.normalised, "TEST");
        assert_eq!(result.class, PayeeClass::Unclassified);
        assert!(result.features.entity_name.is_none());
        assert!(result.features.location.is_none());
    }

    // --- Expand truncations tests ---

    #[test]
    fn test_expand_strathfield() {
        let mut r = NormalisationResult::new("WOOLWORTHS 1624 STRATHF");
        expand(&mut r);
        assert_eq!(r.normalised, "WOOLWORTHS 1624 STRATHFIELD");
    }

    #[test]
    fn test_expand_burwood() {
        let mut r = NormalisationResult::new("COLES BURWOO");
        expand(&mut r);
        assert_eq!(r.normalised, "COLES BURWOOD");
    }

    #[test]
    fn test_expand_pharmacy() {
        let mut r = NormalisationResult::new("DISCOUNT PHARMCY");
        expand(&mut r);
        assert_eq!(r.normalised, "DISCOUNT PHARMACY");
    }

    #[test]
    fn test_expand_no_partial_match() {
        let mut r = NormalisationResult::new("STRATEGIC PLAN");
        expand(&mut r);
        assert_eq!(r.normalised, "STRATEGIC PLAN");
    }

    #[test]
    fn test_expand_multiple() {
        let mut r = NormalisationResult::new("PHARMCY BURWOO");
        expand(&mut r);
        assert_eq!(r.normalised, "PHARMACY BURWOOD");
    }

    #[test]
    fn test_expand_north_strathfield() {
        let mut r = NormalisationResult::new("SHOP NORTH STRATHF");
        expand(&mut r);
        assert_eq!(r.normalised, "SHOP NORTH STRATHFIELD");
    }

    #[test]
    fn test_expand_location_suburb() {
        let mut r = NormalisationResult::new("SHOP STRATHF");
        expand(&mut r);
        assert_eq!(r.normalised, "SHOP STRATHFIELD");
        assert_eq!(r.features.location.as_deref(), Some("STRATHFIELD"));
    }

    #[test]
    fn test_expand_location_word() {
        let mut r = NormalisationResult::new("DISCOUNT PHARMCY");
        expand(&mut r);
        assert_eq!(r.normalised, "DISCOUNT PHARMACY");
        assert!(r.features.location.is_none());
    }

    // --- normalise() integration tests ---

    #[test]
    fn test_normalise_woolworths_full() {
        let result = normalise("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        assert_eq!(result.class, PayeeClass::Merchant);
        assert_eq!(result.features.entity_name.as_deref(), Some("Woolworths"));
        assert_eq!(result.features.location.as_deref(), Some("STRATHFIELD"));
    }
}
