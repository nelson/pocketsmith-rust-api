use std::collections::HashMap;
use std::env;

use anyhow::Result;

use pocketsmith_sync::db;
use pocketsmith_sync::normalise::{self, PayeeClass};

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();
    let dry_run = args.iter().any(|a| a == "--dry-run");

    let conn = db::initialize("pocketsmith.db")?;

    // Read all (id, original_payee) pairs
    let mut stmt = conn.prepare(
        "SELECT id, original_payee FROM transactions WHERE original_payee IS NOT NULL",
    )?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let total_txns = rows.len();

    // Group by original_payee for efficiency
    let mut groups: HashMap<String, Vec<i64>> = HashMap::new();
    for (id, original_payee) in rows {
        groups.entry(original_payee).or_default().push(id);
    }

    // Process each unique original_payee, consuming groups to avoid clones
    let mut results: Vec<(String, normalise::NormalisationResult, Vec<i64>)> = groups
        .into_iter()
        .map(|(original_payee, ids)| {
            let result = normalise::normalise(&original_payee);
            (original_payee, result, ids)
        })
        .collect();

    // Sort by transaction count descending for reports
    results.sort_by(|a, b| b.2.len().cmp(&a.2.len()));

    // Compute the formatted payee for each result
    let formatted: Vec<(&str, String, &normalise::NormalisationResult, &[i64])> = results
        .iter()
        .map(|(orig, result, ids)| {
            let payee = format_payee(result);
            (orig.as_str(), payee, result, ids.as_slice())
        })
        .collect();

    // Write to DB (unless dry-run) — one UPDATE per unique payee
    if !dry_run {
        db::with_transaction_change_log(&conn, "normalisation", |conn| {
            for (_, payee, _, ids) in &formatted {
                if ids.is_empty() {
                    continue;
                }
                let placeholders: Vec<String> =
                    (0..ids.len()).map(|i| format!("?{}", i + 2)).collect();
                let sql = format!(
                    "UPDATE transactions SET payee = ?1 WHERE id IN ({}) AND payee IS NOT ?1",
                    placeholders.join(", ")
                );
                let mut stmt = conn.prepare_cached(&sql)?;
                let mut params: Vec<Box<dyn rusqlite::types::ToSql>> =
                    vec![Box::new(payee.clone())];
                for &id in *ids {
                    params.push(Box::new(id));
                }
                let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p.as_ref()).collect();
                stmt.execute(param_refs.as_slice())?;
            }
            Ok(())
        })?;
    }

    // Print summary
    print_summary(&formatted, total_txns, dry_run);

    Ok(())
}

fn format_payee(result: &normalise::NormalisationResult) -> String {
    match result.class() {
        Some(PayeeClass::Merchant) => {
            match (
                &result.features.entity_name,
                &result.features.location,
            ) {
                (Some(name), Some(loc)) => format!("{} {}", name, loc),
                (Some(name), None) => name.clone(),
                _ => result.normalised.clone(),
            }
        }
        _ => result.normalised.clone(),
    }
}

