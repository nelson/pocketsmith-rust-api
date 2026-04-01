/// OnceLock provides lazy one-time initialization of static data.
/// Patterns are compiled once on first use and reused for all subsequent calls.
use std::sync::OnceLock;

use regex::Regex;

pub(crate) struct StripPattern {
    pub(crate) regex: Regex,
    pub(crate) name: &'static str,
    pub(crate) is_gateway: bool,
}

pub(crate) fn prefix_patterns() -> &'static [StripPattern] {
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

pub(crate) fn suffix_patterns() -> &'static [StripPattern] {
    static PATTERNS: OnceLock<Vec<StripPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let patterns: Vec<(&str, &'static str, bool)> = vec![
            (r",?\s*Card xx(?P<account_ref>\d{4}).*?(?P<date>\d{2}/\d{2}/\d{4}).*$", "Card value date", false),
            (r"\s+Card xx(?P<account_ref>\d{4}).*?(?P<date>\d{2}/\d{2}/\d{4}).*$", "Card value date (space)", false),
            (r"\s+Tap and Pay xx(?P<account_ref>\d{4}).*$", "Tap and Pay", false),
            (r"\s*-?\s*Visa Purchase\s*-\s*Receipt\s+\w+\s*In\s+.*$", "Visa Purchase receipt", false),
            (r"\s*-?\s*Visa Refund\s*-\s*Receipt\s+.*$", "Visa Refund receipt", false),
            (r"\s*-?\s*Osko Payment.*Receipt\s+\d+.*$", "Osko Payment receipt", false),
            (r"\s*-\s*Deposit\s*-\s*Receipt\s+.*$", "Deposit receipt", false),
            (r"\s*-\s*Alipay$", "Alipay", true),
            (r"\s+Card\s+\d{6}x{6}(?P<account_ref>\d{4})$", "Full card number", false),
            (r"\s+Value [Dd]ate:?\s+(?P<date>\d{2}/\d{2}/\d{4})$", "Standalone value date", false),
            (r"\s+(?P<location_raw>NSWAU)$", "NSWAU suffix", false),
            (r"\s+(?P<location_raw>NS) AUS$", "NS AUS suffix", false),
            (r"\s+(?P<location_raw>AU) AUS$", "AU AUS suffix", false),
            (r"\s+(?P<location_raw>AUS)$", "AUS suffix", false),
            (r"\s+(?P<location_raw>AU)$", "AU suffix", false),
            (r"\s+(?P<location_raw>NLD)$", "NLD suffix", false),
            (r"\s+(?P<location_raw>SGP)$", "SGP suffix", false),
            (r"\s+(?P<location_raw>USA)$", "USA suffix", false),
            (r"\s+(?P<location_raw>IDN)$", "IDN suffix", false),
            (r"\s+(?P<location_raw>GBR)$", "GBR suffix", false),
            (r"\s+(?P<foreign_currency>[A-Z]{3})\s+(?P<foreign_amount>\d+\.\d{2})$", "Foreign currency amount", false),
            (r"\s*,\s*\d{4}$", "Trailing code", false),
            (r"\s*-\s*negative\s+\$[\d.]+.*$", "Negative amount", false),
            (r"\s*-?\s*Eftpos (?:Purchase|Cash Out)\s*-\s*Receipt\s+.*$", "EFTPOS receipt", false),
            (r"\s+Eftpos Purchase\s*-\s*Receipt\s+.*$", "EFTPOS Purchase receipt", false),
            (r"\s*-\s*Eftpos Purchase\s*-\s*Receipt\s+\d+Date.*$", "EFTPOS receipt (no space)", false),
            (r"\s*,\s*\d{4}\s+Last 4 Card Digits\s+\d{4}$", "Last 4 Card Digits", false),
            (r"\s*Foreign Currency Amount:?\s+\d+In\s+.*$", "Foreign currency receipt", false),
            (r"\s*,?\s*\d{4}\s+Last\s+4\s+Card\s+Digits\s+\d{4}$", "Last 4 card digits", false),
            (r"\s*-\s*Internal Transfer\s*-\s*Receipt\s+\d+.*$", "Internal Transfer receipt", false),
            (r"\s+Card\s+\d[A-Z]\d{4}[A-Za-z]{6}\d{4}$", "Masked card number", false),
            (r"\s*-\s*[\w.+-]+@[\w.-]+$", "Email suffix", false),
            (r"\s+PTY\.?\s*LTD?\.?\s*$", "PTY LTD suffix", false),
            (r"\s+P/L\s*$", "P/L suffix", false),
            (r"\s+\d{7,}$", "Long reference number", false),
            (r"\s+(?P<location>(?:NSW|VIC|QLD|WA|SA|TAS|ACT|NT)\s+\d{4,6})$", "State + postcode", false),
            (r"\s+(?P<location>(?:NSW|VIC|QLD|WA|SA|TAS|ACT|NT))$", "State suffix", false),
        ];
        patterns
            .into_iter()
            .map(|(p, n, g)| StripPattern {
                regex: Regex::new(p).expect("invalid suffix pattern"),
                name: n,
                is_gateway: g,
            })
            .collect()
    })
}
