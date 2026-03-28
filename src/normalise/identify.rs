use crate::normalise::{meta, Metadata};
use crate::normalise::rules::{CompiledEntityExtraction, CompiledIdentifyRules};
use serde_json::Value;

/// Stage 4: Identity-based normalisation.
/// Routes by payee type (salary, transfer, banking_operation, merchant)
/// and applies identity mappings, person canonicalisation, merchant patterns.
/// Returns the normalised payee string.
pub fn apply(
    payee: &str,
    original_payee: &str,
    metadata: &mut Metadata,
    rules: &CompiledIdentifyRules,
) -> String {
    let payee_type = metadata
        .get(meta::TYPE)
        .and_then(|v| v.as_str())
        .unwrap_or("merchant")
        .to_string();

    match payee_type.as_str() {
        "salary" => apply_salary(payee, metadata, rules),
        "transfer_in" | "transfer_out" => {
            apply_transfer(payee, original_payee, &payee_type, metadata, rules)
        }
        "banking_operation" => apply_banking(payee, original_payee, metadata, rules),
        "merchant" => apply_merchant(payee, metadata, rules),
        _ => payee.to_string(),
    }
}

fn apply_salary(payee: &str, metadata: &mut Metadata, rules: &CompiledIdentifyRules) -> String {
    let entity = metadata
        .get(meta::EXTRACTED_ENTITY)
        .and_then(|v| v.as_str())
        .unwrap_or(payee);
    let canonical = resolve_employer(entity, rules);
    metadata.insert("identity".into(), Value::String(canonical.clone()));
    canonical
}

fn apply_transfer(
    payee: &str,
    original_payee: &str,
    payee_type: &str,
    metadata: &mut Metadata,
    rules: &CompiledIdentifyRules,
) -> String {
    // Check internal account transfers first
    for m in &rules.internal_account_mappings {
        if m.re.is_match(original_payee) {
            metadata.insert("identity".into(), Value::String(m.canonical.clone()));
            return m.canonical.clone();
        }
    }

    // Extract entity
    let entity = extract_entity(original_payee, &rules.transfer_entity_extraction, true)
        .or_else(|| {
            metadata
                .get(meta::EXTRACTED_ENTITY)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| payee.to_string());

    // Check if entity is a known employer
    let resolved = resolve_employer(&entity, rules);
    if resolved != entity {
        if payee_type == "transfer_out" {
            // Outgoing to employer = donation
            let paren_re = crate::normalise::rules::Re::new(r"\(.*?\)").expect("static regex");
            let out_label = paren_re.replace(&resolved, "(Donation)").to_string();
            metadata.insert("identity".into(), Value::String(out_label.clone()));
            return out_label;
        }
        metadata.insert("identity".into(), Value::String(resolved.clone()));
        return resolved;
    }

    // Canonicalise as person
    let canonical = canonicalise_person(&entity, rules);
    metadata.insert("identity".into(), Value::String(canonical.clone()));
    canonical
}

fn apply_banking(
    payee: &str,
    original_payee: &str,
    metadata: &mut Metadata,
    rules: &CompiledIdentifyRules,
) -> String {
    // Strip known prefixes for matching
    let mut stripped_orig = original_payee.to_string();
    if let Some(rest) = stripped_orig.strip_prefix("Cafes - ") {
        stripped_orig = rest.to_string();
    }
    let return_re = crate::normalise::rules::Re::new(r"^Return \d{2}/\d{2}/\d{2},?\s*")
        .expect("static regex");
    if let Some((_start, end)) = return_re.find(&stripped_orig) {
        stripped_orig = stripped_orig[end..].to_string();
    }

    // Check banking identity mappings
    for b in &rules.banking_identity_mappings {
        if b.re.is_match(original_payee) || b.re.is_match(&stripped_orig) {
            metadata.insert("identity".into(), Value::String(b.canonical.clone()));
            return b.canonical.clone();
        }
    }

    // Extract banking entity
    if let Some(entity) = extract_entity(original_payee, &rules.banking_entity_extraction, false) {
        let resolved = resolve_employer(&entity, rules);
        if resolved != entity {
            metadata.insert("identity".into(), Value::String(resolved.clone()));
            return resolved;
        }
        metadata.insert("identity".into(), Value::String(entity.clone()));
        return entity;
    }

    // Fallback: use extracted_entity from stage 2
    if let Some(entity) = metadata
        .get(meta::EXTRACTED_ENTITY)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        metadata.insert("identity".into(), Value::String(entity.clone()));
        return entity;
    }

    payee.to_string()
}

