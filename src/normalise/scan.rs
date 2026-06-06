//! `normalise scan` — populate the `payee_normalisations` staging table from
//! the current contents of `transactions`. Mirrors the transfer scan/apply
//! paradigm.
//!
//! Policy table (see PLAN.md):
//!
//! | existing row | proposed vs existing | proposed vs current payee | action                       |
//! |--------------|----------------------|---------------------------|------------------------------|
//! | None         | n/a                  | equal                     | skip (rule a)                |
//! | None         | n/a                  | different                 | INSERT pending               |
//! | Some(any)    | equal                | n/a                       | UPDATE txn_count only        |
//! | Some(any)    | different            | n/a                       | overwrite to pending (F3)    |
//!
//! "Current payee" is the representative `transactions.payee` value for the
//! group (we take MIN — within a single `original_payee` they should all
//! agree). "Proposed" is the output of [`crate::normalise::format_payee`]
//! applied to the pipeline result.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::Connection;

use crate::db::payee_normalisations::{
    self as pn, PayeeNormalisationRow,
};
use crate::normalise::{class_tag, features_to_json, format_payee, normalise, PipelineCtx, RuleCache};
use crate::transfers::Status;

/// Counts returned from a scan run. Useful for CLI summary and tests.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanStats {
    /// No existing row and proposed already matches current payee (rule a).
    pub skipped_no_change: usize,
    /// No existing row, proposal differs from current payee — inserted as pending.
    pub inserted: usize,
    /// Existing row, proposal unchanged — only `txn_count` updated.
    pub txn_count_updated: usize,
    /// Existing row, proposal differs — overwritten back to pending.
    pub overwritten: usize,
}

