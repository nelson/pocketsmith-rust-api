use std::collections::HashMap;
use std::env;
use std::io::{self, BufRead, Read, Write};
use std::os::fd::AsRawFd;

use anyhow::{bail, Result};

use pocketsmith_sync::db;
use pocketsmith_sync::normalise::{self, PayeeClass};

fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let args: Vec<String> = env::args().collect();

    if args.iter().any(|a| a == "--review") {
        return review_mode(&args);
    }
    if args.iter().any(|a| a == "--apply") {
        return apply_mode();
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
        _ => bail!("--review requires a number (e.g. --review 10)"),
    }
}

fn review_mode(args: &[String]) -> Result<()> {
    let limit = parse_review_limit(args)?;
    let conn = db::initialize("pocketsmith.db")?;
    let payees = load_payees(&conn)?;

    let mut groups: Vec<(normalise::NormalisationResult, usize, Vec<String>)> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for (payee, count) in &payees {
        let result = normalise::normalise(payee);
        if result.class == PayeeClass::Unclassified {
            if let Some(&idx) = seen.get(&result.normalised) {
                groups[idx].1 += count;
                if groups[idx].2.len() < 3 {
                    groups[idx].2.push(payee.clone());
                }
            } else {
                seen.insert(result.normalised.clone(), groups.len());
                groups.push((result, *count, vec![payee.clone()]));
            }
        }
    }

    groups.sort_by(|a, b| b.1.cmp(&a.1));
    let groups: Vec<_> = groups.into_iter().take(limit).collect();

    if groups.is_empty() {
        println!("No unclassified payees to review.");
        return Ok(());
    }

    let total = groups.len();
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    let mut skipped = 0usize;

    let is_tty = unsafe { libc::isatty(io::stdin().as_raw_fd()) } != 0;
    let _raw_guard = if is_tty { Some(RawModeGuard::enter()?) } else { None };
    let mut lines = if !is_tty { Some(io::stdin().lock().lines()) } else { None };

    for (i, (result, count, samples)) in groups.iter().enumerate() {
        let prompt = format_review_prompt(i + 1, total, result, *count, samples);
        print!("{prompt}");
        io::stdout().flush().ok();

        let key = if is_tty {
            match read_key() {
                Some(k) => { println!("{k}"); k }
                None => break,
            }
        } else {
            match lines.as_mut().unwrap().next() {
                Some(Ok(line)) => { let ch = line.chars().next().unwrap_or('y'); println!("{ch}"); ch }
                _ => break,
            }
        };

        match key {
            'y' | '\n' | '\r' => accepted += 1,
            'n' => rejected += 1,
            'q' => break,
            _ => skipped += 1,
        }
    }

    drop(_raw_guard);
    println!("\nReview: {accepted} accepted, {rejected} rejected, {skipped} skipped");
    Ok(())
}

fn apply_mode() -> Result<()> {
    let conn = db::initialize("pocketsmith.db")?;
    let payees = load_payees(&conn)?;
    let mut update_count = 0usize;

    db::with_transaction_change_log(&conn, "normalise", |conn| {
        for (original_payee, _) in &payees {
            let result = normalise::normalise(original_payee);
            if result.class != PayeeClass::Unclassified {
                conn.execute(
                    "UPDATE transactions SET payee = ?1 WHERE COALESCE(original_payee, payee, '') = ?2 AND payee != ?1",
                    rusqlite::params![result.normalised, original_payee],
                )?;
                update_count += conn.changes() as usize;
            }
        }
        Ok(())
    })?;

    println!("Updated {update_count} transactions with normalised payee names.");
    Ok(())
}

fn format_review_prompt(
    index: usize,
    total: usize,
    result: &normalise::NormalisationResult,
    count: usize,
    samples: &[String],
) -> String {
    let mut s = format!("[{index}/{total}] ({count}x) {:?}\n  Normalised: {}\n", result.class, result.normalised);
    for sample in samples.iter().take(3) {
        s.push_str(&format!("  Original:   {sample}\n"));
    }
    if let Some(ref gw) = result.features.payment_gateway {
        s.push_str(&format!("  Gateway:    {gw}\n"));
    }
    if let Some(ref loc) = result.features.location {
        s.push_str(&format!("  Location:   {loc}\n"));
    }
    s.push_str("  [y]es [n]o [s]kip [q]uit > ");
    s
}

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
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
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
        unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, &self.original); }
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

    #[test]
    fn test_format_review_prompt() {
        let result = normalise::normalise("WOOLWORTHS 1624 STRATHF, Card xx9172 Value Date: 01/01/2026");
        let prompt = format_review_prompt(1, 10, &result, 5, &["WOOLWORTHS 1624 STRATHF".into()]);
        assert!(prompt.contains("[1/10]"));
        assert!(prompt.contains("(5x)"));
    }

    #[test]
    fn test_apply_updates_transactions() {
        use pocketsmith_sync::db::{
            upsert_transaction, upsert_transaction_account, with_transaction_change_log,
        };
        use pocketsmith_sync::models::{Transaction, TransactionAccount};

        let conn = pocketsmith_sync::db::initialize_in_memory().unwrap();
        let acct = TransactionAccount {
            id: 100, name: Some("Savings".into()), number: None,
            currency_code: None, account_type: None, current_balance: None,
            current_balance_date: None, current_balance_in_base_currency: None,
            current_balance_exchange_rate: None, safe_balance: None,
            safe_balance_in_base_currency: None, starting_balance: None,
            starting_balance_date: None, created_at: None, updated_at: None,
        };
        upsert_transaction_account(&conn, &acct).unwrap();

        with_transaction_change_log(&conn, "test", |conn| {
            let t1 = Transaction {
                id: 1, transaction_type: None,
                payee: Some("WOOLWORTHS 1624 STRATHF".into()),
                amount: Some(-50.0), amount_in_base_currency: None,
                date: Some("2026-03-01".into()), cheque_number: None,
                memo: None, is_transfer: Some(false), category: None,
                note: None, labels: None,
                original_payee: Some("WOOLWORTHS 1624 STRATHF".into()),
                upload_source: None, closing_balance: None,
                transaction_account: Some(acct.clone()),
                status: None, needs_review: None, created_at: None, updated_at: None,
            };
            upsert_transaction(conn, &t1)?;
            Ok(())
        }).unwrap();

        with_transaction_change_log(&conn, "normalise", |conn| {
            let result = normalise::normalise("WOOLWORTHS 1624 STRATHF");
            conn.execute(
                "UPDATE transactions SET payee = ?1 WHERE COALESCE(original_payee, payee, '') = ?2",
                rusqlite::params![result.normalised, "WOOLWORTHS 1624 STRATHF"],
            )?;
            Ok(())
        }).unwrap();

        let payee: String = conn
            .query_row("SELECT payee FROM transactions WHERE id = 1", [], |row| row.get(0))
            .unwrap();
        assert!(payee.contains("Woolworths"), "got: {payee}");
    }
}
