use std::sync::OnceLock;

use regex::Regex;

use super::NormalisationResult;

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
    operation: Option<&'static str>,
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
    operation: Option<&'static str>,
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
        for pat in data() {
            if let Some(caps) = pat.regex.captures(&result.normalised) {
                if let Some(gw) = pat.gateway {
                    result.features.gateway = Some(gw.to_string());
                }
                if let Some(op) = pat.operation {
                    // TODO: map to BankingOperation when Purchase/Refund variants are added
                    let _ = op;
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
                        result.features.location = Some(map_location(loc.as_str()).to_string());
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
    Suffix { pattern: r"\s*,\s*(?P<account>\d{4})$", has_account: true, ..DEFAULT },
    Suffix { pattern: r"\s*,\s*\d{4}\s+Last 4 Card Digits\s+(?P<account>\d{4})$", has_account: true, ..DEFAULT },
    Suffix { pattern: r"\s*,?\s*\d{4}\s+Last\s+4\s+Card\s+Digits\s+(?P<account>\d{4})$", has_account: true, ..DEFAULT },
    // --- Date only ---
    Suffix { pattern: r"\s+Value [Dd]ate:?\s+(?P<date>\d{2}/\d{2}/\d{4})$", has_date: true, ..DEFAULT },
    // --- Operations (institution + operation type) ---
    Suffix { pattern: r"\s*-?\s*Visa Purchase\s*-\s*Receipt\s+\w+\s*In\s+.*$", operation: Some("Purchase"), institution: Some("Visa"), ..DEFAULT },
    Suffix { pattern: r"\s*-?\s*Visa Refund\s*-\s*Receipt\s+.*$", operation: Some("Refund"), institution: Some("Visa"), ..DEFAULT },
    Suffix { pattern: r"\s*-?\s*Osko Payment.*Receipt\s+\d+.*$", operation: Some("Transfer"), institution: Some("Osko"), ..DEFAULT },
    Suffix { pattern: r"\s*-\s*Deposit\s*-\s*Receipt\s+.*$", operation: Some("Deposit"), ..DEFAULT },
    Suffix { pattern: r"\s*-?\s*Eftpos (?:Purchase|Cash Out)\s*-\s*Receipt\s+.*$", operation: Some("Purchase"), institution: Some("Eftpos"), ..DEFAULT },
    Suffix { pattern: r"\s+Eftpos Purchase\s*-\s*Receipt\s+.*$", operation: Some("Purchase"), institution: Some("Eftpos"), ..DEFAULT },
    Suffix { pattern: r"\s*-\s*Eftpos Purchase\s*-\s*Receipt\s+\d+Date.*$", operation: Some("Purchase"), institution: Some("Eftpos"), ..DEFAULT },
    Suffix { pattern: r"\s*-\s*Internal Transfer\s*-\s*Receipt\s+\d+.*$", operation: Some("Transfer"), ..DEFAULT },
    // --- Gateway ---
    Suffix { pattern: r"\s*-\s*Alipay$", gateway: Some("Alipay"), ..DEFAULT },
    // --- Location (country codes, mapped to standard form) ---
    Suffix { pattern: r"\s+(?P<location>NSWAU)$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>NS) AUS$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>AU) AUS$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>AUS)$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>AU)$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>NLD)$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>SGP)$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>USA)$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>IDN)$", has_location: true, ..DEFAULT },
    Suffix { pattern: r"\s+(?P<location>GBR)$", has_location: true, ..DEFAULT },
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

/// Map raw location codes to standard form. Unknown values pass through unchanged.
fn map_location(raw: &str) -> &str {
    match raw {
        "NSWAU" | "NS" => "NSW",
        "AU" | "AUS" => "AU",
        "NLD" => "NL",
        "SGP" => "SG",
        "USA" => "US",
        "IDN" => "ID",
        "GBR" => "GB",
        other => other,
    }
}

fn parse_amount_cents(s: &str) -> Option<u32> {
    s.replace('.', "").parse().ok()
}

fn data() -> &'static [CompiledSuffix] {
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
