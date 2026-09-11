//! Cache of Google Places (New) `searchText` lookups (categorisation
//! final stage). Keyed by the normalised query so the distinct merchants
//! drive at most one API call each, ever. A `categorise scan` reads this
//! first and only hits the network on a cache miss.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

/// Outcome of a lookup, stored in `place_lookups.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupStatus {
    /// A place was found and `primary_type` / `types` populated.
    Ok,
    /// The query returned no candidate place.
    NoResult,
    /// The API call failed (kept so we don't hammer a broken query).
    Error,
}

impl LookupStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LookupStatus::Ok => "ok",
            LookupStatus::NoResult => "no_result",
            LookupStatus::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> LookupStatus {
        match s {
            "no_result" => LookupStatus::NoResult,
            "error" => LookupStatus::Error,
            _ => LookupStatus::Ok,
        }
    }
}

/// One cached Places lookup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceLookupRow {
    pub query: String,
    pub place_id: Option<String>,
    pub display_name: Option<String>,
    pub primary_type: Option<String>,
    /// Full `types` array from the API response.
    pub types: Vec<String>,
    /// Raw response body, retained for audit / re-derivation.
    pub response_json: String,
    pub status: LookupStatus,
}

fn row_to_lookup(row: &rusqlite::Row) -> rusqlite::Result<PlaceLookupRow> {
    let types_json: String = row.get(4)?;
    let status_str: String = row.get(6)?;
    Ok(PlaceLookupRow {
        query: row.get(0)?,
        place_id: row.get(1)?,
        display_name: row.get(2)?,
        primary_type: row.get(3)?,
        types: serde_json::from_str(&types_json).unwrap_or_default(),
        response_json: row.get(5)?,
        status: LookupStatus::from_str(&status_str),
    })
}

const SELECT_COLS: &str =
    "query, place_id, display_name, primary_type, types_json, response_json, status";

/// Insert or overwrite the cache row for `query`.
pub fn upsert(conn: &Connection, row: &PlaceLookupRow) -> Result<()> {
    let types_json = serde_json::to_string(&row.types).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO place_lookups
            (query, place_id, display_name, primary_type, types_json, response_json, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(query) DO UPDATE SET
            place_id      = excluded.place_id,
            display_name  = excluded.display_name,
            primary_type  = excluded.primary_type,
            types_json    = excluded.types_json,
            response_json = excluded.response_json,
            status        = excluded.status,
            fetched_at    = strftime('%Y-%m-%dT%H:%M:%fZ','now')",
        params![
            row.query,
            row.place_id,
            row.display_name,
            row.primary_type,
            types_json,
            row.response_json,
            row.status.as_str(),
        ],
    )
    .context("upsert place_lookups")?;
    Ok(())
}

/// Fetch the cached lookup for `query`, if present.
pub fn get(conn: &Connection, query: &str) -> Result<Option<PlaceLookupRow>> {
    let sql = format!("SELECT {SELECT_COLS} FROM place_lookups WHERE query = ?1");
    let row = conn
        .query_row(&sql, params![query], row_to_lookup)
        .optional()?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;

    fn sample(query: &str) -> PlaceLookupRow {
        PlaceLookupRow {
            query: query.into(),
            place_id: Some("ChIJabc".into()),
            display_name: Some("Woolworths".into()),
            primary_type: Some("supermarket".into()),
            types: vec!["supermarket".into(), "grocery_store".into(), "store".into()],
            response_json: r#"{"places":[]}"#.into(),
            status: LookupStatus::Ok,
        }
    }

    #[test]
    fn upsert_get_roundtrip() {
        let conn = initialize_in_memory().unwrap();
        assert!(get(&conn, "woolworths strathfield").unwrap().is_none());

        let row = sample("woolworths strathfield");
        upsert(&conn, &row).unwrap();

        let got = get(&conn, "woolworths strathfield").unwrap().unwrap();
        assert_eq!(got, row);
        assert_eq!(got.types, vec!["supermarket", "grocery_store", "store"]);
        assert_eq!(got.status, LookupStatus::Ok);
    }

    #[test]
    fn upsert_overwrites_in_place() {
        let conn = initialize_in_memory().unwrap();
        upsert(&conn, &sample("q")).unwrap();

        let mut updated = sample("q");
        updated.primary_type = Some("cafe".into());
        updated.types = vec!["cafe".into()];
        updated.status = LookupStatus::Ok;
        upsert(&conn, &updated).unwrap();

        let got = get(&conn, "q").unwrap().unwrap();
        assert_eq!(got.primary_type.as_deref(), Some("cafe"));
        assert_eq!(got.types, vec!["cafe"]);

        // Still a single row.
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM place_lookups", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn no_result_and_error_states_roundtrip() {
        let conn = initialize_in_memory().unwrap();
        let miss = PlaceLookupRow {
            query: "unknown junk".into(),
            place_id: None,
            display_name: None,
            primary_type: None,
            types: vec![],
            response_json: r#"{"places":[]}"#.into(),
            status: LookupStatus::NoResult,
        };
        upsert(&conn, &miss).unwrap();
        let got = get(&conn, "unknown junk").unwrap().unwrap();
        assert_eq!(got.status, LookupStatus::NoResult);
        assert!(got.primary_type.is_none());
        assert!(got.types.is_empty());
    }
}
