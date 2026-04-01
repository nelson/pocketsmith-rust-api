mod expand;
pub use expand::expand_truncations;
mod prefix;
use prefix::prefix_patterns;
mod suffix;
use suffix::suffix_patterns;

use regex::Regex;

pub(crate) struct StripPattern {
    pub(crate) regex: Regex,
    pub(crate) name: &'static str,
    pub(crate) is_gateway: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BankingOperation {
    Transfer,
    DirectDebit,
    DirectCredit,
    Salary,
    Atm,
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
    pub banking_op: Option<BankingOperation>,
    pub reason: Option<String>,
    pub payment_gateway: Option<String>,
    pub account_ref: Option<String>,
    pub bank_name: Option<String>,
    pub date: Option<String>,
    pub foreign_currency: Option<String>,
    pub foreign_amount: Option<u32>, // cents
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

/// Strip metadata prefixes and suffixes from a payee string.
///
/// Uses a unified single loop: each iteration tries all prefix patterns
/// then all suffix patterns, strips the first match, and restarts.
pub fn strip_metadata(result: &mut NormalisationResult) {
    loop {
        let mut matched = false;

        for pat in prefix_patterns() {
            if let Some(caps) = pat.regex.captures(&result.normalised) {
                extract_features(&caps, &mut result.features, pat);
                result.normalised = result.normalised[caps.get(0).unwrap().end()..].to_string();
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }

        for pat in suffix_patterns() {
            if let Some(caps) = pat.regex.captures(&result.normalised) {
                extract_features(&caps, &mut result.features, pat);
                result.normalised = result.normalised[..caps.get(0).unwrap().start()].to_string();
                matched = true;
                break;
            }
        }

        if !matched {
            break;
        }
    }

    result.normalised = result.normalised.trim().to_string();
}

/// Suffix-only variant (used by normalise_check binary).
pub fn strip_metadata_suffix_only(result: &mut NormalisationResult) {
    for pat in suffix_patterns() {
        if let Some(caps) = pat.regex.captures(&result.normalised) {
            extract_features(&caps, &mut result.features, pat);
            result.normalised = result.normalised[..caps.get(0).unwrap().start()].to_string();
        }
    }

    result.normalised = result.normalised.trim().to_string();
}

fn extract_features(caps: &regex::Captures, features: &mut Features, pat: &StripPattern) {
    if pat.is_gateway {
        features.payment_gateway = Some(pat.name.to_string());
    }
    if let Some(date) = caps.name("date") {
        features.date = Some(date.as_str().to_string());
    }
    if let Some(account_ref) = caps.name("account_ref") {
        features.account_ref = Some(account_ref.as_str().to_string());
    }
    if let Some(location) = caps.name("location") {
        features.location = Some(location.as_str().to_string());
    } else if let Some(raw) = caps.name("location_raw") {
        features.location = Some(map_location_raw(raw.as_str()).to_string());
    }
    if let Some(currency) = caps.name("foreign_currency") {
        features.foreign_currency = Some(currency.as_str().to_string());
    }
    if let Some(amount) = caps.name("foreign_amount") {
        features.foreign_amount = parse_amount_cents(amount.as_str());
    }
}

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

    #[test]
    fn test_features_default() {
        let f = Features::default();
        assert!(f.entity_name.is_none());
        assert!(f.location.is_none());
        assert!(f.banking_op.is_none());
        assert!(f.date.is_none());
        assert!(f.foreign_currency.is_none());
        assert!(f.foreign_amount.is_none());
    }

    #[test]
    fn test_payee_class_equality() {
        assert_eq!(PayeeClass::Person, PayeeClass::Person);
        assert_ne!(PayeeClass::Person, PayeeClass::Merchant);
    }

    #[test]
    fn test_banking_operation_variants() {
        assert_eq!(BankingOperation::Transfer, BankingOperation::Transfer);
        assert_ne!(BankingOperation::Transfer, BankingOperation::DirectDebit);
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

    // --- Strip metadata prefix tests ---

    #[test]
    fn test_strip_prefix_square() {
        let mut r = NormalisationResult::new("SQ *SOME MERCHANT SYDNEY");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "SOME MERCHANT SYDNEY");
        assert_eq!(r.features.payment_gateway.as_deref(), Some("Square"));
    }

    #[test]
    fn test_strip_prefix_doordash() {
        let mut r = NormalisationResult::new("DOORDASH*THAI PLACE");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "THAI PLACE");
        assert_eq!(r.features.payment_gateway.as_deref(), Some("DoorDash"));
    }

    #[test]
    fn test_strip_prefix_visa_debit() {
        let mut r = NormalisationResult::new("Visa Debit Purchase Card 9172 MERCHANT NAME");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT NAME");
        assert_eq!(r.features.account_ref.as_deref(), Some("9172"));
    }

    #[test]
    fn test_strip_prefix_date() {
        let mut r = NormalisationResult::new("28/01/26, Direct Debit 123 ENTITY");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "Direct Debit 123 ENTITY");
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
    }

