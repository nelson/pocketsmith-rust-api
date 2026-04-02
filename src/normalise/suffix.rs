use std::sync::OnceLock;

use regex::Regex;

use super::StripPattern;

pub(crate) fn suffix_patterns() -> &'static [StripPattern] {
    static PATTERNS: OnceLock<Vec<StripPattern>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        let patterns: Vec<(&str, Option<&'static str>)> = vec![
            (r",?\s*Card xx(?P<account_ref>\d{4}).*?(?P<date>\d{2}/\d{2}/\d{4}).*$", None),
            (r"\s+Card xx(?P<account_ref>\d{4}).*?(?P<date>\d{2}/\d{2}/\d{4}).*$", None),
            (r"\s+Tap and Pay xx(?P<account_ref>\d{4}).*$", None),
            (r"\s*-?\s*Visa Purchase\s*-\s*Receipt\s+\w+\s*In\s+.*$", None),
            (r"\s*-?\s*Visa Refund\s*-\s*Receipt\s+.*$", None),
            (r"\s*-?\s*Osko Payment.*Receipt\s+\d+.*$", None),
            (r"\s*-\s*Deposit\s*-\s*Receipt\s+.*$", None),
            (r"\s*-\s*(?P<payment_gateway>Alipay)$", None),
            (r"\s+Card\s+\d{6}x{6}(?P<account_ref>\d{4})$", None),
            (r"\s+Value [Dd]ate:?\s+(?P<date>\d{2}/\d{2}/\d{4})$", None),
            (r"\s+(?P<location_raw>NSWAU)$", None),
            (r"\s+(?P<location_raw>NS) AUS$", None),
            (r"\s+(?P<location_raw>AU) AUS$", None),
            (r"\s+(?P<location_raw>AUS)$", None),
            (r"\s+(?P<location_raw>AU)$", None),
            (r"\s+(?P<location_raw>NLD)$", None),
            (r"\s+(?P<location_raw>SGP)$", None),
            (r"\s+(?P<location_raw>USA)$", None),
            (r"\s+(?P<location_raw>IDN)$", None),
            (r"\s+(?P<location_raw>GBR)$", None),
            (r"\s+(?P<foreign_currency>[A-Z]{3})\s+(?P<foreign_amount>\d+\.\d{2})$", None),
            (r"\s*,\s*\d{4}$", None),
            (r"\s*-\s*negative\s+\$[\d.]+.*$", None),
            (r"\s*-?\s*Eftpos (?:Purchase|Cash Out)\s*-\s*Receipt\s+.*$", None),
            (r"\s+Eftpos Purchase\s*-\s*Receipt\s+.*$", None),
            (r"\s*-\s*Eftpos Purchase\s*-\s*Receipt\s+\d+Date.*$", None),
            (r"\s*,\s*\d{4}\s+Last 4 Card Digits\s+\d{4}$", None),
            (r"\s*Foreign Currency Amount:?\s+\d+In\s+.*$", None),
            (r"\s*,?\s*\d{4}\s+Last\s+4\s+Card\s+Digits\s+\d{4}$", None),
            (r"\s*-\s*Internal Transfer\s*-\s*Receipt\s+\d+.*$", None),
            (r"\s+Card\s+\d[A-Z]\d{4}[A-Za-z]{6}\d{4}$", None),
            (r"\s*-\s*[\w.+-]+@[\w.-]+$", None),
            (r"\s+PTY\.?\s*LTD?\.?\s*$", None),
            (r"\s+P/L\s*$", None),
            (r"\s+\d{7,}$", None),
            (r"\s+(?P<location>(?:NSW|VIC|QLD|WA|SA|TAS|ACT|NT)\s+\d{4,6})$", None),
            (r"\s+(?P<location>(?:NSW|VIC|QLD|WA|SA|TAS|ACT|NT))$", None),
        ];
        patterns
            .into_iter()
            .map(|(p, gw)| StripPattern {
                regex: Regex::new(p).expect("invalid suffix pattern"),
                gateway_name: gw,
            })
            .collect()
    })
}
