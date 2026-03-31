/// OnceLock provides lazy one-time initialization of static data.
/// Patterns are compiled once on first use and reused for all subsequent calls.
use std::sync::OnceLock;

use regex::Regex;

use crate::known_entities::{
    KNOWN_BANKING_OPS, KNOWN_EMPLOYERS, KNOWN_LOCATIONS, KNOWN_MERCHANT_PATTERNS, KNOWN_PERSONS,
    PERSONS_STRIP_MEMO,
};

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

/// Title-case a string (capitalize first letter of each word).
pub fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    let lower: String = chars.as_str().to_lowercase();
                    format!("{upper}{lower}")
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Run the full normalisation pipeline on a payee string.
/// Currently a skeleton: strip -> expand -> title-case fallback.
pub fn normalise(original: &str) -> NormalisationResult {
    let (stripped, gateway) = strip_metadata(original);
    let expanded = expand_truncations(&stripped);
    let normalised = title_case(&expanded);

    NormalisationResult {
        original: original.to_string(),
        normalised,
        features: Features {
            payment_gateway: gateway,
            ..Default::default()
        },
        class: PayeeClass::Unclassified,
    }
}

// --- Feature Extraction ---

struct DirectionPattern {
    regex: Regex,
    direction: Direction,
}

fn direction_patterns() -> &'static Vec<DirectionPattern> {
    static PATTERNS: OnceLock<Vec<DirectionPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let raw: Vec<(&str, Direction)> = vec![
            (r"(?i)^Salary\b", Direction::Salary),
            (r"(?i)^PAY/SALARY FROM\b", Direction::Salary),
            (r"(?i)^Employer Contribution From\b", Direction::Salary),
            (r"(?i)^Salary - Salary Deposit", Direction::Salary),
            (r"(?i)^Fast Transfer From\b", Direction::TransferIn),
            (r"(?i)^Transfer From\b", Direction::TransferIn),
            (r"(?i)^From\b", Direction::TransferIn),
            (r"(?i)^Transfer [Tt]o\b", Direction::TransferOut),
            (r"(?i)^Fast Transfer To\b", Direction::TransferOut),
            (r"(?i)^To\b", Direction::TransferOut),
            (r"(?i)^Mortgage\s*-?\s*Transfer", Direction::TransferOut),
            (r"(?i)^Amex - Transfer", Direction::TransferOut),
            (r"- Osko Payment - Receipt", Direction::TransferIn),
            (r"- Osko Payment to", Direction::TransferOut),
            (r"(?i)^Direct Debit\b", Direction::DirectDebit),
            (r"(?i)^Direct Credit\b", Direction::DirectCredit),
            (r"(?i)^BPAY PAYMENT", Direction::BankingOperation),
            (r"(?i)^Loan Repayment", Direction::BankingOperation),
            (r"(?i)^Interest Charge", Direction::BankingOperation),
            (r"(?i)^Interest Adjustment", Direction::BankingOperation),
            (r"(?i)^Credit Card$", Direction::BankingOperation),
            (r"(?i)^Funds [Tt]ransfer$", Direction::BankingOperation),
            (r"(?i)^ACCOUNT SERVICING FEE$", Direction::BankingOperation),
            (r"(?i)^ONLINE PAYMENT", Direction::BankingOperation),
            (r"(?i)^PAYMENT FROM\b", Direction::BankingOperation),
            (r"(?i)^PAYMENT TO\b", Direction::BankingOperation),
            (r"(?i)^from account", Direction::BankingOperation),
            (r"(?i)^to account", Direction::BankingOperation),
            (r"(?i)^Wdl ATM", Direction::BankingOperation),
            (r"(?i)^CASH DEPOSIT", Direction::BankingOperation),
            (r"(?i)^Repayment/Payment", Direction::BankingOperation),
            (r"(?i)^Internal Transfer", Direction::BankingOperation),
        ];
        raw.into_iter()
            .map(|(p, d)| DirectionPattern {
                regex: Regex::new(p).expect("invalid direction pattern"),
                direction: d,
            })
            .collect()
    })
}

