// Storage for normalisation proposals — the staging layer for the
// `normalise` scan/apply paradigm, mirroring `transfer_pairs` for transfers.
//
// One row per unique `original_payee` seen in `transactions`. The row carries
// the proposed normalised payee, classification metadata, and a `status`
// (pending / confirmed / rejected) shared with the transfer pairs lookup
// table. Apply drains confirmed rows; rejected rows persist to suppress
// re-prompting until a rule change produces a different proposal.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::transfers::Status;

/// 16-char lowercase hex of XXH3-64 hash of `original_payee`. Stable across
/// Rust versions (xxhash spec). Used as the URL slug for the review UI.
pub fn slug_for(original_payee: &str) -> String {
    let h = xxhash_rust::xxh3::xxh3_64(original_payee.as_bytes());
    format!("{:016x}", h)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayeeNormalisationRow {
    pub original_payee: String,
    pub proposed_payee: String,
    pub slug: String,
    pub class: Option<String>,
    pub features_json: String,
    pub txn_count: i64,
    pub status: Status,
}

fn row_to_pn(row: &rusqlite::Row) -> rusqlite::Result<PayeeNormalisationRow> {
    let status_int: i32 = row.get(6)?;
    Ok(PayeeNormalisationRow {
        original_payee: row.get(0)?,
        proposed_payee: row.get(1)?,
        slug: row.get(2)?,
        class: row.get(3)?,
        features_json: row.get(4)?,
        txn_count: row.get(5)?,
        status: Status::from_i32(status_int).unwrap_or(Status::Pending),
    })
}

const SELECT_COLS: &str =
    "original_payee, proposed_payee, slug, class, features_json, txn_count, status";

/// Insert or fully overwrite the row for `original_payee`. Resets status to
/// pending on overwrite (caller decides when to call this — scan logic
/// guards against overwriting an unchanged proposal). See scan rule (d).
pub fn upsert(conn: &Connection, row: &PayeeNormalisationRow) -> Result<()> {
    conn.execute(
        "INSERT INTO payee_normalisations
            (original_payee, proposed_payee, slug, class, features_json, txn_count, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(original_payee) DO UPDATE SET
            proposed_payee = excluded.proposed_payee,
            slug           = excluded.slug,
            class          = excluded.class,
            features_json  = excluded.features_json,
            txn_count      = excluded.txn_count,
            status         = excluded.status",
        params![
            row.original_payee,
            row.proposed_payee,
            row.slug,
            row.class,
            row.features_json,
            row.txn_count,
            row.status.to_i32(),
        ],
    )
    .context("upsert payee_normalisations")?;
    Ok(())
}

/// Update `txn_count` only (no status change). Used by scan when the proposed
/// payee matches the existing row but the underlying transaction set has
/// grown or shrunk.
pub fn update_txn_count(conn: &Connection, original_payee: &str, txn_count: i64) -> Result<()> {
    conn.execute(
        "UPDATE payee_normalisations SET txn_count = ?1 WHERE original_payee = ?2",
        params![txn_count, original_payee],
    )?;
    Ok(())
}

pub fn update_status(conn: &Connection, original_payee: &str, status: Status) -> Result<()> {
    conn.execute(
        "UPDATE payee_normalisations SET status = ?1 WHERE original_payee = ?2",
        params![status.to_i32(), original_payee],
    )?;
    Ok(())
}

pub fn get_by_original(
    conn: &Connection,
    original_payee: &str,
) -> Result<Option<PayeeNormalisationRow>> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM payee_normalisations WHERE original_payee = ?1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![original_payee], row_to_pn)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

pub fn get_by_slug(conn: &Connection, slug: &str) -> Result<Option<PayeeNormalisationRow>> {
    let sql = format!("SELECT {SELECT_COLS} FROM payee_normalisations WHERE slug = ?1");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params![slug], row_to_pn)?;
    match rows.next() {
        Some(r) => Ok(Some(r?)),
        None => Ok(None),
    }
}

pub fn list_all(conn: &Connection) -> Result<Vec<PayeeNormalisationRow>> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM payee_normalisations ORDER BY txn_count DESC, original_payee ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map([], row_to_pn)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn list_by_status(conn: &Connection, status: Status) -> Result<Vec<PayeeNormalisationRow>> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM payee_normalisations
         WHERE status = ?1
         ORDER BY txn_count DESC, original_payee ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params![status.to_i32()], row_to_pn)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

