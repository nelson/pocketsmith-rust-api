use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};

use pocketsmith_sync::db;
use pocketsmith_sync::db::payee_metadata;
use pocketsmith_sync::normalise::main::{normalise_payee, PipelineRules};
use pocketsmith_sync::normalise::meta;

fn main() -> Result<()> {
    let conn = db::initialize("pocketsmith.db")?;
    let rules = PipelineRules::load(Path::new("rules"))?;

    // Load all transactions
    let mut stmt = conn
        .prepare("SELECT original_payee, payee FROM transactions")
        .context("Loading transactions")?;
    let rows: Vec<(Option<String>, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    if rows.is_empty() {
        println!("No transactions found.");
        return Ok(());
    }

    // Normalise and group
    struct PayeeInfo {
        normalised: String,
        metadata: HashMap<String, serde_json::Value>,
        sample_original: String,
        count: i64,
    }

    let mut norm_cache: HashMap<String, (String, HashMap<String, serde_json::Value>)> =
        HashMap::new();
    let mut payee_map: HashMap<String, PayeeInfo> = HashMap::new();

    for (original_payee, payee) in &rows {
        let raw = original_payee
            .as_deref()
            .or(payee.as_deref())
            .unwrap_or("");
        if raw.is_empty() {
            continue;
        }

        let (normalised, metadata) = norm_cache
            .entry(raw.to_string())
            .or_insert_with(|| normalise_payee(raw, &rules))
            .clone();

        payee_map
            .entry(normalised.clone())
            .and_modify(|info| info.count += 1)
            .or_insert(PayeeInfo {
                normalised,
                metadata,
                sample_original: raw.to_string(),
                count: 1,
            });
    }

    // Persist to DB
    let mut persisted = 0;
    for info in payee_map.values() {
        payee_metadata::upsert(
            &conn,
            &info.normalised,
            &info.metadata,
            &info.sample_original,
            info.count,
        )?;
        persisted += 1;
    }

    println!("Persisted {} unique payees to payee_metadata.", persisted);

    // Summary by type
    let mut type_counts: HashMap<String, (usize, i64)> = HashMap::new();
    for info in payee_map.values() {
        let payee_type = info
            .metadata
            .get(meta::TYPE)
            .and_then(|v| v.as_str())
            .unwrap_or("merchant")
            .to_string();
        let entry = type_counts.entry(payee_type).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += info.count;
    }

    let mut type_list: Vec<_> = type_counts.into_iter().collect();
    type_list.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));

    println!("\n=== Payee Type Distribution ===");
    println!("{:<20} {:>8} {:>10}", "Type", "Payees", "Txns");
    for (typ, (payees, txns)) in &type_list {
        println!("{:<20} {:>8} {:>10}", typ, payees, txns);
    }

    // Top payees by transaction count
    let mut top_payees: Vec<_> = payee_map.values().collect();
    top_payees.sort_by(|a, b| b.count.cmp(&a.count));

    println!("\n=== Top 30 Payees ===");
    println!(
        "{:<40} {:<20} {:>6}",
        "Normalised Payee", "Type", "Txns"
    );
    for info in top_payees.iter().take(30) {
        let typ = info
            .metadata
            .get(meta::TYPE)
            .and_then(|v| v.as_str())
            .unwrap_or("merchant");
        println!("{:<40} {:<20} {:>6}", info.normalised, typ, info.count);
    }

    Ok(())
}