    #[test]
    fn test_strip_prefix_none() {
        let mut r = NormalisationResult::new("Woolworths Strathfield");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "Woolworths Strathfield");
        assert!(r.features.payment_gateway.is_none());
    }

    #[test]
    fn test_strip_prefix_paypal() {
        let mut r = NormalisationResult::new("PAYPAL *SOME STORE");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "SOME STORE");
        assert_eq!(r.features.payment_gateway.as_deref(), Some("PayPal"));
    }

    #[test]
    fn test_strip_multiple_prefixes() {
        let mut r = NormalisationResult::new("28/01/26, SQ *COFFEE SHOP");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "COFFEE SHOP");
        assert_eq!(r.features.payment_gateway.as_deref(), Some("Square"));
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
    }

    // --- Strip metadata suffix tests ---

    #[test]
    fn test_strip_suffix_card() {
        let mut r = NormalisationResult::new("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "WOOLWORTHS 1624 STRATHF");
        assert_eq!(r.features.date.as_deref(), Some("01/01/2026"));
        assert_eq!(r.features.account_ref.as_deref(), Some("9172"));
    }

    #[test]
    fn test_strip_suffix_full_card_number() {
        let mut r = NormalisationResult::new("MERCHANT Card 123456xxxxxx7890");
        strip_metadata(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.account_ref.as_deref(), Some("7890"));
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
        assert_eq!(r.features.payment_gateway.as_deref(), Some("Alipay"));
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
        assert_eq!(r.features.payment_gateway.as_deref(), Some("Square Marketplace"));
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
        assert_eq!(r.features.foreign_currency.as_deref(), Some("SGD"));
        assert_eq!(r.features.foreign_amount, Some(1250));
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
        expand_truncations(&mut r);
        assert_eq!(r.normalised, "WOOLWORTHS 1624 STRATHFIELD");
    }

    #[test]
    fn test_expand_burwood() {
        let mut r = NormalisationResult::new("COLES BURWOO");
        expand_truncations(&mut r);
        assert_eq!(r.normalised, "COLES BURWOOD");
    }

    #[test]
    fn test_expand_pharmacy() {
        let mut r = NormalisationResult::new("DISCOUNT PHARMCY");
        expand_truncations(&mut r);
        assert_eq!(r.normalised, "DISCOUNT PHARMACY");
    }

    #[test]
    fn test_expand_no_partial_match() {
        let mut r = NormalisationResult::new("STRATEGIC PLAN");
        expand_truncations(&mut r);
        assert_eq!(r.normalised, "STRATEGIC PLAN");
    }

    #[test]
    fn test_expand_multiple() {
        let mut r = NormalisationResult::new("PHARMCY BURWOO");
        expand_truncations(&mut r);
        assert_eq!(r.normalised, "PHARMACY BURWOOD");
    }

    #[test]
    fn test_expand_north_strathfield() {
        let mut r = NormalisationResult::new("SHOP NORTH STRATHF");
        expand_truncations(&mut r);
        assert_eq!(r.normalised, "SHOP NORTH STRATHFIELD");
    }

    #[test]
    fn test_expand_location_suburb() {
        let mut r = NormalisationResult::new("SHOP STRATHF");
        expand_truncations(&mut r);
        assert_eq!(r.normalised, "SHOP STRATHFIELD");
        assert_eq!(r.features.location.as_deref(), Some("STRATHFIELD"));
    }

    #[test]
    fn test_expand_location_word() {
        let mut r = NormalisationResult::new("DISCOUNT PHARMCY");
        expand_truncations(&mut r);
        assert_eq!(r.normalised, "DISCOUNT PHARMACY");
        assert!(r.features.location.is_none());
    }
}
