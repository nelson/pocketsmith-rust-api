mod prefix;
use prefix::prefix_patterns;
mod suffix;
use suffix::suffix_patterns;

use regex::Regex;
use std::sync::OnceLock;

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

pub struct StripResult {
    pub stripped: String,
    pub features: Features,
}

/// Extract named capture groups from a regex match into features.
fn extract_captures(caps: &regex::Captures, features: &mut Features, pat: &StripPattern) {
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

/// Strip metadata prefixes and suffixes from a payee string.
///
/// Uses a unified single loop: each iteration tries all prefix patterns
/// then all suffix patterns, strips the first match, and restarts.
pub fn strip_metadata(payee: &str) -> StripResult {
    let mut s = payee.to_string();
    let mut features = Features::default();

    loop {
        let mut matched = false;

        for pat in prefix_patterns() {
            if let Some(caps) = pat.regex.captures(&s) {
                extract_captures(&caps, &mut features, pat);
                s = s[caps.get(0).unwrap().end()..].to_string();
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }

        for pat in suffix_patterns() {
            if let Some(caps) = pat.regex.captures(&s) {
                extract_captures(&caps, &mut features, pat);
                s = s[..caps.get(0).unwrap().start()].to_string();
                matched = true;
                break;
            }
        }

        if !matched {
            break;
        }
    }

    s = s.trim().to_string();
    StripResult { stripped: s, features }
}

/// Suffix-only variant (used by normalise_check binary).
pub fn strip_metadata_suffix_only(payee: &str) -> StripResult {
    let mut s = payee.to_string();
    let mut features = Features::default();

    for pat in suffix_patterns() {
        if let Some(caps) = pat.regex.captures(&s) {
            extract_captures(&caps, &mut features, pat);
            s = s[..caps.get(0).unwrap().start()].to_string();
        }
    }

    s = s.trim().to_string();
    StripResult { stripped: s, features }
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

struct Expansion {
    from: &'static str,
    to: &'static str,
    is_location: bool,
}

fn truncation_expansions() -> &'static Vec<Expansion> {
    static EXPANSIONS: OnceLock<Vec<Expansion>> = OnceLock::new();
    EXPANSIONS.get_or_init(|| {
        vec![
            // Multi-word suburbs (longest first)
            Expansion { from: "NORTH STRATHFIE", to: "NORTH STRATHFIELD", is_location: true },
            Expansion { from: "NORTH STRATHFAU", to: "NORTH STRATHFIELD", is_location: true },
            Expansion { from: "NORTH STRATHF", to: "NORTH STRATHFIELD", is_location: true },
            Expansion { from: "NORTH STRATH", to: "NORTH STRATHFIELD", is_location: true },
            Expansion { from: "STRATHFIEL", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRATHFIE", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRATHFI", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRATHFAU", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRATHF", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRATH", to: "STRATHFIELD", is_location: true },
            Expansion { from: "STRAT", to: "STRATHFIELD", is_location: true },
            Expansion { from: "NORTH RY", to: "NORTH RYDE", is_location: true },
            Expansion { from: "WEST RY", to: "WEST RYDE", is_location: true },
            Expansion { from: "BURWOO", to: "BURWOOD", is_location: true },
            Expansion { from: "BURWO", to: "BURWOOD", is_location: true },
            Expansion { from: "BURW", to: "BURWOOD", is_location: true },
            Expansion { from: "MACQUARIE PAR", to: "MACQUARIE PARK", is_location: true },
            Expansion { from: "MACQUARIE PA", to: "MACQUARIE PARK", is_location: true },
            Expansion { from: "MACQUARIE CEN", to: "MACQUARIE CENTRE", is_location: true },
            Expansion { from: "MACQUARI", to: "MACQUARIE", is_location: true },
            Expansion { from: "MACQUAR", to: "MACQUARIE", is_location: true },
            Expansion { from: "HABERFIEL", to: "HABERFIELD", is_location: true },
            Expansion { from: "HEBERFIELD", to: "HABERFIELD", is_location: true },
            Expansion { from: "HOMEBUSH WES", to: "HOMEBUSH WEST", is_location: true },
            Expansion { from: "HOMEBUSH WEA", to: "HOMEBUSH WEST", is_location: true },
            Expansion { from: "SOUTH GRANVIL", to: "SOUTH GRANVILLE", is_location: true },
            Expansion { from: "DARLINGHURS", to: "DARLINGHURST", is_location: true },
            Expansion { from: "WOOLLOOMOOL", to: "WOOLLOOMOOLOO", is_location: true },
            Expansion { from: "BALGOWNI", to: "BALGOWNIE", is_location: true },
            Expansion { from: "COOLANGATT", to: "COOLANGATTA", is_location: true },
            Expansion { from: "PARRAMATT", to: "PARRAMATTA", is_location: true },
            Expansion { from: "BARANGARO", to: "BARANGAROO", is_location: true },
            Expansion { from: "PETERSHA", to: "PETERSHAM", is_location: true },
            Expansion { from: "STANMOR", to: "STANMORE", is_location: true },
            Expansion { from: "SURFERS PARADIS", to: "SURFERS PARADISE", is_location: true },
            Expansion { from: "MELBOURNE AIRPO", to: "MELBOURNE AIRPORT", is_location: true },
            Expansion { from: "MARSFIEL", to: "MARSFIELD", is_location: true },
            Expansion { from: "MARSFIE", to: "MARSFIELD", is_location: true },
            Expansion { from: "NEWINGT", to: "NEWINGTON", is_location: true },
            Expansion { from: "CHULLOR", to: "CHULLORA", is_location: true },
            Expansion { from: "CONCOR", to: "CONCORD", is_location: true },
            Expansion { from: "CROYD", to: "CROYDON", is_location: true },
            Expansion { from: "PALM BEAC", to: "PALM BEACH", is_location: true },
            Expansion { from: "MONA VAL", to: "MONA VALE", is_location: true },
            Expansion { from: "SUMMER HIL", to: "SUMMER HILL", is_location: true },
            Expansion { from: "BROADWA", to: "BROADWAY", is_location: true },
            Expansion { from: "BROADW", to: "BROADWAY", is_location: true },
            Expansion { from: "GATEWA", to: "GATEWAY", is_location: true },
            Expansion { from: "CHARLESTOW", to: "CHARLESTOWN", is_location: true },
            Expansion { from: "HEATHCO", to: "HEATHCOTE", is_location: true },
            Expansion { from: "KIRRIBILL", to: "KIRRIBILLI", is_location: true },
            Expansion { from: "SHELL COV", to: "SHELL COVE", is_location: true },
            Expansion { from: "SHELL C", to: "SHELL COVE", is_location: true },
            Expansion { from: "BOMADERR", to: "BOMADERRY", is_location: true },
            Expansion { from: "WOLLONGON", to: "WOLLONGONG", is_location: true },
            Expansion { from: "HURSTV", to: "HURSTVILLE", is_location: true },
            Expansion { from: "FIVE DOC", to: "FIVE DOCK", is_location: true },
            Expansion { from: "ASHFIEL", to: "ASHFIELD", is_location: true },
            Expansion { from: "BELFIEL", to: "BELFIELD", is_location: true },
            Expansion { from: "CROWS NES", to: "CROWS NEST", is_location: true },
            Expansion { from: "DICKSO", to: "DICKSON", is_location: true },
            Expansion { from: "FORTITUD", to: "FORTITUDE VALLEY", is_location: true },
            // Word truncations
            Expansion { from: "PHARMCY", to: "PHARMACY", is_location: false },
            Expansion { from: "MKTPL", to: "MARKETPLACE", is_location: false },
            Expansion { from: "MKTPLC", to: "MARKETPLACE", is_location: false },
            Expansion { from: "RETA", to: "RETAIL", is_location: false },
            Expansion { from: "AUSTRA", to: "AUSTRALIA", is_location: false },
            Expansion { from: "SUPERMARKE", to: "SUPERMARKET", is_location: false },
            Expansion { from: "SUPERMAR", to: "SUPERMARKET", is_location: false },
            Expansion { from: "RESTAURAN", to: "RESTAURANT", is_location: false },
            Expansion { from: "INTERNATIO", to: "INTERNATIONAL", is_location: false },
            Expansion { from: "INTERNATIONA", to: "INTERNATIONAL", is_location: false },
            Expansion { from: "ENTERPRI", to: "ENTERPRISES", is_location: false },
            Expansion { from: "ENTERPRIS", to: "ENTERPRISES", is_location: false },
            Expansion { from: "ENTERPRISE", to: "ENTERPRISES", is_location: false },
            Expansion { from: "CHOCOLA", to: "CHOCOLATES", is_location: false },
            Expansion { from: "ACUPUNCT", to: "ACUPUNCTURE", is_location: false },
            Expansion { from: "CHEMIS", to: "CHEMIST", is_location: false },
            Expansion { from: "CHEMI", to: "CHEMIST", is_location: false },
            Expansion { from: "KITCHE", to: "KITCHEN", is_location: false },
            Expansion { from: "KITCH", to: "KITCHEN", is_location: false },
            Expansion { from: "GELAT", to: "GELATO", is_location: false },
            Expansion { from: "ENTERTAIN", to: "ENTERTAINMENT", is_location: false },
            Expansion { from: "ENTERTAINMEN", to: "ENTERTAINMENT", is_location: false },
            Expansion { from: "BOULEVAR", to: "BOULEVARD", is_location: false },
            Expansion { from: "TOWE", to: "TOWER", is_location: false },
            Expansion { from: "COF", to: "COFFEE", is_location: false },
            Expansion { from: "COFF", to: "COFFEE", is_location: false },
            Expansion { from: "COSME", to: "COSMETICS", is_location: false },
            Expansion { from: "STARBUC", to: "STARBUCKS", is_location: false },
            Expansion { from: "BREADTO", to: "BREADTOP", is_location: false },
        ]
    })
}

/// Expand truncated words in a payee string using word-boundary matching.
pub fn expand_truncations(s: &str) -> String {
    let mut result = s.to_string();
    let mut changed = true;

    while changed {
        changed = false;
        let upper = result.to_uppercase();

        for exp in truncation_expansions() {
            if exp.from == exp.to {
                continue;
            }

            if let Some(pos) = upper.find(exp.from) {
                let at_word_start =
                    pos == 0 || !upper.as_bytes()[pos - 1].is_ascii_alphanumeric();
                let end = pos + exp.from.len();
                let at_word_end =
                    end == upper.len() || !upper.as_bytes()[end].is_ascii_alphanumeric();

                if at_word_start && at_word_end {
                    result = format!("{}{}{}", &result[..pos], exp.to, &result[end..]);
                    changed = true;
                    break;
                }
            }
        }
    }

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
        assert_eq!(r.features.payment_gateway.as_deref(), Some("Square"));
    }

    #[test]
    fn test_strip_prefix_doordash() {
        let r = strip_metadata("DOORDASH*THAI PLACE");
        assert_eq!(r.stripped, "THAI PLACE");
        assert_eq!(r.features.payment_gateway.as_deref(), Some("DoorDash"));
    }

    #[test]
    fn test_strip_prefix_visa_debit() {
        let r = strip_metadata("Visa Debit Purchase Card 9172 MERCHANT NAME");
        assert_eq!(r.stripped, "MERCHANT NAME");
        assert_eq!(r.features.account_ref.as_deref(), Some("9172"));
    }

    #[test]
    fn test_strip_prefix_date() {
        let r = strip_metadata("28/01/26, Direct Debit 123 ENTITY");
        assert_eq!(r.stripped, "Direct Debit 123 ENTITY");
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
    }

    #[test]
    fn test_strip_prefix_none() {
        let r = strip_metadata("Woolworths Strathfield");
        assert_eq!(r.stripped, "Woolworths Strathfield");
        assert!(r.features.payment_gateway.is_none());
    }

    #[test]
    fn test_strip_prefix_paypal() {
        let r = strip_metadata("PAYPAL *SOME STORE");
        assert_eq!(r.stripped, "SOME STORE");
        assert_eq!(r.features.payment_gateway.as_deref(), Some("PayPal"));
    }

    #[test]
    fn test_strip_multiple_prefixes() {
        let r = strip_metadata("28/01/26, SQ *COFFEE SHOP");
        assert_eq!(r.stripped, "COFFEE SHOP");
        assert_eq!(r.features.payment_gateway.as_deref(), Some("Square"));
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
    }

    // --- Strip metadata suffix tests ---

    #[test]
    fn test_strip_suffix_card() {
        let r = strip_metadata("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        assert_eq!(r.stripped, "WOOLWORTHS 1624 STRATHF");
        assert_eq!(r.features.date.as_deref(), Some("01/01/2026"));
        assert_eq!(r.features.account_ref.as_deref(), Some("9172"));
    }

    #[test]
    fn test_strip_suffix_full_card_number() {
        let r = strip_metadata("MERCHANT Card 123456xxxxxx7890");
        assert_eq!(r.stripped, "MERCHANT");
        assert_eq!(r.features.account_ref.as_deref(), Some("7890"));
    }

    #[test]
    fn test_strip_suffix_standalone_value_date() {
        let r = strip_metadata("MERCHANT Value Date: 15/03/2026");
        assert_eq!(r.stripped, "MERCHANT");
        assert_eq!(r.features.date.as_deref(), Some("15/03/2026"));
    }

    #[test]
    fn test_strip_suffix_country_code() {
        let r = strip_metadata("SOME MERCHANT NSWAU");
        assert_eq!(r.stripped, "SOME MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("NSW"));
    }

    #[test]
    fn test_strip_suffix_state_postcode() {
        let r = strip_metadata("MERCHANT NSW 2140");
        assert_eq!(r.stripped, "MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("NSW 2140"));
    }

    #[test]
    fn test_strip_suffix_au_aus() {
        let r = strip_metadata("MERCHANT AU AUS");
        assert_eq!(r.stripped, "MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("AU"));
    }

    #[test]
    fn test_strip_suffix_state_only() {
        let r = strip_metadata("MERCHANT VIC");
        assert_eq!(r.stripped, "MERCHANT");
        assert_eq!(r.features.location.as_deref(), Some("VIC"));
    }

    #[test]
    fn test_strip_suffix_pty_ltd() {
        let r = strip_metadata("COMPANY NAME PTY LTD");
        assert_eq!(r.stripped, "COMPANY NAME");
    }

    #[test]
    fn test_strip_suffix_alipay_gateway() {
        let r = strip_metadata("MERCHANT - Alipay");
        assert_eq!(r.stripped, "MERCHANT");
        assert_eq!(r.features.payment_gateway.as_deref(), Some("Alipay"));
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
        assert_eq!(r.features.payment_gateway.as_deref(), Some("Square Marketplace"));
    }

    #[test]
    fn test_strip_eftpos_receipt() {
        let r = strip_metadata("MERCHANT - Eftpos Purchase - Receipt 123Date01/01");
        assert_eq!(r.stripped, "MERCHANT");
    }

    #[test]
    fn test_strip_suffix_foreign_currency() {
        let r = strip_metadata("MERCHANT SGD 12.50");
        assert_eq!(r.stripped, "MERCHANT");
        assert_eq!(r.features.foreign_currency.as_deref(), Some("SGD"));
        assert_eq!(r.features.foreign_amount, Some(1250));
    }

    #[test]
    fn test_strip_email_suffix() {
        let r = strip_metadata("PAYPAL - paypal-aud@airbnb.com");
        assert_eq!(r.stripped, "PAYPAL");
    }

    // --- Expand truncations tests ---

    #[test]
    fn test_expand_strathfield() {
        assert_eq!(expand_truncations("WOOLWORTHS 1624 STRATHF"), "WOOLWORTHS 1624 STRATHFIELD");
    }

    #[test]
    fn test_expand_burwood() {
        assert_eq!(expand_truncations("COLES BURWOO"), "COLES BURWOOD");
    }

    #[test]
    fn test_expand_pharmacy() {
        assert_eq!(expand_truncations("DISCOUNT PHARMCY"), "DISCOUNT PHARMACY");
    }

    #[test]
    fn test_expand_no_partial_match() {
        assert_eq!(expand_truncations("STRATEGIC PLAN"), "STRATEGIC PLAN");
    }

    #[test]
    fn test_expand_multiple() {
        assert_eq!(expand_truncations("PHARMCY BURWOO"), "PHARMACY BURWOOD");
    }

    #[test]
    fn test_expand_north_strathfield() {
        assert_eq!(expand_truncations("SHOP NORTH STRATHF"), "SHOP NORTH STRATHFIELD");
    }
}
