#[cfg(test)]
const PREFIXES: &[Prefix] = &[
    // --- Non-gateway prefixes ---
    Prefix { pattern: r"^(?P<date>\d{2}/\d{2}/\d{2,4}),?\s+", has_date: true, ..DEFAULT },
    Prefix { pattern: r"^-([A-Z]+-)*", ..DEFAULT },
    Prefix { pattern: r"^(?i)Refund Purchase,?\s*", operation: Some(BankingOperation::Refund), ..DEFAULT },
    Prefix { pattern: r"^EFTPOS\s+", ..DEFAULT },
    Prefix { pattern: r"^\*\s+", ..DEFAULT },
    Prefix { pattern: r"^\s*-\s+", ..DEFAULT },
    Prefix { pattern: r"^% ", ..DEFAULT },
    Prefix { pattern: r"^Return\s+", ..DEFAULT },
    Prefix { pattern: r"^SP ", ..DEFAULT },
    Prefix { pattern: r"^Visa Debit Purchase Card (?P<account>\d{4})\s+", has_account: true, ..DEFAULT },
    // --- Direct Debit/Credit prefixes ---
    Prefix { pattern: r"^(?i)Direct Debit (?:XX)?(?P<account>\d+)\s+", operation: Some(BankingOperation::DirectDebit), has_account: true, ..DEFAULT },
    Prefix { pattern: r"^(?i)Direct Credit (?:PENSION )?(?:XX)?(?P<account>\d+)\s+", operation: Some(BankingOperation::DirectCredit), has_account: true, ..DEFAULT },
    // --- Gateway prefixes ---
    Prefix { pattern: r"^ALI\*", gateway: Some("AliExpress"), ..DEFAULT },
    Prefix { pattern: r"^Alipay ", gateway: Some("Alipay"), ..DEFAULT },
    Prefix { pattern: r"^(?i)BEEM IT$", gateway: Some("Beem"), operation: Some(BankingOperation::Cash), ..DEFAULT },
    Prefix { pattern: r"^(?i)BEEM IT\b\s*-?\s*", gateway: Some("Beem"), ..DEFAULT },
    Prefix { pattern: r"^(?i)BEEM\.COM\.AU\s*-?\s*", gateway: Some("Beem"), ..DEFAULT },
    Prefix { pattern: r"^CKO\*", gateway: Some("Checkout.com"), ..DEFAULT },
    Prefix { pattern: r"^DBS\*", gateway: Some("DBS"), ..DEFAULT },
    Prefix { pattern: r"^DNH\*", gateway: Some("DNH"), ..DEFAULT },
    Prefix { pattern: r"^DOORDASH\*", gateway: Some("DoorDash"), ..DEFAULT },
    Prefix { pattern: r"^EB\s*\*", gateway: Some("Eventbrite"), ..DEFAULT },
    Prefix { pattern: r"^EZI\*", gateway: Some("Ezi"), ..DEFAULT },
    Prefix { pattern: r"^FLEXISCHOOLS\*", gateway: Some("Flexischools"), ..DEFAULT },
    Prefix { pattern: r"^GLOBAL-E\* ", gateway: Some("Global-E"), ..DEFAULT },
    Prefix { pattern: r"^LIGHTSPEED\*(?:SR-)?(?:LS\s+)?", gateway: Some("Lightspeed"), ..DEFAULT },
    Prefix { pattern: r"^LIME\*", gateway: Some("Lime"), ..DEFAULT },
    Prefix { pattern: r"^LS\s+", gateway: Some("Lightspeed"), ..DEFAULT },
    Prefix { pattern: r"^MPASS \*", gateway: Some("mPass"), ..DEFAULT },
    Prefix { pattern: r"^MR YUM\*", gateway: Some("Mr Yum"), ..DEFAULT },
    Prefix { pattern: r"^NAYAXAU\*", gateway: Some("Nayax"), ..DEFAULT },
    Prefix { pattern: r"^PAYPAL \*", gateway: Some("PayPal"), ..DEFAULT },
    Prefix { pattern: r"^PP\*", gateway: Some("PP"), ..DEFAULT },
    Prefix { pattern: r"^(?i:Revolut)\*", gateway: Some("Revolut"), ..DEFAULT },
    Prefix { pattern: r"^SMP\*", gateway: Some("Square Marketplace"), ..DEFAULT },
    Prefix { pattern: r"^SQ \*", gateway: Some("Square"), ..DEFAULT },
    Prefix { pattern: r"^TITHE\.LY\*", gateway: Some("Tithe.ly"), ..DEFAULT },
    Prefix { pattern: r"^TST\*\s*", gateway: Some("Toast"), ..DEFAULT },
    Prefix { pattern: r"^TRYBOOKING\*", gateway: Some("TryBooking"), ..DEFAULT },
    Prefix { pattern: r"^Weixin ", gateway: Some("Weixin"), ..DEFAULT },
    Prefix { pattern: r"^WINDCAVE\*", gateway: Some("Windcave"), ..DEFAULT },
    Prefix { pattern: r"^ZLR\*", gateway: Some("Zeller"), ..DEFAULT },
];