fn apply_merchant(
    payee: &str,
    metadata: &mut Metadata,
    rules: &CompiledIdentifyRules,
) -> String {
    // Try merchant identity mappings
    if let Some(canonical) = apply_merchant_mappings(payee, rules) {
        metadata.insert("identity".into(), Value::String(canonical.clone()));
        return canonical;
    }

    // Strip PTY LTD etc
    let mut result = strip_suffixes(payee, rules);
    if result != payee {
        metadata.insert("pty_stripped".into(), Value::Bool(true));
    }

    // Check default_locations
    if let Some((canonical_name, location)) = rules.default_locations.get(&result.to_uppercase()) {
        result = format!("{} {}", canonical_name, location);
        metadata.insert("default_location".into(), Value::String(location.clone()));
    }

    // Tag merchant group
    for grp in &rules.merchant_groups {
        if grp.re.is_match(&result) {
            metadata.insert("merchant_group".into(), Value::String(grp.canonical.clone()));
            break;
        }
    }

    result
}

fn resolve_employer(entity: &str, rules: &CompiledIdentifyRules) -> String {
    let upper = entity.to_uppercase();
    for emp in &rules.employers {
        for pat in &emp.patterns_upper {
            if upper.contains(pat.as_str()) {
                return emp.canonical.clone();
            }
        }
    }
    entity.to_string()
}

fn canonicalise_person(name: &str, rules: &CompiledIdentifyRules) -> String {
    // Exact match
    if let Some(canonical) = rules.persons.get(name) {
        return canonical.clone();
    }

    // Case-insensitive
    if let Some(canonical) = rules.persons_upper.get(&name.to_uppercase()) {
        return canonical.clone();
    }

    // Normalise whitespace
    let stripped: String = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if stripped != name {
        if let Some(canonical) = rules.persons.get(&stripped) {
            return canonical.clone();
        }
    }

    // Strip title prefix and retry
    if let Some((_start, end)) = rules.title_re.find(name) {
        let title_stripped = &name[end..];
        if let Some(canonical) = rules.persons.get(title_stripped) {
            return canonical.clone();
        }
        if let Some(canonical) = rules.persons_upper.get(&title_stripped.to_uppercase()) {
            return canonical.clone();
        }
    }

    // Check persons_strip_memo (name starts with known person + extra)
    let upper = name.to_uppercase();
    for person in &rules.persons_strip_memo {
        let person_upper = person.to_uppercase();
        if upper.starts_with(&person_upper) && name.len() > person.len() {
            return canonicalise_person(person, rules);
        }
    }

    name.to_string()
}

fn clean_capture(value: &str, rules: &CompiledIdentifyRules) -> String {
    let mut result = value.trim().to_string();
    for _ in 0..3 {
        let cleaned = rules.capture_noise.replace_all(&result, "").trim().to_string();
        if cleaned == result {
            break;
        }
        result = cleaned;
    }
    // Remove duplicate trailing location (e.g., "Fairy Meadow Fairy Meadow" → "Fairy Meadow")
    let upper = result.to_uppercase();
    for loc in &rules.known_locations {
        if let Some(first) = upper.find(loc.as_str()) {
            if let Some(second) = upper[first + loc.len()..].find(loc.as_str()) {
                let cut = first + loc.len() + second;
                result = result[..cut].trim().to_string();
                break;
            }
        }
    }
    result
}