fn extract_direction(original: &str) -> Option<Direction> {
    for pat in direction_patterns() {
        if pat.regex.is_match(original) {
            return Some(pat.direction.clone());
        }
    }
    None
}

fn extract_employer(original: &str) -> Option<String> {
    static SALARY_PATTERNS: OnceLock<Vec<(Regex, usize)>> = OnceLock::new();
    let patterns = SALARY_PATTERNS.get_or_init(|| {
        vec![
            (Regex::new(r"(?i)^Salary [Ff]rom (.+?)(?:\s*-\s*.+)?$").unwrap(), 1),
            (Regex::new(r"(?i)^PAY/SALARY FROM (.+?)(?:\s+SALARY)?$").unwrap(), 1),
            (Regex::new(r"(?i)^Employer Contribution From (.+)$").unwrap(), 1),
            (Regex::new(r"^Salary (AFES)").unwrap(), 1),
        ]
    });

    for (regex, group) in patterns {
        if let Some(caps) = regex.captures(original) {
            if let Some(m) = caps.get(*group) {
                let employer_raw = m.as_str().trim();
                let upper = employer_raw.to_uppercase();
                for emp in KNOWN_EMPLOYERS {
                    if upper.contains(emp.pattern) {
                        return Some(emp.canonical.to_string());
                    }
                }
                return Some(title_case(employer_raw));
            }
        }
    }
    None
}

fn extract_account_ref(s: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\bxx(\d{4})\b").unwrap());
    re.captures(s).map(|c| format!("xx{}", &c[1]))
}

fn extract_person(stripped: &str, original: &str) -> Option<String> {
    let upper_stripped = stripped.to_uppercase();

    // Check persons_strip_memo: if stripped text starts with a known person, strip memo
    for memo_name in PERSONS_STRIP_MEMO {
        if upper_stripped.starts_with(memo_name) {
            let upper_name = &upper_stripped[..memo_name.len()];
            for person in KNOWN_PERSONS {
                if upper_name == person.pattern {
                    return Some(person.canonical.to_string());
                }
            }
        }
    }

    // Transfer entity extraction patterns on the original
    static TRANSFER_ENTITY_PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    let transfer_patterns = TRANSFER_ENTITY_PATTERNS.get_or_init(|| {
        vec![
            Regex::new(r"(?i)^Fast Transfer From (.+?),\s*to PayID.*$").unwrap(),
            Regex::new(r"(?i)^Fast Transfer From (.+?),\s*(.+)$").unwrap(),
            Regex::new(r"(?i)^Transfer To (.+?)\s+CommBank App.*$").unwrap(),
            Regex::new(r"(?i)^Transfer To (.+?),\s*CommBank App.*$").unwrap(),
            Regex::new(r"(?i)^Transfer To (.+?),\s*PayID.*$").unwrap(),
            Regex::new(r"(?i)^To (.+?)\s*-\s*.*$").unwrap(),
            Regex::new(r"(?i)^Transfer to (.+?)\s*-\s*Receipt.*$").unwrap(),
            Regex::new(r"(?i)^TRANSFER FROM (.+?)\s+/REF/.*$").unwrap(),
            Regex::new(r"(?i)^Transfer From (.+?)\s+(.+)$").unwrap(),
            Regex::new(r"(?i)^From (.+?)\s*-\s*.*$").unwrap(),
            Regex::new(r"^ANZ MOBILE BANKING PAYMENT \d+ TO (.+)$").unwrap(),
            Regex::new(r"^(.+?)\s*-\s*Osko Payment to\s.*$").unwrap(),
            Regex::new(r"^(.+?)\s+-\s*Osko Payment\s*-\s*Receipt.*$").unwrap(),
        ]
    });

    for re in transfer_patterns {
        if let Some(caps) = re.captures(original) {
            if let Some(m) = caps.get(1) {
                let extracted = m.as_str().trim();
                let upper_extracted = extracted.to_uppercase();
                for person in KNOWN_PERSONS {
                    if upper_extracted == person.pattern
                        || upper_extracted.starts_with(&format!("{} ", person.pattern))
                    {
                        return Some(person.canonical.to_string());
                    }
                }
                return Some(title_case(extracted));
            }
        }
    }

    // Direct match on stripped text
    for person in KNOWN_PERSONS {
        if upper_stripped == person.pattern {
            return Some(person.canonical.to_string());
        }
    }

    // Prefix match
    for person in KNOWN_PERSONS {
        if person.pattern.len() >= 4 && upper_stripped.starts_with(person.pattern) {
            let after = upper_stripped.len() == person.pattern.len()
                || upper_stripped.as_bytes().get(person.pattern.len()) == Some(&b' ');
            if after {
                return Some(person.canonical.to_string());
            }
        }
    }

    None
}

