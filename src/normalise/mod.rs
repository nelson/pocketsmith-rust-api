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
pub fn strip_metadata(payee: &str) -> StripResult {
    let mut s = payee.to_string();
    let mut features = Features::default();

    // Strips multiple prefixes — typically one or more non-gateway prefixes
    // and at most one gateway prefix.
    loop {
        let mut matched = false;
        for pat in prefix_patterns() {
            if let Some(caps) = pat.regex.captures(&s) {
                extract_captures(&caps, &mut features, pat);
                s = s[caps.get(0).unwrap().end()..].to_string();
                matched = true;
                break; // restart from beginning of pattern list
            }
        }
        if !matched {
            break;
        }
    }

    for pat in suffix_patterns() {
        if let Some(caps) = pat.regex.captures(&s) {
            extract_captures(&caps, &mut features, pat);
            s = s[..caps.get(0).unwrap().start()].to_string();
        }
    }

    s = s.trim().to_string();
    StripResult { stripped: s, features }
}

/// Suffix-only variant: skip prefix stripping entirely.
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

/// Unified single-loop variant: all patterns (prefix then suffix) in one
/// iterative loop. Each iteration tries every pattern; strips the match
/// (prefix from start, suffix from end) and restarts until nothing matches.
pub fn strip_metadata_unified(payee: &str) -> StripResult {
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
}

