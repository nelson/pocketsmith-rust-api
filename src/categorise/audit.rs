use anyhow::Result;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaceResult {
    pub place_name: Option<String>,
    pub place_types: Vec<String>,
    pub place_address: Option<String>,
}

/// A cached categorisation decision.
#[derive(Debug, Clone)]
pub struct AuditRow {
    pub normalised_payee: String,
    pub method: String,
    pub category: Option<String>,
    pub reason: Option<String>,
}

/// Upsert a categorisation decision (rule or LLM).
pub fn upsert(
    conn: &Connection,
    normalised_payee: &str,
    payee_type: Option<&str>,
    method: &str,
    category: Option<&str>,
    reason: &str,
    transaction_count: i64,
    metadata: Option<&str>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO categorisation_audit
            (normalised_payee, payee_type, method, category, reason, transaction_count, metadata)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(normalised_payee) DO UPDATE SET
            payee_type = excluded.payee_type,
            method = excluded.method,
            category = excluded.category,
            reason = excluded.reason,
            transaction_count = excluded.transaction_count,
            metadata = excluded.metadata,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        rusqlite::params![
            normalised_payee,
            payee_type,
            method,
            category,
            reason,
            transaction_count,
            metadata,
        ],
    )?;
    Ok(())
}

/// Upsert a categorisation decision with Google Places data.
pub fn upsert_with_places(
    conn: &Connection,
    normalised_payee: &str,
    payee_type: Option<&str>,
    category: Option<&str>,
    reason: &str,
    transaction_count: i64,
    place: &PlaceResult,
    raw_response: Option<&str>,
) -> Result<()> {
    let types_json = serde_json::to_string(&place.place_types)?;
    conn.execute(
        "INSERT INTO categorisation_audit
            (normalised_payee, payee_type, method, category, reason, transaction_count,
             place_name, place_types, place_address, raw_response)
         VALUES (?1, ?2, 'google_places', ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(normalised_payee) DO UPDATE SET
            payee_type = excluded.payee_type,
            method = excluded.method,
            category = excluded.category,
            reason = excluded.reason,
            transaction_count = excluded.transaction_count,
            place_name = excluded.place_name,
            place_types = excluded.place_types,
            place_address = excluded.place_address,
            raw_response = excluded.raw_response,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        rusqlite::params![
            normalised_payee,
            payee_type,
            category,
            reason,
            transaction_count,
            place.place_name,
            types_json,
            place.place_address,
            raw_response,
        ],
    )?;
    Ok(())
}

/// Look up a cached categorisation decision.
pub fn get_cached(conn: &Connection, normalised_payee: &str) -> Result<Option<CachedAudit>> {
    let result = conn
        .query_row(
            "SELECT method, category, reason, place_types FROM categorisation_audit
             WHERE normalised_payee = ?1",
            [normalised_payee],
            |row| {
                let types_json: Option<String> = row.get(3)?;
                Ok(CachedAudit {
                    method: row.get(0)?,
                    category: row.get(1)?,
                    reason: row.get(2)?,
                    place_types: types_json
                        .and_then(|j| serde_json::from_str(&j).ok())
                        .unwrap_or_default(),
                })
            },
        )
        .ok();
    Ok(result)
}

/// Cached audit result for pipeline lookups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedAudit {
    pub method: String,
    pub category: Option<String>,
    pub reason: Option<String>,
    pub place_types: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn test_upsert_and_get_cached() {
        let conn = db::initialize_in_memory().unwrap();

        upsert(
            &conn,
            "Apple (Salary)",
            Some("salary"),
            "rule",
            Some("_Income"),
            "type:salary→_Income",
            10,
            None,
        )
        .unwrap();

        let cached = get_cached(&conn, "Apple (Salary)").unwrap().unwrap();
        assert_eq!(cached.method, "rule");
        assert_eq!(cached.category, Some("_Income".into()));
    }

    #[test]
    fn test_upsert_with_places() {
        let conn = db::initialize_in_memory().unwrap();
        let place = PlaceResult {
            place_name: Some("Woolworths Strathfield".into()),
            place_types: vec!["supermarket".into(), "grocery_store".into()],
            place_address: Some("123 Main St".into()),
        };

        upsert_with_places(
            &conn,
            "Woolworths Strathfield",
            Some("merchant"),
            Some("_Groceries"),
            "google:supermarket→category",
            20,
            &place,
            Some("{}"),
        )
        .unwrap();

        let cached = get_cached(&conn, "Woolworths Strathfield")
            .unwrap()
            .unwrap();
        assert_eq!(cached.method, "google_places");
        assert_eq!(cached.category, Some("_Groceries".into()));
        assert_eq!(cached.place_types, vec!["supermarket", "grocery_store"]);
    }

    #[test]
    fn test_cache_miss_returns_none() {
        let conn = db::initialize_in_memory().unwrap();
        let cached = get_cached(&conn, "Nonexistent").unwrap();
        assert!(cached.is_none());
    }

    #[test]
    fn test_upsert_overwrites() {
        let conn = db::initialize_in_memory().unwrap();

        upsert(&conn, "Test", None, "rule", Some("_Bills"), "first", 1, None).unwrap();
        upsert(
            &conn,
            "Test",
            None,
            "llm",
            Some("_Dining"),
            "second",
            1,
            None,
        )
        .unwrap();

        let cached = get_cached(&conn, "Test").unwrap().unwrap();
        assert_eq!(cached.method, "llm");
        assert_eq!(cached.category, Some("_Dining".into()));
    }

}
