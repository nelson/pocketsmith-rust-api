//! `pocketsmith transfers` — detect / apply / annotate transfer pairs.

use anyhow::{bail, Result};

use pocketsmith::db;
use pocketsmith::db::transfer_pairs;
use pocketsmith::transfers::{self, Status};

pub fn run(args: &[String]) -> Result<()> {
    if args.iter().any(|a| a == "--apply") {
        return apply(args);
    }
    if args.iter().any(|a| a == "--annotate-existing") {
        return annotate_existing(args);
    }
    if args.iter().any(|a| a == "--review") {
        bail!(
            "--review is no longer supported. Use the web UI: \
             `pocketsmith serve` then open http://127.0.0.1:3141"
        );
    }

    detect(args)
}

fn detect(_args: &[String]) -> Result<()> {
    let conn = db::open_app_db()?;

    let pairs = transfers::find_pairs(&conn)?;
    if pairs.is_empty() {
        println!("No new transfer pairs found.");
        return Ok(());
    }

    let mut inserted = 0;
    for pair in pairs {
        transfer_pairs::insert_pair(&conn, &pair)?;
        inserted += 1;
    }

    println!("Inserted {inserted} new transfer pairs (all pending review).");

    print_status_summary(&conn)?;
    Ok(())
}

fn annotate_existing(_args: &[String]) -> Result<()> {
    // One-shot retroactive backfill: append `[paired:<other_id>]` to the
    // memo of every already-applied (is_transfer=1) transfer pair whose
    // memo doesn't already carry the marker. Idempotent across re-runs.
    // After this prints `Annotated N transactions.`, run `push` to send the
    // new memos upstream.
    let conn = db::open_app_db()?;
    let updated = transfers::annotate_existing_pairs(&conn)?;
    println!("Annotated {updated} transactions with paired-marker memos.");
    Ok(())
}

fn apply(_args: &[String]) -> Result<()> {
    let conn = db::open_app_db()?;
    let stats = transfers::apply_confirmed(&conn)?;
    if stats.rows_drained == 0 {
        println!("No confirmed pairs to apply.");
    } else {
        println!(
            "Applied {} pairs ({} transactions updated).",
            stats.rows_drained, stats.transactions_updated
        );
    }
    print_status_summary(&conn)?;
    Ok(())
}

fn print_status_summary(conn: &rusqlite::Connection) -> Result<()> {
    let counts = transfer_pairs::count_by_status(conn)?;
    println!("\nTotal pairs by status:");
    for status in [Status::Pending, Status::Confirmed, Status::Rejected] {
        let n = counts.get(&status).unwrap_or(&0);
        println!("  {status}: {n}");
    }
    Ok(())
}
