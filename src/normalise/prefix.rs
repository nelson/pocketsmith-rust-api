use std::sync::OnceLock;

use regex::Regex;

use super::StripPattern;

pub(crate) fn prefix_patterns() -> &'static [StripPattern] {
    static PATTERNS: OnceLock<Vec<StripPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        // Sorted: non-gateway (alphabetical by name), then gateway (alphabetical by name)
        let patterns: Vec<(&str, &'static str, bool)> = vec![
            // --- Non-gateway prefixes ---
            // (r"^Cafes - ", "CBA auto-pay", false),
            (r"^(?P<date>\d{2}/\d{2}/\d{2,4}),?\s+", "Date prefix", false),
            (r"^-([A-Z]+-)*", "dashed prefix", false),
            (r"^EFTPOS\s+", "EFTPOS", false),
            (r"^\*\s+", "Leading asterisk", false),
            (r"^\s*-\s+", "leading dash-space", false),
            (r"^% ", "percent prefix", false),
            (r"^Return\s+", "return", false),
            (r"^SP ", "SP prefix", false),
            (r"^Visa Debit Purchase Card (?P<account_ref>\d{4})\s+", "Visa Debit Purchase", false),
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