fn compiled_merchant_patterns() -> &'static Vec<(Regex, &'static str)> {
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        KNOWN_MERCHANT_PATTERNS
            .iter()
            .map(|mp| {
                let re = Regex::new(mp.pattern)
                    .unwrap_or_else(|e| panic!("invalid merchant pattern '{}': {}", mp.pattern, e));
                (re, mp.canonical)
            })
            .collect()
    })
}

fn extract_merchant(stripped: &str, original: &str) -> Option<String> {
    let upper_stripped = stripped.to_uppercase();
    let upper_original = original.to_uppercase();

    for (re, canonical) in compiled_merchant_patterns() {
        if re.is_match(&upper_original) || re.is_match(&upper_stripped) {
            return Some(canonical.to_string());
        }
    }
    None
}

fn extract_location(s: &str) -> Option<String> {
    let upper = s.to_uppercase();
    for &loc in KNOWN_LOCATIONS {
        if let Some(pos) = upper.find(loc) {
            let at_word_start = pos == 0 || !upper.as_bytes()[pos - 1].is_ascii_alphanumeric();
            let end = pos + loc.len();
            let at_word_end = end == upper.len() || !upper.as_bytes()[end].is_ascii_alphanumeric();
            if at_word_start && at_word_end {
                return Some(title_case(loc));
            }
        }
    }
    None
}

fn extract_banking_op(original: &str) -> Option<String> {
    let upper = original.to_uppercase();
    for op in KNOWN_BANKING_OPS {
        if upper.contains(op.pattern) {
            return Some(op.canonical.to_string());
        }
    }
    None
}

