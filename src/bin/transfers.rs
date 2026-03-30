use std::env;
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;

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

    let _raw_guard = RawModeGuard::enter()?;

    for (i, pair) in pairs.iter().enumerate() {
        print_review_prompt(i + 1, total, pair);

        let key = match read_key() {
            Some(k) => k,
            None => break,
        };
        // Echo the key and move to next line
        println!("{key}");

        match key {
            'y' => {
                transfer_pairs::update_status(
                    &conn,
                    pair.txn_id_a,
                    pair.txn_id_b,
                    Status::Confirmed,
                )?;
                confirmed += 1;
            }
            'n' => {
                transfer_pairs::update_status(
                    &conn,
                    pair.txn_id_a,
                    pair.txn_id_b,
                    Status::Rejected,
                )?;
                rejected += 1;
            }
            'q' => break,
            _ => {
                skipped += 1;
            }
        }
    }

    drop(_raw_guard);
    println!("\nReview session: {confirmed} confirmed, {rejected} rejected, {skipped} skipped");
    print_status_summary(&conn)?;
    Ok(())
}

fn apply_mode() -> Result<()> {
    let conn = db::initialize("pocketsmith.db")?;

    let pairs = transfer_pairs::get_confirmed_pairs(&conn)?;
    if pairs.is_empty() {
        println!("No confirmed pairs to apply.");
        return Ok(());
    }

    // Look up _Transfer category
    let transfer_category_id: i64 = conn
        .query_row(
            "SELECT id FROM categories WHERE title = '_Transfer' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| anyhow::anyhow!("No '_Transfer' category found in categories table"))?;

    let count = pairs.len();
    db::with_transaction_change_log(&conn, "transfers", |conn| {
        for pair in &pairs {
            conn.execute(
                "UPDATE transactions SET category_id = ?1, is_transfer = 1 WHERE id = ?2",
                rusqlite::params![transfer_category_id, pair.txn_id_a],
            )?;
            conn.execute(
                "UPDATE transactions SET category_id = ?1, is_transfer = 1 WHERE id = ?2",
                rusqlite::params![transfer_category_id, pair.txn_id_b],
            )?;
        }
        Ok(())
    })?;

    println!("Applied {count} pairs ({} transactions updated).", count * 2);
    print_status_summary(&conn)?;
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

/// RAII guard that puts stdin into raw mode and restores on drop.
struct RawModeGuard {
    original: libc::termios,
    fd: i32,
}

impl RawModeGuard {
    fn enter() -> Result<Self> {
        let fd = io::stdin().as_raw_fd();
        let mut original: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut original) } != 0 {
            bail!("failed to get terminal attributes");
        }
        let mut raw = original;
        // Disable canonical mode (line buffering) and echo
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        // Read one byte at a time
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            bail!("failed to set raw mode");
        }
        Ok(Self { original, fd })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        unsafe {
            libc::tcsetattr(self.fd, libc::TCSANOW, &self.original);
        }
    }
}

fn read_key() -> Option<char> {
    let mut buf = [0u8; 1];
    match io::stdin().read_exact(&mut buf) {
        Ok(()) => Some(buf[0] as char),
        Err(_) => None,
    }
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

    #[test]
    fn test_apply_updates_transactions() {
        use pocketsmith_sync::db::{
            upsert_category, upsert_transaction, upsert_transaction_account,
            with_transaction_change_log,
        };
        use pocketsmith_sync::models::{Category, Transaction, TransactionAccount};

        let conn = pocketsmith_sync::db::initialize_in_memory().unwrap();

        // Create _Transfer category
        let transfer_cat = Category {
            id: 999,
            title: Some("_Transfer".into()),
            colour: None,
            children: None,
            parent_id: None,
            is_transfer: Some(true),
            is_bill: Some(false),
            roll_up: Some(false),
            refund_behaviour: None,
            created_at: None,
            updated_at: None,
        };
        upsert_category(&conn, &transfer_cat).unwrap();

        let acct1 = TransactionAccount {
            id: 100,
            name: Some("Savings".into()),
            number: None,
            currency_code: None,
            account_type: None,
            current_balance: None,
            current_balance_date: None,
            current_balance_in_base_currency: None,
            current_balance_exchange_rate: None,
            safe_balance: None,
            safe_balance_in_base_currency: None,
            starting_balance: None,
            starting_balance_date: None,
            created_at: None,
            updated_at: None,
        };
        let acct2 = TransactionAccount { id: 200, name: Some("Everyday".into()), ..acct1.clone() };
        upsert_transaction_account(&conn, &acct1).unwrap();
        upsert_transaction_account(&conn, &acct2).unwrap();

        with_transaction_change_log(&conn, "test", |conn| {
            let t1 = Transaction {
                id: 1,
                transaction_type: None,
                payee: Some("Transfer to xx8005".into()),
                amount: Some(500.0),
                amount_in_base_currency: None,
                date: Some("2026-03-01".into()),
                cheque_number: None,
                memo: None,
                is_transfer: Some(false),
                category: None,
                note: None,
                labels: None,
                original_payee: Some("Transfer to xx8005".into()),
                upload_source: None,
                closing_balance: None,
                transaction_account: Some(acct1.clone()),
                status: None,
                needs_review: None,
                created_at: None,
                updated_at: None,
            };
            upsert_transaction(conn, &t1)?;

            let t2 = Transaction {
                id: 2,
                payee: Some("Transfer from xx8820".into()),
                amount: Some(-500.0),
                date: Some("2026-03-01".into()),
                original_payee: Some("Transfer from xx8820".into()),
                transaction_account: Some(acct2.clone()),
                ..t1.clone()
            };
            upsert_transaction(conn, &t2)?;
            Ok(())
        })
        .unwrap();

        // Insert a confirmed pair
        transfer_pairs::insert_pair(
            &conn,
            &transfers::TransferPair {
                txn_id_a: 1,
                txn_id_b: 2,
                amount_cents: 50000,
                confidence: Confidence::High,
                status: Status::Confirmed,
            },
        )
        .unwrap();

        // Apply
        let transfer_category_id: i64 = conn
            .query_row(
                "SELECT id FROM categories WHERE title = '_Transfer' LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let pairs = transfer_pairs::get_confirmed_pairs(&conn).unwrap();
        assert_eq!(pairs.len(), 1);

        with_transaction_change_log(&conn, "transfers", |conn| {
            for pair in &pairs {
                conn.execute(
                    "UPDATE transactions SET category_id = ?1, is_transfer = 1 WHERE id = ?2",
                    rusqlite::params![transfer_category_id, pair.txn_id_a],
                )?;
                conn.execute(
                    "UPDATE transactions SET category_id = ?1, is_transfer = 1 WHERE id = ?2",
                    rusqlite::params![transfer_category_id, pair.txn_id_b],
                )?;
            }
            Ok(())
        })
        .unwrap();

        // Verify transactions updated
        let cat_id: i64 = conn
            .query_row(
                "SELECT category_id FROM transactions WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(cat_id, 999);

        let is_transfer: bool = conn
            .query_row(
                "SELECT is_transfer FROM transactions WHERE id = 2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(is_transfer);

        // Verify history created
        let (version, _) =
            pocketsmith_sync::db::get_last_change(&conn, "transfers")
                .unwrap()
                .unwrap();
        let history_count: i64 = conn
            .query_row(
                "SELECT transactions_updated FROM _transaction_change_log WHERE version = ?1",
                [version],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history_count, 2);
    }
}
