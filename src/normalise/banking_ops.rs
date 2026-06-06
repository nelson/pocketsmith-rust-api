#[cfg(test)]
use std::sync::OnceLock;

use regex::Regex;

use super::{BankingOperation, NormalisationResult, PayeeClass};

#[cfg(test)]
struct BankingOp {
    operation: BankingOperation,
    patterns: &'static [&'static str],
    has_account: bool,
}

pub(crate) struct CompiledBankingOp {
    regex: Regex,
    operation: BankingOperation,
    has_account: bool,
}

/// First-match-wins banking-op detection: set operation (+ optional
/// account capture), then class=Other if still unclassified. Shared by
/// [`apply_with_db`] and the const test oracle [`apply`].
fn run_match(result: &mut NormalisationResult, compiled: &[CompiledBankingOp]) {
    if result.features.operation.is_none() {
        for cop in compiled {
            if let Some(caps) = cop.regex.captures(&result.normalised) {
                result.features.operation = Some(cop.operation);
                if cop.has_account {
                    if let Some(account) = caps.name("account") {
                        result.features.account = Some(account.as_str().to_string());
                    }
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
        out.push(CompiledBankingOp { regex, operation, has_account });
    }
    Ok(out)
}

/// Const-backed banking-op pass: fidelity oracle.
#[cfg(test)]
pub(crate) fn apply(result: &mut NormalisationResult) {
    run_match(result, compiled_banking_ops());
}

#[cfg(test)]
const BANKING_OPS: &[BankingOp] = &[
    BankingOp {
        operation: BankingOperation::BPay,
        has_account: false,
        patterns: &[
            r"(?i)BPAY PAYMENT",
            r"(?i)ANZ INTERNET BANKING BPAY",
        ],
    },
    BankingOp {
        operation: BankingOperation::Deposit,
        has_account: false,
        patterns: &[r"(?i)CASH DEPOSIT"],
    },
    BankingOp {
        operation: BankingOperation::Fee,
        has_account: false,
        patterns: &[
            r"(?i)ACCOUNT SERVICING FEE",
            r"(?i)ACCOUNT FEE$",
            r"(?i)ADMINISTRATION FEE$",
            r"(?i)CONTRIBUTION TAX ADJUSTMENT$",
            r"(?i)CONTRIBUTION TAX$",
            r"(?i)INTERNATIONAL TRANSACTION FEE",
            r"(?i)UNPAID PAYMENT FEE",
            r"(?i)PACKAGE FEE$",
        ],
    },
    BankingOp {
        operation: BankingOperation::DirectCredit,
        has_account: false,
        patterns: &[
            r"(?i)PAYID PAYMENT RECEIVED",
        ],
    },
    BankingOp {
        operation: BankingOperation::Interest,
        has_account: false,
        patterns: &[
            r"(?i)INTEREST CHARGE",
            r"(?i)INTEREST ADJUSTMENT",
            r"(?i)INTEREST CORRECTION",
        ],
    },
    BankingOp {
        operation: BankingOperation::InternalTransfer,
        has_account: true,
        patterns: &[
            r"(?i)INTERNAL TRANSFER",
            r"(?i)TRANSFER (?:TO|FROM) XX(?P<account>\d{4})",
            r"(?i)(?:TO|FROM) ACCOUNT XX(?P<account>\d{4})",
        ],
    },
    BankingOp {
        operation: BankingOperation::Loan,
        has_account: false,
        patterns: &[
            r"(?i)LOAN REPAYMENT",
            r"(?i)REPAYMENT/PAYMENT",
        ],
    },
    BankingOp {
        operation: BankingOperation::Transfer,
        has_account: false,
        patterns: &[
            r"(?i)FUNDS TRANSFER",
            r"(?i)ONLINE PAYMENT RECEIVED",
            r"(?i)TRANSFER TO CBA",
            r"(?i)TRANSFER TO OTHER BANK",
        ],
    },
    BankingOp {
        operation: BankingOperation::CreditCard,
        has_account: false,
        patterns: &[r"(?i)CREDIT CARD"],
    },
    BankingOp {
        operation: BankingOperation::Withdrawal,
        has_account: false,
        patterns: &[r"(?i)WDL ATM"],
    },
];

#[cfg(test)]
fn compiled_banking_ops() -> &'static [CompiledBankingOp] {
    static COMPILED: OnceLock<Vec<CompiledBankingOp>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        BANKING_OPS
            .iter()
            .flat_map(|op| {
                op.patterns.iter().map(move |&pat| CompiledBankingOp {
                    regex: Regex::new(pat).expect("invalid banking op pattern"),
                    operation: op.operation,
                    has_account: op.has_account,
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DB-backed banking-op path must reproduce the const oracle.
    #[test]
    fn db_apply_matches_const_oracle() {
        let p = crate::normalise::OwnedPipeline::seeded_in_memory().unwrap();
        let ctx = p.ctx();
        for inp in [
            "INTEREST CHARGE", "BPAY PAYMENT", "INTERNAL TRANSFER",
            "TRANSFER TO OTHER BANK", "TRANSFER TO XX1234", "Contribution Tax",
            "Unpaid Payment Fee", "Account Fee", "PayID Payment Received, Thank you",
            "WDL ATM", "CASH DEPOSIT", "SOMETHING UNMATCHED",
        ] {
            let mut a = NormalisationResult::new(inp);
            apply(&mut a);
            let mut b = NormalisationResult::new(inp);
            apply_with_db(&mut b, &ctx);
            assert_eq!(a.features.operation, b.features.operation, "op differs for {inp:?}");
            assert_eq!(a.features.account, b.features.account, "account differs for {inp:?}");
            assert_eq!(a.class(), b.class(), "class differs for {inp:?}");
        }
    }

    #[test]
    fn test_interest_charge() {
        let mut r = NormalisationResult::new("INTEREST CHARGE");
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::Interest));
        assert_eq!(r.class(), Some(&PayeeClass::Other));
        assert!(r.features.entity_name.is_none());
    }

    #[test]
    fn test_bpay_payment() {
        let mut r = NormalisationResult::new("BPAY PAYMENT");
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::BPay));
        assert_eq!(r.class(), Some(&PayeeClass::Other));
    }

    #[test]
    fn test_internal_transfer() {
        let mut r = NormalisationResult::new("INTERNAL TRANSFER");
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::InternalTransfer));
        assert_eq!(r.class(), Some(&PayeeClass::Other));
    }

    #[test]
    fn test_transfer_to_other_bank() {
        let mut r = NormalisationResult::new("TRANSFER TO OTHER BANK");
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::Transfer));
        assert_eq!(r.class(), Some(&PayeeClass::Other));
    }

    #[test]
    fn test_transfer_with_account() {
        let mut r = NormalisationResult::new("TRANSFER TO XX1234");
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::InternalTransfer));
        assert_eq!(r.features.account.as_deref(), Some("1234"));
        assert_eq!(r.class(), Some(&PayeeClass::Other));
    }

    #[test]
    fn test_skip_class_if_already_classified() {
        let mut r = NormalisationResult::new("BPAY PAYMENT");
        r.set_class(PayeeClass::Person);
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::BPay));
        assert_eq!(r.class(), Some(&PayeeClass::Person));
    }

    #[test]
    fn test_contribution_tax() {
        let mut r = NormalisationResult::new("Contribution Tax");
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::Fee));
        assert_eq!(r.class(), Some(&PayeeClass::Other));
    }

    #[test]
    fn test_unpaid_payment_fee() {
        let mut r = NormalisationResult::new("Unpaid Payment Fee");
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::Fee));
    }

    #[test]
    fn test_administration_fee() {
        let mut r = NormalisationResult::new("Administration Fee");
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::Fee));
    }

    #[test]
    fn test_account_fee() {
        let mut r = NormalisationResult::new("Account Fee");
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::Fee));
    }

    #[test]
    fn test_payid_payment_received() {
        let mut r = NormalisationResult::new("PayID Payment Received, Thank you");
        apply(&mut r);
        assert_eq!(r.features.operation, Some(BankingOperation::DirectCredit));
        assert_eq!(r.class(), Some(&PayeeClass::Other));
    }
}
