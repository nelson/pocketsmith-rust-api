use std::collections::HashMap;

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::normalise::main::{normalise_payee, PipelineRules};
use crate::normalise::meta;

use super::cache::{self, PlaceResult};
use super::llm::LlmClient;
use super::mapping::map_place_to_category;
use super::places::PlacesClient;
use super::rules::{try_rules, CategoriseRules};
use super::{CategoriseResult, CategoriseSource};

/// A transaction row with the fields we need.
struct TxnRow {
    id: i64,
    payee: Option<String>,
    original_payee: Option<String>,
    category_id: Option<i64>,
}

/// A grouped payee with its normalised form, metadata, and transaction IDs.
struct PayeeGroup {
    normalised: String,
    txn_type: Option<String>,
    txn_ids: Vec<i64>,
    current_category_ids: Vec<Option<i64>>,
}

/// Proposed category change for display/approval.
#[derive(Debug)]
pub struct CategoryChange {
    pub normalised_payee: String,
    pub txn_count: usize,
    pub old_category: Option<String>,
    pub new_category: String,
    pub source: CategoriseSource,
    pub reason: String,
    pub txn_ids: Vec<i64>,
    pub new_category_id: i64,
}

pub struct PipelineConfig {
    pub google_places_key: Option<String>,
    pub anthropic_key: Option<String>,
}

/// Run the full categorisation pipeline. Returns (results, changes).
pub fn run(
    conn: &Connection,
    normalise_rules: &PipelineRules,
    categorise_rules: &CategoriseRules,
    config: &PipelineConfig,
) -> Result<(Vec<CategoriseResult>, Vec<CategoryChange>)> {
    // 1. Load category mappings (both directions, single query)
    let (cat_name_to_id, cat_id_to_name) = load_category_maps(conn)?;

    // 2. Load all transactions
    let txns = load_transactions(conn)?;
    if txns.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    // 3. Normalise and group by payee
    let groups = normalise_and_group(&txns, normalise_rules);

    // 4. Categorise each group
    let mut results: Vec<CategoriseResult> = Vec::new();
    let mut unresolved_merchants: Vec<(usize, String)> = Vec::new(); // (results_idx, payee)

    let places_client = config
        .google_places_key
        .as_ref()
        .map(|k| PlacesClient::new(k.clone()));

    for group in &groups {
        let count = group.txn_ids.len();
        let txn_type = group.txn_type.as_deref();

        // 4a. Try rule-based
        if let Some(r) = try_rules(&group.normalised, txn_type, count, categorise_rules) {
            results.push(r);
            continue;
        }

        // 4b. Check cache
        if let Some(cached) = cache::get_cached(conn, &group.normalised, "google_places")? {
            if let Some(cat) =
                map_place_to_category(&cached.place_types, &categorise_rules.google_places_mappings)
            {
                let type_str = cached.place_types.first().cloned().unwrap_or_default();
                results.push(CategoriseResult {
                    normalised_payee: group.normalised.clone(),
                    category: Some(cat),
                    source: CategoriseSource::Cache,
                    reason: format!("cache:{}→category", type_str),
                    transaction_count: count,
                });
                continue;
            }
        }

        // 4c. Google Places API
        if let Some(ref client) = places_client {
            match client.search_raw(&group.normalised) {
                Ok((Some(place), raw)) => {
                    cache::set_cached(
                        conn,
                        &group.normalised,
                        "google_places",
                        &place,
                        Some(&raw),
                    )?;
                    if let Some(cat) = map_place_to_category(
                        &place.place_types,
                        &categorise_rules.google_places_mappings,
                    ) {
                        let type_str = place.place_types.first().cloned().unwrap_or_default();
                        results.push(CategoriseResult {
                            normalised_payee: group.normalised.clone(),
                            category: Some(cat),
                            source: CategoriseSource::GooglePlaces,
                            reason: format!("google:{}→category", type_str),
                            transaction_count: count,
                        });
                        continue;
                    }
                }
                Ok((None, raw)) => {
                    let empty = PlaceResult {
                        place_name: None,
                        place_types: vec![],
                        place_address: None,
                    };
                    cache::set_cached(
                        conn,
                        &group.normalised,
                        "google_places",
                        &empty,
                        Some(&raw),
                    )?;
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Google Places API error for '{}': {}",
                        group.normalised, e
                    );
                }
            }
        }

        // 4d. Queue for LLM batch
        let idx = results.len();
        results.push(CategoriseResult {
            normalised_payee: group.normalised.clone(),
            category: None,
            source: CategoriseSource::Unknown,
            reason: "pending LLM".into(),
            transaction_count: count,
        });
        unresolved_merchants.push((idx, group.normalised.clone()));
    }

    // 5. LLM batch for unresolved
    if !unresolved_merchants.is_empty() {
        if let Some(ref api_key) = config.anthropic_key {
            let client = LlmClient::new(api_key.clone());
            let payees: Vec<String> =
                unresolved_merchants.iter().map(|(_, p)| p.clone()).collect();

            for (chunk_idx, chunk) in payees.chunks(20).enumerate() {
                let chunk_vec: Vec<String> = chunk.to_vec();
                match client.categorise_batch(&chunk_vec) {
                    Ok(llm_results) => {
                        let base = chunk_idx * 20;
                        for (i, llm_r) in llm_results.into_iter().enumerate() {
                            let global_idx = unresolved_merchants[base + i].0;
                            results[global_idx].source = CategoriseSource::Llm;
                            results[global_idx].category = llm_r.category;
                            results[global_idx].reason = llm_r.reason;
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: LLM batch error: {}", e);
                    }
                }
            }
        }
    }

    // 6. Build changes list
    let mut changes: Vec<CategoryChange> = Vec::new();

    for (i, group) in groups.iter().enumerate() {
        let result = &results[i];
        if let Some(ref new_cat) = result.category {
            if let Some(&new_cat_id) = cat_name_to_id.get(new_cat.as_str()) {
                let dominant_old = dominant_category(&group.current_category_ids);
                let old_name = dominant_old.and_then(|id| cat_id_to_name.get(&id).cloned());

                if old_name.as_deref() != Some(new_cat.as_str()) {
                    changes.push(CategoryChange {
                        normalised_payee: group.normalised.clone(),
                        txn_count: group.txn_ids.len(),
                        old_category: old_name,
                        new_category: new_cat.clone(),
                        source: result.source.clone(),
                        reason: result.reason.clone(),
                        txn_ids: group.txn_ids.clone(),
                        new_category_id: new_cat_id,
                    });
                }
            }
        }
    }

    changes.sort_by(|a, b| b.txn_count.cmp(&a.txn_count));

    Ok((results, changes))
}

