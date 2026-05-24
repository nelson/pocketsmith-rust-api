//! Generic helpers shared by the transfers and normalise serve tabs.
//!
//! Both tabs maintain a list of items, a session-local map of
//! `Decision`s keyed by row identifier, and an "active" pointer that
//! moves to the next visible row after each action. The functions here
//! capture that shared shape so neither tab has to duplicate it.

use std::collections::HashMap;
use std::hash::Hash;

use crate::state::Decision;

/// Find the item in `items` for which `is_current` returns true and
/// return the next item in the slice — or stay on the tail if the match
/// is the last item. Returns `None` if no item matches.
///
/// Predicate-based so callers don't have to expose a comparable key
/// (transfer pairs have a composite (txn_id_a, txn_id_b) key that is
/// awkward to materialise; normalise rows have a single `slug` field).
pub fn next_after<'a, T, P>(items: &'a [T], is_current: P) -> Option<&'a T>
where
    P: Fn(&T) -> bool,
{
    let idx = items.iter().position(is_current)?;
    items.get((idx + 1).min(items.len().saturating_sub(1)))
}

/// Count decisions of a given kind in a session decision map. Generic so
/// both tabs (keyed by `(i64, i64)` and `String`) share one implementation.
pub fn count_decisions<K: Eq + Hash>(decisions: &HashMap<K, Decision>, d: Decision) -> usize {
    decisions.values().filter(|v| **v == d).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_after_steps_through_then_stays_on_tail() {
        let xs = ["a", "b", "c"];
        assert_eq!(next_after(&xs, |s| *s == "a"), Some(&"b"));
        assert_eq!(next_after(&xs, |s| *s == "b"), Some(&"c"));
        // Tail stays on tail.
        assert_eq!(next_after(&xs, |s| *s == "c"), Some(&"c"));
    }

    #[test]
    fn next_after_returns_none_for_missing_or_empty() {
        let xs = ["a", "b"];
        assert_eq!(next_after(&xs, |s| *s == "z"), None);
        let empty: [&str; 0] = [];
        assert_eq!(next_after(&empty, |s| *s == "a"), None);
    }

    #[test]
    fn count_decisions_filters_by_decision() {
        let mut m: HashMap<i32, Decision> = HashMap::new();
        m.insert(1, Decision::Confirm);
        m.insert(2, Decision::Confirm);
        m.insert(3, Decision::Reject);
        m.insert(4, Decision::Skip);
        assert_eq!(count_decisions(&m, Decision::Confirm), 2);
        assert_eq!(count_decisions(&m, Decision::Reject), 1);
        assert_eq!(count_decisions(&m, Decision::Skip), 1);
    }
}
