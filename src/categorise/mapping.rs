use std::collections::HashMap;

use super::confidence;

/// Map Google Places types to a PocketSmith category.
/// Iterates the place's types in order (most specific first from Google's API).
/// Returns (category, confidence) or None if no type matches.
pub fn map_place_to_category(
    types: &[String],
    mappings: &HashMap<String, String>,
) -> Option<(String, f64)> {
    const GENERIC_TYPES: &[&str] = &["store", "point_of_interest", "establishment", "food"];

    for t in types {
        if let Some(cat) = mappings.get(t.as_str()) {
            let conf = if GENERIC_TYPES.contains(&t.as_str()) {
                confidence::PLACES_GENERIC
            } else {
                confidence::PLACES_SPECIFIC
            };
            return Some((cat.clone(), conf));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categorise::rules::CategoriseRules;
    use std::path::Path;

    fn mappings() -> HashMap<String, String> {
        CategoriseRules::load(Path::new("rules"))
            .unwrap()
            .google_places_mappings
    }

    #[test]
    fn test_supermarket_maps_to_groceries() {
        let m = mappings();
        let types = vec!["supermarket".into(), "grocery_store".into(), "store".into()];
        let (cat, conf) = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Groceries");
        assert!(conf > 0.80);
    }

    #[test]
    fn test_restaurant_maps_to_dining() {
        let m = mappings();
        let types = vec!["restaurant".into(), "food".into(), "point_of_interest".into()];
        let (cat, conf) = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Dining");
        assert!(conf > 0.80);
    }

    #[test]
    fn test_generic_store_lower_confidence() {
        let m = mappings();
        let types = vec!["store".into(), "point_of_interest".into()];
        let (cat, conf) = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Shopping");
        assert!(conf <= 0.70);
    }

    #[test]
    fn test_unknown_type_returns_none() {
        let m = mappings();
        let types = vec!["point_of_interest".into(), "establishment".into()];
        let result = map_place_to_category(&types, &m);
        assert!(result.is_none());
    }

    #[test]
    fn test_first_matching_type_wins() {
        let m = mappings();
        // cafe before store -> should be _Dining not _Shopping
        let types = vec!["cafe".into(), "store".into()];
        let (cat, _) = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Dining");
    }

    #[test]
    fn test_empty_types_returns_none() {
        let m = mappings();
        let result = map_place_to_category(&[], &m);
        assert!(result.is_none());
    }

    #[test]
    fn test_gas_station_maps_to_transport() {
        let m = mappings();
        let types = vec!["gas_station".into()];
        let (cat, _) = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Transport");
    }

    #[test]
    fn test_pharmacy_maps_to_bills() {
        let m = mappings();
        let types = vec!["pharmacy".into(), "store".into()];
        let (cat, _) = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Bills");
    }
}
