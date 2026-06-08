use anyhow::Result;
use regex::Regex;
use rusqlite::Connection;

use super::{BankingOperation, NormalisationResult, PipelineCtx};

pub(crate) struct CompiledPrefix {
    regex: Regex,
    gateway: Option<String>,
    operation: Option<BankingOperation>,
    has_account: bool,
    has_date: bool,
}

/// One pass of the prefix loop over a compiled rule set: strip metadata
/// prefixes until no rule matches. Driven by the
/// DB-backed [`apply_with_db`].
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
mod tests {
    use super::*;
    use crate::normalise::{BankingOperation, NormalisationResult, OwnedPipeline};

    /// Run the DB-backed prefix stage against the seeded in-memory pipeline
    /// (rules from `src/rules/prefixes.sql`).
    fn run(input: &str) -> NormalisationResult {
        thread_local! {
            static PIPELINE: OwnedPipeline = OwnedPipeline::seeded_in_memory().unwrap();
        }
        PIPELINE.with(|p| {
            let ctx = p.ctx();
            let mut r = NormalisationResult::new(input);
            apply_with_db(&mut r, &ctx);
            r
        })
    }

    // Kept guards exercise multi-feature capture and the strip loop;
    // single-prefix smoke cases are covered by the hermetic
    // `prefix_stage_reads_its_rules_from_the_db` test in `mod.rs`.

    #[test]
    fn test_date_account_operation() {
        let r = run("28/01/26, Direct Debit 123 ENTITY");
        assert_eq!(r.normalised, "ENTITY");
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
        assert_eq!(r.features.operation, Some(BankingOperation::DirectDebit));
        assert_eq!(r.features.account.as_deref(), Some("123"));
    }

    #[test]
    fn test_multiple_prefixes_loop() {
        let r = run("28/01/26, SQ *COFFEE SHOP");
        assert_eq!(r.normalised, "COFFEE SHOP");
        assert_eq!(r.features.gateway.as_deref(), Some("Square"));
        assert_eq!(r.features.date.as_deref(), Some("28/01/26"));
    }

    #[test]
    fn test_direct_credit_pension() {
        let r = run("Direct Credit PENSION XX1234 CHILDASSISTPYMT");
        assert_eq!(r.normalised, "CHILDASSISTPYMT");
        assert_eq!(r.features.operation, Some(BankingOperation::DirectCredit));
    }

    #[test]
    fn test_visa_debit_account_capture() {
        let r = run("Visa Debit Purchase Card 9172 MERCHANT NAME");
        assert_eq!(r.normalised, "MERCHANT NAME");
        assert_eq!(r.features.account.as_deref(), Some("9172"));
    }
}