pub fn delete_confirmed(conn: &Connection) -> Result<usize> {
    let n = conn.execute(
        "DELETE FROM payee_normalisations WHERE status = ?1",
        params![Status::Confirmed.to_i32()],
    )?;
    Ok(n)
}

pub fn count_by_status(
    conn: &Connection,
) -> Result<std::collections::HashMap<Status, usize>> {
    let mut stmt =
        conn.prepare("SELECT status, COUNT(*) FROM payee_normalisations GROUP BY status")?;
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

    fn sample(original: &str, proposed: &str, status: Status) -> PayeeNormalisationRow {
        PayeeNormalisationRow {
            original_payee: original.into(),
            proposed_payee: proposed.into(),
            slug: slug_for(original),
            class: Some("merchant".into()),
            features_json: "{}".into(),
            txn_count: 1,
            status,
        }
    }

    #[test]
    fn roundtrip_insert_get_update_status_list_delete() {
        let conn = initialize_in_memory().unwrap();

        // Insert two rows, one pending and one confirmed.
        upsert(&conn, &sample("WOOLWORTHS 1624 STRATHF", "Woolworths Strathfield", Status::Pending))
            .unwrap();
        upsert(&conn, &sample("COLES 0042", "Coles", Status::Confirmed)).unwrap();

        // get_by_original round-trip.
        let row = get_by_original(&conn, "WOOLWORTHS 1624 STRATHF").unwrap().unwrap();
        assert_eq!(row.proposed_payee, "Woolworths Strathfield");
        assert_eq!(row.status, Status::Pending);
        assert_eq!(row.slug, slug_for("WOOLWORTHS 1624 STRATHF"));

        // get_by_slug round-trip.
        let row = get_by_slug(&conn, &slug_for("COLES 0042")).unwrap().unwrap();
        assert_eq!(row.original_payee, "COLES 0042");

        // list_all returns both, ordered by txn_count desc then alpha.
        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 2);

        // list_by_status filters.
        let pending = list_by_status(&conn, Status::Pending).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].original_payee, "WOOLWORTHS 1624 STRATHF");

        // update_status flips pending -> rejected.
        update_status(&conn, "WOOLWORTHS 1624 STRATHF", Status::Rejected).unwrap();
        let row = get_by_original(&conn, "WOOLWORTHS 1624 STRATHF").unwrap().unwrap();
        assert_eq!(row.status, Status::Rejected);

        // update_txn_count writes count without touching status.
        update_txn_count(&conn, "WOOLWORTHS 1624 STRATHF", 42).unwrap();
        let row = get_by_original(&conn, "WOOLWORTHS 1624 STRATHF").unwrap().unwrap();
        assert_eq!(row.txn_count, 42);
        assert_eq!(row.status, Status::Rejected);

        // upsert overwrites in place, resetting status if the caller supplies pending.
        upsert(
            &conn,
            &sample("WOOLWORTHS 1624 STRATHF", "Woolworths", Status::Pending),
        )
        .unwrap();
        let row = get_by_original(&conn, "WOOLWORTHS 1624 STRATHF").unwrap().unwrap();
        assert_eq!(row.proposed_payee, "Woolworths");
        assert_eq!(row.status, Status::Pending);
        // upsert also resets txn_count to whatever the caller passes (1 here).
        assert_eq!(row.txn_count, 1);

        // delete_confirmed removes only confirmed rows; rejected stays.
        let removed = delete_confirmed(&conn).unwrap();
        assert_eq!(removed, 1);
        assert!(get_by_original(&conn, "COLES 0042").unwrap().is_none());
        assert!(get_by_original(&conn, "WOOLWORTHS 1624 STRATHF").unwrap().is_some());

        // count_by_status.
        let counts = count_by_status(&conn).unwrap();
        assert_eq!(counts.get(&Status::Pending), Some(&1));
        assert_eq!(counts.get(&Status::Confirmed), None);

        // slug stability sanity check.
        assert_eq!(slug_for("hello").len(), 16);
        assert_ne!(slug_for("hello"), slug_for("world"));
    }
}
