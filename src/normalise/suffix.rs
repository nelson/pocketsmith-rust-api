use anyhow::Result;
use regex::Regex;
use rusqlite::Connection;

use super::{BankingOperation, NormalisationResult, PipelineCtx};

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
/// per iteration). Driven by the DB-backed [`apply_with_db`].
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
                        result.features.region = Some(location.to_string());
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
/// order (sort_order, then id). Rows come from the typed
/// [`crud::load_for_compile`] read (rule-cli §3.1).
pub(crate) fn load_compiled(conn: &Connection) -> Result<Vec<CompiledSuffix>> {
    use crate::rules::{crud, model::RuleData, Stage};
    let mut out = Vec::new();
    for data in crud::load_for_compile(conn, Stage::Suffixes)? {
        if let RuleData::Suffix {
            pattern, gateway, operation, institution, has_account, has_date, has_location,
            has_currency_code, has_amount, ..
        } = data
        {
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
    }
    Ok(out)
}

fn parse_amount_cents(s: &str) -> Option<u32> {
    s.replace('.', "").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::{NormalisationResult, OwnedPipeline};

    fn pipeline() -> &'static std::thread::LocalKey<OwnedPipeline> {
        thread_local! {
            static PIPELINE: OwnedPipeline = OwnedPipeline::seeded_in_memory().unwrap();
        }
        &PIPELINE
    }

    /// Run the DB-backed suffix stage against the seeded in-memory pipeline
    /// (rules from `rules/suffixes.sql`).
    fn run(input: &str) -> NormalisationResult {
        pipeline().with(|p| {
            let ctx = p.ctx();
            let mut r = NormalisationResult::new(input);
            apply_with_db(&mut r, &ctx);
            r
        })
    }

    // Kept guards exercise multi-feature capture, the NS->NSW special
    // case, currency/amount parsing, and a prefix+suffix interaction.

    #[test]
    fn test_card_and_date() {
        let r = run("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        assert_eq!(r.normalised, "WOOLWORTHS 1624 STRATHF");
        assert_eq!(r.features.date.as_deref(), Some("01/01/2026"));
        assert_eq!(r.features.account.as_deref(), Some("9172"));
    }

    #[test]
    fn test_ns_aus_normalises_to_nsw() {
        let r = run("SOME MERCHANT NS AUS");
        assert_eq!(r.normalised, "SOME MERCHANT");
        assert_eq!(r.features.region.as_deref(), Some("NSW"));
    }

    #[test]
    fn test_state_postcode() {
        let r = run("MERCHANT NSW 2140");
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.region.as_deref(), Some("NSW 2140"));
    }

    #[test]
    fn test_foreign_currency_amount() {
        let r = run("MERCHANT SGD 12.50");
        assert_eq!(r.normalised, "MERCHANT");
        assert_eq!(r.features.currency_code.as_deref(), Some("SGD"));
        assert_eq!(r.features.amount_in_cents, Some(1250));
    }

    #[test]
    fn test_prefix_then_suffix() {
        let r = pipeline().with(|p| {
            let ctx = p.ctx();
            let mut r = NormalisationResult::new("SMP*CAFE NAME, Card xx1234 Value Date: 01/01/2026");
            crate::normalise::prefix::apply_with_db(&mut r, &ctx);
            apply_with_db(&mut r, &ctx);
            r
        });
        assert_eq!(r.normalised, "CAFE NAME");
        assert_eq!(r.features.gateway.as_deref(), Some("Square Marketplace"));
    }
}
