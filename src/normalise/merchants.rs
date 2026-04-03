pub fn extract_merchant(_stripped: &str, _original: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_merchant_woolworths() {
        let result = extract_merchant("WOOLWORTHS 1624 STRATHFIELD", "WOOLWORTHS 1624 STRATHFIELD");
        assert_eq!(result, Some("Woolworths".to_string()));
    }
}
