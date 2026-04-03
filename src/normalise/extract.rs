use super::{
    banking_ops, employers, locations, merchants, persons, NormalisationResult, PayeeClass,
};

/// Orchestrate entity extraction from all sub-extractors.
/// Sets `features.entity_name` from whichever matched, and `result.class` by priority.
pub fn extract_entities(result: &mut NormalisationResult) {
    let stripped = &result.normalised;
    let original = &result.original;

    // Try each extractor in priority order
    if let Some(employer) = employers::extract_employer(original) {
        result.features.entity_name = Some(employer);
        result.class = PayeeClass::Employer;
        return;
    }

    if let Some(person) = persons::extract_person(stripped, original) {
        result.features.entity_name = Some(person);
        result.class = PayeeClass::Person;
        return;
    }

    if let Some(merchant) = merchants::extract_merchant(stripped, original) {
        result.features.entity_name = Some(merchant);
        result.class = PayeeClass::Merchant;
        return;
    }

    if let Some(op) = banking_ops::extract_banking_op(original) {
        result.features.banking_op = Some(op);
        result.class = PayeeClass::Other;
        return;
    }

    // Extract location even if no entity matched
    if result.features.location.is_none() {
        result.features.location = locations::extract_location(stripped);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_entities_woolworths() {
        let mut result = NormalisationResult::new("WOOLWORTHS 1624 STRATHFIELD");
        extract_entities(&mut result);
        assert_eq!(result.features.entity_name.as_deref(), Some("Woolworths"));
        assert_eq!(result.class, PayeeClass::Merchant);
    }
}
