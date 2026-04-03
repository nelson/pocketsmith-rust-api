pub fn extract_location(_s: &str) -> Option<String> {
    None
}

pub fn is_known_location(_s: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_location_strathfield() {
        let result = extract_location("WOOLWORTHS 1624 STRATHFIELD");
        assert_eq!(result, Some("Strathfield".to_string()));
    }
}
