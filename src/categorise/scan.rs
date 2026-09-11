//! `categorise scan` — stage category proposals for merchant payees.
//!
//! Mirrors `normalise::scan`: it groups `transactions` by `original_payee`,
//! runs the deterministic normalisation pipeline to recover each payee's
//! merchant identity (entity + location + region), and \u2014 for payees the
//! pipeline classifies as **merchants** \u2014 aggregates them by that identity
//! (the *merchant key*). For each distinct merchant it does a cache-first
//! Google Places lookup, maps the place type through the hardcoded
//! taxonomy, and upserts a pending [`category_proposals`] row.
//!
//! Keying on the merchant identity (not raw `original_payee`) collapses
//! the `SMP*Hero Sushi \u2026` / `Hero Sushi \u2026` style variants into a single
//! review, per the plan decision "key on the confirmed merchant".
//!
//! Policy table (parallels `normalise::scan`):
//!
//! | existing row | proposal vs existing | action                 |
//! |--------------|----------------------|------------------------|
//! | (proposal already applied to txns) | equal | skip (equivalence guard) |
//! | None         | empty (unmapped)     | skip (nothing to do)   |
//! | None         | non-empty            | INSERT pending         |
//! | Some         | equal                | UPDATE txn_count only  |
//! | Some         | different            | overwrite to pending   |
//!
//! Two gates run before the policy table:
//!   * **eligibility** ([`crate::categorise::gate`]): only payees whose
//!     normalisation is applied-or-confirmed (and not pending) are scanned
//!     — so we never spend a Places lookup on a string about to change.
//!   * **equivalence guard** (mirrors `normalise`'s skip-no-change): if the
//!     merchant's transactions already carry the proposed category+labels,
//!     skip — nothing to stage.

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::Connection;

use crate::categorise::places::PlacesClient;
use crate::categorise::propose::{self, Proposal};
use crate::categorise::{gate, merchant_key, places};
use crate::db::category_proposals::{self as cp, CategoryProposalRow};
use crate::normalise::{normalise, PayeeClass, PipelineCtx, RuleCache};
use crate::review::Status;

/// Counts returned from a categorise scan.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanStats {
    /// Distinct merchants whose proposal was newly staged.
    pub inserted: usize,
    /// Existing proposal unchanged — only `txn_count` refreshed.
    pub txn_count_updated: usize,
    /// Existing proposal differed — overwritten back to pending.
    pub overwritten: usize,
    /// Merchants whose place type didn't map to anything (no category and
    /// no label) — not staged.
    pub skipped_unmapped: usize,
    /// Merchants whose Places lookup failed (transport/API error) — not
    /// staged and not cached, so a re-scan retries them.
    pub skipped_error: usize,
    /// Merchants whose transactions already carry the proposed
    /// category+labels (equivalence guard) — not staged.
    pub skipped_no_change: usize,
    /// Distinct eligible merchant keys seen (denominator for progress).
    pub merchants_seen: usize,
}

/// Per-merchant aggregation: total txn count + the eligible original
/// payees that resolve to this merchant key (used for the equivalence
/// guard's "current category/labels" lookup).
struct MerchantAgg {
    txn_count: i64,
    payees: Vec<String>,
}

/// Run a categorise scan against `conn`, using `client` for any uncached
/// Places lookups. All writes occur inside a single
/// `with_operation("categorise-scan", ...)`.
pub fn scan(conn: &Connection, client: &dyn PlacesClient) -> Result<ScanStats> {
    let by_merchant = aggregate_merchants(conn)?;

    let mut stats = ScanStats {
        merchants_seen: by_merchant.len(),
        ..ScanStats::default()
    };

    crate::db::with_operation(conn, "categorise-scan", |conn| {
        for (key, agg) in &by_merchant {
            // Cache-first lookup, then map through the taxonomy.
            let lookup = places::lookup(conn, client, key)?;
            if lookup.status == crate::db::place_lookups::LookupStatus::Error {
                stats.skipped_error += 1;
                continue;
            }
            let proposal = propose::build(conn, &lookup)?;

            if is_empty(&proposal) {
                stats.skipped_unmapped += 1;
                continue;
            }

            // Equivalence guard: if every matching transaction already has
            // the proposed category + labels, there is nothing to do.
            if let Some((cur_cat, cur_labels)) = current_uniform(conn, &agg.payees)? {
                if cur_cat == proposal.category_id && cur_labels == proposal.labels {
                    stats.skipped_no_change += 1;
                    continue;
                }
            }

            let new_row = CategoryProposalRow {
                merchant_key: key.clone(),
                proposed_category: proposal.category_id,
                proposed_labels: proposal.labels.clone(),
                place_type: proposal.place_type.clone(),
                txn_count: agg.txn_count,
                status: Status::Pending,
            };

            match cp::get(conn, key)? {
                None => {
                    cp::upsert(conn, &new_row)?;
                    stats.inserted += 1;
                }
                Some(existing) => {
                    if same_proposal(&existing, &new_row) {
                        if existing.txn_count != agg.txn_count {
                            cp::update_txn_count(conn, key, agg.txn_count)?;
                        }
                        stats.txn_count_updated += 1;
                    } else {
                        cp::upsert(conn, &new_row)?;
                        stats.overwritten += 1;
                    }
                }
            }
        }
        Ok(())
    })?;

    Ok(stats)
}

