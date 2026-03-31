/// OnceLock provides lazy one-time initialization of static data.
/// Patterns are compiled once on first use and reused for all subsequent calls.
use std::sync::OnceLock;

use regex::Regex;

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
}

#[derive(Debug, Clone)]
pub struct NormalisationResult {
    pub original: String,
    pub normalised: String,
    pub class: PayeeClass,
    pub features: Features,
}

struct StripPattern {
    regex: Regex,
    name: &'static str,
    is_gateway: bool,
}

fn prefix_patterns() -> &'static Vec<StripPattern> {
    static PATTERNS: OnceLock<Vec<StripPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        // Sorted: non-gateway (alphabetical by name), then gateway (alphabetical by name)
        let patterns: Vec<(&str, &'static str, bool)> = vec![
            // --- Non-gateway prefixes ---
            // (r"^Cafes - ", "CBA auto-pay", false),
            (r"^\d{2}/\d{2}/\d{2,4},?\s+", "Date prefix", false),
            (r"^-([A-Z]+-)*", "dashed prefix", false),
            (r"^EFTPOS\s+", "EFTPOS", false),
            (r"^\*\s+", "Leading asterisk", false),
            (r"^\s*-\s+", "leading dash-space", false),
            (r"^% ", "percent prefix", false),
            (r"^Return\s+", "return", false),
            (r"^SP ", "SP prefix", false),
            (r"^Visa Debit Purchase Card \d{4}\s+", "Visa Debit Purchase", false),
            // --- Gateway prefixes ---
            (r"^ALI\*", "AliExpress", true),
            (r"^Alipay ", "Alipay", true),
            (r"^CKO\*", "Checkout.com", true),
            (r"^DBS\*", "DBS", true),
            (r"^DNH\*", "DNH", true),
            (r"^DOORDASH\*", "DoorDash", true),
            (r"^EB\s*\*", "Eventbrite", true),
            (r"^EZI\*", "Ezi", true),
            (r"^FLEXISCHOOLS\*", "Flexischools", true),
            (r"^GLOBAL-E\* ", "Global-E", true),
            (r"^LIGHTSPEED\*(?:SR-)?(?:LS\s+)?", "Lightspeed", true),
            (r"^LIME\*", "Lime", true),
            (r"^LS\s+", "Lightspeed", true),
            (r"^MPASS \*", "mPass", true),
            (r"^MR YUM\*", "Mr Yum", true),
            (r"^NAYAXAU\*", "Nayax", true),
            (r"^PAYPAL \*", "PayPal", true),
            (r"^PP\*", "PP", true),
            (r"^(?i:Revolut)\*", "Revolut", true),
            (r"^SMP\*", "Square Marketplace", true),
            (r"^SQ \*", "Square", true),
            (r"^TITHE\.LY\*", "Tithe.ly", true),
            (r"^TST\*\s*", "Toast", true),
            (r"^TRYBOOKING\*", "TryBooking", true),
            (r"^Weixin ", "Weixin", true),
            (r"^WINDCAVE\*", "Windcave", true),
            (r"^ZLR\*", "Zeller", true),
        ];
        patterns
            .into_iter()
            .map(|(p, n, g)| StripPattern {
                regex: Regex::new(p).expect("invalid prefix pattern"),
                name: n,
                is_gateway: g,
            })
            .collect()
    })
}

/// Strip metadata prefixes from a payee string.
/// Returns (stripped_string, detected_payment_gateway, extracted_date).
/// Strips multiple prefixes — typically one or more non-gateway prefixes
/// and at most one gateway prefix.
pub fn strip_metadata(payee: &str) -> (String, Option<String>, Option<String>) {
    let mut s = payee.to_string();
    let mut gateway = None;
    let mut date = None;

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

    s = s.trim().to_string();
    (s, gateway, date)
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
        let (stripped, gw, _) = strip_metadata("SQ *SOME MERCHANT SYDNEY");
        assert_eq!(stripped, "SOME MERCHANT SYDNEY");
        assert_eq!(gw.as_deref(), Some("Square"));
    }

    #[test]
    fn test_strip_prefix_doordash() {
        let (stripped, gw, _) = strip_metadata("DOORDASH*THAI PLACE");
        assert_eq!(stripped, "THAI PLACE");
        assert_eq!(gw.as_deref(), Some("DoorDash"));
    }

    #[test]
    fn test_strip_prefix_visa_debit() {
        let (stripped, _, _) = strip_metadata("Visa Debit Purchase Card 9172 MERCHANT NAME");
        assert_eq!(stripped, "MERCHANT NAME");
    }

    #[test]
    fn test_strip_prefix_date() {
        let (stripped, _, date) = strip_metadata("28/01/26, Direct Debit 123 ENTITY");
        assert_eq!(stripped, "Direct Debit 123 ENTITY");
        assert_eq!(date.as_deref(), Some("28/01/26"));
    }

    #[test]
    fn test_strip_prefix_none() {
        let (stripped, gw, _) = strip_metadata("Woolworths Strathfield");
        assert_eq!(stripped, "Woolworths Strathfield");
        assert!(gw.is_none());
    }

    #[test]
    fn test_strip_prefix_paypal() {
        let (stripped, gw, _) = strip_metadata("PAYPAL *SOME STORE");
        assert_eq!(stripped, "SOME STORE");
        assert_eq!(gw.as_deref(), Some("PayPal"));
    }

    #[test]
    fn test_strip_multiple_prefixes() {
        let (stripped, gw, date) = strip_metadata("28/01/26, SQ *COFFEE SHOP");
        assert_eq!(stripped, "COFFEE SHOP");
        assert_eq!(gw.as_deref(), Some("Square"));
        assert_eq!(date.as_deref(), Some("28/01/26"));
    }
}
