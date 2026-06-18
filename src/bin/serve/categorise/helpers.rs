//! Helpers for the `/categorise/*` tab: the status filter enum and the
//! filtered proposal query (with session decisions overlaid), mirroring
//! the normalise tab's helpers.

use std::collections::HashMap;

use pocketsmith_sync::db::category_proposals::{self as cp, CategoryProposalRow};
use pocketsmith_sync::review::Status;

use crate::state::Decision;

/// Status filter for the categorise queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatStatusFilter {
    All,
    Pending,
    Confirmed,
    Rejected,
    Skipped,
}

impl CatStatusFilter {
    pub const ALL: [CatStatusFilter; 5] = [
        CatStatusFilter::All,
        CatStatusFilter::Pending,
        CatStatusFilter::Confirmed,
        CatStatusFilter::Rejected,
        CatStatusFilter::Skipped,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            CatStatusFilter::All => "all",
            CatStatusFilter::Pending => "pending",
            CatStatusFilter::Confirmed => "confirmed",
            CatStatusFilter::Rejected => "rejected",
            CatStatusFilter::Skipped => "skipped",
        }
    }

    pub fn parse(s: &str) -> CatStatusFilter {
        match s {
            "pending" => CatStatusFilter::Pending,
            "confirmed" => CatStatusFilter::Confirmed,
            "rejected" => CatStatusFilter::Rejected,
            "skipped" => CatStatusFilter::Skipped,
            _ => CatStatusFilter::All,
        }
    }
}

/// Return the proposals matching `status`, with session `decisions`
/// overlaid (a skipped row only appears under the Skipped filter, etc.).
pub fn get_filtered_proposals(
    conn: &rusqlite::Connection,
    status: CatStatusFilter,
    decisions: &HashMap<String, Decision>,
) -> Vec<CategoryProposalRow> {
    let all = cp::list_all(conn).unwrap_or_default();
    all.into_iter()
        .filter(|row| {
            let session = decisions.get(&row.merchant_key).copied();
            match status {
                CatStatusFilter::All => true,
                CatStatusFilter::Skipped => session == Some(Decision::Skip),
                CatStatusFilter::Pending => {
                    session.is_none() && row.status == Status::Pending
                }
                CatStatusFilter::Confirmed => {
                    session == Some(Decision::Confirm)
                        || (session.is_none() && row.status == Status::Confirmed)
                }
                CatStatusFilter::Rejected => {
                    session == Some(Decision::Reject)
                        || (session.is_none() && row.status == Status::Rejected)
                }
            }
        })
        .collect()
}

/// Resolve a category id to its title for display.
pub fn category_title(conn: &rusqlite::Connection, id: Option<i64>) -> Option<String> {
    use rusqlite::OptionalExtension;
    let id = id?;
    conn.query_row(
        "SELECT title FROM categories WHERE id = ?1",
        rusqlite::params![id],
        |r| r.get::<_, Option<String>>(0),
    )
    .optional()
    .ok()
    .flatten()
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::category_proposals::CategoryProposalRow;
    use pocketsmith_sync::db::initialize_in_memory;

    fn seed(conn: &rusqlite::Connection, key: &str, status: Status) {
        cp::upsert(
            conn,
            &CategoryProposalRow {
                merchant_key: key.into(),
                proposed_category: None,
                proposed_labels: vec!["cafe".into()],
                place_type: Some("cafe".into()),
                txn_count: 1,
                status,
            },
        )
        .unwrap();
    }

    #[test]
    fn filter_respects_status_and_session_decisions() {
        let conn = initialize_in_memory().unwrap();
        seed(&conn, "a", Status::Pending);
        seed(&conn, "b", Status::Confirmed);
        let mut decisions = HashMap::new();
        decisions.insert("a".to_string(), Decision::Skip);

        // Pending filter hides the skipped 'a'.
        let pending = get_filtered_proposals(&conn, CatStatusFilter::Pending, &decisions);
        assert!(pending.is_empty());

        // Skipped filter shows only 'a'.
        let skipped = get_filtered_proposals(&conn, CatStatusFilter::Skipped, &decisions);
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].merchant_key, "a");

        // Confirmed filter shows 'b'.
        let confirmed = get_filtered_proposals(&conn, CatStatusFilter::Confirmed, &decisions);
        assert_eq!(confirmed.len(), 1);
        assert_eq!(confirmed[0].merchant_key, "b");

        // All shows both.
        assert_eq!(get_filtered_proposals(&conn, CatStatusFilter::All, &decisions).len(), 2);
    }
}
