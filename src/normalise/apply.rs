//! `normalise apply` — drain confirmed rows from `payee_normalisations`,
//! writing each `proposed_payee` to all matching `transactions.payee`, then
//! deleting the confirmed staging row. Rejected rows persist (their presence
//! is what suppresses re-prompting in the next scan).

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::db::payee_normalisations as pn;
use crate::review::{ApplyStats, Status};

/// Write confirmed proposals into `transactions.payee`, then delete the
/// confirmed staging rows. Runs inside a single
/// `with_operation("normalise-apply", ...)` so the row-history triggers
/// stamp every update consistently.
pub fn apply_confirmed(conn: &Connection) -> Result<ApplyStats> {
    let confirmed = pn::list_by_status(conn, Status::Confirmed)?;
    if confirmed.is_empty() {
        return Ok(ApplyStats::default());
    }

    let mut stats = ApplyStats::default();
    crate::db::with_operation(conn, "normalise-apply", |conn| {
        for row in &confirmed {
            // Only touch rows whose payee actually differs — keeps the
            // _transaction_changes table free of no-op writes.
            let n = conn.execute(
                "UPDATE transactions
                    SET payee = ?1
                  WHERE original_payee = ?2
                    AND payee IS NOT ?1",
                params![row.proposed_payee, row.original_payee],
            )?;
            stats.transactions_updated += n;
        }
        stats.rows_drained = pn::delete_confirmed(conn)?;
        Ok(())
    })?;

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;
    use crate::db::payee_normalisations::PayeeNormalisationRow;

    fn setup(conn: &Connection) {
        conn.execute(
            "INSERT INTO transaction_accounts (id, name) VALUES (1, 'Test') ON CONFLICT DO NOTHING",
            [],
        )
        .unwrap();
    }

    fn insert_txn(conn: &Connection, id: i64, original_payee: &str, payee: &str) {
        crate::db::with_operation(conn, "test-seed", |conn| {
            conn.execute(
                "INSERT INTO transactions (id, transaction_account_id, date, amount, original_payee, payee)
                 VALUES (?1, 1, '2026-01-01', -10.0, ?2, ?3)",
                params![id, original_payee, payee],
            )?;
            Ok(())
        })
        .unwrap();
    }

    fn stage(
        conn: &Connection,
        original: &str,
        proposed: &str,
        status: Status,
    ) {
        pn::upsert(
            conn,
            &PayeeNormalisationRow {
                original_payee: original.into(),
                proposed_payee: proposed.into(),
                slug: pn::slug_for(original),
                class: Some("merchant".into()),
                features_json: "{}".into(),
                txn_count: 1,
                status,
            },
        )
        .unwrap();
    }

    fn current_payee(conn: &Connection, txn_id: i64) -> Option<String> {
        conn.query_row(
            "SELECT payee FROM transactions WHERE id = ?1",
            params![txn_id],
            |r| r.get(0),
        )
        .ok()
    }

    #[test]
    fn apply_writes_confirmed_payees_and_drains_staging() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        // Three transactions sharing the same original_payee.
        for id in 1..=3 {
            insert_txn(&conn, id, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF");
        }
        stage(&conn, "WOOLWORTHS 1624 STRATHF", "Woolworths", Status::Confirmed);

        let stats = apply_confirmed(&conn).unwrap();
        assert_eq!(stats.transactions_updated, 3);
        assert_eq!(stats.rows_drained, 1);

        // All three transactions now carry the proposed payee.
        for id in 1..=3 {
            assert_eq!(current_payee(&conn, id).as_deref(), Some("Woolworths"));
        }
        // Confirmed staging row gone.
        assert!(pn::get_by_original(&conn, "WOOLWORTHS 1624 STRATHF")
            .unwrap()
            .is_none());
    }

    #[test]
    fn apply_leaves_rejected_rows_in_place_and_skips_their_transactions() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        insert_txn(&conn, 1, "COLES 0042", "COLES 0042");
        stage(&conn, "COLES 0042", "Coles", Status::Rejected);

        let stats = apply_confirmed(&conn).unwrap();
        assert_eq!(stats.transactions_updated, 0);
        assert_eq!(stats.rows_drained, 0);

        // Transaction unchanged.
        assert_eq!(current_payee(&conn, 1).as_deref(), Some("COLES 0042"));
        // Rejected row persists.
        let row = pn::get_by_original(&conn, "COLES 0042").unwrap().unwrap();
        assert_eq!(row.status, Status::Rejected);
    }

    #[test]
    fn apply_is_noop_when_no_confirmed_rows_present() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        insert_txn(&conn, 1, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF");
        stage(&conn, "WOOLWORTHS 1624 STRATHF", "Woolworths", Status::Pending);

        let stats = apply_confirmed(&conn).unwrap();
        assert_eq!(stats, ApplyStats::default());

        // Pending row still pending, transaction unchanged.
        let row = pn::get_by_original(&conn, "WOOLWORTHS 1624 STRATHF")
            .unwrap()
            .unwrap();
        assert_eq!(row.status, Status::Pending);
        assert_eq!(
            current_payee(&conn, 1).as_deref(),
            Some("WOOLWORTHS 1624 STRATHF")
        );
    }
}