/// Group eligible `transactions` by `original_payee`, run the pipeline,
/// and aggregate by merchant key for payees classified as merchants.
/// Returns a sorted map for deterministic iteration.
fn aggregate_merchants(conn: &Connection) -> Result<BTreeMap<String, MerchantAgg>> {
    let eligible = gate::eligible_payees(conn)?;

    let mut stmt = conn.prepare(
        "SELECT original_payee, COUNT(*)
           FROM transactions
          WHERE original_payee IS NOT NULL
          GROUP BY original_payee",
    )?;
    let groups: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let cache = RuleCache::new();
    let ctx = PipelineCtx::new(conn, &cache);

    let mut by_merchant: BTreeMap<String, MerchantAgg> = BTreeMap::new();
    for (original_payee, txn_count) in &groups {
        if !eligible.contains(original_payee) {
            continue;
        }
        let result = normalise(original_payee, &ctx);
        if result.class() != Some(&PayeeClass::Merchant) {
            continue;
        }
        if let Some(key) = merchant_key(&result.features) {
            let entry = by_merchant.entry(key).or_insert(MerchantAgg {
                txn_count: 0,
                payees: Vec::new(),
            });
            entry.txn_count += *txn_count;
            entry.payees.push(original_payee.clone());
        }
    }
    Ok(by_merchant)
}

