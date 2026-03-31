use std::collections::HashMap;
use std::env;

use anyhow::Result;

use pocketsmith_sync::db;
use pocketsmith_sync::normalise::{self, PayeeClass};

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--review") {
        unimplemented!("review mode");
    }
    if args.iter().any(|a| a == "--apply") {
        unimplemented!("apply mode");
    }

    analyse_mode(&args)
}

fn load_payees(conn: &rusqlite::Connection) -> Result<Vec<(String, usize)>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(original_payee, payee, ''), COUNT(*) as cnt
         FROM transactions
         WHERE payee IS NOT NULL
         GROUP BY COALESCE(original_payee, payee, '')
         ORDER BY cnt DESC",
    )?;
    let rows: Vec<(String, usize)> = stmt
        .query_map([], |row| {
            let payee: String = row.get(0)?;
            let count: usize = row.get(1)?;
            Ok((payee, count))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

struct PayeeGroup {
    normalised: String,
    class: PayeeClass,
    count: usize,
    original_samples: Vec<String>,
}

fn analyse_mode(args: &[String]) -> Result<()> {
    let verbose = args.iter().any(|a| a == "--verbose");
    let conn = db::initialize("pocketsmith.db")?;
    let payees = load_payees(&conn)?;

    let total_distinct = payees.len();
    let total_txns: usize = payees.iter().map(|(_, c)| c).sum();

    let mut groups: HashMap<String, PayeeGroup> = HashMap::new();
    let mut classified_txns = 0usize;
    let mut class_counts: HashMap<&str, usize> = HashMap::new();

    for (payee, count) in &payees {
        let result = normalise::normalise(payee);
        let class_name = match result.class {
            PayeeClass::Person => "Person",
            PayeeClass::Merchant => "Merchant",
            PayeeClass::Employer => "Employer",
            PayeeClass::Other => "Other",
            PayeeClass::Unclassified => "Unclassified",
        };
        *class_counts.entry(class_name).or_insert(0) += count;

        if result.class != PayeeClass::Unclassified {
            classified_txns += count;
        }

        if verbose {
            println!(
                "{:>5}x  {:50} -> {:40} [{:?}]",
                count, payee, result.normalised, result.class
            );
        }

        let group = groups
            .entry(result.normalised.clone())
            .or_insert_with(|| PayeeGroup {
                normalised: result.normalised.clone(),
                class: result.class.clone(),
                count: 0,
                original_samples: Vec::new(),
            });
        group.count += count;
        if group.original_samples.len() < 3 {
            group.original_samples.push(payee.clone());
        }
    }

    println!("\n=== Normalisation Coverage ===");
    println!("Distinct payees: {total_distinct}  |  Total transactions: {total_txns}");
    let coverage = if total_txns > 0 {
        classified_txns as f64 / total_txns as f64 * 100.0
    } else {
        0.0
    };
    let unclassified_txns = total_txns - classified_txns;
    println!("Classified: {classified_txns} ({coverage:.1}%)  |  Unclassified: {unclassified_txns}");

    println!("\nBy class:");
    let mut sorted_classes: Vec<_> = class_counts.iter().collect();
    sorted_classes.sort_by(|a, b| b.1.cmp(a.1));
    for (class, count) in sorted_classes {
        let pct = *count as f64 / total_txns as f64 * 100.0;
        println!("  {class:15} {count:>6} ({pct:.1}%)");
    }

    let mut unclassified_groups: Vec<&PayeeGroup> = groups
        .values()
        .filter(|g| g.class == PayeeClass::Unclassified)
        .collect();
    unclassified_groups.sort_by(|a, b| b.count.cmp(&a.count));

    if !unclassified_groups.is_empty() {
        println!("\nTop 20 unclassified:");
        for g in unclassified_groups.iter().take(20) {
            let sample = g.original_samples.first().map(|s| s.as_str()).unwrap_or("");
            println!("  {:>5}x  {:50} <- {}", g.count, g.normalised, sample);
        }
    }

    println!("\nDistinct normalised payees: {} (from {total_distinct} originals)", groups.len());
    Ok(())
}

fn parse_review_limit(args: &[String]) -> Result<usize> {
    let pos = args.iter().position(|a| a == "--review");
    match pos {
        Some(i) if i + 1 < args.len() => {
            let n: usize = args[i + 1]
                .parse()
                .map_err(|_| anyhow::anyhow!("--review requires a number"))?;
            Ok(n)
        }
        _ => anyhow::bail!("--review requires a number (e.g. --review 10)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_review_limit() {
        let args = vec!["normalise".into(), "--review".into(), "10".into()];
        assert_eq!(parse_review_limit(&args).unwrap(), 10);
    }

    #[test]
    fn test_parse_review_limit_missing_number() {
        let args = vec!["normalise".into(), "--review".into()];
        assert!(parse_review_limit(&args).is_err());
    }
}
