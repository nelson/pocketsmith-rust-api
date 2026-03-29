use std::collections::HashMap;

/// Map Google Places types to a PocketSmith category.
/// Iterates the place's types in order (most specific first from Google's API).
/// Returns the category name or None if no type matches.
pub fn map_place_to_category(
    types: &[String],
    mappings: &HashMap<String, String>,
) -> Option<String> {
    for t in types {
        if let Some(cat) = mappings.get(t.as_str()) {
            return Some(cat.clone());
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
        let cat = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Groceries");
    }

    #[test]
    fn test_restaurant_maps_to_dining() {
        let m = mappings();
        let types = vec!["restaurant".into(), "food".into(), "point_of_interest".into()];
        let cat = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Dining");
    }

    #[test]
    fn test_generic_store_maps_to_shopping() {
        let m = mappings();
        let types = vec!["store".into(), "point_of_interest".into()];
        let cat = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Shopping");
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
        let types = vec!["cafe".into(), "store".into()];
        let cat = map_place_to_category(&types, &m).unwrap();
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
        let cat = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Transport");
    }

    #[test]
    fn test_pharmacy_maps_to_bills() {
        let m = mappings();
        let types = vec!["pharmacy".into(), "store".into()];
        let cat = map_place_to_category(&types, &m).unwrap();
        assert_eq!(cat, "_Bills");
    }
}
