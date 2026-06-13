//! The hardcoded Google-place-type -> category/label taxonomy
//! (categorisation final stage).
//!
//! Per plan feedback #1, the place-type set is small and relatively
//! static, so the mapping is **hardcoded here** rather than a DB table.
//! This single [`TAXONOMY`] table is the one reviewable source of truth
//! for the whole categorisation hierarchy (feedback #4):
//!
//! ```text
//!   domain  ->  category title  ->  leaf  ->  Google place types
//! ```
//!
//! - The **category title** is matched to the user's `categories.title`
//!   at runtime to resolve a `category_id`, so this table is not tied to
//!   one account's numeric ids.
//! - The **leaf** is the only thing written to `transactions.labels`
//!   (feedback #3) — never the `domain/leaf` path. Leaf keys are unique
//!   across the whole taxonomy so a bare leaf is unambiguous (enforced by
//!   a unit test).
//! - A place type absent from the taxonomy is **unmapped**: the proposal
//!   gets no category and no label, and is flagged for manual review.

/// A leaf node: the label written to Pocketsmith plus the Google place
/// types that collapse into it.
#[derive(Debug, Clone, Copy)]
pub struct Leaf {
    /// The label string written to `transactions.labels` (leaf only).
    pub leaf: &'static str,
    /// Google Places `primaryType` / `types` values that map to this leaf.
    pub place_types: &'static [&'static str],
}

/// A domain groups leaves under a single Pocketsmith category. The domain
/// key exists only for review/grouping; it is never written anywhere.
#[derive(Debug, Clone, Copy)]
pub struct Domain {
    /// Review-only grouping key (e.g. "dining").
    pub domain: &'static str,
    /// Must match a `categories.title` in the user's account.
    pub category_title: &'static str,
    pub leaves: &'static [Leaf],
}

/// The complete categorisation taxonomy. Edit this table to change how
/// Google place types map to categories + labels. Reviewable at a glance.
pub static TAXONOMY: &[Domain] = &[
    Domain {
        domain: "dining",
        category_title: "Eating Out",
        leaves: &[
            Leaf { leaf: "restaurant", place_types: &["restaurant", "meal_takeaway", "meal_delivery", "fast_food_restaurant", "food"] },
            Leaf { leaf: "cafe", place_types: &["cafe", "coffee_shop", "bakery", "cafeteria"] },
            Leaf { leaf: "bar", place_types: &["bar", "pub", "night_club", "wine_bar", "liquor_store"] },
        ],
    },
    Domain {
        domain: "groceries",
        category_title: "_Groceries",
        leaves: &[
            Leaf { leaf: "supermarket", place_types: &["supermarket", "grocery_store", "convenience_store", "market", "butcher_shop", "greengrocer"] },
        ],
    },
    Domain {
        domain: "transport",
        category_title: "_Transport",
        leaves: &[
            Leaf { leaf: "fuel", place_types: &["gas_station"] },
            Leaf { leaf: "transit", place_types: &["parking", "transit_station", "train_station", "bus_station", "taxi_stand", "subway_station", "light_rail_station", "airport"] },
        ],
    },
    Domain {
        domain: "shopping",
        category_title: "_Shopping",
        leaves: &[
            Leaf { leaf: "clothing", place_types: &["clothing_store", "shoe_store", "jewelry_store"] },
            Leaf { leaf: "retail", place_types: &["department_store", "shopping_mall", "store"] },
        ],
    },
    Domain {
        domain: "household",
        category_title: "_Household",
        leaves: &[
            Leaf { leaf: "home_goods", place_types: &["electronics_store", "hardware_store", "furniture_store", "home_goods_store", "home_improvement_store"] },
        ],
    },
    Domain {
        domain: "bills",
        category_title: "_Bills",
        leaves: &[
            Leaf { leaf: "health", place_types: &["pharmacy", "drugstore", "hospital", "doctor", "dentist", "physiotherapist"] },
            Leaf { leaf: "financial", place_types: &["bank", "atm", "insurance_agency", "accounting"] },
            Leaf { leaf: "utilities", place_types: &["electrician", "plumber", "telecommunications_service_provider"] },
        ],
    },
    Domain {
        domain: "education",
        category_title: "_Education",
        leaves: &[
            Leaf { leaf: "education", place_types: &["school", "university", "library", "book_store", "primary_school", "secondary_school", "preschool"] },
        ],
    },
    Domain {
        domain: "holidays",
        category_title: "_Holidays",
        leaves: &[
            Leaf { leaf: "travel", place_types: &["lodging", "hotel", "travel_agency", "resort_hotel", "motel", "guest_house", "campground", "tourist_attraction"] },
        ],
    },
    Domain {
        domain: "giving",
        category_title: "_Giving",
        leaves: &[
            Leaf { leaf: "charity", place_types: &["church", "place_of_worship", "hindu_temple", "mosque", "synagogue"] },
        ],
    },
];

