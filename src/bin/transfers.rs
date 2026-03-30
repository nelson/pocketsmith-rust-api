use std::env;

use anyhow::Result;

use pocketsmith_sync::db;
use pocketsmith_sync::db::transfer_pairs;
use pocketsmith_sync::transfers::{self, Confidence, Status};

fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--review") {
        return review_mode(&args);
    }
    if args.iter().any(|a| a == "--apply") {
        return apply_mode();
    }

    detect_mode(&args)
}

fn detect_mode(args: &[String]) -> Result<()> {
    let no_auto = args.iter().any(|a| a == "--no-auto");
    let conn = db::initialize("pocketsmith.db")?;

    let pairs = transfers::find_pairs(&conn)?;
    if pairs.is_empty() {
        println!("No new transfer pairs found.");
        return Ok(());
    }

    let mut inserted = 0;
    let mut auto_confirmed = 0;
    for mut pair in pairs {
        if !no_auto && pair.confidence == Confidence::High {
            pair.status = Status::Confirmed;
            auto_confirmed += 1;
        }
        transfer_pairs::insert_pair(&conn, &pair)?;
        inserted += 1;
    }

    println!("Inserted {inserted} new transfer pairs.");
    if auto_confirmed > 0 {
        println!("Auto-confirmed {auto_confirmed} high-confidence pairs.");
    }

    let counts = transfer_pairs::count_by_status(&conn)?;
    println!("\nTotal pairs by status:");
    for status in [Status::Pending, Status::Confirmed, Status::Rejected] {
        let n = counts.get(&status).unwrap_or(&0);
        println!("  {status}: {n}");
    }

    Ok(())
}

fn review_mode(_args: &[String]) -> Result<()> {
    // Placeholder — implemented in stage 8
    println!("Review mode not yet implemented.");
    Ok(())
}

fn apply_mode() -> Result<()> {
    // Placeholder — implemented in stage 9
    println!("Apply mode not yet implemented.");
    Ok(())
}
