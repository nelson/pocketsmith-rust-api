use super::NormalisationResult;

pub fn extract_entities(_result: &mut NormalisationResult) {
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::PayeeClass;

    #[test]
    fn test_extract_entities_woolworths() {
        let mut result = NormalisationResult::new("WOOLWORTHS 1624 STRATHFIELD");
        extract_entities(&mut result);
        assert_eq!(result.features.entity_name.as_deref(), Some("Woolworths"));
        assert_eq!(result.class, PayeeClass::Merchant);
    }
}
