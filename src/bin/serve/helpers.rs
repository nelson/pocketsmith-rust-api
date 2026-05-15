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
}
