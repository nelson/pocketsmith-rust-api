mod banking_ops;
mod employers;
mod expand;
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
    DirectDebit,
    DirectCredit,
    BPay,
    InternalTransfer,
    Fee,
}

/// Listed in order of priority for classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayeeClass {
    Person,
    Employer,
    Merchant,
    Other,
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
    original: String,
    pub normalised: String,
    class: Option<PayeeClass>,
    pub features: Features,
}

impl NormalisationResult {
    pub fn new(payee: &str) -> Self {
        Self {
            original: payee.to_string(),
            normalised: payee.to_string(),
            class: None,
            features: Features::default(),
        }
    }

    pub fn original(&self) -> &str {
        &self.original
    }

    pub fn class(&self) -> Option<&PayeeClass> {
        self.class.as_ref()
    }

    pub fn set_class(&mut self, class: PayeeClass) {
        if self.class.is_some() {
            panic!("class already set");
        }
        self.class = Some(class);
    }
}

/// Run the full normalisation pipeline on a raw payee string.
pub fn normalise(original: &str) -> NormalisationResult {
    let mut result = NormalisationResult::new(original);
    prefix::apply(&mut result);
    suffix::apply(&mut result);
    expand::apply(&mut result);
    persons::apply(&mut result);
    employers::apply(&mut result);
    merchants::apply(&mut result);
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
    fn test_normalisation_result_new() {
        let result = NormalisationResult::new("TEST");
        assert_eq!(result.original(), "TEST");
        assert_eq!(result.normalised, "TEST");
        assert!(result.class().is_none());
        assert!(result.features.entity_name.is_none());
        assert!(result.features.location.is_none());
    }

    #[test]
    #[should_panic(expected = "class already set")]
    fn test_set_class_twice_panics() {
        let mut r = NormalisationResult::new("TEST");
        r.set_class(PayeeClass::Person);
        r.set_class(PayeeClass::Merchant);
    }

    // --- Expand truncations tests ---

    #[test]
    fn test_expand_strathfield() {
        let mut r = NormalisationResult::new("WOOLWORTHS 1624 STRATHF");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "WOOLWORTHS 1624 STRATHFIELD");
    }

    #[test]
    fn test_expand_burwood() {
        let mut r = NormalisationResult::new("COLES BURWOO");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "COLES BURWOOD");
    }

    #[test]
    fn test_expand_pharmacy() {
        let mut r = NormalisationResult::new("DISCOUNT PHARMCY");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "DISCOUNT PHARMACY");
    }

    #[test]
    fn test_expand_no_partial_match() {
        let mut r = NormalisationResult::new("STRATEGIC PLAN");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "STRATEGIC PLAN");
    }

    #[test]
    fn test_expand_multiple() {
        let mut r = NormalisationResult::new("PHARMCY BURWOO");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "PHARMACY BURWOOD");
    }

    #[test]
    fn test_expand_north_strathfield() {
        let mut r = NormalisationResult::new("SHOP NORTH STRATHF");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "SHOP NORTH STRATHFIELD");
    }

    #[test]
    fn test_expand_location_suburb() {
        let mut r = NormalisationResult::new("SHOP STRATHF");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "SHOP STRATHFIELD");
    }

    #[test]
    fn test_expand_location_word() {
        let mut r = NormalisationResult::new("DISCOUNT PHARMCY");
        expand::apply(&mut r);
        assert_eq!(r.normalised, "DISCOUNT PHARMACY");
        assert!(r.features.location.is_none());
    }

    // --- normalise() integration tests ---

    #[test]
    fn test_normalise_woolworths_full() {
        let result = normalise("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        assert_eq!(result.class(), Some(&PayeeClass::Merchant));
        assert_eq!(result.features.entity_name.as_deref(), Some("Woolworths"));
    }
}
