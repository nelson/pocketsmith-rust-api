//! `categorise apply` — drain confirmed `category_proposals` into
//! `transactions`, writing each proposal's `category_id` + leaf `labels`
//! to every transaction whose merchant identity matches, then deleting
//! the confirmed staging row. Rejected rows persist (suppressing
//! re-prompting). Mirrors `normalise::apply`.
//!
//! Matching is by **merchant key**: the same pipeline-derived identity
//! the scan staged on. We recompute, per distinct `original_payee`, the
//! merchant key and update those transactions when a confirmed proposal
//! exists for that key. The confirmed `category_id` + `labels` then flow
//! upstream on the next `push` (push already tracks both fields).

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::categorise::merchant_key;
use crate::db::category_proposals as cp;
use crate::normalise::{normalise, PayeeClass, PipelineCtx, RuleCache};
use crate::review::{ApplyStats, Status};

/// Write confirmed category proposals into `transactions`, then delete the
/// confirmed staging rows. Runs inside a single
/// `with_operation("categorise-apply", ...)`.
pub fn apply_confirmed(conn: &Connection) -> Result<ApplyStats> {
    let confirmed = cp::list_by_status(conn, Status::Confirmed)?;
    if confirmed.is_empty() {
        return Ok(ApplyStats::default());
    }

    // Map each confirmed merchant key -> the original_payees that resolve
    // to it, so we can target the right transactions.
    let key_to_payees = merchant_key_to_payees(conn)?;

    let mut stats = ApplyStats::default();
    crate::db::with_operation(conn, "categorise-apply", |conn| {
        for row in &confirmed {
            let Some(payees) = key_to_payees.get(&row.merchant_key) else {
                // No live transactions for this merchant any more; still
                // drain it below.
                continue;
            };
            // Labels stored as a JSON array (matching the sync/push shape).
            let labels_json = serde_json::to_string(&row.proposed_labels)
                .unwrap_or_else(|_| "[]".to_string());
            let labels_arg: Option<String> = if row.proposed_labels.is_empty() {
                None
            } else {
                Some(labels_json)
            };

            for original_payee in payees {
                let n = conn.execute(
                    "UPDATE transactions
                        SET category_id = COALESCE(?1, category_id),
                            labels      = COALESCE(?2, labels)
                      WHERE original_payee = ?3
                        AND (
                            (?1 IS NOT NULL AND category_id IS NOT ?1)
                            OR (?2 IS NOT NULL AND labels IS NOT ?2)
                        )",
                    params![row.proposed_category, labels_arg, original_payee],
                )?;
                stats.transactions_updated += n;
            }
        }
        stats.rows_drained = cp::delete_confirmed(conn)?;
        Ok(())
    })?;

    Ok(stats)
}

/// Build `merchant_key -> [original_payee]` for every merchant payee in
/// `transactions`, via the deterministic pipeline (same derivation the
/// scan used).
fn merchant_key_to_payees(conn: &Connection) -> Result<HashMap<String, Vec<String>>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT original_payee FROM transactions WHERE original_payee IS NOT NULL",
    )?;
    let payees: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let cache = RuleCache::new();
    let ctx = PipelineCtx::new(conn, &cache);

    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for original_payee in payees {
        let result = normalise(&original_payee, &ctx);
        if result.class() != Some(&PayeeClass::Merchant) {
            continue;
        }
        if let Some(key) = merchant_key(&result.features) {
            map.entry(key).or_default().push(original_payee);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categorise::places::tests_support::FakeClient;
    use crate::categorise::scan::scan;
    use crate::db::initialize_in_memory;
    use crate::test_support::{seed_account, seed_pn, seed_txn};

    fn setup(conn: &Connection) {
        seed_account(conn, 1, "Test").unwrap();
        crate::rules::load_into_db(conn).unwrap();
        conn.execute(
            "INSERT INTO categories (id, title) VALUES (1, 'Eating Out'), (2, '_Groceries')",
            [],
        )
        .unwrap();
    }

    /// Mark a payee eligible for categorisation (applied-or-confirmed gate).
    fn make_eligible(conn: &Connection, original: &str) {
        seed_pn(conn, original, "Proposed", Status::Confirmed, 1).unwrap();
    }

    fn category_of(conn: &Connection, txn_id: i64) -> Option<i64> {
        conn.query_row(
            "SELECT category_id FROM transactions WHERE id = ?1",
            params![txn_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    fn labels_of(conn: &Connection, txn_id: i64) -> Option<String> {
        conn.query_row(
            "SELECT labels FROM transactions WHERE id = ?1",
            params![txn_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn apply_writes_confirmed_category_and_labels_then_drains() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        seed_txn(&conn, 2, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        make_eligible(&conn, "WOOLWORTHS 1624 STRATHF");

        scan(&conn, &FakeClient::supermarket()).unwrap();
        let key = cp::list_all(&conn).unwrap()[0].merchant_key.clone();
        cp::update_status(&conn, &key, Status::Confirmed).unwrap();

        let stats = apply_confirmed(&conn).unwrap();
        assert_eq!(stats.rows_drained, 1);
        assert_eq!(stats.transactions_updated, 2);

        assert_eq!(category_of(&conn, 1), Some(2));
        assert_eq!(category_of(&conn, 2), Some(2));
        assert_eq!(labels_of(&conn, 1).as_deref(), Some(r#"["supermarket"]"#));

        // Staging row drained.
        assert!(cp::list_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn apply_records_change_for_push() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        make_eligible(&conn, "WOOLWORTHS 1624 STRATHF");
        scan(&conn, &FakeClient::supermarket()).unwrap();
        let key = cp::list_all(&conn).unwrap()[0].merchant_key.clone();
        cp::update_status(&conn, &key, Status::Confirmed).unwrap();
        apply_confirmed(&conn).unwrap();

        // A _transaction_changes row with the category (bit2) + labels
        // (bit8) mask bits set exists for txn 1 under categorise-apply.
        let mask: i64 = conn
            .query_row(
                "SELECT mask FROM _transaction_changes
                  WHERE transaction_id = 1
                    AND operation_id = (SELECT id FROM _operations WHERE reason='categorise-apply')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mask & 2, 2, "category bit set");
        assert_eq!(mask & 8, 8, "labels bit set");
    }

    #[test]
    fn pending_proposals_are_not_applied() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        make_eligible(&conn, "WOOLWORTHS 1624 STRATHF");
        scan(&conn, &FakeClient::supermarket()).unwrap();
        // Leave it pending.
        let stats = apply_confirmed(&conn).unwrap();
        assert_eq!(stats, ApplyStats::default());
        assert_eq!(category_of(&conn, 1), None);
    }

    #[test]
    fn apply_is_noop_second_time() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        make_eligible(&conn, "WOOLWORTHS 1624 STRATHF");
        scan(&conn, &FakeClient::supermarket()).unwrap();
        let key = cp::list_all(&conn).unwrap()[0].merchant_key.clone();
        cp::update_status(&conn, &key, Status::Confirmed).unwrap();
        apply_confirmed(&conn).unwrap();
        // Second apply: nothing confirmed remains.
        let stats = apply_confirmed(&conn).unwrap();
        assert_eq!(stats, ApplyStats::default());
    }
}