/// Apply approved category changes.
pub fn apply_changes(conn: &Connection, changes: &[CategoryChange]) -> Result<usize> {
    let mut total = 0;
    crate::db::with_transaction_change_log(conn, "categorise", |conn| {
        for change in changes {
            for &txn_id in &change.txn_ids {
                conn.execute(
                    "UPDATE transactions SET category_id = ?1 WHERE id = ?2",
                    rusqlite::params![change.new_category_id, txn_id],
                )?;
                total += 1;
            }
        }
        Ok(())
    })?;
    Ok(total)
}

fn load_transactions(conn: &Connection) -> Result<Vec<TxnRow>> {
    let mut stmt = conn
        .prepare("SELECT id, payee, original_payee, category_id FROM transactions")
        .context("Loading transactions")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(TxnRow {
                id: row.get(0)?,
                payee: row.get(1)?,
                original_payee: row.get(2)?,
                category_id: row.get(3)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn normalise_and_group(txns: &[TxnRow], rules: &PipelineRules) -> Vec<PayeeGroup> {
    let mut map: HashMap<String, PayeeGroup> = HashMap::new();
    let mut norm_cache: HashMap<String, (String, Option<String>)> = HashMap::new();

    for txn in txns {
        let raw = txn
            .original_payee
            .as_deref()
            .or(txn.payee.as_deref())
            .unwrap_or("");
        if raw.is_empty() {
            continue;
        }

        let (normalised, txn_type) = norm_cache
            .entry(raw.to_string())
            .or_insert_with(|| {
                let (n, metadata) = normalise_payee(raw, rules);
                let t = metadata
                    .get(meta::TYPE)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                (n, t)
            })
            .clone();

        let entry = map.entry(normalised.clone()).or_insert_with(|| PayeeGroup {
            normalised,
            txn_type,
            txn_ids: Vec::new(),
            current_category_ids: Vec::new(),
        });
        entry.txn_ids.push(txn.id);
        entry.current_category_ids.push(txn.category_id);
    }

    let mut groups: Vec<PayeeGroup> = map.into_values().collect();
    groups.sort_by(|a, b| b.txn_ids.len().cmp(&a.txn_ids.len()));
    groups
}

fn load_category_maps(
    conn: &Connection,
) -> Result<(HashMap<String, i64>, HashMap<i64, String>)> {
    let mut stmt = conn.prepare("SELECT id, title FROM categories")?;
    let mut name_to_id = HashMap::new();
    let mut id_to_name = HashMap::new();
    stmt.query_map([], |row| {
        let id: i64 = row.get(0)?;
        let title: String = row.get(1)?;
        Ok((id, title))
    })?
    .filter_map(|r| r.ok())
    .for_each(|(id, title)| {
        name_to_id.insert(title.clone(), id);
        id_to_name.insert(id, title);
    });
    Ok((name_to_id, id_to_name))
}

fn dominant_category(ids: &[Option<i64>]) -> Option<i64> {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for id in ids.iter().flatten() {
        *counts.entry(*id).or_default() += 1;
    }
    counts.into_iter().max_by_key(|(_, c)| *c).map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::db::test_helpers::*;
    use std::path::Path;

    fn setup_db_with_categories(conn: &Connection) {
        for (id, title) in [
            (1, "_Bills"),
            (2, "_Dining"),
            (3, "_Education"),
            (4, "_Giving"),
            (5, "_Groceries"),
            (6, "_Holidays"),
            (7, "_Household"),
            (8, "_Income"),
            (9, "_Mortgage"),
            (10, "_Shopping"),
            (11, "_Transfer"),
            (12, "_Transport"),
        ] {
            db::upsert_category(conn, &make_category(id, title)).unwrap();
        }
    }

    fn insert_txn(conn: &Connection, id: i64, payee: &str, original_payee: &str) {
        let mut txn = make_transaction(id, payee);
        txn.original_payee = Some(original_payee.into());
        db::upsert_transaction(conn, &txn).unwrap();
    }

    #[test]
    fn test_pipeline_salary_to_income() {
        let conn = db::initialize_in_memory().unwrap();
        setup_db_with_categories(&conn);

        db::with_transaction_change_log(&conn, "test", |conn| {
            insert_txn(conn, 1, "Apple", "Salary from APPLE PTY LTD");
            Ok(())
        })
        .unwrap();

        let norm_rules = PipelineRules::load(Path::new("rules")).unwrap();
        let cat_rules = CategoriseRules::load(Path::new("rules")).unwrap();
        let config = PipelineConfig {
            google_places_key: None,
            anthropic_key: None,
        };

        let (results, changes) = run(&conn, &norm_rules, &cat_rules, &config).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category, Some("_Income".into()));
        assert_eq!(results[0].source, CategoriseSource::Rule);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].new_category, "_Income");
    }

    #[test]
    fn test_pipeline_transfer_to_transfer() {
        let conn = db::initialize_in_memory().unwrap();
        setup_db_with_categories(&conn);

        db::with_transaction_change_log(&conn, "test", |conn| {
            insert_txn(conn, 1, "Transfer", "Transfer to xx1234 CommBank App");
            Ok(())
        })
        .unwrap();

        let norm_rules = PipelineRules::load(Path::new("rules")).unwrap();
        let cat_rules = CategoriseRules::load(Path::new("rules")).unwrap();
        let config = PipelineConfig {
            google_places_key: None,
            anthropic_key: None,
        };

        let (results, _) = run(&conn, &norm_rules, &cat_rules, &config).unwrap();
        assert_eq!(results[0].category, Some("_Transfer".into()));
    }

    #[test]
    fn test_pipeline_merchant_without_api_stays_unknown() {
        let conn = db::initialize_in_memory().unwrap();
        setup_db_with_categories(&conn);

        db::with_transaction_change_log(&conn, "test", |conn| {
            insert_txn(conn, 1, "Woolworths", "WOOLWORTHS 1234 STRATHFIELD");
            Ok(())
        })
        .unwrap();

        let norm_rules = PipelineRules::load(Path::new("rules")).unwrap();
        let cat_rules = CategoriseRules::load(Path::new("rules")).unwrap();
        let config = PipelineConfig {
            google_places_key: None,
            anthropic_key: None,
        };

        let (results, _) = run(&conn, &norm_rules, &cat_rules, &config).unwrap();
        assert_eq!(results[0].source, CategoriseSource::Unknown);
    }

    #[test]
    fn test_pipeline_cached_place_used() {
        let conn = db::initialize_in_memory().unwrap();
        setup_db_with_categories(&conn);

        db::with_transaction_change_log(&conn, "test", |conn| {
            insert_txn(conn, 1, "Woolworths", "WOOLWORTHS 1234 STRATHFIELD");
            Ok(())
        })
        .unwrap();

        let place = crate::categorise::cache::PlaceResult {
            place_name: Some("Woolworths Strathfield".into()),
            place_types: vec!["supermarket".into(), "grocery_store".into()],
            place_address: None,
        };
        crate::categorise::cache::set_cached(
            &conn,
            "Woolworths Strathfield",
            "google_places",
            &place,
            None,
        )
        .unwrap();

        let norm_rules = PipelineRules::load(Path::new("rules")).unwrap();
        let cat_rules = CategoriseRules::load(Path::new("rules")).unwrap();
        let config = PipelineConfig {
            google_places_key: None,
            anthropic_key: None,
        };

        let (results, changes) = run(&conn, &norm_rules, &cat_rules, &config).unwrap();
        assert_eq!(results[0].category, Some("_Groceries".into()));
        assert_eq!(results[0].source, CategoriseSource::Cache);
        assert_eq!(changes[0].new_category, "_Groceries");
    }

    #[test]
    fn test_apply_changes() {
        let conn = db::initialize_in_memory().unwrap();
        setup_db_with_categories(&conn);

        db::with_transaction_change_log(&conn, "test", |conn| {
            insert_txn(conn, 1, "Store", "Salary from APPLE PTY LTD");
            insert_txn(conn, 2, "Store", "Salary from APPLE PTY LTD");
            Ok(())
        })
        .unwrap();

        let changes = vec![CategoryChange {
            normalised_payee: "Apple (Salary)".into(),
            txn_count: 2,
            old_category: None,
            new_category: "_Income".into(),
            source: CategoriseSource::Rule,
            reason: "type:salary→_Income".into(),
            txn_ids: vec![1, 2],
            new_category_id: 8,
        }];

        let count = apply_changes(&conn, &changes).unwrap();
        assert_eq!(count, 2);

        let cat_id: i64 = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cat_id, 8);
    }
}
