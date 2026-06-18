//! Pure proposal builder: turn a cached Places lookup into a proposed
//! category + leaf labels via the hardcoded taxonomy (categorisation
//! final stage, plan step 4).
//!
//! This is the deterministic core the scan drives. Given a lookup row and
//! the taxonomy, it picks the first resolvable place type (Places returns
//! the primary/most-specific type first), resolves the taxonomy category
//! title to the user's `category_id`, and emits the single leaf label.
//! Unmapped (no resolvable type, or the account lacks the target
//! category) yields an empty proposal flagged for review.

use anyhow::Result;
use rusqlite::Connection;

use crate::categorise::map::{self, Mapping};
use crate::db::place_lookups::{LookupStatus, PlaceLookupRow};

/// The category + labels a lookup maps to. `category_id == None` means
/// "unmapped" (leave uncategorised, flag for review).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Proposal {
    pub category_id: Option<i64>,
    /// Leaf labels (controlled vocabulary). At most one today, but kept a
    /// Vec so the model accommodates a future multi-label leaf.
    pub labels: Vec<String>,
    /// The Google place type that drove the mapping (for display / audit).
    pub place_type: Option<String>,
}

/// The candidate place types from a lookup, primary first. Used so the
/// precedence is "primary type, then the remaining `types` in order".
fn candidate_types(lookup: &PlaceLookupRow) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(pt) = &lookup.primary_type {
        out.push(pt.clone());
    }
    for t in &lookup.types {
        if Some(t) != lookup.primary_type.as_ref() {
            out.push(t.clone());
        }
    }
    out
}

/// Build a proposal from a cached lookup. A non-`Ok` lookup (no result /
/// error) always yields an empty proposal.
pub fn build(conn: &Connection, lookup: &PlaceLookupRow) -> Result<Proposal> {
    if lookup.status != LookupStatus::Ok {
        return Ok(Proposal::default());
    }

    let candidates = candidate_types(lookup);
    let Some(Mapping { category_title, leaf, .. }) =
        map::map_types(candidates.iter().map(|s| s.as_str()))
    else {
        // A type was returned but none is in our taxonomy.
        return Ok(Proposal {
            place_type: lookup.primary_type.clone(),
            ..Proposal::default()
        });
    };

    // The driving place type is the first candidate that resolved.
    let driving = candidates
        .iter()
        .find(|t| map::map_place_type(t).is_some())
        .cloned()
        .or_else(|| lookup.primary_type.clone());

    let category_id = map::resolve_category(conn, category_title)?;
    // If the account lacks the mapped category, treat as unmapped category
    // but still record the leaf label + place type for the reviewer.
    Ok(Proposal {
        category_id,
        labels: vec![leaf.to_string()],
        place_type: driving,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;

    fn seed_categories(conn: &Connection) {
        conn.execute(
            "INSERT INTO categories (id, title) VALUES
                (1, 'Eating Out'), (2, '_Groceries'), (3, '_Transport'),
                (4, '_Shopping'), (5, '_Bills')",
            [],
        )
        .unwrap();
    }

    fn lookup(primary: Option<&str>, types: &[&str], status: LookupStatus) -> PlaceLookupRow {
        PlaceLookupRow {
            query: "q".into(),
            place_id: Some("id".into()),
            display_name: Some("Name".into()),
            primary_type: primary.map(|s| s.to_string()),
            types: types.iter().map(|s| s.to_string()).collect(),
            response_json: "{}".into(),
            status,
        }
    }

    #[test]
    fn primary_type_drives_the_proposal() {
        let conn = initialize_in_memory().unwrap();
        seed_categories(&conn);
        let p = build(&conn, &lookup(Some("supermarket"), &["supermarket", "store"], LookupStatus::Ok)).unwrap();
        assert_eq!(p.category_id, Some(2));
        assert_eq!(p.labels, vec!["supermarket"]);
        assert_eq!(p.place_type.as_deref(), Some("supermarket"));
    }

    #[test]
    fn cafe_maps_to_eating_out() {
        let conn = initialize_in_memory().unwrap();
        seed_categories(&conn);
        let p = build(&conn, &lookup(Some("cafe"), &["cafe", "food"], LookupStatus::Ok)).unwrap();
        assert_eq!(p.category_id, Some(1));
        assert_eq!(p.labels, vec!["cafe"]);
    }

    #[test]
    fn falls_through_to_first_resolvable_type() {
        let conn = initialize_in_memory().unwrap();
        seed_categories(&conn);
        // primary unmapped; first resolvable in `types` wins.
        let p = build(
            &conn,
            &lookup(Some("point_of_interest"), &["establishment", "clothing_store"], LookupStatus::Ok),
        )
        .unwrap();
        assert_eq!(p.category_id, Some(4));
        assert_eq!(p.labels, vec!["clothing"]);
        assert_eq!(p.place_type.as_deref(), Some("clothing_store"));
    }

    #[test]
    fn unmapped_type_yields_empty_proposal_with_place_type() {
        let conn = initialize_in_memory().unwrap();
        seed_categories(&conn);
        let p = build(&conn, &lookup(Some("zoo"), &["zoo", "tourist_attraction_x"], LookupStatus::Ok)).unwrap();
        // 'zoo' unmapped; the second is a made-up unmapped type too.
        assert_eq!(p.category_id, None);
        assert!(p.labels.is_empty());
        assert_eq!(p.place_type.as_deref(), Some("zoo"));
    }

    #[test]
    fn no_result_lookup_is_empty() {
        let conn = initialize_in_memory().unwrap();
        seed_categories(&conn);
        let p = build(&conn, &lookup(None, &[], LookupStatus::NoResult)).unwrap();
        assert_eq!(p, Proposal::default());
    }

    #[test]
    fn error_lookup_is_empty() {
        let conn = initialize_in_memory().unwrap();
        seed_categories(&conn);
        let p = build(&conn, &lookup(Some("cafe"), &["cafe"], LookupStatus::Error)).unwrap();
        assert_eq!(p, Proposal::default());
    }

    #[test]
    fn mapped_type_but_missing_category_keeps_label_drops_id() {
        let conn = initialize_in_memory().unwrap();
        // No categories seeded at all -> resolve_category returns None.
        let p = build(&conn, &lookup(Some("cafe"), &["cafe"], LookupStatus::Ok)).unwrap();
        assert_eq!(p.category_id, None);
        assert_eq!(p.labels, vec!["cafe"], "label still proposed for the reviewer");
        assert_eq!(p.place_type.as_deref(), Some("cafe"));
    }

    #[test]
    fn primary_absent_uses_types_order() {
        let conn = initialize_in_memory().unwrap();
        seed_categories(&conn);
        let p = build(&conn, &lookup(None, &["gas_station", "store"], LookupStatus::Ok)).unwrap();
        assert_eq!(p.category_id, Some(3));
        assert_eq!(p.labels, vec!["fuel"]);
    }
}
