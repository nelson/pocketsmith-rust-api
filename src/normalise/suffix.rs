#[cfg(test)]
use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;
use rusqlite::Connection;

use super::{BankingOperation, NormalisationResult, PipelineCtx};

#[cfg(test)]
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

#[cfg(test)]
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

pub(crate) struct CompiledSuffix {
    regex: Regex,
    gateway: Option<String>,
    operation: Option<BankingOperation>,
    institution: Option<String>,
    has_account: bool,
    has_date: bool,
    has_location: bool,
    has_currency_code: bool,
    has_amount: bool,
}

/// One run of the suffix loop over a compiled rule set (first match wins
/// per iteration). Shared by [`apply_with_db`] and the test oracle
/// [`apply`].
fn run_loop(result: &mut NormalisationResult, compiled: &[CompiledSuffix]) {
    loop {
        let mut matched = false;
        for pat in compiled {
            if let Some(caps) = pat.regex.captures(&result.normalised) {
                if let Some(gw) = &pat.gateway {
                    result.features.gateway = Some(gw.clone());
                }
                if let Some(op) = pat.operation {
                    result.features.operation = Some(op);
                }
                if let Some(inst) = &pat.institution {
                    result.features.institution = Some(inst.clone());
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

/// Strip metadata suffixes using the DB-backed, cached rule set.
pub fn apply_with_db(result: &mut NormalisationResult, ctx: &PipelineCtx) {
    match ctx.cache.suffixes(ctx.conn) {
        Ok(compiled) => run_loop(result, &compiled),
        Err(e) => eprintln!("suffix: rule load failed, stage skipped: {e:#}"),
    }
}

/// Load and compile the suffix rules from `rule_suffixes` in apply
/// order (sort_order, then id).
pub(crate) fn load_compiled(conn: &Connection) -> Result<Vec<CompiledSuffix>> {
    let mut stmt = conn.prepare(
        "SELECT pattern, gateway, operation, institution, has_account, has_date, \
                has_location, has_currency_code, has_amount \
           FROM rule_suffixes ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, i64>(4)? != 0,
            row.get::<_, i64>(5)? != 0,
            row.get::<_, i64>(6)? != 0,
            row.get::<_, i64>(7)? != 0,
            row.get::<_, i64>(8)? != 0,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (pattern, gateway, operation, institution, has_account, has_date, has_location, has_currency_code, has_amount) = r?;
        let regex = Regex::new(&pattern)
            .map_err(|e| anyhow::anyhow!("invalid suffix pattern {pattern:?}: {e}"))?;
        out.push(CompiledSuffix {
            regex,
            gateway,
            operation: operation.as_deref().and_then(BankingOperation::from_display_name),
            institution,
            has_account,
            has_date,
            has_location,
            has_currency_code,
            has_amount,
        });
    }
    Ok(out)
}

#[cfg(test)]
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

#[cfg(test)]
fn compiled_suffixes() -> &'static [CompiledSuffix] {
    static COMPILED: OnceLock<Vec<CompiledSuffix>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        SUFFIXES
            .iter()
            .map(|s| CompiledSuffix {
                regex: Regex::new(s.pattern).expect("invalid suffix pattern"),
                gateway: s.gateway.map(|x| x.to_string()),
                operation: s.operation,
                institution: s.institution.map(|x| x.to_string()),
                has_account: s.has_account,
                has_date: s.has_date,
                has_location: s.has_location,
                has_currency_code: s.has_currency_code,
                has_amount: s.has_amount,
            })
            .collect()
    })
}

/// Const-backed suffix pass: the original behaviour, kept as the
/// fidelity oracle the DB-backed path is tested against.
#[cfg(test)]
pub(crate) fn apply(result: &mut NormalisationResult) {
    run_loop(result, compiled_suffixes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::NormalisationResult;

    /// DB-backed suffix path must reproduce the const oracle exactly.
    #[test]
    fn db_apply_matches_const_oracle() {
        let p = crate::normalise::OwnedPipeline::seeded_in_memory().unwrap();
        let ctx = p.ctx();
        let inputs = [
            "WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026",
            "MERCHANT Card 123456xxxxxx7890",
            "MERCHANT Value Date: 15/03/2026",
            "SOME MERCHANT NS AUS",
            "MERCHANT NSW 2140",
            "MERCHANT AU AUS",
            "MERCHANT VIC",
            "COMPANY NAME PTY LTD",
            "MERCHANT - Alipay",
            "MERCHANT 12345678",
            "MERCHANT - Eftpos Purchase - Receipt 123Date01/01",
            "MERCHANT SGD 12.50",
            "PAYPAL - paypal-aud@airbnb.com",
        ];
        for inp in inputs {
            let mut a = NormalisationResult::new(inp);
            apply(&mut a);
            let mut b = NormalisationResult::new(inp);
            apply_with_db(&mut b, &ctx);
            assert_eq!(a.normalised, b.normalised, "normalised differs for {inp:?}");
            assert_eq!(
                crate::normalise::features_to_json(&a.features),
                crate::normalise::features_to_json(&b.features),
                "features differ for {inp:?}"
            );
        }
    }

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
