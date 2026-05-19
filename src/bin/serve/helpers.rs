use std::collections::HashMap;

use pocketsmith_sync::db::transfer_pairs::{self, TransferPairRow};
use pocketsmith_sync::transfers::{self, Confidence, Status};

use crate::state::Decision;

pub fn format_dollars(cents: i64) -> String {
    let abs_cents = cents.abs();
    let whole = abs_cents / 100;
    let frac = abs_cents % 100;
    let whole_str = whole.to_string();
    let mut result = String::new();
    for (i, c) in whole_str.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    let formatted: String = result.chars().rev().collect();
    format!("${formatted}.{frac:02}")
}

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

pub fn format_short_date(date: &str) -> String {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 { return date.to_string(); }
    let month: u8 = parts[1].parse().unwrap_or(0);
    let day: u8 = parts[2].parse().unwrap_or(0);
    let month_name = match month {
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr",
        5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug",
        9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec",
        _ => "???",
    };
    format!("{month_name} {day}")
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

pub fn find_pair_index(pairs: &[TransferPairRow], id: (i64, i64)) -> Option<usize> {
    pairs.iter().position(|p| (p.txn_id_a, p.txn_id_b) == id)
}

pub fn next_pair_after(pairs: &[TransferPairRow], current: (i64, i64)) -> Option<(i64, i64)> {
    let idx = find_pair_index(pairs, current)?;
    let next_idx = if idx + 1 < pairs.len() { idx + 1 } else { idx };
    Some((pairs[next_idx].txn_id_a, pairs[next_idx].txn_id_b))
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

pub fn extract_param(query: &str, key: &str) -> Option<String> {
    query.split('&')
        .find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let k = parts.next()?;
            let v = parts.next()?;
            if k == key { Some(v.to_string()) } else { None }
        })
}

