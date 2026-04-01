use anyhow::Result;

use pocketsmith_sync::db;
use pocketsmith_sync::normalise::{
    strip_metadata, strip_metadata_suffix_only, strip_metadata_unified, Features,
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
    let mut diff_ab = Vec::new();
    let mut diff_ac = Vec::new();
    let mut feature_diff_ab = 0usize;
    let mut feature_diff_ac = 0usize;

    for payee in &payees {
        let a = strip_metadata(payee);
        let b = strip_metadata_suffix_only(payee);
        let c = strip_metadata_unified(payee);

        if a.stripped != b.stripped {
            diff_ab.push((payee.clone(), a.stripped.clone(), b.stripped.clone(), c.stripped.clone()));
        }
        if a.stripped != c.stripped {
            diff_ac.push((payee.clone(), a.stripped.clone(), b.stripped.clone(), c.stripped.clone()));
        }
        if features_differ(&a.features, &b.features) {
            feature_diff_ab += 1;
        }
        if features_differ(&a.features, &c.features) {
            feature_diff_ac += 1;
        }
    }

    println!("=== Normalise Check Report ===");
    println!("Total distinct payees: {total}");
    println!();
    println!("Stripped string diffs (A=current vs B=suffix-only): {}", diff_ab.len());
    println!("Stripped string diffs (A=current vs C=unified):     {}", diff_ac.len());
    println!("Feature diffs (A vs B): {feature_diff_ab}");
    println!("Feature diffs (A vs C): {feature_diff_ac}");

    if !diff_ab.is_empty() {
        println!("\n--- First 20 diffs: A vs B ---");
        for (payee, a, b, c) in diff_ab.iter().take(20) {
            println!("  payee: {payee}");
            println!("    A: {a}");
            println!("    B: {b}");
            println!("    C: {c}");
        }
    }

    if !diff_ac.is_empty() {
        println!("\n--- First 20 diffs: A vs C ---");
        for (payee, a, b, c) in diff_ac.iter().take(20) {
            println!("  payee: {payee}");
            println!("    A: {a}");
            println!("    B: {b}");
            println!("    C: {c}");
        }
    }

    println!("\n--- Summary ---");
    if diff_ac.is_empty() && feature_diff_ac == 0 {
        println!("C (unified) is equivalent to A (current) — safe to replace two-loop with unified.");
    } else if diff_ab.is_empty() && feature_diff_ab == 0 {
        println!("B (suffix-only) is equivalent to A (current) — prefix loop is unnecessary.");
    } else {
        println!("Neither B nor C is equivalent to A — keep current two-loop design.");
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
