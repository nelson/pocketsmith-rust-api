#[path = "01_strip.rs"]
mod _01_strip;
use _01_strip::{prefix_patterns, suffix_patterns};

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

pub struct StripResult {
    pub stripped: String,
    pub gateway: Option<String>,
    pub date: Option<String>,
}

/// Strip metadata prefixes and suffixes from a payee string.
pub fn strip_metadata(payee: &str) -> StripResult {
    let mut s = payee.to_string();
    let mut gateway = None;
    let mut date = None;

    // Strips multiple prefixes — typically one or more non-gateway prefixes
    // and at most one gateway prefix.
    loop {
        let mut matched = false;
        for pat in prefix_patterns() {
            if let Some(m) = pat.regex.find(&s) {
                if pat.is_gateway {
                    gateway = Some(pat.name.to_string());
                }
                if pat.name == "Date prefix" {
                    date = Some(m.as_str().trim_end_matches(|c: char| c == ',' || c.is_whitespace()).to_string());
                }
                s = s[m.end()..].to_string();
                matched = true;
                break; // restart from beginning of pattern list
            }
        }
        if !matched {
            break;
        }
    }

    for pat in suffix_patterns() {
        if let Some(m) = pat.regex.find(&s) {
            s = s[..m.start()].to_string();
        }
    }

    s = s.trim().to_string();
    StripResult { stripped: s, gateway, date }
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

    // --- Strip metadata prefix tests ---

    #[test]
    fn test_strip_prefix_square() {
        let r = strip_metadata("SQ *SOME MERCHANT SYDNEY");
        assert_eq!(r.stripped, "SOME MERCHANT SYDNEY");
        assert_eq!(r.gateway.as_deref(), Some("Square"));
    }

    #[test]
    fn test_strip_prefix_doordash() {
        let r = strip_metadata("DOORDASH*THAI PLACE");
        assert_eq!(r.stripped, "THAI PLACE");
        assert_eq!(r.gateway.as_deref(), Some("DoorDash"));
    }

    #[test]
    fn test_strip_prefix_visa_debit() {
        let r = strip_metadata("Visa Debit Purchase Card 9172 MERCHANT NAME");
        assert_eq!(r.stripped, "MERCHANT NAME");
    }

    #[test]
    fn test_strip_prefix_date() {
        let r = strip_metadata("28/01/26, Direct Debit 123 ENTITY");
        assert_eq!(r.stripped, "Direct Debit 123 ENTITY");
        assert_eq!(r.date.as_deref(), Some("28/01/26"));
    }

    #[test]
    fn test_strip_prefix_none() {
        let r = strip_metadata("Woolworths Strathfield");
        assert_eq!(r.stripped, "Woolworths Strathfield");
        assert!(r.gateway.is_none());
    }

    #[test]
    fn test_strip_prefix_paypal() {
        let r = strip_metadata("PAYPAL *SOME STORE");
        assert_eq!(r.stripped, "SOME STORE");
        assert_eq!(r.gateway.as_deref(), Some("PayPal"));
    }

    #[test]
    fn test_strip_multiple_prefixes() {
        let r = strip_metadata("28/01/26, SQ *COFFEE SHOP");
        assert_eq!(r.stripped, "COFFEE SHOP");
        assert_eq!(r.gateway.as_deref(), Some("Square"));
        assert_eq!(r.date.as_deref(), Some("28/01/26"));
    }

    // --- Strip metadata suffix tests ---

    #[test]
    fn test_strip_suffix_card() {
        let r = strip_metadata("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        assert_eq!(r.stripped, "WOOLWORTHS 1624 STRATHF");
    }

    #[test]
    fn test_strip_suffix_country_code() {
        let r = strip_metadata("SOME MERCHANT NSWAU");
        assert_eq!(r.stripped, "SOME MERCHANT");
    }

    #[test]
    fn test_strip_suffix_state_postcode() {
        let r = strip_metadata("MERCHANT NSW 2140");
        assert_eq!(r.stripped, "MERCHANT");
    }

    #[test]
    fn test_strip_suffix_pty_ltd() {
        let r = strip_metadata("COMPANY NAME PTY LTD");
        assert_eq!(r.stripped, "COMPANY NAME");
    }

    #[test]
    fn test_strip_suffix_long_reference() {
        let r = strip_metadata("MERCHANT 12345678");
        assert_eq!(r.stripped, "MERCHANT");
    }

    #[test]
    fn test_strip_both_prefix_and_suffix() {
        let r = strip_metadata("SMP*CAFE NAME, Card xx1234 Value Date: 01/01/2026");
        assert_eq!(r.stripped, "CAFE NAME");
        assert_eq!(r.gateway.as_deref(), Some("Square Marketplace"));
    }

    #[test]
    fn test_strip_eftpos_receipt() {
        let r = strip_metadata("MERCHANT - Eftpos Purchase - Receipt 123Date01/01");
        assert_eq!(r.stripped, "MERCHANT");
    }

    #[test]
    fn test_strip_email_suffix() {
        let r = strip_metadata("PAYPAL - paypal-aud@airbnb.com");
        assert_eq!(r.stripped, "PAYPAL");
    }
}
