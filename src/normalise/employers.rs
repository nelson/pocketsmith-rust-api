pub fn extract_employer(_original: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_employer_apple() {
        let result = extract_employer("Salary from Apple Pty Ltd");
        assert_eq!(result, Some("Apple".to_string()));
    }
}