/// The uniform `(category_id, labels)` currently on the transactions for
/// `payees`, or `None` if they disagree (or there are none) — in which
/// case the equivalence guard does not fire.
fn current_uniform(
    conn: &Connection,
    payees: &[String],
) -> Result<Option<(Option<i64>, Vec<String>)>> {
    if payees.is_empty() {
        return Ok(None);
    }
    let placeholders = vec!["?"; payees.len()].join(",");
    let sql = format!(
        "SELECT DISTINCT category_id, labels FROM transactions WHERE original_payee IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<(Option<i64>, Option<String>)> = stmt
        .query_map(rusqlite::params_from_iter(payees.iter()), |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if rows.len() != 1 {
        return Ok(None); // mixed current state -> don't skip
    }
    let (cat, labels_json) = &rows[0];
    let labels: Vec<String> = labels_json
        .as_ref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    Ok(Some((*cat, labels)))
}

fn is_empty(p: &Proposal) -> bool {
    p.category_id.is_none() && p.labels.is_empty()
}

fn same_proposal(existing: &CategoryProposalRow, candidate: &CategoryProposalRow) -> bool {
    existing.proposed_category == candidate.proposed_category
        && existing.proposed_labels == candidate.proposed_labels
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::categorise::places::tests_support::FakeClient;
    use crate::db::initialize_in_memory;
    use crate::test_support::{seed_account, seed_pn, seed_txn};

    fn setup(conn: &Connection) {
        seed_account(conn, 1, "Test").unwrap();
        crate::rules::load_into_db(conn).unwrap();
        // Categories the taxonomy maps onto.
        conn.execute(
            "INSERT INTO categories (id, title) VALUES (1, 'Eating Out'), (2, '_Groceries')",
            [],
        )
        .unwrap();
    }

    /// Make a payee eligible for categorisation by giving it a confirmed
    /// normalisation staging row (the "applied OR confirmed" gate).
    fn make_eligible(conn: &Connection, original: &str) {
        seed_pn(conn, original, "Proposed", Status::Confirmed, 1).unwrap();
    }

    #[test]
    fn ineligible_payee_is_not_scanned() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        // Merchant payee, but normalisation is still pending -> not eligible.
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        seed_pn(&conn, "WOOLWORTHS 1624 STRATHF", "Woolworths", Status::Pending, 1).unwrap();
        let client = FakeClient::supermarket();
        let stats = scan(&conn, &client).unwrap();
        assert_eq!(stats.merchants_seen, 0, "pending payee is gated out");
        assert_eq!(client.calls(), 0, "no Places call for an ineligible payee");
        assert!(cp::list_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn scan_stages_pending_for_a_merchant() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        // A payee the pipeline classifies as a merchant (Woolworths).
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "WOOLWORTHS 1624 STRATHF").unwrap();
        make_eligible(&conn, "WOOLWORTHS 1624 STRATHF");

        let client = FakeClient::supermarket();
        let stats = scan(&conn, &client).unwrap();
        assert_eq!(stats.inserted, 1);
        assert_eq!(stats.merchants_seen, 1);

        let rows = cp::list_by_status(&conn, Status::Pending).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].proposed_category, Some(2));
        assert_eq!(rows[0].proposed_labels, vec!["supermarket"]);
        assert_eq!(rows[0].status, Status::Pending);
    }

    #[test]
    fn scan_collapses_variants_into_one_proposal() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        // Two raw payees that normalise to the same merchant identity.
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        seed_txn(&conn, 2, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        make_eligible(&conn, "WOOLWORTHS 1624 STRATHF");
        let client = FakeClient::supermarket();
        let stats = scan(&conn, &client).unwrap();
        assert_eq!(stats.inserted, 1);
        let rows = cp::list_all(&conn).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].txn_count, 2, "txn counts summed across variants");
        // Only one API call thanks to the cache + single merchant key.
        assert_eq!(client.calls(), 1);
    }

    #[test]
    fn rescan_is_idempotent_and_makes_no_new_calls() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        make_eligible(&conn, "WOOLWORTHS 1624 STRATHF");
        let client = FakeClient::supermarket();
        scan(&conn, &client).unwrap();
        let calls_after_first = client.calls();

        let stats = scan(&conn, &client).unwrap();
        assert_eq!(stats.txn_count_updated, 1);
        assert_eq!(stats.inserted, 0);
        assert_eq!(client.calls(), calls_after_first, "cache prevents re-calling");
    }

    #[test]
    fn unmapped_place_is_not_staged() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        make_eligible(&conn, "WOOLWORTHS 1624 STRATHF");
        let client = FakeClient::unmapped(); // returns 'zoo'
        let stats = scan(&conn, &client).unwrap();
        assert_eq!(stats.inserted, 0);
        assert_eq!(stats.skipped_unmapped, 1);
        assert!(cp::list_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn confirmed_proposal_survives_rescan_when_unchanged() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        make_eligible(&conn, "WOOLWORTHS 1624 STRATHF");
        let client = FakeClient::supermarket();
        scan(&conn, &client).unwrap();
        let key = cp::list_all(&conn).unwrap()[0].merchant_key.clone();
        cp::update_status(&conn, &key, Status::Confirmed).unwrap();

        let stats = scan(&conn, &client).unwrap();
        assert_eq!(stats.txn_count_updated, 1);
        let row = cp::get(&conn, &key).unwrap().unwrap();
        assert_eq!(row.status, Status::Confirmed, "unchanged proposal keeps confirmed status");
    }

    #[test]
    fn equivalence_guard_skips_already_categorised_merchant() {
        let conn = initialize_in_memory().unwrap();
        setup(&conn);
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        make_eligible(&conn, "WOOLWORTHS 1624 STRATHF");
        let client = FakeClient::supermarket();
        // Scan -> confirm -> apply, so the txn now carries the proposed
        // category + labels.
        scan(&conn, &client).unwrap();
        let key = cp::list_all(&conn).unwrap()[0].merchant_key.clone();
        cp::update_status(&conn, &key, Status::Confirmed).unwrap();
        crate::categorise::apply::apply_confirmed(&conn).unwrap();

        // Re-scan: the equivalence guard fires; nothing is re-staged.
        let stats = scan(&conn, &client).unwrap();
        assert_eq!(stats.skipped_no_change, 1);
        assert_eq!(stats.inserted, 0);
        assert_eq!(stats.overwritten, 0);
        assert!(cp::list_all(&conn).unwrap().is_empty(), "no proposal re-staged");
    }
}