fn apply_merchant_mappings(payee: &str, rules: &CompiledIdentifyRules) -> Option<String> {
    for mapping in &rules.merchant_mappings {
        if let Some(caps) = mapping.re.captures(payee) {
            let mut canonical = mapping.canonical.clone();
            for i in 1..10 {
                let placeholder = format!("{{{}}}", i);
                if canonical.contains(&placeholder) {
                    let replacement = caps
                        .get(i)
                        .map(|m| clean_capture(m.as_str(), rules))
                        .unwrap_or_default();
                    canonical = canonical.replace(&placeholder, &replacement);
                }
            }
            return Some(canonical.trim().to_string());
        }
    }
    None
}

fn strip_suffixes(payee: &str, rules: &CompiledIdentifyRules) -> String {
    let mut result = payee.to_string();
    for re in &rules.strip_patterns {
        result = re.replace_all(&result, "").into_owned();
    }
    result.trim().to_string()
}

fn extract_entity(
    text: &str,
    extractions: &[CompiledEntityExtraction],
    trim_punct: bool,
) -> Option<String> {
    for e in extractions {
        if let Some(caps) = e.re.captures(text) {
            if let Some(m) = caps.get(e.group) {
                let entity = if trim_punct {
                    m.as_str().trim().trim_end_matches(&[',', ';', '.'][..])
                } else {
                    m.as_str().trim()
                };
                let result = match &e.prefix {
                    Some(prefix) => format!("{} {}", prefix, entity),
                    None => entity.to_string(),
                };
                return Some(result);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::rules::CompiledIdentifyRules;
    use std::path::Path;

    fn rules() -> CompiledIdentifyRules {
        CompiledIdentifyRules::load(Path::new("rules")).unwrap()
    }

    fn meta_with_type(t: &str) -> Metadata {
        let mut m = Metadata::new();
        m.insert(meta::TYPE.into(), Value::String(t.into()));
        m
    }

    // Salary tests
    #[test]
    fn test_salary_apple() {
        let r = rules();
        let mut meta = meta_with_type("salary");
        meta.insert(meta::EXTRACTED_ENTITY.into(), Value::String("APPLE PTY LTD".into()));
        let result = apply("APPLE PTY LTD", "Salary from APPLE PTY LTD", &mut meta, &r);
        assert_eq!(result, "Apple (Salary)");
    }

    #[test]
    fn test_salary_afes() {
        let r = rules();
        let mut meta = meta_with_type("salary");
        meta.insert(meta::EXTRACTED_ENTITY.into(), Value::String("AFES".into()));
        let result = apply("AFES", "Salary AFES", &mut meta, &r);
        assert_eq!(result, "AFES (Sophia Salary)");
    }

    // Transfer tests
    #[test]
    fn test_transfer_internal() {
        let r = rules();
        let mut meta = meta_with_type("transfer_out");
        let result = apply(
            "Transfer to xx1234",
            "Transfer to xx1234 CommBank App",
            &mut meta,
            &r,
        );
        assert_eq!(result, "Internal Account Transfer");
    }

    #[test]
    fn test_transfer_person_osko() {
        let r = rules();
        let mut meta = meta_with_type("transfer_in");
        let _result = apply(
            "John Smith",
            "John Smith 12345678 - Osko Payment - Receipt 12345",
            &mut meta,
            &r,
        );
        assert_eq!(meta.get("identity").unwrap().as_str().unwrap(), "John Smith");
    }

    #[test]
    fn test_transfer_known_person() {
        let r = rules();
        let mut meta = meta_with_type("transfer_in");
        let result = apply(
            "Johnny Tam",
            "Fast Transfer From Johnny Tam, PayID Phone",
            &mut meta,
            &r,
        );
        assert_eq!(result, "Johnny Tam");
    }

    // Banking operation tests
    #[test]
    fn test_banking_afes_donation() {
        let r = rules();
        let mut meta = meta_with_type("banking_operation");
        let result = apply(
            "Direct Debit 12345 AFES",
            "Direct Debit 12345 AFES",
            &mut meta,
            &r,
        );
        assert_eq!(result, "AFES (Donation)");
    }

    #[test]
    fn test_banking_bpay() {
        let r = rules();
        let mut meta = meta_with_type("banking_operation");
        let result = apply(
            "BPAY Payment to AGL",
            "BPAY Payment to AGL Energy",
            &mut meta,
            &r,
        );
        assert_eq!(result, "BPAY Payment");
    }

    // Merchant tests
    #[test]
    fn test_merchant_woolworths() {
        let r = rules();
        let mut meta = meta_with_type("merchant");
        let result = apply(
            "WOOLWORTHS 1234 STRATHFIELD",
            "WOOLWORTHS 1234 STRATHFIELD",
            &mut meta,
            &r,
        );
        // Capture group preserves input casing; title casing happens in stage 5
        assert_eq!(result, "Woolworths STRATHFIELD");
        assert_eq!(meta["identity"], "Woolworths STRATHFIELD");
    }

    #[test]
    fn test_merchant_mcdonalds() {
        let r = rules();
        let mut meta = meta_with_type("merchant");
        let result = apply(
            "MCDONALD'S STRATHFIELD",
            "MCDONALD'S STRATHFIELD",
            &mut meta,
            &r,
        );
        assert_eq!(result, "McDonald's Strathfield South");
    }

    #[test]
    fn test_merchant_amazon() {
        let r = rules();
        let mut meta = meta_with_type("merchant");
        let result = apply(
            "AMAZON MKTPL*AB1234CD",
            "AMAZON MKTPL*AB1234CD",
            &mut meta,
            &r,
        );
        assert_eq!(result, "Amazon Marketplace");
    }

    #[test]
    fn test_merchant_strip_pty_ltd() {
        let r = rules();
        let mut meta = meta_with_type("merchant");
        let result = apply(
            "SOME COMPANY PTY LTD",
            "SOME COMPANY PTY LTD",
            &mut meta,
            &r,
        );
        assert_eq!(result, "SOME COMPANY");
        assert_eq!(meta.get("pty_stripped"), Some(&Value::Bool(true)));
    }

    #[test]
    fn test_merchant_default_location() {
        let r = rules();
        let mut meta = meta_with_type("merchant");
        let result = apply(
            "HANARO MART",
            "HANARO MART",
            &mut meta,
            &r,
        );
        assert_eq!(result, "Hanaro Mart Strathfield");
        assert_eq!(meta["default_location"], "Strathfield");
    }

    #[test]
    fn test_merchant_transport_nsw() {
        let r = rules();
        let mut meta = meta_with_type("merchant");
        let result = apply(
            "TRANSPORTFORNSWTRAVEL SYDNEY",
            "TRANSPORTFORNSWTRAVEL SYDNEY",
            &mut meta,
            &r,
        );
        assert_eq!(result, "Transport NSW");
    }

    // Person canonicalisation tests
    #[test]
    fn test_person_canonicalise_exact() {
        let r = rules();
        assert_eq!(canonicalise_person("Nelson Tam", &r), "Nelson Tam");
    }

    #[test]
    fn test_person_canonicalise_case_insensitive() {
        let r = rules();
        assert_eq!(canonicalise_person("NELSON TAM", &r), "Nelson Tam");
    }

    #[test]
    fn test_person_strip_memo() {
        let r = rules();
        assert_eq!(
            canonicalise_person("Johnny Tam some memo text", &r),
            "Johnny Tam"
        );
    }

    #[test]
    fn test_person_title_strip() {
        let r = rules();
        assert_eq!(
            canonicalise_person("Mr David James Greenaway", &r),
            "David Greenaway"
        );
    }

    // Merchant capture group tests
    #[test]
    fn test_merchant_with_capture() {
        let r = rules();
        let mut meta = meta_with_type("merchant");
        let result = apply(
            "COLES 1234 STRATHFIELD",
            "COLES 1234 STRATHFIELD",
            &mut meta,
            &r,
        );
        // Capture group preserves input casing; title casing happens in stage 5
        assert_eq!(result, "Coles STRATHFIELD");
    }

    #[test]
    fn test_merchant_starbucks_remap() {
        let r = rules();
        let mut meta = meta_with_type("merchant");
        let result = apply(
            "STARBUCKS COFFEE B123 BROADWAY NSW",
            "STARBUCKS COFFEE B123 BROADWAY NSW",
            &mut meta,
            &r,
        );
        assert_eq!(result, "Starbucks Ultimo");
    }
}
