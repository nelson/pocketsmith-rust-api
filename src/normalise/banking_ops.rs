pub fn extract_banking_op(_original: &str) -> Option<super::BankingOperation> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::BankingOperation;

    #[test]
    fn test_extract_banking_op_interest() {
        let result = extract_banking_op("INTEREST CHARGE");
        assert_eq!(result, Some(BankingOperation::Interest));
    }
}
