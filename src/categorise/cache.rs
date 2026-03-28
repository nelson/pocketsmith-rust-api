use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceResult {
    pub place_name: Option<String>,
    pub place_types: Vec<String>,
    pub place_address: Option<String>,
}

pub fn get_cached(conn: &Connection, query: &str, provider: &str) -> Result<Option<PlaceResult>> {
    let mut stmt = conn.prepare(
        "SELECT place_name, place_types, place_address FROM places_cache
         WHERE query = ?1 AND provider = ?2",
    )?;
    let result = stmt
        .query_row(rusqlite::params![query, provider], |row| {
            let name: Option<String> = row.get(0)?;
            let types_json: Option<String> = row.get(1)?;
            let address: Option<String> = row.get(2)?;
            Ok(PlaceResult {
                place_name: name,
                place_types: types_json
                    .and_then(|j| serde_json::from_str(&j).ok())
                    .unwrap_or_default(),
                place_address: address,
            })
        })
        .ok();
    Ok(result)
}

pub fn set_cached(
    conn: &Connection,
    query: &str,
    provider: &str,
    result: &PlaceResult,
    raw_response: Option<&str>,
) -> Result<()> {
    let types_json = serde_json::to_string(&result.place_types)?;
    conn.execute(
        "INSERT OR REPLACE INTO places_cache (query, provider, place_name, place_types, place_address, raw_response)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            query,
            provider,
            result.place_name,
            types_json,
            result.place_address,
            raw_response,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn test_cache_miss_returns_none() {
        let conn = db::initialize_in_memory().unwrap();
        let result = get_cached(&conn, "Woolworths", "google_places").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_insert_then_retrieve() {
        let conn = db::initialize_in_memory().unwrap();
        let place = PlaceResult {
            place_name: Some("Woolworths Strathfield".into()),
            place_types: vec!["supermarket".into(), "grocery_store".into()],
            place_address: Some("123 Main St".into()),
        };
        set_cached(&conn, "Woolworths", "google_places", &place, None).unwrap();

        let cached = get_cached(&conn, "Woolworths", "google_places")
            .unwrap()
            .unwrap();
        assert_eq!(cached.place_name, Some("Woolworths Strathfield".into()));
        assert_eq!(cached.place_types, vec!["supermarket", "grocery_store"]);
        assert_eq!(cached.place_address, Some("123 Main St".into()));
    }

    #[test]
    fn test_overwrite_existing() {
        let conn = db::initialize_in_memory().unwrap();
        let place1 = PlaceResult {
            place_name: Some("Old Name".into()),
            place_types: vec!["store".into()],
            place_address: None,
        };
        set_cached(&conn, "test", "google_places", &place1, None).unwrap();

        let place2 = PlaceResult {
            place_name: Some("New Name".into()),
            place_types: vec!["restaurant".into()],
            place_address: Some("456 New St".into()),
        };
        set_cached(&conn, "test", "google_places", &place2, None).unwrap();

        let cached = get_cached(&conn, "test", "google_places").unwrap().unwrap();
        assert_eq!(cached.place_name, Some("New Name".into()));
        assert_eq!(cached.place_types, vec!["restaurant"]);
    }

    #[test]
    fn test_different_providers_separate() {
        let conn = db::initialize_in_memory().unwrap();
        let place = PlaceResult {
            place_name: Some("Test".into()),
            place_types: vec!["cafe".into()],
            place_address: None,
        };
        set_cached(&conn, "query", "google_places", &place, None).unwrap();

        let cached = get_cached(&conn, "query", "llm").unwrap();
        assert!(cached.is_none());
    }
}
