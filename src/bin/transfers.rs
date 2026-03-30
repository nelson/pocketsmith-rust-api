use std::env;
use std::io::{self, BufRead, Write};

use anyhow::{bail, Result};

use pocketsmith_sync::db;
use pocketsmith_sync::db::transfer_pairs::{self, TransferPairRow};
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

    print_status_summary(&conn)?;
    Ok(())
}

fn review_mode(args: &[String]) -> Result<()> {
    let limit = parse_review_limit(args)?;
    let conn = db::initialize("pocketsmith.db")?;

    let pairs = transfer_pairs::get_pending_pairs(&conn, limit)?;
    if pairs.is_empty() {
        println!("No pending pairs to review.");
        return Ok(());
    }

    let total = pairs.len();
    let mut confirmed = 0usize;
    let mut rejected = 0usize;
    let mut skipped = 0usize;

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    for (i, pair) in pairs.iter().enumerate() {
        print_review_prompt(i + 1, total, pair);

        let response = match lines.next() {
            Some(Ok(line)) => line.trim().to_lowercase(),
            _ => break,
        };

        match response.as_str() {
            "y" | "yes" => {
                transfer_pairs::update_status(
                    &conn,
                    pair.txn_id_a,
                    pair.txn_id_b,
                    Status::Confirmed,
                )?;
                confirmed += 1;
            }
            "n" | "no" => {
                transfer_pairs::update_status(
                    &conn,
                    pair.txn_id_a,
                    pair.txn_id_b,
                    Status::Rejected,
                )?;
                rejected += 1;
            }
            "q" | "quit" => break,
            _ => {
                skipped += 1;
            }
        }
    }

    println!("\nReview session: {confirmed} confirmed, {rejected} rejected, {skipped} skipped");
    print_status_summary(&conn)?;
    Ok(())
}

fn apply_mode() -> Result<()> {
    // Placeholder — implemented in stage 9
    println!("Apply mode not yet implemented.");
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
        _ => bail!("--review requires a number (e.g. --review 10)"),
    }
}

pub fn format_review_prompt(index: usize, total: usize, pair: &TransferPairRow) -> String {
    let dollars = pair.amount_cents.abs() as f64 / 100.0;
    let confidence = pair.confidence.as_str().to_uppercase();
    let amount_str = format_dollars(dollars);
    format!(
        "[{index}/{total}] {amount_str} ({} -> {}) {confidence}\n  A: {:<40} (acct: {})\n  B: {:<40} (acct: {})\n  [y]es [n]o [s]kip [q]uit > ",
        pair.date_a,
        pair.date_b,
        pair.payee_a,
        pair.account_name_a,
        pair.payee_b,
        pair.account_name_b,
    )
}

fn format_dollars(amount: f64) -> String {
    let cents = (amount * 100.0).round() as i64;
    let whole = cents / 100;
    let frac = (cents % 100).abs();

    // Add thousands separators
    let whole_str = whole.to_string();
    let mut result = String::new();
    for (i, c) in whole_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 && c != '-' {
            result.push(',');
        }
        result.push(c);
    }
    let whole_formatted: String = result.chars().rev().collect();
    format!("${whole_formatted}.{frac:02}")
}

fn print_review_prompt(index: usize, total: usize, pair: &TransferPairRow) {
    let prompt = format_review_prompt(index, total, pair);
    print!("{prompt}");
    io::stdout().flush().ok();
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

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::transfers::Confidence;

    #[test]
    fn test_format_review_prompt() {
        let pair = TransferPairRow {
            txn_id_a: 1,
            txn_id_b: 2,
            amount_cents: 100000,
            confidence: Confidence::High,
            status: Status::Pending,
            date_a: "2026-03-03".into(),
            date_b: "2026-03-03".into(),
            payee_a: "Transfer to xx8005 CommBank app".into(),
            payee_b: "Transfer from xx8820 CommBank app".into(),
            account_name_a: "Savings".into(),
            account_name_b: "Everyday".into(),
        };
        let output = format_review_prompt(1, 10, &pair);
        assert!(output.contains("[1/10]"));
        assert!(output.contains("$1,000.00"));
        assert!(output.contains("HIGH"));
        assert!(output.contains("Transfer to xx8005"));
        assert!(output.contains("Savings"));
        assert!(output.contains("Everyday"));
    }

    #[test]
    fn test_parse_review_limit() {
        let args = vec![
            "transfers".into(),
            "--review".into(),
            "10".into(),
        ];
        assert_eq!(parse_review_limit(&args).unwrap(), 10);
    }

    #[test]
    fn test_parse_review_limit_missing_number() {
        let args = vec!["transfers".into(), "--review".into()];
        assert!(parse_review_limit(&args).is_err());
    }
}