pub fn extract_features(stripped: &str, original: &str, gateway: Option<String>) -> Features {
    let mut f = Features {
        payment_gateway: gateway,
        ..Default::default()
    };

    f.direction = extract_direction(original);
    f.account_ref = extract_account_ref(original);

    if matches!(f.direction, Some(Direction::Salary)) {
        f.employer_name = extract_employer(original);
    }

    f.person_name = extract_person(stripped, original);
    f.merchant_name = extract_merchant(stripped, original);
    f.location = extract_location(stripped);

    if f.merchant_name.is_none() && f.person_name.is_none() {
        if let Some(op) = extract_banking_op(original) {
            f.bank_name = Some(op);
        }
    }

    f
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

    // --- Title case tests ---

    #[test]
    fn test_title_case_basic() {
        assert_eq!(title_case("WOOLWORTHS STRATHFIELD"), "Woolworths Strathfield");
    }

    #[test]
    fn test_title_case_empty() {
        assert_eq!(title_case(""), "");
    }

    // --- Skeleton normalise tests ---

    #[test]
    fn test_normalise_chains_strip_expand_titlecase() {
        let result = normalise("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        assert_eq!(result.normalised, "Woolworths 1624 Strathfield");
        assert_eq!(result.class, PayeeClass::Unclassified);
    }

    #[test]
    fn test_normalise_with_gateway() {
        let result = normalise("SQ *CAFE BLUE SYDNEY AU");
        assert_eq!(result.features.payment_gateway.as_deref(), Some("Square"));
        assert!(result.normalised.contains("Cafe Blue"));
    }

    #[test]
    fn test_normalise_preserves_original() {
        let original = "SMP*TEST MERCHANT PTY LTD";
        let result = normalise(original);
        assert_eq!(result.original, original);
    }

    // --- Direction/employer/account_ref extraction tests ---

    #[test]
    fn test_extract_direction_salary() {
        assert_eq!(extract_direction("Salary from Apple Pty Ltd"), Some(Direction::Salary));
        assert_eq!(extract_direction("PAY/SALARY FROM APPLE"), Some(Direction::Salary));
    }

    #[test]
    fn test_extract_direction_transfer() {
        assert_eq!(extract_direction("Transfer to xx8005 CommBank app"), Some(Direction::TransferOut));
        assert_eq!(extract_direction("Fast Transfer From John Smith"), Some(Direction::TransferIn));
    }

    #[test]
    fn test_extract_direction_banking() {
        assert_eq!(extract_direction("Interest Charge"), Some(Direction::BankingOperation));
        assert_eq!(extract_direction("BPAY PAYMENT 12345"), Some(Direction::BankingOperation));
    }

    #[test]
    fn test_extract_direction_none() {
        assert_eq!(extract_direction("Woolworths Strathfield"), None);
    }

    #[test]
    fn test_extract_employer_apple() {
        assert_eq!(extract_employer("Salary from Apple Pty Ltd"), Some("Apple".into()));
    }

    #[test]
    fn test_extract_account_ref() {
        assert_eq!(extract_account_ref("Transfer to xx8005 CommBank app"), Some("xx8005".into()));
        assert_eq!(extract_account_ref("Woolworths"), None);
    }

    // --- Person extraction tests ---

    #[test]
    fn test_extract_person_known() {
        assert_eq!(
            extract_person("JOHNNY TAM", "Fast Transfer From Johnny Tam, to PayID Phone"),
            Some("Johnny Tam".into())
        );
    }

    #[test]
    fn test_extract_person_transfer_entity() {
        assert_eq!(
            extract_person("", "Transfer To Nelson Tam CommBank App something"),
            Some("Nelson Tam".into())
        );
    }

    #[test]
    fn test_extract_person_none() {
        assert_eq!(extract_person("WOOLWORTHS", "WOOLWORTHS"), None);
    }

    // --- Merchant/location/banking extraction tests ---

    #[test]
    fn test_extract_merchant_woolworths() {
        assert_eq!(extract_merchant("WOOLWORTHS 1624 STRATHFIELD", "WOOLWORTHS 1624 STRATHFIELD"), Some("Woolworths".into()));
    }

    #[test]
    fn test_extract_merchant_banking_identity() {
        assert_eq!(
            extract_merchant("Direct Debit 123 AUSTRALIAN FELLO", "Direct Debit 123 AUSTRALIAN FELLO"),
            Some("AFES (Donation)".into())
        );
    }

    #[test]
    fn test_extract_location() {
        assert_eq!(extract_location("WOOLWORTHS 1624 STRATHFIELD"), Some("Strathfield".into()));
        assert_eq!(extract_location("COLES BURWOOD"), Some("Burwood".into()));
        assert_eq!(extract_location("SOME RANDOM TEXT"), None);
    }

    #[test]
    fn test_extract_features_wires_all() {
        let f = extract_features("WOOLWORTHS 1624 STRATHFIELD", "WOOLWORTHS 1624 STRATHFIELD", None);
        assert_eq!(f.merchant_name.as_deref(), Some("Woolworths"));
        assert_eq!(f.location.as_deref(), Some("Strathfield"));
        assert!(f.person_name.is_none());
    }
}