fn truncation_expansions() -> &'static Vec<Expansion> {
    static EXPANSIONS: OnceLock<Vec<Expansion>> = OnceLock::new();
    EXPANSIONS.get_or_init(|| {
        vec![
            // Multi-word suburbs (longest first)
            Expansion { from: "NORTH STRATHFIE", to: "NORTH STRATHFIELD" },
            Expansion { from: "NORTH STRATHFAU", to: "NORTH STRATHFIELD" },
            Expansion { from: "NORTH STRATHF", to: "NORTH STRATHFIELD" },
            Expansion { from: "NORTH STRATH", to: "NORTH STRATHFIELD" },
            Expansion { from: "STRATHFIEL", to: "STRATHFIELD" },
            Expansion { from: "STRATHFIE", to: "STRATHFIELD" },
            Expansion { from: "STRATHFI", to: "STRATHFIELD" },
            Expansion { from: "STRATHFAU", to: "STRATHFIELD" },
            Expansion { from: "STRATHF", to: "STRATHFIELD" },
            Expansion { from: "STRATH", to: "STRATHFIELD" },
            Expansion { from: "STRAT", to: "STRATHFIELD" },
            Expansion { from: "NORTH RY", to: "NORTH RYDE" },
            Expansion { from: "WEST RY", to: "WEST RYDE" },
            Expansion { from: "BURWOO", to: "BURWOOD" },
            Expansion { from: "BURWO", to: "BURWOOD" },
            Expansion { from: "BURW", to: "BURWOOD" },
            Expansion { from: "MACQUARIE PAR", to: "MACQUARIE PARK" },
            Expansion { from: "MACQUARIE PA", to: "MACQUARIE PARK" },
            Expansion { from: "MACQUARIE CEN", to: "MACQUARIE CENTRE" },
            Expansion { from: "MACQUARI", to: "MACQUARIE" },
            Expansion { from: "MACQUAR", to: "MACQUARIE" },
            Expansion { from: "HABERFIEL", to: "HABERFIELD" },
            Expansion { from: "HEBERFIELD", to: "HABERFIELD" },
            Expansion { from: "HOMEBUSH WES", to: "HOMEBUSH WEST" },
            Expansion { from: "HOMEBUSH WEA", to: "HOMEBUSH WEST" },
            Expansion { from: "SOUTH GRANVIL", to: "SOUTH GRANVILLE" },
            Expansion { from: "DARLINGHURS", to: "DARLINGHURST" },
            Expansion { from: "WOOLLOOMOOL", to: "WOOLLOOMOOLOO" },
            Expansion { from: "BALGOWNI", to: "BALGOWNIE" },
            Expansion { from: "COOLANGATT", to: "COOLANGATTA" },
            Expansion { from: "PARRAMATT", to: "PARRAMATTA" },
            Expansion { from: "BARANGARO", to: "BARANGAROO" },
            Expansion { from: "PETERSHA", to: "PETERSHAM" },
            Expansion { from: "STANMOR", to: "STANMORE" },
            Expansion { from: "SURFERS PARADIS", to: "SURFERS PARADISE" },
            Expansion { from: "MELBOURNE AIRPO", to: "MELBOURNE AIRPORT" },
            Expansion { from: "MARSFIEL", to: "MARSFIELD" },
            Expansion { from: "MARSFIE", to: "MARSFIELD" },
            Expansion { from: "NEWINGT", to: "NEWINGTON" },
            Expansion { from: "CHULLOR", to: "CHULLORA" },
            Expansion { from: "CONCOR", to: "CONCORD" },
            Expansion { from: "CROYD", to: "CROYDON" },
            Expansion { from: "PALM BEAC", to: "PALM BEACH" },
            Expansion { from: "MONA VAL", to: "MONA VALE" },
            Expansion { from: "SUMMER HIL", to: "SUMMER HILL" },
            Expansion { from: "BROADWA", to: "BROADWAY" },
            Expansion { from: "BROADW", to: "BROADWAY" },
            Expansion { from: "GATEWA", to: "GATEWAY" },
            Expansion { from: "CHARLESTOW", to: "CHARLESTOWN" },
            Expansion { from: "HEATHCO", to: "HEATHCOTE" },
            Expansion { from: "KIRRIBILL", to: "KIRRIBILLI" },
            Expansion { from: "SHELL COV", to: "SHELL COVE" },
            Expansion { from: "SHELL C", to: "SHELL COVE" },
            Expansion { from: "BOMADERR", to: "BOMADERRY" },
            Expansion { from: "WOLLONGON", to: "WOLLONGONG" },
            Expansion { from: "HURSTV", to: "HURSTVILLE" },
            Expansion { from: "FIVE DOC", to: "FIVE DOCK" },
            Expansion { from: "ASHFIEL", to: "ASHFIELD" },
            Expansion { from: "BELFIEL", to: "BELFIELD" },
            Expansion { from: "CROWS NES", to: "CROWS NEST" },
            Expansion { from: "DICKSO", to: "DICKSON" },
            Expansion { from: "FORTITUD", to: "FORTITUDE VALLEY" },
            // Word truncations
            Expansion { from: "PHARMCY", to: "PHARMACY" },
            Expansion { from: "MKTPL", to: "MARKETPLACE" },
            Expansion { from: "MKTPLC", to: "MARKETPLACE" },
            Expansion { from: "RETA", to: "RETAIL" },
            Expansion { from: "AUSTRA", to: "AUSTRALIA" },
            Expansion { from: "SUPERMARKE", to: "SUPERMARKET" },
            Expansion { from: "SUPERMAR", to: "SUPERMARKET" },
            Expansion { from: "RESTAURAN", to: "RESTAURANT" },
            Expansion { from: "INTERNATIO", to: "INTERNATIONAL" },
            Expansion { from: "INTERNATIONA", to: "INTERNATIONAL" },
            Expansion { from: "ENTERPRI", to: "ENTERPRISES" },
            Expansion { from: "ENTERPRIS", to: "ENTERPRISES" },
            Expansion { from: "ENTERPRISE", to: "ENTERPRISES" },
            Expansion { from: "CHOCOLA", to: "CHOCOLATES" },
            Expansion { from: "ACUPUNCT", to: "ACUPUNCTURE" },
            Expansion { from: "CHEMIS", to: "CHEMIST" },
            Expansion { from: "CHEMI", to: "CHEMIST" },
            Expansion { from: "KITCHE", to: "KITCHEN" },
            Expansion { from: "KITCH", to: "KITCHEN" },
            Expansion { from: "GELAT", to: "GELATO" },
            Expansion { from: "ENTERTAIN", to: "ENTERTAINMENT" },
            Expansion { from: "ENTERTAINMEN", to: "ENTERTAINMENT" },
            Expansion { from: "BOULEVAR", to: "BOULEVARD" },
            Expansion { from: "TOWE", to: "TOWER" },
            Expansion { from: "COF", to: "COFFEE" },
            Expansion { from: "COFF", to: "COFFEE" },
            Expansion { from: "COSME", to: "COSMETICS" },
            Expansion { from: "STARBUC", to: "STARBUCKS" },
            Expansion { from: "BREADTO", to: "BREADTOP" },
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
