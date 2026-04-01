use anyhow::Result;

use pocketsmith_sync::db;
use pocketsmith_sync::normalise::{
    strip_metadata, strip_metadata_suffix_only, Features,
};

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let conn = db::initialize("pocketsmith.db")?;

    let mut stmt = conn.prepare(
        "SELECT DISTINCT COALESCE(original_payee, payee) FROM transactions WHERE payee IS NOT NULL",
    )?;
    let payees: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .collect();

    let total = payees.len();
    let mut diffs = Vec::new();
    let mut feature_diffs = 0usize;

    for payee in &payees {
        let a = strip_metadata(payee);
        let b = strip_metadata_suffix_only(payee);

        if a.stripped != b.stripped {
            diffs.push((payee.clone(), a.stripped.clone(), b.stripped.clone()));
        }
        if features_differ(&a.features, &b.features) {
            feature_diffs += 1;
        }
    }

    println!("=== Normalise Check Report ===");
    println!("Total distinct payees: {total}");
    println!();
    println!("Stripped string diffs (current vs suffix-only): {}", diffs.len());
    println!("Feature diffs (current vs suffix-only): {feature_diffs}");

    if !diffs.is_empty() {
        println!("\n--- First 20 diffs ---");
        for (payee, a, b) in diffs.iter().take(20) {
            println!("  payee: {payee}");
            println!("    current:     {a}");
            println!("    suffix-only: {b}");
        }
    }

    Ok(())
}

fn features_differ(a: &Features, b: &Features) -> bool {
    a.date != b.date
        || a.account_ref != b.account_ref
        || a.location != b.location
        || a.payment_gateway != b.payment_gateway
        || a.foreign_currency != b.foreign_currency
        || a.foreign_amount != b.foreign_amount
}
