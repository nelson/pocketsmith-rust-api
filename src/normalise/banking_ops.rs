use std::sync::OnceLock;

use regex::Regex;

use super::{BankingOperation, NormalisationResult, PayeeClass};

struct BankingOp {
    operation: BankingOperation,
    patterns: &'static [&'static str],
    has_account: bool,
}

struct CompiledBankingOp {
    regex: Regex,
    operation: BankingOperation,
    has_account: bool,
}

pub fn apply(result: &mut NormalisationResult) {
    if result.class().is_some() {
        return;
    }
    for cop in compiled_banking_ops() {
        if let Some(caps) = cop.regex.captures(&result.normalised) {
            result.features.operation = Some(cop.operation.clone());
            if cop.has_account {
                if let Some(account) = caps.name("account") {
                    result.features.account = Some(account.as_str().to_string());
                }
            }
            result.set_class(PayeeClass::Other);
            return;
        }
    }
}

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
            r"(?i)CONTRIBUTION TAX ADJUSTMENT$",
            r"(?i)INTERNATIONAL TRANSACTION FEE",
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

fn compiled_banking_ops() -> &'static [CompiledBankingOp] {
    static COMPILED: OnceLock<Vec<CompiledBankingOp>> = OnceLock::new();
    COMPILED.get_or_init(|| {
        BANKING_OPS
            .iter()
            .flat_map(|op| {
                op.patterns.iter().map(move |&pat| CompiledBankingOp {
                    regex: Regex::new(pat).expect("invalid banking op pattern"),
                    operation: op.operation.clone(),
                    has_account: op.has_account,
                })
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_skip_if_classified() {
        let mut r = NormalisationResult::new("BPAY PAYMENT");
        r.set_class(PayeeClass::Person);
        apply(&mut r);
        assert!(r.features.operation.is_none());
    }
}