#[cfg(test)]
use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;
use rusqlite::Connection;

use super::{BankingOperation, NormalisationResult, PipelineCtx};

#[cfg(test)]
const DEFAULT: Prefix = Prefix { pattern: "", gateway: None, operation: None, has_account: false, has_date: false };

#[cfg(test)]
struct Prefix {
    pattern: &'static str,
    gateway: Option<&'static str>,
    operation: Option<BankingOperation>,
    has_account: bool,
    has_date: bool,
}

pub(crate) struct CompiledPrefix {
    regex: Regex,
    gateway: Option<String>,
    operation: Option<BankingOperation>,
    has_account: bool,
    has_date: bool,
}

/// One pass of the prefix loop over a compiled rule set: strip metadata
/// prefixes until no rule matches. Shared by the DB-backed
/// [`apply_with_db`] and the const-backed test oracle [`apply`].
fn run_loop(result: &mut NormalisationResult, compiled: &[CompiledPrefix]) {
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
                if pat.has_date {
                    if let Some(date) = caps.name("date") {
                        result.features.date = Some(date.as_str().to_string());
                    }
                }
                if pat.has_account {
                    if let Some(account) = caps.name("account") {
                        result.features.account = Some(account.as_str().to_string());
                    }
                }
                // Remove the matched prefix, trim remaining whitespace.
                let remainder = &result.normalised[caps.get(0).unwrap().end()..];
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

/// Strip metadata prefixes using the DB-backed, cached rule set.
pub fn apply_with_db(result: &mut NormalisationResult, ctx: &PipelineCtx) {
    match ctx.cache.prefixes(ctx.conn) {
        Ok(compiled) => run_loop(result, &compiled),
        Err(e) => eprintln!("prefix: rule load failed, stage skipped: {e:#}"),
    }
}

/// Load and compile the prefix rules from `rule_prefixes` in apply
/// order (sort_order, then id).
pub(crate) fn load_compiled(conn: &Connection) -> Result<Vec<CompiledPrefix>> {
    let mut stmt = conn.prepare(
        "SELECT pattern, gateway, operation, has_account, has_date \
           FROM rule_prefixes ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, i64>(3)? != 0,
            row.get::<_, i64>(4)? != 0,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (pattern, gateway, operation, has_account, has_date) = r?;
        let regex = Regex::new(&pattern)
            .map_err(|e| anyhow::anyhow!("invalid prefix pattern {pattern:?}: {e}"))?;
        out.push(CompiledPrefix {
            regex,
            gateway,
            operation: operation.as_deref().and_then(BankingOperation::from_display_name),
            has_account,
            has_date,
        });
    }
    Ok(out)
}

#[cfg(test)]
fn compiled_prefixes() -> &'static [CompiledPrefix] {
    static COMPILED: OnceLock<Vec<CompiledPrefix>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        PREFIXES
            .iter()
            .map(|p| CompiledPrefix {
                regex: Regex::new(p.pattern).expect("invalid prefix pattern"),
                gateway: p.gateway.map(|s| s.to_string()),
                operation: p.operation,
                has_account: p.has_account,
                has_date: p.has_date,
            })
            .collect()
    })
}

