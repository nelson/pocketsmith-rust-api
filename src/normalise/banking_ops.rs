use regex::Regex;

use super::{BankingOperation, NormalisationResult, PayeeClass};

pub(crate) struct CompiledBankingOp {
    regex: Regex,
    operation: BankingOperation,
    has_account: bool,
    pattern: String,
}

/// First-match-wins banking-op detection: set operation (+ optional
/// account capture), then class=Other if still unclassified. Driven by
/// the DB-backed [`apply_with_db`].
fn run_match(result: &mut NormalisationResult, compiled: &[CompiledBankingOp]) {
    if result.features.operation.is_none() {
        for cop in compiled {
            if let Some(caps) = cop.regex.captures(&result.normalised) {
                let span = caps.get(0).map(|m| (m.start(), m.end()));
                let account = if cop.has_account {
                    caps.name("account").map(|a| a.as_str().to_string())
                } else {
                    None
                };
                result.record_match(cop.pattern.clone(), span);
                result.features.operation = Some(cop.operation);
                if let Some(account) = account {
                    result.features.account = Some(account);
                }
                break;
            }
        }
    }
    if result.class().is_none() && result.features.operation.is_some() {
        result.set_class(PayeeClass::Other);
    }
}

/// DB-backed banking-op detection.
pub fn apply_with_db(result: &mut NormalisationResult, ctx: &super::PipelineCtx) {
    match ctx.cache.banking_ops(ctx.conn) {
        Ok(compiled) => run_match(result, &compiled),
        Err(e) => eprintln!("banking_ops: rule load failed, stage skipped: {e:#}"),
    }
}

/// Load + compile banking-op rules in apply order (sort_order, id).
/// Each row's `operation` is a stored display name mapped back to the
/// enum; an unrecognised name is an error (corrupt rule table).
pub(crate) fn load_compiled(conn: &rusqlite::Connection) -> anyhow::Result<Vec<CompiledBankingOp>> {
    let mut stmt = conn.prepare(
        "SELECT operation, pattern, has_account FROM rule_banking_ops ORDER BY sort_order, id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)? != 0,
        ))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (operation, pattern, has_account) = r?;
        let operation = BankingOperation::from_display_name(&operation)
            .ok_or_else(|| anyhow::anyhow!("unknown banking operation {operation:?}"))?;
        let regex = Regex::new(&pattern)
            .map_err(|e| anyhow::anyhow!("invalid banking op pattern {pattern:?}: {e}"))?;
        out.push(CompiledBankingOp { regex, operation, has_account, pattern });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::OwnedPipeline;

    /// Run the DB-backed banking-op stage against the seeded in-memory
    /// pipeline (rules from `src/rules/banking_ops.sql`).
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

    #[test]
    fn test_interest_charge() {
        let r = run("INTEREST CHARGE");
        assert_eq!(r.features.operation, Some(BankingOperation::Interest));
        assert_eq!(r.class(), Some(&PayeeClass::Other));
        assert!(r.features.entity_name.is_none());
    }

    #[test]
    fn test_transfer_with_account() {
        let r = run("TRANSFER TO XX1234");
        assert_eq!(r.features.operation, Some(BankingOperation::InternalTransfer));
        assert_eq!(r.features.account.as_deref(), Some("1234"));
        assert_eq!(r.class(), Some(&PayeeClass::Other));
    }

    #[test]
    fn test_payid_payment_received() {
        let r = run("PayID Payment Received, Thank you");
        assert_eq!(r.features.operation, Some(BankingOperation::DirectCredit));
        assert_eq!(r.class(), Some(&PayeeClass::Other));
    }

    #[test]
    fn test_skip_class_if_already_classified() {
        thread_local! {
            static PIPELINE: OwnedPipeline = OwnedPipeline::seeded_in_memory().unwrap();
        }
        PIPELINE.with(|p| {
            let ctx = p.ctx();
            let mut r = NormalisationResult::new("BPAY PAYMENT");
            r.set_class(PayeeClass::Person);
            apply_with_db(&mut r, &ctx);
            assert_eq!(r.features.operation, Some(BankingOperation::BPay));
            assert_eq!(r.class(), Some(&PayeeClass::Person));
        });
    }
}