fn print_summary(
    formatted: &[(&str, String, &normalise::NormalisationResult, &[i64])],
    total_txns: usize,
    dry_run: bool,
) {
    if dry_run {
        println!("=== DRY RUN (no DB writes) ===\n");
    }

    let total_unique = formatted.len();

    // Classification counts (unique + transaction)
    let mut merchant_count = 0usize;
    let mut person_count = 0usize;
    let mut employer_count = 0usize;
    let mut other_count = 0usize;

    let mut merchant_txns = 0usize;
    let mut person_txns = 0usize;
    let mut employer_txns = 0usize;
    let mut other_txns = 0usize;
    let mut unclassified_txns = 0usize;

    let mut merchant_with_entity = 0usize;
    let mut merchant_with_location = 0usize;
    let mut merchant_with_both = 0usize;

    let mut unclassified: Vec<(&str, &str, usize)> = Vec::new();
    let mut missing_entity: Vec<(&str, usize)> = Vec::new();
    let mut missing_location: Vec<(&str, usize)> = Vec::new();

    for (orig, payee, result, ids) in formatted {
        let txn_count = ids.len();
        match result.class() {
            Some(PayeeClass::Merchant) => {
                merchant_count += 1;
                merchant_txns += txn_count;
                let has_entity = result.features.entity_name.is_some();
                let has_location = result.features.location.is_some();
                if has_entity {
                    merchant_with_entity += 1;
                } else {
                    missing_entity.push((payee, txn_count));
                }
                if has_location {
                    merchant_with_location += 1;
                } else {
                    missing_location.push((payee, txn_count));
                }
                if has_entity && has_location {
                    merchant_with_both += 1;
                }
            }
            Some(PayeeClass::Person) => {
                person_count += 1;
                person_txns += txn_count;
            }
            Some(PayeeClass::Employer) => {
                employer_count += 1;
                employer_txns += txn_count;
            }
            Some(PayeeClass::Other) => {
                other_count += 1;
                other_txns += txn_count;
            }
            None => {
                unclassified_txns += txn_count;
                unclassified.push((orig, payee, txn_count));
            }
        }
    }

    println!("=== Normalisation Summary ===");
    println!("Total unique original_payees: {total_unique}");
    println!("Total transactions: {total_txns}");
    println!(
        "  Merchant:      {:>4} unique ({merchant_txns} txns, {:.0}%)",
        merchant_count,
        pct(merchant_txns, total_txns)
    );
    println!(
        "  Person:        {:>4} unique ({person_txns} txns, {:.0}%)",
        person_count,
        pct(person_txns, total_txns)
    );
    println!(
        "  Employer:      {:>4} unique ({employer_txns} txns, {:.0}%)",
        employer_count,
        pct(employer_txns, total_txns)
    );
    println!(
        "  Other:         {:>4} unique ({other_txns} txns, {:.0}%)",
        other_count,
        pct(other_txns, total_txns)
    );
    println!(
        "  Unclassified:  {:>4} unique ({unclassified_txns} txns, {:.0}%)",
        unclassified.len(),
        pct(unclassified_txns, total_txns)
    );

    println!("\n=== Merchant Coverage ===");
    println!(
        "  entity_name extracted: {merchant_with_entity}/{merchant_count} ({:.0}%)",
        pct(merchant_with_entity, merchant_count)
    );
    println!(
        "  location extracted:    {merchant_with_location}/{merchant_count} ({:.0}%)",
        pct(merchant_with_location, merchant_count)
    );
    println!(
        "  full query (both):     {merchant_with_both}/{merchant_count} ({:.0}%)",
        pct(merchant_with_both, merchant_count)
    );

    println!("\n=== Top Unclassified (by txn count) ===");
    for (i, (orig, payee, count)) in unclassified.iter().take(20).enumerate() {
        println!("  {:>2}. {:?} → {:?} ({count} txns)", i + 1, orig, payee);
    }

    println!("\n=== Top Merchants Missing entity_name ===");
    for (i, (payee, count)) in missing_entity.iter().take(20).enumerate() {
        println!("  {:>2}. {:?} ({count} txns)", i + 1, payee);
    }

    println!("\n=== Top Merchants Missing location ===");
    for (i, (payee, count)) in missing_location.iter().take(20).enumerate() {
        println!("  {:>2}. {:?} ({count} txns)", i + 1, payee);
    }
}

fn pct(n: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        n as f64 / total as f64 * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_payee_merchant_with_both() {
        let mut result = normalise::NormalisationResult::new("WOOLWORTHS STRATHFIELD");
        result.normalised = "WOOLWORTHS STRATHFIELD".into();
        result.set_class(PayeeClass::Merchant);
        result.features.entity_name = Some("Woolworths".into());
        result.features.location = Some("Strathfield".into());
        assert_eq!(format_payee(&result), "Woolworths Strathfield");
    }

    #[test]
    fn test_format_payee_merchant_entity_only() {
        let mut result = normalise::NormalisationResult::new("VODAFONE");
        result.normalised = "VODAFONE".into();
        result.set_class(PayeeClass::Merchant);
        result.features.entity_name = Some("Vodafone Australia".into());
        assert_eq!(format_payee(&result), "Vodafone Australia");
    }

    #[test]
    fn test_format_payee_merchant_no_entity() {
        let mut result = normalise::NormalisationResult::new("SOME MERCHANT");
        result.normalised = "Some Merchant".into();
        result.set_class(PayeeClass::Merchant);
        assert_eq!(format_payee(&result), "Some Merchant");
    }

    #[test]
    fn test_format_payee_person() {
        let mut result = normalise::NormalisationResult::new("JOHN SMITH");
        result.normalised = "John Smith".into();
        result.set_class(PayeeClass::Person);
        assert_eq!(format_payee(&result), "John Smith");
    }

    #[test]
    fn test_format_payee_unclassified() {
        let result = normalise::NormalisationResult::new("UNKNOWN");
        assert_eq!(format_payee(&result), "UNKNOWN");
    }
}