/// Const-backed prefix pass: the original behaviour, kept as the
/// fidelity oracle the DB-backed path is tested against.
#[cfg(test)]
pub(crate) fn apply(result: &mut NormalisationResult) {
    run_loop(result, compiled_prefixes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::{BankingOperation, NormalisationResult};

    /// The DB-backed path must reproduce the const oracle exactly across
    /// representative inputs (date/account captures, gateways, operations,
    /// multi-prefix loops). This is the per-stage fidelity gate.
    #[test]
    fn db_apply_matches_const_oracle() {
        let p = crate::normalise::OwnedPipeline::seeded_in_memory().unwrap();
        let ctx = p.ctx();
        let inputs = [
            "SQ *SOME MERCHANT SYDNEY",
            "DOORDASH*THAI PLACE",
            "Visa Debit Purchase Card 9172 MERCHANT NAME",
            "28/01/26, Direct Debit 123 ENTITY",
            "Woolworths Strathfield",
            "PAYPAL *SOME STORE",
            "28/01/26, SQ *COFFEE SHOP",
            "Direct Debit 062246 CommInsure",
            "Direct Credit 002221 MCARE BENEFITS",
            "Direct Credit PENSION XX1234 CHILDASSISTPYMT",
            "EFTPOS BUPA",
            "BEEM IT",
            "ALI*Something",
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
    fn test_square() {
        let mut r = NormalisationResult::new("SQ *SOME MERCHANT SYDNEY");
        apply(&mut r);
        assert_eq!(r.normalised, "SOME MERCHANT SYDNEY");
        assert_eq!(r.features.gateway.as_deref(), Some("Square"));
    }

    #[test]
    fn test_doordash() {
        let mut r = NormalisationResult::new("DOORDASH*THAI PLACE");
        apply(&mut r);
        assert_eq!(r.normalised, "THAI PLACE");
        assert_eq!(r.features.gateway.as_deref(), Some("DoorDash"));
    }

    #[test]
    fn test_visa_debit() {
        let mut r = NormalisationResult::new("Visa Debit Purchase Card 9172 MERCHANT NAME");
        apply(&mut r);
        assert_eq!(r.normalised, "MERCHANT NAME");
        assert_eq!(r.features.account.as_deref(), Some("9172"));
    }

    #[test]
    fn test_date() {
        let mut r = NormalisationResult::new("28/01/26, Direct Debit 123 ENTITY");
        apply(&mut r);
        assert_eq!(r.normalised, "ENTITY");
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
        assert_eq!(r.features.operation, Some(BankingOperation::DirectDebit));
        assert_eq!(r.features.account.as_deref(), Some("123"));
    }

    #[test]
    fn test_none() {
        let mut r = NormalisationResult::new("Woolworths Strathfield");
        apply(&mut r);
        assert_eq!(r.normalised, "Woolworths Strathfield");
        assert!(r.features.gateway.is_none());
    }

    #[test]
    fn test_paypal() {
        let mut r = NormalisationResult::new("PAYPAL *SOME STORE");
        apply(&mut r);
        assert_eq!(r.normalised, "SOME STORE");
        assert_eq!(r.features.gateway.as_deref(), Some("PayPal"));
    }

    #[test]
    fn test_multiple_prefixes() {
        let mut r = NormalisationResult::new("28/01/26, SQ *COFFEE SHOP");
        apply(&mut r);
        assert_eq!(r.normalised, "COFFEE SHOP");
        assert_eq!(r.features.gateway.as_deref(), Some("Square"));
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
    }

    #[test]
    fn test_direct_debit() {
        let mut r = NormalisationResult::new("Direct Debit 062246 CommInsure");
        apply(&mut r);
        assert_eq!(r.normalised, "CommInsure");
        assert_eq!(r.features.operation, Some(BankingOperation::DirectDebit));
        assert_eq!(r.features.account.as_deref(), Some("062246"));
    }

    #[test]
    fn test_direct_credit() {
        let mut r = NormalisationResult::new("Direct Credit 002221 MCARE BENEFITS");
        apply(&mut r);
        assert_eq!(r.normalised, "MCARE BENEFITS");
        assert_eq!(r.features.operation, Some(BankingOperation::DirectCredit));
        assert_eq!(r.features.account.as_deref(), Some("002221"));
    }

    #[test]
    fn test_direct_credit_pension() {
        let mut r = NormalisationResult::new("Direct Credit PENSION XX1234 CHILDASSISTPYMT");
        apply(&mut r);
        assert_eq!(r.normalised, "CHILDASSISTPYMT");
        assert_eq!(r.features.operation, Some(BankingOperation::DirectCredit));
    }

    #[test]
    fn test_date_then_direct_debit() {
        let mut r = NormalisationResult::new("28/01/26, Direct Debit 123456 ENTITY");
        apply(&mut r);
        assert_eq!(r.normalised, "ENTITY");
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
        assert_eq!(r.features.operation, Some(BankingOperation::DirectDebit));
        assert_eq!(r.features.account.as_deref(), Some("123456"));
    }
}
