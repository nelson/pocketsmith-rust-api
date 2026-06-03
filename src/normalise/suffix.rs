use std::sync::OnceLock;

use regex::Regex;

use super::{BankingOperation, NormalisationResult};

const DEFAULT: Suffix = Suffix {
    pattern: "",
    gateway: None,
    operation: None,
    institution: None,
    has_account: false,
    has_date: false,
    has_location: false,
    has_currency_code: false,
    has_amount: false,
};

struct Suffix {
    pattern: &'static str,
    gateway: Option<&'static str>,
    operation: Option<BankingOperation>,
    institution: Option<&'static str>,
    has_account: bool,
    has_date: bool,
    has_location: bool,
    has_currency_code: bool,
    has_amount: bool,
}

struct CompiledSuffix {
    regex: Regex,
    gateway: Option<&'static str>,
    operation: Option<BankingOperation>,
    institution: Option<&'static str>,
    has_account: bool,
    has_date: bool,
    has_location: bool,
    has_currency_code: bool,
    has_amount: bool,
}

/// Strip metadata suffixes in a loop (first match wins per iteration).
pub fn apply(result: &mut NormalisationResult) {
    loop {
        let mut matched = false;
        for pat in compiled_suffixes() {
            if let Some(caps) = pat.regex.captures(&result.normalised) {
                if let Some(gw) = pat.gateway {
                    result.features.gateway = Some(gw.to_string());
                }
                if let Some(op) = pat.operation {
                    result.features.operation = Some(op);
                }
                if let Some(inst) = pat.institution {
                    result.features.institution = Some(inst.to_string());
                }
                if pat.has_account {
                    if let Some(account) = caps.name("account") {
                        result.features.account = Some(account.as_str().to_string());
                    }
                }
                if pat.has_date {
                    if let Some(date) = caps.name("date") {
                        result.features.date = Some(date.as_str().to_string());
                    }
                }
                if pat.has_location {
                    if let Some(loc) = caps.name("location") {
                        let location = match loc.as_str() {
                            "NS" => "NSW",
                            other => other,
                        };
                        result.features.location = Some(location.to_string());
                    }
                }
                if pat.has_currency_code {
                    if let Some(currency) = caps.name("currency_code") {
                        result.features.currency_code = Some(currency.as_str().to_string());
                    }
                }
                if pat.has_amount {
                    if let Some(amount) = caps.name("amount_in_cents") {
                        result.features.amount_in_cents = parse_amount_cents(amount.as_str());
                    }
                }
                // Remove the matched suffix, trim remaining whitespace.
                let remainder = &result.normalised[..caps.get(0).unwrap().start()];
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

const SUFFIXES: &[Suffix] = &[
    // --- Card + date (has_account + has_date) ---
    Suffix { pattern: r",?\s*Card xx(?P<account>\d{4}).*?(?P<date>\d{2}/\d{2}/\d{4}).*$", has_account: true, has_date: true, ..DEFAULT },
    Suffix { pattern: r"\s+Card xx(?P<account>\d{4}).*?(?P<date>\d{2}/\d{2}/\d{4}).*$", has_account: true, has_date: true, ..DEFAULT },
    // --- Account only ---
    Suffix { pattern: r"\s+Tap and Pay xx(?P<account>\d{4}).*$", has_account: true, ..DEFAULT },
    Suffix { pattern: r"\s+Card\s+\d{6}x{6}(?P<account>\d{4})$", has_account: true, ..DEFAULT },
    Suffix { pattern: r"\s+Card\s+\d[A-Z]\d{4}[A-Za-z]{6}(?P<account>\d{4})$", has_account: true, ..DEFAULT },
    Suffix { pattern: r"\s+Card\s+xx(?P<account>\d{4})\s*$", has_account: true, ..DEFAULT },
    Suffix { pattern: r"\s*,\s*(?P<account>\d{4})$", has_account: true, ..DEFAULT },
    Suffix { pattern: r"\s*,\s*\d{4}\s+Last 4 Card Digits\s+(?P<account>\d{4})$", has_account: true, ..DEFAULT },
    Suffix { pattern: r"\s*,?\s*\d{4}\s+Last\s+4\s+Card\s+Digits\s+(?P<account>\d{4})$", has_account: true, ..DEFAULT },
    // --- Date only ---
    Suffix { pattern: r"\s+Value [Dd]ate:?\s+(?P<date>\d{2}/\d{2}/\d{4})$", has_date: true, ..DEFAULT },
    // --- Operations (institution + operation type) ---
    Suffix { pattern: r"\s*-?\s*Visa Purchase\s*-\s*Receipt\s+\w+\s*In\s+.*$", operation: Some(BankingOperation::Purchase), institution: Some("Visa"), ..DEFAULT },
    Suffix { pattern: r"\s*-?\s*Visa Refund\s*-\s*Receipt\s+.*$", operation: Some(BankingOperation::Refund), institution: Some("Visa"), ..DEFAULT },
    Suffix { pattern: r"\s*-?\s*Osko Payment.*Receipt\s+\d+.*$", operation: Some(BankingOperation::Transfer), institution: Some("Osko"), ..DEFAULT },
    Suffix { pattern: r"\s*-\s*Deposit\s*-\s*Receipt\s+.*$", operation: Some(BankingOperation::Deposit), ..DEFAULT },
    Suffix { pattern: r"\s*-?\s*Eftpos (?:Purchase|Cash Out)\s*-\s*Receipt\s+.*$", operation: Some(BankingOperation::Purchase), institution: Some("Eftpos"), ..DEFAULT },
    Suffix { pattern: r"\s+Eftpos Purchase\s*-\s*Receipt\s+.*$", operation: Some(BankingOperation::Purchase), institution: Some("Eftpos"), ..DEFAULT },
    Suffix { pattern: r"\s*-\s*Eftpos Purchase\s*-\s*Receipt\s+\d+Date.*$", operation: Some(BankingOperation::Purchase), institution: Some("Eftpos"), ..DEFAULT },
    Suffix { pattern: r"\s*-?\s*Eftpos Refund\s*-\s*Receipt\s+.*$", operation: Some(BankingOperation::Refund), institution: Some("Eftpos"), ..DEFAULT },
    Suffix { pattern: r"(?i)\s*-?\s*Cash Out\s*-\s*Receipt\s+.*$", operation: Some(BankingOperation::Cash), ..DEFAULT },
    Suffix { pattern: r"(?i)\s*-?\s*Refund\s*-\s*Receipt\s+.*$", operation: Some(BankingOperation::Refund), ..DEFAULT },
    Suffix { pattern: r"(?i)\s*-?\s*Purchase\s*-\s*Receipt\s+.*$", operation: Some(BankingOperation::Purchase), ..DEFAULT },
    Suffix { pattern: r"\s*-\s*Internal Transfer\s*-\s*Receipt\s+\d+.*$", operation: Some(BankingOperation::Transfer), ..DEFAULT },
    Suffix { pattern: r"(?i)\s*-?\s*Receipt\s+\d+\s*$", ..DEFAULT },
    // --- Gateway ---
    Suffix { pattern: r"\s*-\s*Alipay$", gateway: Some("Alipay"), ..DEFAULT },
    Suffix { pattern: r"(?i)\s*-?\s*Beem It\s*$", gateway: Some("Beem"), ..DEFAULT },
    // --- Location (country codes, stripped with location extraction) ---
    Suffix { pattern: r"\s+(?P<location>NS) AUS$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>AU) AUS$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>AU)$", has_location: true, ..DEFAULT },
    // --- Location (state + optional postcode) ---
    Suffix { pattern: r"\s+(?P<location>(?:NSW|VIC|QLD|WA|SA|TAS|ACT|NT)\s+\d{4,6})$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>(?:NSW|VIC|QLD|WA|SA|TAS|ACT|NT))$", has_location: true, ..DEFAULT },
    // --- Currency + amount ---
    Suffix { pattern: r"\s+(?P<currency_code>[A-Z]{3})\s+(?P<amount_in_cents>\d+\.\d{2})$", has_currency_code: true, has_amount: true, ..DEFAULT },
    Suffix { pattern: r"\s*-\s*negative\s+\$(?P<amount_in_cents>[\d.]+).*$", has_amount: true, ..DEFAULT },
    // --- Noise (no features extracted) ---
    Suffix { pattern: r"\s*Foreign Currency Amount:?\s+\d+In\s+.*$", ..DEFAULT },
    Suffix { pattern: r"\s*-\s*[\w.+-]+@[\w.-]+$", ..DEFAULT },
    Suffix { pattern: r"\s+PTY\.?\s*LTD?\.?\s*$", ..DEFAULT },
    Suffix { pattern: r"\s+P/L\s*$", ..DEFAULT },
    Suffix { pattern: r"\s+\d{7,}$", ..DEFAULT },
];

fn parse_amount_cents(s: &str) -> Option<u32> {
    s.replace('.', "").parse().ok()
}

fn compiled_suffixes() -> &'static [CompiledSuffix] {
    static COMPILED: OnceLock<Vec<CompiledSuffix>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        SUFFIXES
            .iter()
            .map(|s| CompiledSuffix {
                regex: Regex::new(s.pattern).expect("invalid suffix pattern"),
                gateway: s.gateway,
                operation: s.operation,
                institution: s.institution,
                has_account: s.has_account,
                has_date: s.has_date,
                has_location: s.has_location,
                has_currency_code: s.has_currency_code,
                has_amount: s.has_amount,
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::NormalisationResult;

    #[test]
    fn test_card_and_date() {
        let mut r = NormalisationResult::new("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        apply(&mut r);
        assert_eq!(r.normalised, "WOOLWORTHS 1624 STRATHF");
        assert_eq!(r.features.date.as_deref(), Some("01/01/2026"));
        assert_eq!(r.features.account.as_deref(), Some("9172"));
    }

    #[test]
    fn test_full_card_number() {
        let mut r = NormalisationResult::new("MERCHANT Card 123456xxxxxx7890");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.account.as_deref(), Some("7890"));
    }

    #[test]
    fn test_standalone_value_date() {
        let mut r = NormalisationResult::new("MERCHANT Value Date: 15/03/2026");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.date.as_deref(), Some("15/03/2026"));
    }

    #[test]
    fn test_ns_aus() {
        let mut r = NormalisationResult::new("SOME MERCHANT NS AUS");
        apply(&mut r);
        assert_eq!(r.normalised, "SOME MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("NSW"));
    }

    #[test]
    fn test_state_postcode() {
        let mut r = NormalisationResult::new("MERCHANT NSW 2140");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("NSW 2140"));
    }

    #[test]
    fn test_au_aus() {
        let mut r = NormalisationResult::new("MERCHANT AU AUS");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("AU"));
    }

    #[test]
    fn test_state_only() {
        let mut r = NormalisationResult::new("MERCHANT VIC");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("VIC"));
    }

    #[test]
    fn test_pty_ltd() {
        let mut r = NormalisationResult::new("COMPANY NAME PTY LTD");
        apply(&mut r);
        assert_eq!(r.normalised, "COMPANY NAME");
    }

    #[test]
    fn test_alipay_gateway() {
        let mut r = NormalisationResult::new("MERCHANT - Alipay");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.gateway.as_deref(), Some("Alipay"));
    }

    #[test]
    fn test_long_reference() {
        let mut r = NormalisationResult::new("MERCHANT 12345678");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
    }

    #[test]
    fn test_prefix_then_suffix() {
        let mut r = NormalisationResult::new("SMP*CAFE NAME, Card xx1234 Value Date: 01/01/2026");
        crate::normalise::prefix::apply(&mut r);
        apply(&mut r);
        assert_eq!(r.normalised, "CAFE NAME");
        assert_eq!(r.features.gateway.as_deref(), Some("Square Marketplace"));
    }

    #[test]
    fn test_eftpos_receipt() {
        let mut r = NormalisationResult::new("MERCHANT - Eftpos Purchase - Receipt 123Date01/01");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
    }

    #[test]
    fn test_foreign_currency() {
        let mut r = NormalisationResult::new("MERCHANT SGD 12.50");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.currency_code.as_deref(), Some("SGD"));
        assert_eq!(r.features.amount_in_cents, Some(1250));
    }

    #[test]
    fn test_email_suffix() {
        let mut r = NormalisationResult::new("PAYPAL - paypal-aud@airbnb.com");
        apply(&mut r);
        assert_eq!(r.normalised, "PAYPAL");
    }
}