/// Result of mapping a Google place type through the taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapping {
    pub domain: &'static str,
    pub category_title: &'static str,
    pub leaf: &'static str,
}

/// Map a single Google place type to its taxonomy entry, if any. Matching
/// is case-insensitive on the place type. Returns `None` for an unmapped
/// type.
pub fn map_place_type(place_type: &str) -> Option<Mapping> {
    let pt = place_type.trim().to_ascii_lowercase();
    for domain in TAXONOMY {
        for leaf in domain.leaves {
            if leaf.place_types.iter().any(|t| t.eq_ignore_ascii_case(&pt)) {
                return Some(Mapping {
                    domain: domain.domain,
                    category_title: domain.category_title,
                    leaf: leaf.leaf,
                });
            }
        }
    }
    None
}

/// Map a list of place types (e.g. a Places `types` array, primary first)
/// to the first one that resolves through the taxonomy. The Places API
/// returns the most specific/primary type first, so first-match is the
/// right precedence.
pub fn map_types<'a, I>(types: I) -> Option<Mapping>
where
    I: IntoIterator<Item = &'a str>,
{
    types.into_iter().find_map(map_place_type)
}

/// Resolve a taxonomy `category_title` to the user's `categories.id`.
/// Returns `None` if the account has no category with that exact title
/// (e.g. the user renamed it) — the caller then treats the proposal as
/// unmapped rather than guessing.
pub fn resolve_category(
    conn: &rusqlite::Connection,
    category_title: &str,
) -> anyhow::Result<Option<i64>> {
    use rusqlite::OptionalExtension;
    let id = conn
        .query_row(
            "SELECT id FROM categories WHERE title = ?1",
            rusqlite::params![category_title],
            |r| r.get::<_, i64>(0),
        )
        .optional()?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn maps_known_types_to_expected_leaf_and_category() {
        let m = map_place_type("cafe").unwrap();
        assert_eq!(m.category_title, "Eating Out");
        assert_eq!(m.leaf, "cafe");
        assert_eq!(m.domain, "dining");

        assert_eq!(map_place_type("supermarket").unwrap().leaf, "supermarket");
        assert_eq!(map_place_type("grocery_store").unwrap().category_title, "_Groceries");
        assert_eq!(map_place_type("gas_station").unwrap().leaf, "fuel");
        assert_eq!(map_place_type("clothing_store").unwrap().leaf, "clothing");
        assert_eq!(map_place_type("bank").unwrap().leaf, "financial");
        assert_eq!(map_place_type("school").unwrap().leaf, "education");
        assert_eq!(map_place_type("hotel").unwrap().category_title, "_Holidays");
        assert_eq!(map_place_type("church").unwrap().leaf, "charity");
    }

    #[test]
    fn matching_is_case_insensitive_and_trims() {
        assert_eq!(map_place_type("  CAFE ").unwrap().leaf, "cafe");
        assert_eq!(map_place_type("Gas_Station").unwrap().leaf, "fuel");
    }

    #[test]
    fn unmapped_type_returns_none() {
        assert!(map_place_type("zoo").is_none());
        assert!(map_place_type("").is_none());
        assert!(map_place_type("political").is_none());
    }

    #[test]
    fn map_types_picks_first_resolvable_in_order() {
        // primary first: an unmapped primary falls through to a mapped one.
        let m = map_types(["point_of_interest", "establishment", "cafe", "bakery"]).unwrap();
        assert_eq!(m.leaf, "cafe", "first resolvable wins (cafe before bakery)");

        // all unmapped -> None.
        assert!(map_types(["establishment", "point_of_interest"]).is_none());
    }

    #[test]
    fn leaf_keys_are_globally_unique() {
        // A bare leaf label must be unambiguous across the whole taxonomy.
        let mut seen = HashSet::new();
        for d in TAXONOMY {
            for l in d.leaves {
                assert!(seen.insert(l.leaf), "duplicate leaf key: {}", l.leaf);
            }
        }
    }

    #[test]
    fn place_types_are_globally_unique() {
        // A place type must not map to two leaves (ambiguous precedence).
        let mut seen = HashSet::new();
        for d in TAXONOMY {
            for l in d.leaves {
                for pt in l.place_types {
                    assert!(seen.insert(*pt), "duplicate place type: {pt}");
                }
            }
        }
    }

    #[test]
    fn resolve_category_matches_title_exactly() {
        let conn = crate::db::initialize_in_memory().unwrap();
        conn.execute(
            "INSERT INTO categories (id, title) VALUES (7, '_Groceries'), (8, 'Eating Out')",
            [],
        )
        .unwrap();
        assert_eq!(resolve_category(&conn, "_Groceries").unwrap(), Some(7));
        assert_eq!(resolve_category(&conn, "Eating Out").unwrap(), Some(8));
        assert_eq!(resolve_category(&conn, "_Nonexistent").unwrap(), None);
    }
}