/// Run the scan against the given connection. All writes occur inside a
/// single `with_operation("normalise-scan", ...)` so the row-history triggers
/// stamp them consistently.
pub fn scan(conn: &Connection) -> Result<ScanStats> {
    // Group by original_payee with a representative current payee and txn count.
    let mut stmt = conn.prepare(
        "SELECT original_payee, MIN(payee), COUNT(*)
           FROM transactions
          WHERE original_payee IS NOT NULL
          GROUP BY original_payee",
    )?;
    let groups: Vec<(String, Option<String>, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut stats = ScanStats::default();

    // One cache for the whole scan: the rule tables don't change
    // mid-scan, so every payee shares the same compiled rules.
    let cache = RuleCache::new();
    let ctx = PipelineCtx::new(conn, &cache);

    // Pre-load existing rows into a map so we don't issue N+1 queries.
    let existing: HashMap<String, PayeeNormalisationRow> = pn::list_all(conn)?
        .into_iter()
        .map(|r| (r.original_payee.clone(), r))
        .collect();

    crate::db::with_operation(conn, "normalise-scan", |conn| {
        for (original_payee, current_payee, txn_count) in &groups {
            let result = normalise(original_payee, &ctx);
            let proposed = format_payee(&result);
            let class = class_tag(result.class()).map(|s| s.to_string());
            let features_json = features_to_json(&result.features);

            let current = current_payee.as_deref().unwrap_or(original_payee.as_str());

            match existing.get(original_payee) {
                None => {
                    if proposed == current {
                        stats.skipped_no_change += 1;
                        continue;
                    }
                    let row = PayeeNormalisationRow {
                        original_payee: original_payee.clone(),
                        proposed_payee: proposed.clone(),
                        slug: pn::slug_for(original_payee),
                        class,
                        features_json,
                        txn_count: *txn_count,
                        status: Status::Pending,
                    };
                    pn::upsert(conn, &row)?;
                    stats.inserted += 1;
                }
                Some(existing_row) => {
                    if existing_row.proposed_payee == proposed {
                        if existing_row.txn_count != *txn_count {
                            pn::update_txn_count(conn, original_payee, *txn_count)?;
                        }
                        stats.txn_count_updated += 1;
                    } else {
                        let row = PayeeNormalisationRow {
                            original_payee: original_payee.clone(),
                            proposed_payee: proposed.clone(),
                            slug: pn::slug_for(original_payee),
                            class,
                            features_json,
                            txn_count: *txn_count,
                            status: Status::Pending,
                        };
                        pn::upsert(conn, &row)?;
                        stats.overwritten += 1;
                    }
                }
            }
        }
        Ok(())
    })?;

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::initialize_in_memory;
    use crate::test_support::{seed_account, seed_txn};

    fn setup(conn: &Connection) {
        seed_account(conn, 1, "Test").unwrap();
        // The pipeline now reads its rules from the DB; seed them so the
        // scan produces the same proposals it did when rules were const.
        crate::rules::load_into_db(conn).unwrap();
    }

    fn insert_txn_v2(conn: &Connection, id: i64, original_payee: &str, payee: &str) {
        seed_txn(conn, id, 1, original_payee, payee).unwrap();
    }

    // --- Rule (a): no existing row, proposed == current → skip ---
    #[test]
    fn scan_skips_when_proposed_equals_current_payee() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        // A payee that the normaliser leaves unchanged; current payee already matches.
        insert_txn_v2(&conn, 1, "UNCLASSIFIED THING", "UNCLASSIFIED THING");
        let stats = scan(&conn).unwrap();
        assert_eq!(stats.skipped_no_change, 1);
        assert_eq!(stats.inserted, 0);
        assert!(pn::list_all(&conn).unwrap().is_empty());
    }

    // --- Rule (b): no existing row, proposed != current → INSERT pending ---
    #[test]
    fn scan_inserts_pending_when_proposal_differs_from_current() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        insert_txn_v2(&conn, 1, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF");
        let stats = scan(&conn).unwrap();
        assert_eq!(stats.inserted, 1);
        assert_eq!(stats.skipped_no_change, 0);
        let rows = pn::list_all(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].original_payee, "WOOLWORTHS 1624 STRATHF");
        assert_eq!(rows[0].proposed_payee, "Woolworths, Strathfield");
        assert_eq!(rows[0].status, Status::Pending);
        assert_eq!(rows[0].class.as_deref(), Some("merchant"));
        assert_eq!(rows[0].txn_count, 1);
    }

    // --- Rule (c): existing row, proposed == existing → only txn_count touched ---
    #[test]
    fn scan_updates_txn_count_only_when_proposal_unchanged() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        insert_txn_v2(&conn, 1, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF");
        let _ = scan(&conn).unwrap();
        // Confirm the proposal.
        pn::update_status(&conn, "WOOLWORTHS 1624 STRATHF", Status::Confirmed).unwrap();

        // Add a second transaction for the same original payee.
        insert_txn_v2(&conn, 2, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF");

        let stats = scan(&conn).unwrap();
        assert_eq!(stats.txn_count_updated, 1);
        assert_eq!(stats.inserted, 0);
        assert_eq!(stats.overwritten, 0);

        let row = pn::get_by_original(&conn, "WOOLWORTHS 1624 STRATHF").unwrap().unwrap();
        // Status preserved (still confirmed), txn_count bumped to 2.
        assert_eq!(row.status, Status::Confirmed);
        assert_eq!(row.txn_count, 2);
    }

    // --- Rule (d): existing row, proposed differs → overwrite back to pending ---
    #[test]
    fn scan_overwrites_existing_row_when_proposal_changes() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        insert_txn_v2(&conn, 1, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF");
        // Seed an existing (confirmed) row with a stale proposal.
        let stale = PayeeNormalisationRow {
            original_payee: "WOOLWORTHS 1624 STRATHF".into(),
            proposed_payee: "Some Stale Proposal".into(),
            slug: pn::slug_for("WOOLWORTHS 1624 STRATHF"),
            class: Some("merchant".into()),
            features_json: "{}".into(),
            txn_count: 1,
            status: Status::Confirmed,
        };
        pn::upsert(&conn, &stale).unwrap();

        let stats = scan(&conn).unwrap();
        assert_eq!(stats.overwritten, 1);
        let row = pn::get_by_original(&conn, "WOOLWORTHS 1624 STRATHF").unwrap().unwrap();
        // Status reset to pending, proposal replaced.
        assert_eq!(row.status, Status::Pending);
        assert_eq!(row.proposed_payee, "Woolworths, Strathfield");
    }

    // --- Multi-row group: txn_count reflects the full group, single staging row ---
    #[test]
    fn scan_counts_all_transactions_in_group() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        for id in 1..=5 {
            insert_txn_v2(&conn, id, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF");
        }
        let stats = scan(&conn).unwrap();
        assert_eq!(stats.inserted, 1);
        let row = pn::get_by_original(&conn, "WOOLWORTHS 1624 STRATHF").unwrap().unwrap();
        assert_eq!(row.txn_count, 5);
    }

    // --- Skip when current payee already matches the normalised proposal ---
    #[test]
    fn scan_skips_when_current_payee_already_normalised() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        // current payee is already what the normaliser would produce.
        insert_txn_v2(
            &conn,
            1,
            "WOOLWORTHS 1624 STRATHF",
            "Woolworths, Strathfield",
        );
        let stats = scan(&conn).unwrap();
        assert_eq!(stats.skipped_no_change, 1);
        assert_eq!(stats.inserted, 0);
        assert!(pn::list_all(&conn).unwrap().is_empty());
    }

    // --- Transactions with NULL original_payee are ignored ---
    #[test]
    fn scan_ignores_null_original_payee() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        crate::db::with_operation(&conn, "test-seed", |conn| {
            conn.execute(
                "INSERT INTO transactions (id, transaction_account_id, date, amount, original_payee, payee)
                 VALUES (1, 1, '2026-01-01', -10.0, NULL, 'foo')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        let stats = scan(&conn).unwrap();
        assert_eq!(stats, ScanStats::default());
    }

    // --- Multiple distinct original_payees handled independently ---
    #[test]
    fn scan_handles_multiple_distinct_payees() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        insert_txn_v2(&conn, 1, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF");
        insert_txn_v2(&conn, 2, "COLES BURWOO", "COLES BURWOO");
        insert_txn_v2(&conn, 3, "UNCLASSIFIED THING", "UNCLASSIFIED THING");
        let stats = scan(&conn).unwrap();
        // The first two propose non-trivial normalisations; the third is unchanged.
        assert_eq!(stats.inserted, 2);
        assert_eq!(stats.skipped_no_change, 1);
        let rows = pn::list_all(&conn).unwrap();
        assert_eq!(rows.len(), 2);
    }
}