pub fn count_decisions(decisions: &HashMap<(i64, i64), Decision>, d: Decision) -> usize {
    decisions.values().filter(|v| **v == d).count()
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
///
/// Inputs:
///   pairs     -- already filtered by status_filter + confidence_filter
///                (i.e. exactly what the queue shows)
///   decisions -- the in-memory session decisions HashMap
///
/// Output order matches input order so callers can iterate predictably for
/// progress reporting.
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
/// decisions with the DB-persisted status.
///
/// - In-memory decision wins (it's the user's current intent for this session,
///   including Skip which has no DB representation).
/// - Falls back to DB status: Confirmed -> Some(Confirm), Rejected -> Some(Reject),
///   Pending -> None.
///
/// This is what lets the queue UI show ticks on pairs that were confirmed in a
/// previous run (or auto-confirmed by `transfers detect`) without needing a
/// matching in-memory entry. The same code path makes undo work uniformly:
/// the undo button is shown whenever derive_decision returns Confirm/Reject,
/// regardless of which source produced it.
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

pub fn parse_pair_id(path: &str, prefix: &str) -> Option<(i64, i64)> {
    let rest = path.strip_prefix(prefix)?;
    let id_part = rest.split('/').next()?;
    let mut parts = id_part.split('-');
    let a: i64 = parts.next()?.parse().ok()?;
    let b: i64 = parts.next()?.parse().ok()?;
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
    fn find_pair_index_returns_correct_position() {
        let pairs = sample_pairs();
        assert_eq!(find_pair_index(&pairs, (1, 2)), Some(0));
        assert_eq!(find_pair_index(&pairs, (5, 6)), Some(2));
        assert_eq!(find_pair_index(&pairs, (9, 10)), Some(4));
    }

    #[test]
    fn find_pair_index_returns_none_for_missing() {
        let pairs = sample_pairs();
        assert_eq!(find_pair_index(&pairs, (99, 100)), None);
    }

    #[test]
    fn next_pair_after_returns_next_in_list() {
        let pairs = sample_pairs();
        assert_eq!(next_pair_after(&pairs, (1, 2)), Some((3, 4)));
        assert_eq!(next_pair_after(&pairs, (3, 4)), Some((5, 6)));
        assert_eq!(next_pair_after(&pairs, (7, 8)), Some((9, 10)));
    }

    #[test]
    fn next_pair_after_last_stays_on_last() {
        let pairs = sample_pairs();
        assert_eq!(next_pair_after(&pairs, (9, 10)), Some((9, 10)));
    }

    #[test]
    fn next_pair_after_missing_returns_none() {
        let pairs = sample_pairs();
        assert_eq!(next_pair_after(&pairs, (99, 100)), None);
    }

    #[test]
    fn next_pair_after_single_item_stays() {
        let pairs = vec![make_pair(1, 2, Status::Pending, Confidence::High)];
        assert_eq!(next_pair_after(&pairs, (1, 2)), Some((1, 2)));
    }

    #[test]
    fn next_pair_after_empty_list_returns_none() {
        let pairs: Vec<TransferPairRow> = vec![];
        assert_eq!(next_pair_after(&pairs, (1, 2)), None);
    }

    #[test]
    fn action_advances_to_next_not_first() {
        let pairs = sample_pairs();
        let current = (5, 6);
        let next = next_pair_after(&pairs, current);
        assert_eq!(next, Some((7, 8)));
    }

    #[test]
    fn filter_change_keeps_active_if_present() {
        let pairs = sample_pairs();
        let active = Some((5, 6));
        let in_list = active.and_then(|id| find_pair_index(&pairs, id)).is_some();
        assert!(in_list);
    }

    #[test]
    fn filter_change_resets_to_first_if_active_absent() {
        let pairs = sample_pairs();
        let active = Some((99, 100));
        let in_list = active.and_then(|id| find_pair_index(&pairs, id)).is_some();
        assert!(!in_list);
        let new_active = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
        assert_eq!(new_active, Some((1, 2)));
    }

    #[test]
    fn filter_change_empty_list_gives_none() {
        let pairs: Vec<TransferPairRow> = vec![];
        let active = Some((1, 2));
        let in_list = active.and_then(|id| find_pair_index(&pairs, id)).is_some();
        assert!(!in_list);
        let new_active = pairs.first().map(|p| (p.txn_id_a, p.txn_id_b));
        assert_eq!(new_active, None);
    }

    #[test]
    fn arrow_down_from_first_selects_second() {
        let pairs = sample_pairs();
        let current_idx = 0;
        let next_idx = (current_idx + 1).min(pairs.len() - 1);
        assert_eq!(next_idx, 1);
        assert_eq!((pairs[next_idx].txn_id_a, pairs[next_idx].txn_id_b), (3, 4));
    }

    #[test]
    fn arrow_up_from_first_stays_at_first() {
        let current_idx: usize = 0;
        let next_idx = current_idx.saturating_sub(1);
        assert_eq!(next_idx, 0);
    }

    #[test]
    fn arrow_down_from_last_stays_at_last() {
        let pairs = sample_pairs();
        let current_idx = pairs.len() - 1;
        let next_idx = (current_idx + 1).min(pairs.len() - 1);
        assert_eq!(next_idx, 4);
        assert_eq!((pairs[next_idx].txn_id_a, pairs[next_idx].txn_id_b), (9, 10));
    }

    #[test]
    fn click_sets_active_to_clicked_pair() {
        let pairs = sample_pairs();
        let clicked = (7, 8);
        assert!(find_pair_index(&pairs, clicked).is_some());
    }

    #[test]
    fn action_on_last_item_stays_on_last() {
        let pairs = sample_pairs();
        let current = (9, 10);
        let next = next_pair_after(&pairs, current);
        assert_eq!(next, Some((9, 10)));
        // After item removed from new list, fall back to new last
        let new_pairs = &pairs[..4];
        let in_new = find_pair_index(new_pairs, next.unwrap()).is_some();
        assert!(!in_new);
        let fallback = new_pairs.last().map(|p| (p.txn_id_a, p.txn_id_b));
        assert_eq!(fallback, Some((7, 8)));
    }

    #[test]
    fn action_does_not_overflow_past_end() {
        let pairs = vec![
            make_pair(1, 2, Status::Pending, Confidence::High),
            make_pair(3, 4, Status::Pending, Confidence::High),
            make_pair(5, 6, Status::Pending, Confidence::High),
        ];
        let next = next_pair_after(&pairs, (5, 6));
        assert_eq!(next, Some((5, 6)));
    }

    #[test]
    fn action_does_not_loop_back() {
        let pairs = sample_pairs();
        let next = next_pair_after(&pairs, (9, 10));
        assert_eq!(next, Some((9, 10)));
        assert_ne!(next, Some((1, 2)));
    }

    #[test]
    fn navigation_order_matches_sidebar_display_order() {
        let pairs = sample_pairs();
        let order: Vec<(i64, i64)> = pairs.iter().map(|p| (p.txn_id_a, p.txn_id_b)).collect();
        assert_eq!(order, vec![(1, 2), (3, 4), (5, 6), (7, 8), (9, 10)]);

        let mut current = order[0];
        for expected in &order[1..] {
            let next = next_pair_after(&pairs, current).unwrap();
            assert_eq!(next, *expected);
            current = next;
        }
        let next = next_pair_after(&pairs, current).unwrap();
        assert_eq!(next, *order.last().unwrap());
    }

    // --- format_dollars tests ---

    #[test]
    fn format_dollars_positive() {
        assert_eq!(format_dollars(1050), "$10.50");
        assert_eq!(format_dollars(100), "$1.00");
        assert_eq!(format_dollars(1), "$0.01");
        assert_eq!(format_dollars(99), "$0.99");
    }

    #[test]
    fn format_dollars_zero() {
        assert_eq!(format_dollars(0), "$0.00");
    }

    #[test]
    fn format_dollars_negative() {
        assert_eq!(format_dollars(-1050), "$10.50");
        assert_eq!(format_dollars(-1), "$0.01");
    }

    #[test]
    fn format_dollars_large_with_commas() {
        assert_eq!(format_dollars(123456789), "$1,234,567.89");
        assert_eq!(format_dollars(100000), "$1,000.00");
        assert_eq!(format_dollars(10000000), "$100,000.00");
    }

    // --- format_short_date tests ---

    #[test]
    fn format_short_date_valid() {
        assert_eq!(format_short_date("2024-01-15"), "Jan 15");
        assert_eq!(format_short_date("2024-06-01"), "Jun 1");
        assert_eq!(format_short_date("2024-12-31"), "Dec 31");
    }

    #[test]
    fn format_short_date_all_months() {
        let months = [
            ("2024-01-01", "Jan"), ("2024-02-01", "Feb"), ("2024-03-01", "Mar"),
            ("2024-04-01", "Apr"), ("2024-05-01", "May"), ("2024-06-01", "Jun"),
            ("2024-07-01", "Jul"), ("2024-08-01", "Aug"), ("2024-09-01", "Sep"),
            ("2024-10-01", "Oct"), ("2024-11-01", "Nov"), ("2024-12-01", "Dec"),
        ];
        for (input, expected_month) in months {
            let result = format_short_date(input);
            assert!(result.starts_with(expected_month), "expected {expected_month} for {input}, got {result}");
        }
    }

    #[test]
    fn format_short_date_invalid_returns_original() {
        assert_eq!(format_short_date("2024"), "2024");
        assert_eq!(format_short_date(""), "");
    }

    #[test]
    fn format_short_date_non_numeric_parts_produce_fallback() {
        // "not-a-date" splits into 3 parts but parses to month=0, day=0
        assert_eq!(format_short_date("not-a-date"), "??? 0");
    }

    #[test]
    fn format_short_date_invalid_month_shows_question_marks() {
        assert_eq!(format_short_date("2024-13-01"), "??? 1");
        assert_eq!(format_short_date("2024-00-01"), "??? 1");
    }

    // --- parse_pair_id tests ---

    #[test]
    fn parse_pair_id_valid() {
        assert_eq!(parse_pair_id("/pair/123-456", "/pair/"), Some((123, 456)));
        assert_eq!(parse_pair_id("/pair/1-2", "/pair/"), Some((1, 2)));
    }

    #[test]
    fn parse_pair_id_with_trailing_action() {
        assert_eq!(parse_pair_id("/pair/123-456/confirm", "/pair/"), Some((123, 456)));
        assert_eq!(parse_pair_id("/pair/123-456/reject", "/pair/"), Some((123, 456)));
        assert_eq!(parse_pair_id("/pair/123-456/skip", "/pair/"), Some((123, 456)));
        assert_eq!(parse_pair_id("/pair/123-456/undo", "/pair/"), Some((123, 456)));
        assert_eq!(parse_pair_id("/pair/123-456/unskip", "/pair/"), Some((123, 456)));
    }

    #[test]
    fn parse_pair_id_missing_prefix() {
        assert_eq!(parse_pair_id("/other/123-456", "/pair/"), None);
    }

    #[test]
    fn parse_pair_id_malformed() {
        assert_eq!(parse_pair_id("/pair/", "/pair/"), None);
        assert_eq!(parse_pair_id("/pair/abc-def", "/pair/"), None);
        assert_eq!(parse_pair_id("/pair/123", "/pair/"), None);
        assert_eq!(parse_pair_id("/pair/123-", "/pair/"), None);
    }

    // --- extract_param tests ---

    #[test]
    fn extract_param_single() {
        assert_eq!(extract_param("filter=pending", "filter"), Some("pending".to_string()));
    }

    #[test]
    fn extract_param_multiple() {
        assert_eq!(extract_param("filter=pending&conf=high", "filter"), Some("pending".to_string()));
        assert_eq!(extract_param("filter=pending&conf=high", "conf"), Some("high".to_string()));
    }

    #[test]
    fn extract_param_missing_key() {
        assert_eq!(extract_param("filter=pending", "conf"), None);
        assert_eq!(extract_param("", "filter"), None);
    }

    #[test]
    fn extract_param_empty_value() {
        assert_eq!(extract_param("filter=", "filter"), Some("".to_string()));
    }

    // --- confidence_class tests ---

    #[test]
    fn confidence_class_all_variants() {
        assert_eq!(confidence_class(&Confidence::High), "conf-high");
        assert_eq!(confidence_class(&Confidence::Medium), "conf-med");
        assert_eq!(confidence_class(&Confidence::Low), "conf-low");
    }

    // --- confidence_reason tests ---

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

    // --- get_filtered_pairs tests ---

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

    // --- pairs_eligible_for_bulk tests ---

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
        // A bulk action over a filter that includes confirmed pairs should
        // still return them -- the operation is idempotent (re-confirming a
        // confirmed pair is a no-op write), and the user has selected this
        // view deliberately.
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

    // --- derive_decision tests ---

    #[test]
    fn derive_decision_uses_in_memory_when_present() {
        let pair = make_pair(1, 2, Status::Pending, Confidence::High);
        let mut decisions = HashMap::new();
        decisions.insert((1, 2), Decision::Skip);
        assert_eq!(derive_decision(&pair, &decisions), Some(Decision::Skip));
    }

    #[test]
    fn derive_decision_uses_in_memory_even_if_db_disagrees() {
        // DB-confirmed pair with an in-memory skip should still show as Skip.
        // (The in-memory decision is the user's current intent for this session.)
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

    // --- count_decisions tests ---

    #[test]
    fn count_decisions_variants() {
        let mut decisions = HashMap::new();
        decisions.insert((1, 2), Decision::Confirm);
        decisions.insert((3, 4), Decision::Confirm);
        decisions.insert((5, 6), Decision::Reject);
        decisions.insert((7, 8), Decision::Skip);

        assert_eq!(count_decisions(&decisions, Decision::Confirm), 2);
        assert_eq!(count_decisions(&decisions, Decision::Reject), 1);
        assert_eq!(count_decisions(&decisions, Decision::Skip), 1);
    }

    #[test]
    fn count_decisions_empty() {
        let decisions = HashMap::new();
        assert_eq!(count_decisions(&decisions, Decision::Confirm), 0);
    }

    // --- Decision css_class tests ---

    #[test]
    fn decision_css_class_variants() {
        assert_eq!(Decision::Confirm.css_class(), "decided-confirmed");
        assert_eq!(Decision::Reject.css_class(), "decided-rejected");
        assert_eq!(Decision::Skip.css_class(), "decided-skipped");
    }

    // --- test helpers ---

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
