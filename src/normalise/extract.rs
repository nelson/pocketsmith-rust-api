use super::{banking_ops, locations, merchants, NormalisationResult, PayeeClass};

/// Orchestrate entity extraction from all sub-extractors.
/// Sets `features.entity_name` from whichever matched, and `result.class` by priority.
#[allow(dead_code)]
pub fn extract_entities(result: &mut NormalisationResult) {
    let merchant = merchants::extract_merchant(&result.normalised, result.original());
    if let Some(merchant) = merchant {
        result.features.entity_name = Some(merchant);
        result.set_class(PayeeClass::Merchant);
        return;
    }

    let op = banking_ops::extract_banking_op(result.original());
    if let Some(op) = op {
        result.features.operation = Some(op);
        result.set_class(PayeeClass::Other);
        return;
    }

    // Extract location even if no entity matched
    if result.features.location.is_none() {
        let location = locations::extract_location(&result.normalised);
        result.features.location = location;
    }
}
