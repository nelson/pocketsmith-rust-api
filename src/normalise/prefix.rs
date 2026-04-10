use std::sync::OnceLock;

use regex::Regex;

use super::{BankingOperation, NormalisationResult};

const DEFAULT: Prefix = Prefix { pattern: "", gateway: None, operation: None, has_account: false, has_date: false };

struct Prefix {
    pattern: &'static str,
    gateway: Option<&'static str>,
    operation: Option<BankingOperation>,
    has_account: bool,
    has_date: bool,
}

struct CompiledPrefix {
    regex: Regex,
    gateway: Option<&'static str>,
    operation: Option<BankingOperation>,
    has_account: bool,
    has_date: bool,
}

/// Strip metadata prefixes in a loop until no more match.
pub fn apply(result: &mut NormalisationResult) {
    loop {
        let mut matched = false;
        for pat in compiled_prefixes() {
            if let Some(caps) = pat.regex.captures(&result.normalised) {
                if let Some(gw) = pat.gateway {
                    result.features.gateway = Some(gw.to_string());
                }
                if let Some(ref op) = pat.operation {
                    result.features.operation = Some(op.clone());
                }
                if pat.has_date {
                    if let Some(date) = caps.name("date") {
                        result.features.date = Some(date.as_str().to_string());
                    }
                }
                if pat.has_account {
                    if let Some(account) = caps.name("account") {
                        result.features.account = Some(account.as_str().to_string());
                    }
                }
                // Remove the matched prefix, trim remaining whitespace.
                let remainder = &result.normalised[caps.get(0).unwrap().end()..];
                result.normalised = remainder.trim().to_string();
                matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
    }
}

const PREFIXES: &[Prefix] = &[
    // --- Non-gateway prefixes ---
    Prefix { pattern: r"^(?P<date>\d{2}/\d{2}/\d{2,4}),?\s+", has_date: true, ..DEFAULT },
    Prefix { pattern: r"^-([A-Z]+-)*", ..DEFAULT },
    Prefix { pattern: r"^EFTPOS\s+", ..DEFAULT },
    Prefix { pattern: r"^\*\s+", ..DEFAULT },
    Prefix { pattern: r"^\s*-\s+", ..DEFAULT },
    Prefix { pattern: r"^% ", ..DEFAULT },
    Prefix { pattern: r"^Return\s+", ..DEFAULT },
    Prefix { pattern: r"^SP ", ..DEFAULT },
    Prefix { pattern: r"^Visa Debit Purchase Card (?P<account>\d{4})\s+", has_account: true, ..DEFAULT },
    // --- Direct Debit/Credit prefixes ---
    Prefix { pattern: r"^(?i)Direct Debit (?:XX)?(?P<account>\d+)\s+", operation: Some(BankingOperation::DirectDebit), has_account: true, ..DEFAULT },
    Prefix { pattern: r"^(?i)Direct Credit (?:PENSION )?(?:XX)?(?P<account>\d+)\s+", operation: Some(BankingOperation::DirectCredit), has_account: true, ..DEFAULT },
    // --- Gateway prefixes ---
    Prefix { pattern: r"^ALI\*", gateway: Some("AliExpress"), ..DEFAULT },
    Prefix { pattern: r"^Alipay ", gateway: Some("Alipay"), ..DEFAULT },
    Prefix { pattern: r"^CKO\*", gateway: Some("Checkout.com"), ..DEFAULT },
    Prefix { pattern: r"^DBS\*", gateway: Some("DBS"), ..DEFAULT },
    Prefix { pattern: r"^DNH\*", gateway: Some("DNH"), ..DEFAULT },
    Prefix { pattern: r"^DOORDASH\*", gateway: Some("DoorDash"), ..DEFAULT },
    Prefix { pattern: r"^EB\s*\*", gateway: Some("Eventbrite"), ..DEFAULT },
    Prefix { pattern: r"^EZI\*", gateway: Some("Ezi"), ..DEFAULT },
    Prefix { pattern: r"^FLEXISCHOOLS\*", gateway: Some("Flexischools"), ..DEFAULT },
    Prefix { pattern: r"^GLOBAL-E\* ", gateway: Some("Global-E"), ..DEFAULT },
    Prefix { pattern: r"^LIGHTSPEED\*(?:SR-)?(?:LS\s+)?", gateway: Some("Lightspeed"), ..DEFAULT },
    Prefix { pattern: r"^LIME\*", gateway: Some("Lime"), ..DEFAULT },
    Prefix { pattern: r"^LS\s+", gateway: Some("Lightspeed"), ..DEFAULT },
    Prefix { pattern: r"^MPASS \*", gateway: Some("mPass"), ..DEFAULT },
    Prefix { pattern: r"^MR YUM\*", gateway: Some("Mr Yum"), ..DEFAULT },
    Prefix { pattern: r"^NAYAXAU\*", gateway: Some("Nayax"), ..DEFAULT },
    Prefix { pattern: r"^PAYPAL \*", gateway: Some("PayPal"), ..DEFAULT },
    Prefix { pattern: r"^PP\*", gateway: Some("PP"), ..DEFAULT },
    Prefix { pattern: r"^(?i:Revolut)\*", gateway: Some("Revolut"), ..DEFAULT },
    Prefix { pattern: r"^SMP\*", gateway: Some("Square Marketplace"), ..DEFAULT },
    Prefix { pattern: r"^SQ \*", gateway: Some("Square"), ..DEFAULT },
    Prefix { pattern: r"^TITHE\.LY\*", gateway: Some("Tithe.ly"), ..DEFAULT },
    Prefix { pattern: r"^TST\*\s*", gateway: Some("Toast"), ..DEFAULT },
    Prefix { pattern: r"^TRYBOOKING\*", gateway: Some("TryBooking"), ..DEFAULT },
    Prefix { pattern: r"^Weixin ", gateway: Some("Weixin"), ..DEFAULT },
    Prefix { pattern: r"^WINDCAVE\*", gateway: Some("Windcave"), ..DEFAULT },
    Prefix { pattern: r"^ZLR\*", gateway: Some("Zeller"), ..DEFAULT },
];

fn compiled_prefixes() -> &'static [CompiledPrefix] {
    static COMPILED: OnceLock<Vec<CompiledPrefix>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        PREFIXES
            .iter()
            .map(|p| CompiledPrefix {
                regex: Regex::new(p.pattern).expect("invalid prefix pattern"),
                gateway: p.gateway,
                operation: p.operation.clone(),
                has_account: p.has_account,
                has_date: p.has_date,
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::{BankingOperation, NormalisationResult};

    #[test]
    fn test_square() {
        let mut r = NormalisationResult::new("SQ *SOME MERCHANT SYDNEY");
        apply(&mut r);
        assert_eq!(r.normalised, "SOME MERCHANT SYDNEY");
        assert_eq!(r.features.gateway.as_deref(), Some("Square"));
    }

    #[test]
    fn test_doordash() {
        let mut r = NormalisationResult::new("DOORDASH*THAI PLACE");
        apply(&mut r);
        assert_eq!(r.normalised, "THAI PLACE");
        assert_eq!(r.features.gateway.as_deref(), Some("DoorDash"));
    }

    #[test]
    fn test_visa_debit() {
        let mut r = NormalisationResult::new("Visa Debit Purchase Card 9172 MERCHANT NAME");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT NAME");
        assert_eq!(r.features.account.as_deref(), Some("9172"));
    }

    #[test]
    fn test_date() {
        let mut r = NormalisationResult::new("28/01/26, Direct Debit 123 ENTITY");
        apply(&mut r);
        assert_eq!(r.normalised, "ENTITY");
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
        assert_eq!(r.features.operation, Some(BankingOperation::DirectDebit));
        assert_eq!(r.features.account.as_deref(), Some("123"));
    }

    #[test]
    fn test_none() {
        let mut r = NormalisationResult::new("Woolworths Strathfield");
        apply(&mut r);
        assert_eq!(r.normalised, "Woolworths Strathfield");
        assert!(r.features.gateway.is_none());
    }

    #[test]
    fn test_paypal() {
        let mut r = NormalisationResult::new("PAYPAL *SOME STORE");
        apply(&mut r);
        assert_eq!(r.normalised, "SOME STORE");
        assert_eq!(r.features.gateway.as_deref(), Some("PayPal"));
    }

    #[test]
    fn test_multiple_prefixes() {
        let mut r = NormalisationResult::new("28/01/26, SQ *COFFEE SHOP");
        apply(&mut r);
        assert_eq!(r.normalised, "COFFEE SHOP");
        assert_eq!(r.features.gateway.as_deref(), Some("Square"));
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
    }

    #[test]
    fn test_direct_debit() {
        let mut r = NormalisationResult::new("Direct Debit 062246 CommInsure");
        apply(&mut r);
        assert_eq!(r.normalised, "CommInsure");
        assert_eq!(r.features.operation, Some(BankingOperation::DirectDebit));
        assert_eq!(r.features.account.as_deref(), Some("062246"));
    }

    #[test]
    fn test_direct_credit() {
        let mut r = NormalisationResult::new("Direct Credit 002221 MCARE BENEFITS");
        apply(&mut r);
        assert_eq!(r.normalised, "MCARE BENEFITS");
        assert_eq!(r.features.operation, Some(BankingOperation::DirectCredit));
        assert_eq!(r.features.account.as_deref(), Some("002221"));
    }

    #[test]
    fn test_direct_credit_pension() {
        let mut r = NormalisationResult::new("Direct Credit PENSION XX1234 CHILDASSISTPYMT");
        apply(&mut r);
        assert_eq!(r.normalised, "CHILDASSISTPYMT");
        assert_eq!(r.features.operation, Some(BankingOperation::DirectCredit));
    }

    #[test]
    fn test_date_then_direct_debit() {
        let mut r = NormalisationResult::new("28/01/26, Direct Debit 123456 ENTITY");
        apply(&mut r);
        assert_eq!(r.normalised, "ENTITY");
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
        assert_eq!(r.features.operation, Some(BankingOperation::DirectDebit));
        assert_eq!(r.features.account.as_deref(), Some("123456"));
    }
}
