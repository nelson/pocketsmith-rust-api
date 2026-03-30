use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::OnceLock;

use anyhow::Result;
use regex::RegexSet;
use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        }
    }

    pub fn from_str(s: &str) -> Option<Confidence> {
        match s {
            "high" => Some(Confidence::High),
            "medium" => Some(Confidence::Medium),
            "low" => Some(Confidence::Low),
            _ => None,
        }
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    Pending,
    Confirmed,
    Rejected,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Confirmed => "confirmed",
            Status::Rejected => "rejected",
        }
    }

    pub fn from_str(s: &str) -> Option<Status> {
        match s {
            "pending" => Some(Status::Pending),
            "confirmed" => Some(Status::Confirmed),
            "rejected" => Some(Status::Rejected),
            _ => None,
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct TransferPair {
    pub txn_id_a: i64,
    pub txn_id_b: i64,
    pub amount_cents: i64,
    pub confidence: Confidence,
    pub status: Status,
}

fn transfer_patterns() -> &'static RegexSet {
    static PATTERNS: OnceLock<RegexSet> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        RegexSet::new([
            r"(?i)^Transfer (to|from)",
            r"(?i)Fast Transfer (From|To)",
            r"(?i)^(PAYMENT TO|PAYMENT FROM|ONLINE PAYMENT)",
            r"(?i)Funds [Tt]ransfer",
            r"(?i)^(to|from) account",
            r"(?i)^(Mortgage|Amex) - Transfer",
            r"(?i)^LOAN PAYMENT$",
            r"(?i)^Repayment/Payment$",
        ])
        .expect("invalid regex patterns")
    })
}

pub fn is_transfer_like(payee: &str) -> bool {
    transfer_patterns().is_match(payee)
}

fn amount_to_cents(amount: f64) -> i64 {
    (amount * 100.0).round() as i64
}

struct TxnRow {
    id: i64,
    amount_cents: i64,
    date: String,
    original_payee: String,
    transaction_account_id: i64,
}

struct Candidate {
    txn_id_a: i64,
    txn_id_b: i64,
    amount_cents: i64,
    confidence: Confidence,
    date_diff: i64,
}

