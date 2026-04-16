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
    if result.features.operation.is_none() {
        for cop in compiled_banking_ops() {
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
