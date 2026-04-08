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

use regex::Regex;

pub(crate) struct StripPattern {
    pub(crate) regex: Regex,
    pub(crate) gateway_name: Option<&'static str>,
}

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
    pub bank: Option<String>,
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

// @cc make this function redundant
/// Strip metadata prefixes and suffixes from a payee string.
// pub fn strip_metadata(result: &mut NormalisationResult) {
//     prefix::strip_prefixes(result);
//     suffix::strip_suffixes(result);
//     result.normalised = result.normalised.trim().to_string();
// }

// @cc this function is not called anywhere
/// Suffix-only variant (used by normalise_check binary).
// pub fn strip_metadata_suffix_only(result: &mut NormalisationResult) {
//     suffix::strip_suffixes(result);
//     result.normalised = result.normalised.trim().to_string();
// }

/// Run the full normalisation pipeline on a raw payee string.
pub fn normalise(original: &str) -> NormalisationResult {
    let mut result = NormalisationResult::new(original);
    prefix::apply(&mut result);
    suffix::strip_suffixes(&mut result);
    // @cc reomve this line. trim strings after each step instead of at the end?
    result.normalised = result.normalised.trim().to_string();
    expand::expand(&mut result);
    extract::extract_entities(&mut result);
    result
}

// @cc we are trying to get rid of this function
pub(crate) fn extract_features(caps: &regex::Captures, features: &mut Features) {
    if let Some(gateway) = caps.name("gateway") {
        features.gateway = Some(gateway.as_str().to_string());
    }
    if let Some(date) = caps.name("date") {
        features.date = Some(date.as_str().to_string());
    }
    if let Some(account) = caps.name("account") {
        features.account = Some(account.as_str().to_string());
    }
    if let Some(location) = caps.name("location") {
        features.location = Some(location.as_str().to_string());
    } else if let Some(raw) = caps.name("location_raw") {
        features.location = Some(map_location_raw(raw.as_str()).to_string());
    }
    if let Some(currency) = caps.name("currency_code") {
        features.currency_code = Some(currency.as_str().to_string());
    }
    if let Some(amount) = caps.name("amount_in_cents") {
        features.amount_in_cents = parse_amount_cents(amount.as_str());
    }
}

// @cc location_raw doesn't make sense. Move this to
fn map_location_raw(raw: &str) -> &'static str {
    match raw {
        "NSWAU" | "NS" => "NSW",
        "AU" | "AUS" => "AU",
        "NLD" => "NL",
        "SGP" => "SG",
        "USA" => "US",
        "IDN" => "ID",
        "GBR" => "GB",
        other => panic!("unmapped location_raw: {other}"),
    }
}

fn parse_amount_cents(s: &str) -> Option<u32> {
    s.replace('.', "").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_metadata(result: &mut NormalisationResult) {
        prefix::apply(result);
        suffix::strip_suffixes(result);
        result.normalised = result.normalised.trim().to_string();
    }

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

    // --- Strip metadata suffix tests ---

    #[test]
    fn test_strip_suffix_card() {
        let mut r = NormalisationResult::new("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "WOOLWORTHS 1624 STRATHF");
        assert_eq!(r.features.date.as_deref(), Some("01/01/2026"));
        assert_eq!(r.features.account.as_deref(), Some("9172"));
    }

    #[test]
    fn test_strip_suffix_full_card_number() {
        let mut r = NormalisationResult::new("MERCHANT Card 123456xxxxxx7890");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.account.as_deref(), Some("7890"));
    }

    #[test]
    fn test_strip_suffix_standalone_value_date() {
        let mut r = NormalisationResult::new("MERCHANT Value Date: 15/03/2026");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.date.as_deref(), Some("15/03/2026"));
    }

    #[test]
    fn test_strip_suffix_country_code() {
        let mut r = NormalisationResult::new("SOME MERCHANT NSWAU");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "SOME MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("NSW"));
    }

    #[test]
    fn test_strip_suffix_state_postcode() {
        let mut r = NormalisationResult::new("MERCHANT NSW 2140");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("NSW 2140"));
    }

    #[test]
    fn test_strip_suffix_au_aus() {
        let mut r = NormalisationResult::new("MERCHANT AU AUS");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("AU"));
    }

    #[test]
    fn test_strip_suffix_state_only() {
        let mut r = NormalisationResult::new("MERCHANT VIC");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("VIC"));
    }

    #[test]
    fn test_strip_suffix_pty_ltd() {
        let mut r = NormalisationResult::new("COMPANY NAME PTY LTD");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "COMPANY NAME");
    }

    #[test]
    fn test_strip_suffix_alipay_gateway() {
        let mut r = NormalisationResult::new("MERCHANT - Alipay");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.gateway.as_deref(), Some("Alipay"));
    }

    #[test]
    fn test_strip_suffix_long_reference() {
        let mut r = NormalisationResult::new("MERCHANT 12345678");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
    }

    #[test]
    fn test_strip_both_prefix_and_suffix() {
        let mut r = NormalisationResult::new("SMP*CAFE NAME, Card xx1234 Value Date: 01/01/2026");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "CAFE NAME");
        assert_eq!(r.features.gateway.as_deref(), Some("Square Marketplace"));
    }

    #[test]
    fn test_strip_eftpos_receipt() {
        let mut r = NormalisationResult::new("MERCHANT - Eftpos Purchase - Receipt 123Date01/01");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
    }

    #[test]
    fn test_strip_suffix_foreign_currency() {
        let mut r = NormalisationResult::new("MERCHANT SGD 12.50");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.currency_code.as_deref(), Some("SGD"));
        assert_eq!(r.features.amount_in_cents, Some(1250));
    }

    #[test]
    fn test_strip_email_suffix() {
        let mut r = NormalisationResult::new("PAYPAL - paypal-aud@airbnb.com");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "PAYPAL");
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
