use std::sync::OnceLock;

use regex::Regex;

use super::StripPattern;

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
