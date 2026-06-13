//! Categorisation — the final pipeline stage.
//!
//! Consumes confirmed merchant normalisations (entity + location), asks
//! Google Places "what kind of place is this?", maps the answer through
//! the hardcoded [`map::TAXONOMY`] to one of the user's categories plus a
//! controlled leaf label, and stages the result as a proposal reviewed
//! with the same scan -> confirm -> apply paradigm as `normalise`.

pub mod map;
pub mod places;
pub mod propose;
pub mod gate;
pub mod scan;
pub mod apply;

use crate::normalise::Features;

/// The stable identity of a merchant, derived from the pipeline features:
/// `entity_name` plus `location`/`region` when present, lowercased and
/// whitespace-collapsed. This doubles as the Google Places `textQuery`
/// and as the `category_proposals.merchant_key` (so `SMP*Hero Sushi …`
/// and `Hero Sushi …` collapse to one merchant). Returns `None` when no
/// entity was identified (not a categorisable merchant).
pub fn merchant_key(features: &Features) -> Option<String> {
    let entity = features.entity_name.as_deref()?.trim();
    if entity.is_empty() {
        return None;
    }
    let parts = [
        Some(entity),
        features.location.as_deref(),
        features.region.as_deref(),
    ];
    let joined = parts
        .into_iter()
        .flatten()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    // Collapse internal whitespace runs and lowercase.
    let normalised = joined.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    if normalised.is_empty() {
        None
    } else {
        Some(normalised)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalise::Features;

    fn features(entity: Option<&str>, location: Option<&str>, region: Option<&str>) -> Features {
        Features {
            entity_name: entity.map(|s| s.to_string()),
            location: location.map(|s| s.to_string()),
            region: region.map(|s| s.to_string()),
            ..Features::default()
        }
    }

    #[test]
    fn merchant_key_joins_and_normalises() {
        assert_eq!(
            merchant_key(&features(Some("Greenway Meat"), Some("Strathfield"), Some("NSW"))),
            Some("greenway meat strathfield nsw".to_string())
        );
        assert_eq!(
            merchant_key(&features(Some("Woolworths"), None, None)),
            Some("woolworths".to_string())
        );
        // Collapses extra whitespace.
        assert_eq!(
            merchant_key(&features(Some("  Hero   Sushi "), Some(" Burwood "), None)),
            Some("hero sushi burwood".to_string())
        );
    }

    #[test]
    fn merchant_key_none_without_entity() {
        assert_eq!(merchant_key(&features(None, Some("Sydney"), None)), None);
        assert_eq!(merchant_key(&features(Some(""), None, None)), None);
    }
}

/// Medium integration test: the full scan -> confirm -> apply flow over
/// the public library API, using the in-crate fake Places client (no
/// network). Asserts the staged proposal, the applied transaction state,
/// and that the change is recorded for push.
#[cfg(test)]
mod e2e_tests {
    use crate::categorise::places::tests_support::FakeClient;
    use crate::categorise::{apply, scan};
    use crate::db::category_proposals as cp;
    use crate::db::initialize_in_memory;
    use crate::review::Status;
    use crate::test_support::{seed_account, seed_txn};

    #[test]
    fn scan_confirm_apply_end_to_end() {
        let conn = initialize_in_memory().unwrap();
        seed_account(&conn, 1, "Test").unwrap();
        crate::rules::load_into_db(&conn).unwrap();
        conn.execute(
            "INSERT INTO categories (id, title) VALUES (1, 'Eating Out'), (2, '_Groceries')",
            [],
        )
        .unwrap();
        // Two raw payees collapsing to one merchant; plus a cafe merchant.
        seed_txn(&conn, 1, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        seed_txn(&conn, 2, 1, "WOOLWORTHS 1624 STRATHF", "x").unwrap();
        // Eligibility gate: a confirmed normalisation makes it categorisable.
        crate::test_support::seed_pn(
            &conn,
            "WOOLWORTHS 1624 STRATHF",
            "Woolworths, Strathfield",
            Status::Confirmed,
            2,
        )
        .unwrap();

        // 1. SCAN stages one pending proposal (no network: fake client).
        let stats = scan::scan(&conn, &FakeClient::supermarket()).unwrap();
        assert_eq!(stats.inserted, 1);
        let pending = cp::list_by_status(&conn, Status::Pending).unwrap();
        assert_eq!(pending.len(), 1);
        let key = pending[0].merchant_key.clone();
        assert_eq!(pending[0].txn_count, 2);

        // 2. CONFIRM.
        cp::update_status(&conn, &key, Status::Confirmed).unwrap();

        // 3. APPLY writes category + labels to both transactions.
        let astats = apply::apply_confirmed(&conn).unwrap();
        assert_eq!(astats.rows_drained, 1);
        assert_eq!(astats.transactions_updated, 2);

        let cat: Option<i64> = conn
            .query_row("SELECT category_id FROM transactions WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cat, Some(2));
        let labels: Option<String> = conn
            .query_row("SELECT labels FROM transactions WHERE id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(labels.as_deref(), Some(r#"["supermarket"]"#));

        // 4. The change is queued for push (category bit2 + labels bit8).
        let mask: i64 = conn
            .query_row(
                "SELECT mask FROM _transaction_changes
                  WHERE transaction_id = 1
                    AND operation_id = (SELECT id FROM _operations WHERE reason='categorise-apply')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(mask & 2, 2);
        assert_eq!(mask & 8, 8);

        // Staging drained.
        assert!(cp::list_all(&conn).unwrap().is_empty());
    }
}
