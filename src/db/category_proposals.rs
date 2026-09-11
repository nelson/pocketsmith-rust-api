//! Storage for category proposals — the staging layer for the
//! `categorise` scan/confirm/apply paradigm, mirroring
//! `payee_normalisations` for the normalise flow.
//!
//! One row per distinct confirmed-merchant key (the normalised Places
//! query). Each row carries the proposed `category_id`, the proposed leaf
//! labels (controlled vocabulary, JSON array), the Google place type that
//! drove the mapping, and a shared `status` (pending/confirmed/rejected).
//! Apply drains confirmed rows into `transactions`; rejected rows persist
//! to suppress re-prompting.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::review::Status;

/// One staged category proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryProposalRow {
    /// Normalised merchant identity (the Places query). Primary key.
    pub merchant_key: String,
    /// Resolved category id, or `None` when the place type was unmapped.
    pub proposed_category: Option<i64>,
    /// Leaf labels from the hardcoded taxonomy (controlled vocab).
    pub proposed_labels: Vec<String>,
    /// The Google place type that drove the mapping (for display / audit).
    pub place_type: Option<String>,
    pub txn_count: i64,
    pub status: Status,
}

fn row_to_proposal(row: &rusqlite::Row) -> rusqlite::Result<CategoryProposalRow> {
    let labels_json: Option<String> = row.get(2)?;
    let status_int: i32 = row.get(5)?;
    Ok(CategoryProposalRow {
        merchant_key: row.get(0)?,
        proposed_category: row.get(1)?,
        proposed_labels: labels_json
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        place_type: row.get(3)?,
        txn_count: row.get(4)?,
        status: Status::from_i32(status_int).unwrap_or(Status::Pending),
    })
}

const SELECT_COLS: &str =
    "merchant_key, proposed_category, proposed_labels, place_type, txn_count, status";

/// Insert or fully overwrite the row for `merchant_key`. Resets status to
/// whatever the caller supplies (scan logic guards against overwriting an
/// unchanged proposal).
pub fn upsert(conn: &Connection, row: &CategoryProposalRow) -> Result<()> {
    let labels_json = serde_json::to_string(&row.proposed_labels).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT INTO category_proposals
            (merchant_key, proposed_category, proposed_labels, place_type, txn_count, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(merchant_key) DO UPDATE SET
            proposed_category = excluded.proposed_category,
            proposed_labels   = excluded.proposed_labels,
            place_type        = excluded.place_type,
            txn_count         = excluded.txn_count,
            status            = excluded.status",
        params![
            row.merchant_key,
            row.proposed_category,
            labels_json,
            row.place_type,
            row.txn_count,
            row.status.to_i32(),
        ],
    )
    .context("upsert category_proposals")?;
    Ok(())
}

/// Update `txn_count` only (no status change).
pub fn update_txn_count(conn: &Connection, merchant_key: &str, txn_count: i64) -> Result<()> {
    conn.execute(
        "UPDATE category_proposals SET txn_count = ?1 WHERE merchant_key = ?2",
        params![txn_count, merchant_key],
    )?;
    Ok(())
}

pub fn update_status(conn: &Connection, merchant_key: &str, status: Status) -> Result<()> {
    conn.execute(
        "UPDATE category_proposals SET status = ?1 WHERE merchant_key = ?2",
        params![status.to_i32(), merchant_key],
    )?;
    Ok(())
}

pub fn get(conn: &Connection, merchant_key: &str) -> Result<Option<CategoryProposalRow>> {
    let sql = format!("SELECT {SELECT_COLS} FROM category_proposals WHERE merchant_key = ?1");
    let row = conn
        .query_row(&sql, params![merchant_key], row_to_proposal)
        .optional()?;
    Ok(row)
}