pub fn find_pairs(conn: &Connection) -> Result<Vec<TransferPair>> {
    // Load all transactions
    let mut stmt = conn.prepare(
        "SELECT id, amount, date, COALESCE(original_payee, payee, ''), transaction_account_id
         FROM transactions
         WHERE amount IS NOT NULL AND date IS NOT NULL AND transaction_account_id IS NOT NULL",
    )?;
    let txns: Vec<TxnRow> = stmt
        .query_map([], |row| {
            let amount: f64 = row.get(1)?;
            Ok(TxnRow {
                id: row.get(0)?,
                amount_cents: amount_to_cents(amount),
                date: row.get(2)?,
                original_payee: row.get(3)?,
                transaction_account_id: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Check which txn ids are already paired
    let mut already_paired: HashSet<i64> = HashSet::new();
    let mut paired_stmt =
        conn.prepare("SELECT txn_id_a, txn_id_b FROM transfer_pairs")?;
    let paired_rows = paired_stmt.query_map([], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in paired_rows {
        let (a, b) = row?;
        already_paired.insert(a);
        already_paired.insert(b);
    }

    // Group by abs(amount_cents)
    let mut groups: HashMap<i64, Vec<&TxnRow>> = HashMap::new();
    for txn in &txns {
        if already_paired.contains(&txn.id) {
            continue;
        }
        groups.entry(txn.amount_cents.abs()).or_default().push(txn);
    }

    // Generate candidates
    let mut candidates: Vec<Candidate> = Vec::new();
    for (_amount, group) in &groups {
        for (i, a) in group.iter().enumerate() {
            for b in group.iter().skip(i + 1) {
                // Must be opposite signs
                if (a.amount_cents > 0) == (b.amount_cents > 0) {
                    continue;
                }
                // Must be different accounts
                if a.transaction_account_id == b.transaction_account_id {
                    continue;
                }
                // Check date window
                let date_diff = date_diff_days(&a.date, &b.date);
                if date_diff > 2 {
                    continue;
                }

                let a_like = is_transfer_like(&a.original_payee);
                let b_like = is_transfer_like(&b.original_payee);
                let confidence = match (a_like, b_like) {
                    (true, true) => Confidence::High,
                    (true, false) | (false, true) => Confidence::Medium,
                    (false, false) => Confidence::Low,
                };

                // Convention: txn_id_a is positive amount, or lower id if same sign
                let (id_a, id_b) = if a.amount_cents > 0 {
                    (a.id, b.id)
                } else {
                    (b.id, a.id)
                };

                candidates.push(Candidate {
                    txn_id_a: id_a,
                    txn_id_b: id_b,
                    amount_cents: a.amount_cents.abs(),
                    confidence,
                    date_diff,
                });
            }
        }
    }

    // Sort: high confidence first, then by date_diff ascending
    candidates.sort_by(|a, b| {
        let conf_ord = confidence_rank(a.confidence).cmp(&confidence_rank(b.confidence));
        conf_ord.then(a.date_diff.cmp(&b.date_diff))
    });

    // Greedy assign
    let mut matched: HashSet<i64> = HashSet::new();
    let mut pairs: Vec<TransferPair> = Vec::new();
    for c in &candidates {
        if matched.contains(&c.txn_id_a) || matched.contains(&c.txn_id_b) {
            continue;
        }
        matched.insert(c.txn_id_a);
        matched.insert(c.txn_id_b);
        pairs.push(TransferPair {
            txn_id_a: c.txn_id_a,
            txn_id_b: c.txn_id_b,
            amount_cents: c.amount_cents,
            confidence: c.confidence,
            status: Status::Pending,
        });
    }

    Ok(pairs)
}

fn confidence_rank(c: Confidence) -> u8 {
    match c {
        Confidence::High => 0,
        Confidence::Medium => 1,
        Confidence::Low => 2,
    }
}

fn date_diff_days(a: &str, b: &str) -> i64 {
    // Dates are YYYY-MM-DD format; parse to days since epoch for comparison
    fn parse_days(s: &str) -> Option<i64> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 3 {
            return None;
        }
        let y: i64 = parts[0].parse().ok()?;
        let m: i64 = parts[1].parse().ok()?;
        let d: i64 = parts[2].parse().ok()?;
        // Simple days calculation (not astronomically precise, but fine for diff)
        Some(y * 365 + y / 4 - y / 100 + y / 400 + m * 30 + d)
    }
    match (parse_days(a), parse_days(b)) {
        (Some(da), Some(db)) => (da - db).abs(),
        _ => i64::MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_roundtrip() {
        for c in [Confidence::High, Confidence::Medium, Confidence::Low] {
            assert_eq!(Confidence::from_str(c.as_str()), Some(c));
        }
        assert_eq!(Confidence::from_str("invalid"), None);
    }

    #[test]
    fn test_status_roundtrip() {
        for s in [Status::Pending, Status::Confirmed, Status::Rejected] {
            assert_eq!(Status::from_str(s.as_str()), Some(s));
        }
        assert_eq!(Status::from_str("invalid"), None);
    }

    #[test]
    fn test_transfer_pair_construction() {
        let pair = TransferPair {
            txn_id_a: 1,
            txn_id_b: 2,
            amount_cents: 5000,
            confidence: Confidence::High,
            status: Status::Pending,
        };
        assert_eq!(pair.txn_id_a, 1);
        assert_eq!(pair.txn_id_b, 2);
        assert_eq!(pair.amount_cents, 5000);
        assert_eq!(pair.confidence, Confidence::High);
        assert_eq!(pair.status, Status::Pending);
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", Confidence::High), "high");
        assert_eq!(format!("{}", Status::Confirmed), "confirmed");
    }

    #[test]
    fn test_is_transfer_like_positive() {
        let positives = [
            "Transfer to xx8005 CommBank app",
            "Transfer from xx8820 CommBank app",
            "transfer to savings",
            "Fast Transfer From JOHN SMITH",
            "Fast Transfer To JANE DOE",
            "PAYMENT TO AMEX",
            "PAYMENT FROM SAVINGS",
            "ONLINE PAYMENT",
            "Funds Transfer",
            "Funds transfer",
            "to account 1234",
            "from account 5678",
            "Mortgage - Transfer",
            "Amex - Transfer",
            "LOAN PAYMENT",
            "Repayment/Payment",
        ];
        for payee in positives {
            assert!(is_transfer_like(payee), "should match: {payee}");
        }
    }

    #[test]
    fn test_is_transfer_like_negative() {
        let negatives = [
            "Woolworths",
            "Netflix",
            "SALARY",
            "Amazon.com.au",
            "Transfer Station Fees",
            "Payment received",
        ];
        for payee in negatives {
            assert!(!is_transfer_like(payee), "should NOT match: {payee}");
        }
    }

    #[test]
    fn test_find_pairs_basic() {
        use crate::db::test_helpers::*;
        use crate::db::{upsert_transaction, upsert_transaction_account, with_transaction_change_log};

        let conn = test_db();
        upsert_transaction_account(&conn, &make_transaction_account(100, "Savings")).unwrap();
        upsert_transaction_account(&conn, &make_transaction_account(200, "Everyday")).unwrap();

        with_transaction_change_log(&conn, "test", |conn| {
            let mut t1 = make_transaction(1, "Transfer to xx8005");
            t1.amount = Some(1000.0);
            t1.date = Some("2026-03-01".into());
            t1.original_payee = Some("Transfer to xx8005".into());
            t1.transaction_account = Some(make_transaction_account(100, "Savings"));
            upsert_transaction(conn, &t1)?;

            let mut t2 = make_transaction(2, "Transfer from xx8820");
            t2.amount = Some(-1000.0);
            t2.date = Some("2026-03-01".into());
            t2.original_payee = Some("Transfer from xx8820".into());
            t2.transaction_account = Some(make_transaction_account(200, "Everyday"));
            upsert_transaction(conn, &t2)?;
            Ok(())
        })
        .unwrap();

        let pairs = find_pairs(&conn).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].txn_id_a, 1); // positive amount
        assert_eq!(pairs[0].txn_id_b, 2);
        assert_eq!(pairs[0].amount_cents, 100000);
        assert_eq!(pairs[0].confidence, Confidence::High);
    }

    #[test]
    fn test_find_pairs_respects_2_day_window() {
        use crate::db::test_helpers::*;
        use crate::db::{upsert_transaction, upsert_transaction_account, with_transaction_change_log};

        let conn = test_db();
        upsert_transaction_account(&conn, &make_transaction_account(100, "Savings")).unwrap();
        upsert_transaction_account(&conn, &make_transaction_account(200, "Everyday")).unwrap();

        with_transaction_change_log(&conn, "test", |conn| {
            let mut t1 = make_transaction(1, "Transfer to xx8005");
            t1.amount = Some(500.0);
            t1.date = Some("2026-03-01".into());
            t1.original_payee = Some("Transfer to xx8005".into());
            t1.transaction_account = Some(make_transaction_account(100, "Savings"));
            upsert_transaction(conn, &t1)?;

            let mut t2 = make_transaction(2, "Transfer from xx8820");
            t2.amount = Some(-500.0);
            t2.date = Some("2026-03-10".into()); // 9 days later — too far
            t2.original_payee = Some("Transfer from xx8820".into());
            t2.transaction_account = Some(make_transaction_account(200, "Everyday"));
            upsert_transaction(conn, &t2)?;
            Ok(())
        })
        .unwrap();

        let pairs = find_pairs(&conn).unwrap();
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn test_find_pairs_skips_same_account() {
        use crate::db::test_helpers::*;
        use crate::db::{upsert_transaction, upsert_transaction_account, with_transaction_change_log};

        let conn = test_db();
        upsert_transaction_account(&conn, &make_transaction_account(100, "Savings")).unwrap();

        with_transaction_change_log(&conn, "test", |conn| {
            let mut t1 = make_transaction(1, "Transfer to xx8005");
            t1.amount = Some(500.0);
            t1.date = Some("2026-03-01".into());
            t1.transaction_account = Some(make_transaction_account(100, "Savings"));
            upsert_transaction(conn, &t1)?;

            let mut t2 = make_transaction(2, "Transfer from xx8820");
            t2.amount = Some(-500.0);
            t2.date = Some("2026-03-01".into());
            t2.transaction_account = Some(make_transaction_account(100, "Savings")); // Same account
            upsert_transaction(conn, &t2)?;
            Ok(())
        })
        .unwrap();

        let pairs = find_pairs(&conn).unwrap();
        assert_eq!(pairs.len(), 0);
    }

    #[test]
    fn test_find_pairs_no_double_matching() {
        use crate::db::test_helpers::*;
        use crate::db::{upsert_transaction, upsert_transaction_account, with_transaction_change_log};

        let conn = test_db();
        upsert_transaction_account(&conn, &make_transaction_account(100, "Savings")).unwrap();
        upsert_transaction_account(&conn, &make_transaction_account(200, "Everyday")).unwrap();

        with_transaction_change_log(&conn, "test", |conn| {
            // Three transactions with same amount — only one pair should form
            let mut t1 = make_transaction(1, "Transfer to xx8005");
            t1.amount = Some(500.0);
            t1.date = Some("2026-03-01".into());
            t1.original_payee = Some("Transfer to xx8005".into());
            t1.transaction_account = Some(make_transaction_account(100, "Savings"));
            upsert_transaction(conn, &t1)?;

            let mut t2 = make_transaction(2, "Transfer from xx8820");
            t2.amount = Some(-500.0);
            t2.date = Some("2026-03-01".into());
            t2.original_payee = Some("Transfer from xx8820".into());
            t2.transaction_account = Some(make_transaction_account(200, "Everyday"));
            upsert_transaction(conn, &t2)?;

            let mut t3 = make_transaction(3, "Woolworths");
            t3.amount = Some(-500.0);
            t3.date = Some("2026-03-01".into());
            t3.original_payee = Some("Woolworths".into());
            t3.transaction_account = Some(make_transaction_account(200, "Everyday"));
            upsert_transaction(conn, &t3)?;
            Ok(())
        })
        .unwrap();

        let pairs = find_pairs(&conn).unwrap();
        assert_eq!(pairs.len(), 1);
        // Should prefer the high-confidence pair (both transfer-like)
        assert_eq!(pairs[0].confidence, Confidence::High);
    }

    #[test]
    fn test_find_pairs_confidence_levels() {
        use crate::db::test_helpers::*;
        use crate::db::{upsert_transaction, upsert_transaction_account, with_transaction_change_log};

        let conn = test_db();
        upsert_transaction_account(&conn, &make_transaction_account(100, "Savings")).unwrap();
        upsert_transaction_account(&conn, &make_transaction_account(200, "Everyday")).unwrap();

        with_transaction_change_log(&conn, "test", |conn| {
            // Low confidence pair (neither payee is transfer-like)
            let mut t1 = make_transaction(1, "Woolworths");
            t1.amount = Some(200.0);
            t1.date = Some("2026-03-01".into());
            t1.original_payee = Some("Woolworths".into());
            t1.transaction_account = Some(make_transaction_account(100, "Savings"));
            upsert_transaction(conn, &t1)?;

            let mut t2 = make_transaction(2, "Netflix");
            t2.amount = Some(-200.0);
            t2.date = Some("2026-03-01".into());
            t2.original_payee = Some("Netflix".into());
            t2.transaction_account = Some(make_transaction_account(200, "Everyday"));
            upsert_transaction(conn, &t2)?;

            // Medium confidence pair (one side transfer-like)
            let mut t3 = make_transaction(3, "Transfer to xx1234");
            t3.amount = Some(300.0);
            t3.date = Some("2026-03-01".into());
            t3.original_payee = Some("Transfer to xx1234".into());
            t3.transaction_account = Some(make_transaction_account(100, "Savings"));
            upsert_transaction(conn, &t3)?;

            let mut t4 = make_transaction(4, "Amazon");
            t4.amount = Some(-300.0);
            t4.date = Some("2026-03-01".into());
            t4.original_payee = Some("Amazon".into());
            t4.transaction_account = Some(make_transaction_account(200, "Everyday"));
            upsert_transaction(conn, &t4)?;
            Ok(())
        })
        .unwrap();

        let pairs = find_pairs(&conn).unwrap();
        assert_eq!(pairs.len(), 2);

        let low = pairs.iter().find(|p| p.amount_cents == 20000).unwrap();
        assert_eq!(low.confidence, Confidence::Low);

        let medium = pairs.iter().find(|p| p.amount_cents == 30000).unwrap();
        assert_eq!(medium.confidence, Confidence::Medium);
    }

    #[test]
    fn test_find_pairs_skips_already_paired() {
        use crate::db::test_helpers::*;
        use crate::db::{upsert_transaction, upsert_transaction_account, with_transaction_change_log};
        use crate::db::transfer_pairs::insert_pair;

        let conn = test_db();
        upsert_transaction_account(&conn, &make_transaction_account(100, "Savings")).unwrap();
        upsert_transaction_account(&conn, &make_transaction_account(200, "Everyday")).unwrap();

        with_transaction_change_log(&conn, "test", |conn| {
            let mut t1 = make_transaction(1, "Transfer to xx8005");
            t1.amount = Some(500.0);
            t1.date = Some("2026-03-01".into());
            t1.original_payee = Some("Transfer to xx8005".into());
            t1.transaction_account = Some(make_transaction_account(100, "Savings"));
            upsert_transaction(conn, &t1)?;

            let mut t2 = make_transaction(2, "Transfer from xx8820");
            t2.amount = Some(-500.0);
            t2.date = Some("2026-03-01".into());
            t2.original_payee = Some("Transfer from xx8820".into());
            t2.transaction_account = Some(make_transaction_account(200, "Everyday"));
            upsert_transaction(conn, &t2)?;
            Ok(())
        })
        .unwrap();

        // Pre-insert the pair
        insert_pair(
            &conn,
            &TransferPair {
                txn_id_a: 1,
                txn_id_b: 2,
                amount_cents: 50000,
                confidence: Confidence::High,
                status: Status::Confirmed,
            },
        )
        .unwrap();

        let pairs = find_pairs(&conn).unwrap();
        assert_eq!(pairs.len(), 0);
    }
}
