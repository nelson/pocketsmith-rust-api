//! Helpers specific to the `/transfers/*` tab. Mirrors the structure of
//! `crate::normalise::helpers`.

use std::collections::HashMap;

use pocketsmith_sync::db::transfer_pairs::{self, TransferPairRow};
use pocketsmith_sync::review::Status;
use pocketsmith_sync::transfers::{self, Confidence};

use crate::state::Decision;

pub fn confidence_class(c: &Confidence) -> &'static str {
    match c {
        Confidence::High => "conf-high",
        Confidence::Medium => "conf-med",
        Confidence::Low => "conf-low",
    }
}

pub fn confidence_reason(pair: &TransferPairRow) -> &'static str {
    let a_like = transfers::is_transfer_like(&pair.payee_a);
    let b_like = transfers::is_transfer_like(&pair.payee_b);
    match (a_like, b_like) {
        (true, true) => "Both payees match transfer patterns",
        (true, false) => "Only payee A matches a transfer pattern",
        (false, true) => "Only payee B matches a transfer pattern",
        (false, false) => "Neither payee matches transfer patterns (amount/date/account match only)",
    }
}

pub fn get_prior_pairs(
    conn: &rusqlite::Connection,
    account_a: &str,
    account_b: &str,
) -> Vec<(String, i64, Status)> {
    let sql = "
        SELECT ta.date, tp.amount_cents, tp.status
        FROM transfer_pairs tp
        JOIN transactions ta ON ta.id = tp.txn_id_a
        LEFT JOIN transaction_accounts aa ON aa.id = ta.transaction_account_id
        LEFT JOIN transactions tb ON tb.id = tp.txn_id_b
        LEFT JOIN transaction_accounts ab ON ab.id = tb.transaction_account_id
        WHERE tp.status != 0
          AND ((aa.name = ?1 AND ab.name = ?2) OR (aa.name = ?2 AND ab.name = ?1))
        ORDER BY ta.date DESC
        LIMIT 5
    ";
    conn.prepare(sql)
        .ok()
        .map(|mut stmt| {
            stmt.query_map(rusqlite::params![account_a, account_b], |row| {
                let status_int: i32 = row.get(2)?;
                Ok((row.get(0)?, row.get(1)?, Status::from_i32(status_int).unwrap_or(Status::Pending)))
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
        })
        .unwrap_or_default()
}

pub fn get_filtered_pairs(conn: &rusqlite::Connection, status_filter: &str, confidence_filter: &str, decisions: &HashMap<(i64, i64), Decision>) -> Vec<TransferPairRow> {
    let pairs = match status_filter {
        "all" | "skipped" => transfer_pairs::get_all_pairs(conn, 2000).unwrap_or_default(),
        "pending" => transfer_pairs::get_pairs_by_status(conn, Status::Pending, 2000).unwrap_or_default(),
        "confirmed" => transfer_pairs::get_pairs_by_status(conn, Status::Confirmed, 2000).unwrap_or_default(),
        "rejected" => transfer_pairs::get_pairs_by_status(conn, Status::Rejected, 2000).unwrap_or_default(),
        _ => Vec::new(),
    };
    pairs.into_iter()
        .filter(|p| {
            let key = (p.txn_id_a, p.txn_id_b);
            if status_filter == "skipped" {
                return decisions.get(&key) == Some(&Decision::Skip);
            }
            if status_filter == "pending" && decisions.get(&key) == Some(&Decision::Skip) {
                return false;
            }
            if confidence_filter != "all" && p.confidence.as_str() != confidence_filter {
                return false;
            }
            true
        })
        .collect()
}

/// Count of rows in `transfer_pairs` with status = Confirmed. This is the
/// number of pairs that an "Apply all changes" click would touch, because
/// apply_confirmed reads from the DB (not from in-memory decisions). Used
/// by the activity bar to label/disable the Apply button.
pub fn count_confirmed_in_db(conn: &rusqlite::Connection) -> usize {
    conn.query_row(
        "SELECT COUNT(*) FROM transfer_pairs WHERE status = ?1",
        rusqlite::params![Status::Confirmed.to_i32()],
        |r| r.get::<_, i64>(0).map(|n| n as usize),
    )
    .unwrap_or(0)
}

/// Return the (a,b) ids of pairs in `pairs` that a bulk confirm/reject action
/// should touch. Excludes pairs the user has explicitly skipped this session
/// (Skip is the user's "don't act on this for now" signal). Pairs that already
/// have a Confirm or Reject decision are included -- bulk operations are
/// idempotent at the DB level and the user has chosen this view deliberately.
pub fn pairs_eligible_for_bulk(
    pairs: &[TransferPairRow],
    decisions: &HashMap<(i64, i64), Decision>,
) -> Vec<(i64, i64)> {
    pairs
        .iter()
        .filter_map(|p| {
            let id = (p.txn_id_a, p.txn_id_b);
            if decisions.get(&id) == Some(&Decision::Skip) {
                return None;
            }
            Some(id)
        })
        .collect()
}

/// Derive the effective decision for a pair, combining in-memory session
/// decisions with the DB-persisted status. In-memory wins; falls back to DB
/// status (Confirmed -> Some(Confirm), Rejected -> Some(Reject),
/// Pending -> None).
pub fn derive_decision(
    pair: &TransferPairRow,
    decisions: &HashMap<(i64, i64), Decision>,
) -> Option<Decision> {
    if let Some(d) = decisions.get(&(pair.txn_id_a, pair.txn_id_b)) {
        return Some(*d);
    }
    match pair.status {
        Status::Confirmed => Some(Decision::Confirm),
        Status::Rejected => Some(Decision::Reject),
        Status::Pending => None,
    }
}

/// Parse `"<a>-<b>"` into `(a, b)`. Used by the route table to turn
/// the slug portion of a `/transfers/pair/<a>-<b>/<verb>` URL into a
/// typed pair identifier.
pub fn parse_pair_id(id_str: &str) -> Option<(i64, i64)> {
    let mut parts = id_str.split('-');
    let a: i64 = parts.next()?.parse().ok()?;
    let b: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((a, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pair(id_a: i64, id_b: i64, status: Status, confidence: Confidence) -> TransferPairRow {
        TransferPairRow {
            txn_id_a: id_a,
            txn_id_b: id_b,
            amount_cents: 1000,
            confidence,
            status,
            date_a: "2024-01-01".to_string(),
            date_b: "2024-01-02".to_string(),
            payee_a: "A".to_string(),
            payee_b: "B".to_string(),
            account_name_a: "Acc1".to_string(),
            account_name_b: "Acc2".to_string(),
        }
    }

    fn sample_pairs() -> Vec<TransferPairRow> {
        vec![
            make_pair(1, 2, Status::Pending, Confidence::High),
            make_pair(3, 4, Status::Pending, Confidence::Medium),
            make_pair(5, 6, Status::Pending, Confidence::Low),
            make_pair(7, 8, Status::Pending, Confidence::High),
            make_pair(9, 10, Status::Pending, Confidence::Medium),
        ]
    }

    #[test]
    fn filter_change_keeps_active_if_present() {
        let pairs = sample_pairs();
        let active = Some((5, 6));
        let in_list = pairs.iter().any(|p| Some((p.txn_id_a, p.txn_id_b)) == active);
        assert!(in_list);
    }

    #[test]
    fn filter_change_resets_to_first_if_active_absent() {
        let pairs = sample_pairs();
        let active = Some((99, 100));
        let in_list = pairs.iter().any(|p| Some((p.txn_id_a, p.txn_id_b)) == active);
        assert!(!in_list);
        let new_active = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
        assert_eq!(new_active, Some((1, 2)));
    }

    #[test]
    fn filter_change_empty_list_gives_none() {
        let pairs: Vec<TransferPairRow> = vec![];
        let new_active = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
        assert_eq!(new_active, None);
    }

    #[test]
    fn parse_pair_id_valid() {
        assert_eq!(parse_pair_id("123-456"), Some((123, 456)));
        assert_eq!(parse_pair_id("1-2"), Some((1, 2)));
    }

    #[test]
    fn parse_pair_id_malformed() {
        assert_eq!(parse_pair_id(""), None);
        assert_eq!(parse_pair_id("abc-def"), None);
        assert_eq!(parse_pair_id("123"), None);
        assert_eq!(parse_pair_id("123-"), None);
        assert_eq!(parse_pair_id("1-2-3"), None);
    }

    #[test]
    fn confidence_class_all_variants() {
        assert_eq!(confidence_class(&Confidence::High), "conf-high");
        assert_eq!(confidence_class(&Confidence::Medium), "conf-med");
        assert_eq!(confidence_class(&Confidence::Low), "conf-low");
    }

    #[test]
    fn confidence_reason_both_transfer_like() {
        let mut pair = make_pair(1, 2, Status::Pending, Confidence::High);
        pair.payee_a = "Transfer to Savings".to_string();
        pair.payee_b = "Transfer from Checking".to_string();
        assert_eq!(confidence_reason(&pair), "Both payees match transfer patterns");
    }

    #[test]
    fn confidence_reason_neither_transfer_like() {
        let mut pair = make_pair(1, 2, Status::Pending, Confidence::Low);
        pair.payee_a = "Walmart".to_string();
        pair.payee_b = "Target".to_string();
        assert_eq!(confidence_reason(&pair), "Neither payee matches transfer patterns (amount/date/account match only)");
    }

    #[test]
    fn get_filtered_pairs_pending_excludes_skipped() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        setup_test_db(&conn);
        insert_test_pair(&conn, 1, 2, 1000, Status::Pending, Confidence::High);
        insert_test_pair(&conn, 3, 4, 2000, Status::Pending, Confidence::Medium);

        let mut decisions = HashMap::new();
        decisions.insert((1, 2), Decision::Skip);

        let pairs = get_filtered_pairs(&conn, "pending", "all", &decisions);
        assert_eq!(pairs.len(), 1);
        assert_eq!((pairs[0].txn_id_a, pairs[0].txn_id_b), (3, 4));
    }

    #[test]
    fn get_filtered_pairs_skipped_filter_shows_only_skipped() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        setup_test_db(&conn);
        insert_test_pair(&conn, 1, 2, 1000, Status::Pending, Confidence::High);
        insert_test_pair(&conn, 3, 4, 2000, Status::Pending, Confidence::Medium);

        let mut decisions = HashMap::new();
        decisions.insert((1, 2), Decision::Skip);

        let pairs = get_filtered_pairs(&conn, "skipped", "all", &decisions);
        assert_eq!(pairs.len(), 1);
        assert_eq!((pairs[0].txn_id_a, pairs[0].txn_id_b), (1, 2));
    }

    #[test]
    fn get_filtered_pairs_confidence_filter() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        setup_test_db(&conn);
        insert_test_pair(&conn, 1, 2, 1000, Status::Pending, Confidence::High);
        insert_test_pair(&conn, 3, 4, 2000, Status::Pending, Confidence::Medium);
        insert_test_pair(&conn, 5, 6, 3000, Status::Pending, Confidence::Low);

        let decisions = HashMap::new();

        let high = get_filtered_pairs(&conn, "pending", "high", &decisions);
        assert_eq!(high.len(), 1);

        let medium = get_filtered_pairs(&conn, "pending", "medium", &decisions);
        assert_eq!(medium.len(), 1);

        let low = get_filtered_pairs(&conn, "pending", "low", &decisions);
        assert_eq!(low.len(), 1);

        let all = get_filtered_pairs(&conn, "pending", "all", &decisions);
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn get_filtered_pairs_all_filter_shows_all_statuses() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        setup_test_db(&conn);
        insert_test_pair(&conn, 1, 2, 1000, Status::Pending, Confidence::High);
        insert_test_pair(&conn, 3, 4, 2000, Status::Confirmed, Confidence::Medium);
        insert_test_pair(&conn, 5, 6, 3000, Status::Rejected, Confidence::Low);

        let decisions = HashMap::new();
        let all = get_filtered_pairs(&conn, "all", "all", &decisions);
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn get_filtered_pairs_confirmed_filter() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        setup_test_db(&conn);
        insert_test_pair(&conn, 1, 2, 1000, Status::Pending, Confidence::High);
        insert_test_pair(&conn, 3, 4, 2000, Status::Confirmed, Confidence::Medium);

        let decisions = HashMap::new();
        let confirmed = get_filtered_pairs(&conn, "confirmed", "all", &decisions);
        assert_eq!(confirmed.len(), 1);
        assert_eq!((confirmed[0].txn_id_a, confirmed[0].txn_id_b), (3, 4));
    }

    #[test]
    fn get_filtered_pairs_rejected_filter() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        setup_test_db(&conn);
        insert_test_pair(&conn, 1, 2, 1000, Status::Pending, Confidence::High);
        insert_test_pair(&conn, 3, 4, 2000, Status::Rejected, Confidence::Medium);

        let decisions = HashMap::new();
        let rejected = get_filtered_pairs(&conn, "rejected", "all", &decisions);
        assert_eq!(rejected.len(), 1);
        assert_eq!((rejected[0].txn_id_a, rejected[0].txn_id_b), (3, 4));
    }

    #[test]
    fn get_filtered_pairs_unknown_filter_returns_empty() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        setup_test_db(&conn);
        insert_test_pair(&conn, 1, 2, 1000, Status::Pending, Confidence::High);

        let decisions = HashMap::new();
        let result = get_filtered_pairs(&conn, "nonexistent", "all", &decisions);
        assert!(result.is_empty());
    }

    #[test]
    fn pairs_eligible_for_bulk_excludes_session_skipped() {
        let pairs = sample_pairs();
        let mut decisions = HashMap::new();
        decisions.insert((3, 4), Decision::Skip);
        let eligible = pairs_eligible_for_bulk(&pairs, &decisions);
        assert_eq!(eligible, vec![(1, 2), (5, 6), (7, 8), (9, 10)]);
    }

    #[test]
    fn pairs_eligible_for_bulk_includes_already_confirmed() {
        let pairs = vec![
            make_pair(1, 2, Status::Pending, Confidence::High),
            make_pair(3, 4, Status::Confirmed, Confidence::High),
        ];
        let decisions = HashMap::new();
        let eligible = pairs_eligible_for_bulk(&pairs, &decisions);
        assert_eq!(eligible, vec![(1, 2), (3, 4)]);
    }

    #[test]
    fn pairs_eligible_for_bulk_empty_when_all_skipped() {
        let pairs = sample_pairs();
        let mut decisions = HashMap::new();
        for p in &pairs {
            decisions.insert((p.txn_id_a, p.txn_id_b), Decision::Skip);
        }
        let eligible = pairs_eligible_for_bulk(&pairs, &decisions);
        assert!(eligible.is_empty());
    }

    #[test]
    fn derive_decision_uses_in_memory_when_present() {
        let pair = make_pair(1, 2, Status::Pending, Confidence::High);
        let mut decisions = HashMap::new();
        decisions.insert((1, 2), Decision::Skip);
        assert_eq!(derive_decision(&pair, &decisions), Some(Decision::Skip));
    }

    #[test]
    fn derive_decision_uses_in_memory_even_if_db_disagrees() {
        let pair = make_pair(1, 2, Status::Confirmed, Confidence::High);
        let mut decisions = HashMap::new();
        decisions.insert((1, 2), Decision::Skip);
        assert_eq!(derive_decision(&pair, &decisions), Some(Decision::Skip));
    }

    #[test]
    fn derive_decision_falls_back_to_db_confirmed() {
        let pair = make_pair(1, 2, Status::Confirmed, Confidence::High);
        let decisions = HashMap::new();
        assert_eq!(derive_decision(&pair, &decisions), Some(Decision::Confirm));
    }

    #[test]
    fn derive_decision_falls_back_to_db_rejected() {
        let pair = make_pair(1, 2, Status::Rejected, Confidence::High);
        let decisions = HashMap::new();
        assert_eq!(derive_decision(&pair, &decisions), Some(Decision::Reject));
    }

    #[test]
    fn derive_decision_pending_no_session_decision_returns_none() {
        let pair = make_pair(1, 2, Status::Pending, Confidence::High);
        let decisions = HashMap::new();
        assert_eq!(derive_decision(&pair, &decisions), None);
    }

    #[test]
    fn decision_css_class_variants() {
        assert_eq!(Decision::Confirm.css_class(), "decided-confirmed");
        assert_eq!(Decision::Reject.css_class(), "decided-rejected");
        assert_eq!(Decision::Skip.css_class(), "decided-skipped");
    }

    fn setup_test_db(conn: &rusqlite::Connection) {
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS transaction_accounts (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS transactions (
                id INTEGER PRIMARY KEY,
                transaction_account_id INTEGER,
                date TEXT,
                payee TEXT,
                original_payee TEXT,
                amount REAL,
                FOREIGN KEY (transaction_account_id) REFERENCES transaction_accounts(id)
            );
            CREATE TABLE IF NOT EXISTS transfer_pairs (
                txn_id_a INTEGER NOT NULL,
                txn_id_b INTEGER NOT NULL,
                amount_cents INTEGER NOT NULL,
                confidence INTEGER NOT NULL DEFAULT 2,
                status INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (txn_id_a, txn_id_b)
            );
        ").unwrap();
    }

    fn insert_test_pair(conn: &rusqlite::Connection, id_a: i64, id_b: i64, amount: i64, status: Status, confidence: Confidence) {
        conn.execute(
            "INSERT INTO transaction_accounts (id, name) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
            rusqlite::params![100 + id_a, format!("Account-{id_a}")],
        ).unwrap();
        conn.execute(
            "INSERT INTO transaction_accounts (id, name) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
            rusqlite::params![100 + id_b, format!("Account-{id_b}")],
        ).unwrap();
        conn.execute(
            "INSERT INTO transactions (id, transaction_account_id, date, payee, original_payee, amount) VALUES (?1, ?2, '2024-01-01', 'Test', 'Test', ?3)",
            rusqlite::params![id_a, 100 + id_a, amount as f64 / 100.0],
        ).unwrap();
        conn.execute(
            "INSERT INTO transactions (id, transaction_account_id, date, payee, original_payee, amount) VALUES (?1, ?2, '2024-01-02', 'Test', 'Test', ?3)",
            rusqlite::params![id_b, 100 + id_b, -(amount as f64 / 100.0)],
        ).unwrap();
        conn.execute(
            "INSERT INTO transfer_pairs (txn_id_a, txn_id_b, amount_cents, confidence, status) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id_a, id_b, amount, confidence.to_i32(), status.to_i32()],
        ).unwrap();
    }
}