pub fn list_all(conn: &Connection) -> Result<Vec<CategoryProposalRow>> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM category_proposals ORDER BY txn_count DESC, merchant_key ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], row_to_proposal)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn list_by_status(conn: &Connection, status: Status) -> Result<Vec<CategoryProposalRow>> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM category_proposals
         WHERE status = ?1
         ORDER BY txn_count DESC, merchant_key ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![status.to_i32()], row_to_proposal)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_confirmed(conn: &Connection) -> Result<usize> {
    let n = conn.execute(
        "DELETE FROM category_proposals WHERE status = ?1",
        params![Status::Confirmed.to_i32()],
    )?;
    Ok(n)
}

pub fn count_by_status(conn: &Connection) -> Result<std::collections::HashMap<Status, usize>> {
    let mut stmt =
        conn.prepare("SELECT status, COUNT(*) FROM category_proposals GROUP BY status")?;
    let mut map = std::collections::HashMap::new();
    let rows = stmt.query_map([], |r| {
        let s: i32 = r.get(0)?;
        let c: usize = r.get(1)?;
        Ok((s, c))
    })?;
    for r in rows {
        let (s, c) = r?;
        if let Some(status) = Status::from_i32(s) {
            map.insert(status, c);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;

    fn sample(key: &str, status: Status) -> CategoryProposalRow {
        CategoryProposalRow {
            merchant_key: key.into(),
            proposed_category: Some(42),
            proposed_labels: vec!["supermarket".into()],
            place_type: Some("supermarket".into()),
            txn_count: 3,
            status,
        }
    }

    #[test]
    fn roundtrip_insert_get_update_list_delete() {
        let conn = initialize_in_memory().unwrap();
        // Seed a category so the FK is satisfiable.
        conn.execute(
            "INSERT INTO categories (id, title) VALUES (42, '_Groceries')",
            [],
        )
        .unwrap();

        assert!(get(&conn, "woolworths").unwrap().is_none());

        upsert(&conn, &sample("woolworths", Status::Pending)).unwrap();
        upsert(&conn, &sample("coles", Status::Confirmed)).unwrap();

        let got = get(&conn, "woolworths").unwrap().unwrap();
        assert_eq!(got.proposed_category, Some(42));
        assert_eq!(got.proposed_labels, vec!["supermarket"]);
        assert_eq!(got.status, Status::Pending);

        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 2);

        let pending = list_by_status(&conn, Status::Pending).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].merchant_key, "woolworths");

        update_status(&conn, "woolworths", Status::Confirmed).unwrap();
        update_txn_count(&conn, "woolworths", 9).unwrap();
        let got = get(&conn, "woolworths").unwrap().unwrap();
        assert_eq!(got.status, Status::Confirmed);
        assert_eq!(got.txn_count, 9);

        let removed = delete_confirmed(&conn).unwrap();
        assert_eq!(removed, 2);
        assert!(list_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn unmapped_proposal_has_null_category_and_empty_labels() {
        let conn = initialize_in_memory().unwrap();
        let row = CategoryProposalRow {
            merchant_key: "mystery".into(),
            proposed_category: None,
            proposed_labels: vec![],
            place_type: None,
            txn_count: 1,
            status: Status::Pending,
        };
        upsert(&conn, &row).unwrap();
        let got = get(&conn, "mystery").unwrap().unwrap();
        assert_eq!(got, row);
        assert!(got.proposed_category.is_none());
        assert!(got.proposed_labels.is_empty());
    }

    #[test]
    fn updated_at_trigger_bumps_on_edit() {
        let conn = initialize_in_memory().unwrap();
        let row = CategoryProposalRow {
            merchant_key: "x".into(),
            proposed_category: None,
            proposed_labels: vec![],
            place_type: None,
            txn_count: 1,
            status: Status::Pending,
        };
        upsert(&conn, &row).unwrap();
        const OLD: &str = "2000-01-01T00:00:00.000Z";
        conn.execute(
            "UPDATE category_proposals SET updated_at = ?1 WHERE merchant_key = 'x'",
            [OLD],
        )
        .unwrap();
        update_status(&conn, "x", Status::Rejected).unwrap();
        let bumped: String = conn
            .query_row(
                "SELECT updated_at FROM category_proposals WHERE merchant_key = 'x'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_ne!(bumped, OLD, "status edit bumps updated_at");
    }
}
