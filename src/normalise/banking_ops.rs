use super::BankingOperation;

struct KnownBankingOp {
    pattern: &'static str,
    operation: BankingOperation,
}

const KNOWN_BANKING_OPS: &[KnownBankingOp] = &[
    KnownBankingOp { pattern: "INTEREST CHARGE", operation: BankingOperation::Interest },
    KnownBankingOp { pattern: "INTEREST ADJUSTMENT", operation: BankingOperation::Interest },
    KnownBankingOp { pattern: "INTEREST CORRECTION", operation: BankingOperation::Interest },
    KnownBankingOp { pattern: "CREDIT CARD", operation: BankingOperation::CreditCard },
    KnownBankingOp { pattern: "FUNDS TRANSFER", operation: BankingOperation::Transfer },
    KnownBankingOp { pattern: "ACCOUNT SERVICING FEE", operation: BankingOperation::AccountServicing },
    KnownBankingOp { pattern: "LOAN REPAYMENT", operation: BankingOperation::Loan },
    KnownBankingOp { pattern: "CASH DEPOSIT", operation: BankingOperation::Deposit },
    KnownBankingOp { pattern: "WDL ATM", operation: BankingOperation::Withdrawal },
];

/// Match the original payee description against known banking operations.
pub fn extract_banking_op(original: &str) -> Option<BankingOperation> {
    let upper = original.to_uppercase();
    for op in KNOWN_BANKING_OPS {
        if upper.contains(op.pattern) {
            return Some(op.operation.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_banking_op_interest() {
        let result = extract_banking_op("INTEREST CHARGE");
        assert_eq!(result, Some(BankingOperation::Interest));
    }
}
