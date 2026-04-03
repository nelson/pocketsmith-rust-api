pub fn extract_person(_stripped: &str, _original: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_person_johnny_tam() {
        let result = extract_person(
            "JOHNNY TAM",
            "Fast Transfer From Johnny Tam, to PayID Phone",
        );
        assert_eq!(result, Some("Johnny Tam".to_string()));
    }
}
