use crate::normalise::Metadata;
use crate::normalise::rules::CompiledCleanupRules;
use std::collections::HashSet;

/// Stage 5: Final cleanup — whitespace normalisation, dedup, title casing.
/// Returns the cleaned payee string.
pub fn apply(payee: &str, _metadata: &mut Metadata, rules: &CompiledCleanupRules) -> String {
    let mut result = payee.to_string();

    // Remove leading/trailing punctuation
    result = result.trim_matches(|c: char| ",;.\\/".contains(c) || c.is_whitespace()).to_string();

    // Normalise whitespace
    result = result.split_whitespace().collect::<Vec<_>>().join(" ");

    // Remove duplicate consecutive words (case-insensitive)
    let words: Vec<&str> = result.split_whitespace().collect();
    if !words.is_empty() {
        let mut deduped = vec![words[0]];
        for w in &words[1..] {
            if !w.eq_ignore_ascii_case(deduped.last().unwrap()) {
                deduped.push(w);
            }
        }
        result = deduped.join(" ");
    }

    // Remove trailing noise patterns
    for re in &rules.trailing {
        result = re.replace_all(&result, "").trim().to_string();
    }

    // Remove truncated-prefix duplicates
    result = remove_prefix_duplicates(&result);

    // Smart title case
    result = smart_title_case(&result, &rules.upper_set, &rules.lower_set);

    // Restore brand casing
    result = restore_brands(&result);

    result.trim().to_string()
}

const PREFIX_EXCLUDE: &[&str] = &[
    "SAINT", "STREET", "MOUNT", "NORTH", "SOUTH", "EAST", "WEST", "EVERY",
    "KING", "OVER", "UNDER", "CAMP", "PORT", "GRAND", "PARK", "PALM",
];

fn remove_prefix_duplicates(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut cleaned = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if i < words.len() - 1 {
            let a = words[i];
            let b = words[i + 1];
            let au = a.to_uppercase();
            let bu = b.to_uppercase();
            if a.len() >= 4
                && b.len() > a.len()
                && bu.starts_with(&au)
                && au != bu
                && !PREFIX_EXCLUDE.contains(&au.as_str())
                && !au.starts_with("XXXX")
            {
                // Skip the truncated word, keep the full one
                i += 1;
                continue;
            }
        }
        cleaned.push(words[i]);
        i += 1;
    }
    cleaned.join(" ")
}

fn smart_title_case(text: &str, upper_set: &HashSet<String>, lower_set: &HashSet<String>) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut result = Vec::new();

    for (i, word) in words.iter().enumerate() {
        let upper = word.to_uppercase();
        if upper_set.contains(&upper) {
            result.push(upper);
        } else if lower_set.contains(&word.to_lowercase()) && i > 0 {
            result.push(word.to_lowercase());
        } else if word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) && word.len() > 1 {
            // All-caps word: title case it
            result.push(title_case_word(word));
        } else if word.chars().next().map_or(false, |c| c.is_uppercase()) {
            // Already has some capitalisation, keep it
            result.push(word.to_string());
        } else {
            result.push(title_case_word(word));
        }
    }

    result.join(" ")
}

fn title_case_word(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => {
            let mut s = c.to_uppercase().to_string();
            for ch in chars {
                s.extend(ch.to_lowercase());
            }
            s
        }
    }
}

const BRAND_PRESERVES: &[(&str, &str)] = &[
    ("Ebay", "eBay"),
    ("Iphone", "iPhone"),
    ("Ipad", "iPad"),
    ("Youtube", "YouTube"),
    ("Paypal", "PayPal"),
    ("Doordash", "DoorDash"),
    ("Pocketsmith", "PocketSmith"),
    ("Commbank", "CommBank"),
    ("Netbank", "NetBank"),
];

fn restore_brands(text: &str) -> String {
    let mut result = text.to_string();
    for (title_form, correct) in BRAND_PRESERVES {
        // Case-insensitive replace of title-cased form
        if let Some(pos) = result.to_lowercase().find(&title_form.to_lowercase()) {
            let end = pos + title_form.len();
            result = format!("{}{}{}", &result[..pos], correct, &result[end..]);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::rules::CompiledCleanupRules;
    use std::path::Path;

    fn rules() -> CompiledCleanupRules {
        CompiledCleanupRules::load(Path::new("rules")).unwrap()
    }

    #[test]
    fn test_title_case_basic() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("WOOLWORTHS STRATHFIELD", &mut meta, &r);
        assert_eq!(result, "Woolworths Strathfield");
    }

    #[test]
    fn test_uppercase_exception() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("KFC STRATHFIELD", &mut meta, &r);
        assert_eq!(result, "KFC Strathfield");
    }

    #[test]
    fn test_lowercase_exception() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("TASTE OF SHANGHAI", &mut meta, &r);
        assert_eq!(result, "Taste of Shanghai");
    }

    #[test]
    fn test_first_word_not_lowered() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("THE LOCAL ENFIELD", &mut meta, &r);
        assert_eq!(result, "The Local Enfield");
    }

    #[test]
    fn test_duplicate_word_removal() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("COLES COLES STRATHFIELD", &mut meta, &r);
        assert_eq!(result, "Coles Strathfield");
    }

    #[test]
    fn test_trailing_noise_removal() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("WOOLWORTHS STRATHFIELD NS", &mut meta, &r);
        assert_eq!(result, "Woolworths Strathfield");
    }

    #[test]
    fn test_whitespace_normalisation() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("  WOOLWORTHS   STRATHFIELD  ", &mut meta, &r);
        assert_eq!(result, "Woolworths Strathfield");
    }

    #[test]
    fn test_prefix_duplicate_removal() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("STRATH STRATHFIELD", &mut meta, &r);
        assert_eq!(result, "Strathfield");
    }

    #[test]
    fn test_prefix_exclude_preserved() {
        let r = rules();
        let mut meta = Metadata::new();
        // "NORTH" is in prefix_exclude so "NORTH NORTHMEAD" should NOT be deduped
        let result = apply("NORTH NORTHMEAD", &mut meta, &r);
        assert_eq!(result, "North Northmead");
    }

    #[test]
    fn test_brand_preservation() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("PAYPAL EBAY", &mut meta, &r);
        assert_eq!(result, "PayPal eBay");
    }

    #[test]
    fn test_preserves_existing_casing() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("McDonald's Strathfield South", &mut meta, &r);
        assert_eq!(result, "McDonald's Strathfield South");
    }

    #[test]
    fn test_trailing_punctuation() {
        let r = rules();
        let mut meta = Metadata::new();
        let result = apply("SOME MERCHANT,", &mut meta, &r);
        assert_eq!(result, "Some Merchant");
    }
}
