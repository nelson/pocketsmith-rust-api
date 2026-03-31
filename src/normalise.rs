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

fn suffix_patterns() -> &'static Vec<StripPattern> {
    static PATTERNS: OnceLock<Vec<StripPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let patterns: Vec<(&str, &'static str)> = vec![
            (r",?\s*Card xx\d{4}.*$", "Card value date"),
            (r"\s+Card xx\d{4}.*$", "Card value date (space)"),
            (r"\s+Tap and Pay xx\d{4}.*$", "Tap and Pay"),
            (r"\s*-?\s*Visa Purchase\s*-\s*Receipt\s+\w+\s*In\s+.*$", "Visa Purchase receipt"),
            (r"\s*-?\s*Visa Refund\s*-\s*Receipt\s+.*$", "Visa Refund receipt"),
            (r"\s*-?\s*Osko Payment.*Receipt\s+\d+.*$", "Osko Payment receipt"),
            (r"\s*-\s*Deposit\s*-\s*Receipt\s+.*$", "Deposit receipt"),
            (r"\s*-\s*Alipay$", "Alipay suffix"),
            (r"\s+Card\s+\d{6}x{6}\d{4}$", "Full card number"),
            (r"\s+Value [Dd]ate:?\s+\d{2}/\d{2}/\d{4}$", "Standalone value date"),
            (r"\s+NSWAU$", "NSWAU suffix"),
            (r"\s+NS AUS$", "NS AUS suffix"),
            (r"\s+AU AUS$", "AU AUS suffix"),
            (r"\s+AUS$", "AUS suffix"),
            (r"\s+AU$", "AU suffix"),
            (r"\s+NLD$", "NLD suffix"),
            (r"\s+SGP$", "SGP suffix"),
            (r"\s+USA$", "USA suffix"),
            (r"\s+IDN$", "IDN suffix"),
            (r"\s+GBR$", "GBR suffix"),
            (r"\s+[A-Z]{3}\s+\d+\.\d{2}$", "Foreign currency amount"),
            (r"\s*,\s*\d{4}$", "Trailing code"),
            (r"\s*-\s*negative\s+\$[\d.]+.*$", "Negative amount"),
            (r"\s*-?\s*Eftpos (?:Purchase|Cash Out)\s*-\s*Receipt\s+.*$", "EFTPOS receipt"),
            (r"\s+Eftpos Purchase\s*-\s*Receipt\s+.*$", "EFTPOS Purchase receipt"),
            (r"\s*-\s*Eftpos Purchase\s*-\s*Receipt\s+\d+Date.*$", "EFTPOS receipt (no space)"),
            (r"\s*,\s*\d{4}\s+Last 4 Card Digits\s+\d{4}$", "Last 4 Card Digits"),
            (r"\s*Foreign Currency Amount:?\s+\d+In\s+.*$", "Foreign currency receipt"),
            (r"\s*,?\s*\d{4}\s+Last\s+4\s+Card\s+Digits\s+\d{4}$", "Last 4 card digits"),
            (r"\s*-\s*Internal Transfer\s*-\s*Receipt\s+\d+.*$", "Internal Transfer receipt"),
            (r"\s+Card\s+\d[A-Z]\d{4}[A-Za-z]{6}\d{4}$", "Masked card number"),
            (r"\s*-\s*[\w.+-]+@[\w.-]+$", "Email suffix"),
            (r"\s+PTY\.?\s*LTD?\.?\s*$", "PTY LTD suffix"),
            (r"\s+P/L\s*$", "P/L suffix"),
            (r"\s+\d{7,}$", "Long reference number"),
            (r"\s+(?:NSW|VIC|QLD|WA|SA|TAS|ACT|NT)\s+\d{4,6}$", "State + postcode"),
            (r"\s+(?:NSW|VIC|QLD|WA|SA|TAS|ACT|NT)$", "State suffix"),
        ];
        patterns
            .into_iter()
            .map(|(p, n)| StripPattern {
                regex: Regex::new(p).expect("invalid suffix pattern"),
                name: n,
                is_gateway: false,
            })
            .collect()
    })
}

/// Strip metadata prefixes and suffixes from a payee string.
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

    for pat in suffix_patterns() {
        if let Some(m) = pat.regex.find(&s) {
            s = s[..m.start()].to_string();
        }
    }

    s = s.trim().to_string();
    (s, gateway, date)
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

    // --- Strip metadata suffix tests ---

    #[test]
    fn test_strip_suffix_card() {
        let (stripped, _, _) = strip_metadata("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        assert_eq!(stripped, "WOOLWORTHS 1624 STRATHF");
    }

    #[test]
    fn test_strip_suffix_country_code() {
        let (stripped, _, _) = strip_metadata("SOME MERCHANT NSWAU");
        assert_eq!(stripped, "SOME MERCHANT");
    }

    #[test]
    fn test_strip_suffix_state_postcode() {
        let (stripped, _, _) = strip_metadata("MERCHANT NSW 2140");
        assert_eq!(stripped, "MERCHANT");
    }

    #[test]
    fn test_strip_suffix_pty_ltd() {
        let (stripped, _, _) = strip_metadata("COMPANY NAME PTY LTD");
        assert_eq!(stripped, "COMPANY NAME");
    }

    #[test]
    fn test_strip_suffix_long_reference() {
        let (stripped, _, _) = strip_metadata("MERCHANT 12345678");
        assert_eq!(stripped, "MERCHANT");
    }

    #[test]
    fn test_strip_both_prefix_and_suffix() {
        let (stripped, gw, _) = strip_metadata("SMP*CAFE NAME, Card xx1234 Value Date: 01/01/2026");
        assert_eq!(stripped, "CAFE NAME");
        assert_eq!(gw.as_deref(), Some("Square Marketplace"));
    }

    #[test]
    fn test_strip_eftpos_receipt() {
        let (stripped, _, _) = strip_metadata("MERCHANT - Eftpos Purchase - Receipt 123Date01/01");
        assert_eq!(stripped, "MERCHANT");
    }

    #[test]
    fn test_strip_email_suffix() {
        let (stripped, _, _) = strip_metadata("PAYPAL - paypal-aud@airbnb.com");
        assert_eq!(stripped, "PAYPAL");
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
