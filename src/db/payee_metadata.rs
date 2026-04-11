use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::HashMap;

use crate::normalise::meta;

pub struct PayeeMetadataRow {
    pub normalised_payee: String,
    pub payee_type: String,
    pub extracted_entity: Option<String>,
    pub extract_kind: Option<String>,
    pub identity: Option<String>,
    pub merchant_group: Option<String>,
    pub detected_location: Option<String>,
    pub extra: Option<String>,
    pub sample_original: Option<String>,
    pub transaction_count: i64,
}

/// Upsert a row from normalisation metadata. On conflict, updates metadata fields
/// and adds to transaction_count.
pub fn upsert(
    conn: &Connection,
    normalised_payee: &str,
    metadata: &HashMap<String, Value>,
    sample_original: &str,
    transaction_count: i64,
) -> Result<()> {
    let payee_type = metadata
        .get(meta::TYPE)
        .and_then(|v| v.as_str())
        .unwrap_or("merchant");
    let extracted_entity = metadata
        .get(meta::EXTRACTED_ENTITY)
        .and_then(|v| v.as_str());
    let extract_kind = metadata.get(meta::EXTRACT_KIND).and_then(|v| v.as_str());
    let identity = metadata.get("identity").and_then(|v| v.as_str());
    let merchant_group = metadata.get("merchant_group").and_then(|v| v.as_str());
    let detected_location = metadata
        .get(meta::DETECTED_LOCATION)
        .and_then(|v| v.as_str());

    // Collect remaining metadata into extra JSON
    let known_keys = [
        meta::TYPE,
        meta::EXTRACTED_ENTITY,
        meta::EXTRACT_KIND,
        "identity",
        "merchant_group",
        meta::DETECTED_LOCATION,
        meta::PREFIX_STRIPPED,
        meta::SUFFIXES_STRIPPED,
        meta::TRUNCATIONS_EXPANDED,
    ];
    let extra: HashMap<&str, &Value> = metadata
        .iter()
        .filter(|(k, _)| !known_keys.contains(&k.as_str()))
        .map(|(k, v)| (k.as_str(), v))
        .collect();
    let extra_json = if extra.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&extra)?)
    };

    conn.execute(
        "INSERT INTO payee_metadata (normalised_payee, payee_type, extracted_entity, extract_kind,
            identity, merchant_group, detected_location, extra, sample_original, transaction_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(normalised_payee) DO UPDATE SET
            payee_type = excluded.payee_type,
            extracted_entity = excluded.extracted_entity,
            extract_kind = excluded.extract_kind,
            identity = excluded.identity,
            merchant_group = excluded.merchant_group,
            detected_location = excluded.detected_location,
            extra = excluded.extra,
            transaction_count = excluded.transaction_count,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        rusqlite::params![
            normalised_payee,
            payee_type,
            extracted_entity,
            extract_kind,
            identity,
            merchant_group,
            detected_location,
            extra_json,
            sample_original,
            transaction_count,
        ],
    )?;
    Ok(())
}

/// Get the payee type for a normalised payee.
pub fn get_type(conn: &Connection, normalised_payee: &str) -> Result<Option<String>> {
    let result = conn
        .query_row(
            "SELECT payee_type FROM payee_metadata WHERE normalised_payee = ?1",
            [normalised_payee],
            |row| row.get(0),
        )
        .ok();
    Ok(result)
}

/// Get a full row for a normalised payee.
pub fn get_row(conn: &Connection, normalised_payee: &str) -> Result<Option<PayeeMetadataRow>> {
    let result = conn
        .query_row(
            "SELECT normalised_payee, payee_type, extracted_entity, extract_kind,
                    identity, merchant_group, detected_location, extra, sample_original,
                    transaction_count
             FROM payee_metadata WHERE normalised_payee = ?1",
            [normalised_payee],
            |row| {
                Ok(PayeeMetadataRow {
                    normalised_payee: row.get(0)?,
                    payee_type: row.get(1)?,
                    extracted_entity: row.get(2)?,
                    extract_kind: row.get(3)?,
                    identity: row.get(4)?,
                    merchant_group: row.get(5)?,
                    detected_location: row.get(6)?,
                    extra: row.get(7)?,
                    sample_original: row.get(8)?,
                    transaction_count: row.get(9)?,
                })
            },
        )
        .ok();
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn test_upsert_and_get_type() {
        let conn = db::initialize_in_memory().unwrap();
        let mut meta = HashMap::new();
        meta.insert("type".into(), Value::String("merchant".into()));

        upsert(&conn, "Woolworths", &meta, "WOOLWORTHS 1234", 5).unwrap();

        let t = get_type(&conn, "Woolworths").unwrap();
        assert_eq!(t, Some("merchant".into()));
    }

    #[test]
    fn test_upsert_and_get_row() {
        let conn = db::initialize_in_memory().unwrap();
        let mut meta = HashMap::new();
        meta.insert("type".into(), Value::String("salary".into()));
        meta.insert(
            "extracted_entity".into(),
            Value::String("Apple".into()),
        );
        meta.insert("extract_kind".into(), Value::String("employer".into()));
        meta.insert("identity".into(), Value::String("Apple (Salary)".into()));

        upsert(&conn, "Apple (Salary)", &meta, "SALARY APPLE PTY LTD", 10).unwrap();

        let row = get_row(&conn, "Apple (Salary)").unwrap().unwrap();
        assert_eq!(row.payee_type, "salary");
        assert_eq!(row.extracted_entity, Some("Apple".into()));
        assert_eq!(row.identity, Some("Apple (Salary)".into()));
        assert_eq!(row.transaction_count, 10);
    }

    #[test]
    fn test_upsert_updates_on_conflict() {
        let conn = db::initialize_in_memory().unwrap();
        let mut meta = HashMap::new();
        meta.insert("type".into(), Value::String("merchant".into()));

        upsert(&conn, "Woolworths", &meta, "WOOLWORTHS 1234", 5).unwrap();
        upsert(&conn, "Woolworths", &meta, "WOOLWORTHS 1234", 8).unwrap();

        let row = get_row(&conn, "Woolworths").unwrap().unwrap();
        assert_eq!(row.transaction_count, 8);
    }

    #[test]
    fn test_get_type_missing_returns_none() {
        let conn = db::initialize_in_memory().unwrap();
        let t = get_type(&conn, "Nonexistent").unwrap();
        assert!(t.is_none());
    }

    #[test]
    fn test_extra_metadata_stored() {
        let conn = db::initialize_in_memory().unwrap();
        let mut meta = HashMap::new();
        meta.insert("type".into(), Value::String("merchant".into()));
        meta.insert("default_location".into(), Value::String("Sydney".into()));

        upsert(&conn, "Test", &meta, "TEST", 1).unwrap();

        let row = get_row(&conn, "Test").unwrap().unwrap();
        assert!(row.extra.is_some());
        let extra: serde_json::Value = serde_json::from_str(&row.extra.unwrap()).unwrap();
        assert_eq!(extra["default_location"], "Sydney");
    }
}
