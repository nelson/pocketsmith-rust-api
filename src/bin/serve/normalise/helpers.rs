//! Helpers for the `/normalise/*` tab.
//!
//! Mirrors the transfer-side helpers (filter parsing + filtered queries).
//! Session-only state (decisions/skip) lives on `AppState` and is consumed
//! by the handlers — these helpers operate purely against the database
//! and the parsed filter values.

use std::collections::HashMap;

use rusqlite::Connection;

use pocketsmith_sync::db::payee_normalisations::{self as pn, PayeeNormalisationRow};
use pocketsmith_sync::transfers::Status;

use crate::state::Decision;

/// A single transaction row for the "matching transactions" panel on the
/// normalise detail view. Mirrors `transfers::get_prior_pairs` in shape.
#[derive(Debug, Clone)]
pub struct MatchingTxn {
    #[allow(dead_code)] // may be exposed in a future detail-row link
    pub id: i64,
    pub date: String,
    pub payee: Option<String>,
    pub amount_cents: i64,
    pub account_name: Option<String>,
}

/// All transactions whose `original_payee` matches the given value, joined
/// with the account name and ordered by date DESC. Capped at 100 rows so an
/// extremely common payee can't pin the renderer.
pub fn matching_transactions(conn: &Connection, original_payee: &str) -> Vec<MatchingTxn> {
    let mut stmt = match conn.prepare(
        "SELECT t.id, t.date, t.payee, t.amount, ta.name
           FROM transactions t
           LEFT JOIN transaction_accounts ta ON ta.id = t.transaction_account_id
          WHERE t.original_payee = ?1
          ORDER BY t.date DESC, t.id DESC
          LIMIT 100",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let rows = stmt
        .query_map(rusqlite::params![original_payee], |row| {
            let amount: f64 = row.get(3)?;
            Ok(MatchingTxn {
                id: row.get(0)?,
                date: row.get(1)?,
                payee: row.get(2)?,
                amount_cents: (amount * 100.0).round() as i64,
                account_name: row.get(4)?,
            })
        })
        .ok();
    rows.into_iter()
        .flat_map(|it| it.filter_map(Result::ok))
        .collect()
}

/// Status filter for the normalise queue. `Skipped` is session-only — it
/// surfaces rows the user has temporarily set aside in the current serve
/// run (the underlying DB row is still pending).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormStatusFilter {
    All,
    Pending,
    Confirmed,
    Rejected,
    Skipped,
}

impl NormStatusFilter {
    pub fn parse(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "confirmed" => Self::Confirmed,
            "rejected" => Self::Rejected,
            "skipped" => Self::Skipped,
            _ => Self::All,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
            Self::Skipped => "skipped",
        }
    }

    pub const ALL: [NormStatusFilter; 5] = [
        Self::All,
        Self::Pending,
        Self::Confirmed,
        Self::Rejected,
        Self::Skipped,
    ];
}

/// Class filter for the normalise queue. `Unclassified` matches rows whose
/// `class` column is NULL (the pipeline didn't reach a verdict).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NormClassFilter {
    All,
    Merchant,
    Person,
    Employer,
    Other,
    Unclassified,
}

impl NormClassFilter {
    pub fn parse(s: &str) -> Self {
        match s {
            "merchant" => Self::Merchant,
            "person" => Self::Person,
            "employer" => Self::Employer,
            "other" => Self::Other,
            "unclassified" => Self::Unclassified,
            _ => Self::All,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Merchant => "merchant",
            Self::Person => "person",
            Self::Employer => "employer",
            Self::Other => "other",
            Self::Unclassified => "unclassified",
        }
    }

    pub const ALL: [NormClassFilter; 6] = [
        Self::All,
        Self::Merchant,
        Self::Person,
        Self::Employer,
        Self::Other,
        Self::Unclassified,
    ];

    /// Returns true if the row's class column matches this filter.
    pub fn matches(self, class: Option<&str>) -> bool {
        match self {
            Self::All => true,
            Self::Unclassified => class.is_none(),
            Self::Merchant => class == Some("merchant"),
            Self::Person => class == Some("person"),
            Self::Employer => class == Some("employer"),
            Self::Other => class == Some("other"),
        }
    }
}

/// Load all `payee_normalisations` rows and filter by status + class +
/// session decisions. Returns rows ordered by `txn_count DESC,
/// original_payee ASC` (the natural list order from `pn::list_all`).
///
/// `decisions` is the session-only decision map (keyed by
/// `original_payee`). Skip decisions take precedence and pull a row out
/// of every status filter except `Skipped`. Confirm/Reject decisions are
/// already reflected by the DB row's status, so they're not consulted
/// here — the filter just operates on the row's stored status.
pub fn get_filtered_normalisations(
    conn: &Connection,
    status: NormStatusFilter,
    class: NormClassFilter,
    decisions: &HashMap<String, Decision>,
) -> Vec<PayeeNormalisationRow> {
    let all = pn::list_all(conn).unwrap_or_default();
    all.into_iter()
        .filter(|row| {
            let session_decision = decisions.get(&row.original_payee).copied();
            let is_skipped = session_decision == Some(Decision::Skip);
            let status_ok = match status {
                NormStatusFilter::All => true,
                NormStatusFilter::Skipped => is_skipped,
                NormStatusFilter::Pending => !is_skipped && row.status == Status::Pending,
                NormStatusFilter::Confirmed => !is_skipped && row.status == Status::Confirmed,
                NormStatusFilter::Rejected => !is_skipped && row.status == Status::Rejected,
            };
            status_ok && class.matches(row.class.as_deref())
        })
        .collect()
}

/// Find the slug that follows `current` in the filtered list. Mirrors
/// `next_pair_after` for the transfers tab. Stays on the last element if
/// `current` is the tail.
pub fn next_slug_after(rows: &[PayeeNormalisationRow], current: &str) -> Option<String> {
    let idx = rows.iter().position(|r| r.slug == current)?;
    let next_idx = (idx + 1).min(rows.len().saturating_sub(1));
    rows.get(next_idx).map(|r| r.slug.clone())
}

/// Count session decisions of a particular kind on the normalise tab.
pub fn count_norm_decisions(decisions: &HashMap<String, Decision>, d: Decision) -> usize {
    decisions.values().filter(|v| **v == d).count()
}

/// True iff the session has any active Skip decision — used by the
/// "Clear skipped" affordance in the queue header.
pub fn any_skipped(decisions: &HashMap<String, Decision>) -> usize {
    count_norm_decisions(decisions, Decision::Skip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use pocketsmith_sync::db::initialize_in_memory;

    fn upsert(
        conn: &Connection,
        original: &str,
        class: Option<&str>,
        status: Status,
        txn_count: i64,
    ) {
        pn::upsert(
            conn,
            &PayeeNormalisationRow {
                original_payee: original.into(),
                proposed_payee: format!("{original}-proposed"),
                slug: pn::slug_for(original),
                class: class.map(|s| s.into()),
                features_json: "{}".into(),
                txn_count,
                status,
            },
        )
        .unwrap();
    }

    #[test]
    fn filter_parsing_round_trips_known_values_and_defaults_unknown() {
        for s in ["all", "pending", "confirmed", "rejected", "skipped"] {
            assert_eq!(NormStatusFilter::parse(s).as_str(), s);
        }
        // Unknown maps to All (defensive default).
        assert_eq!(NormStatusFilter::parse("garbage"), NormStatusFilter::All);
        assert_eq!(NormStatusFilter::parse(""), NormStatusFilter::All);

        for s in [
            "all",
            "merchant",
            "person",
            "employer",
            "other",
            "unclassified",
        ] {
            assert_eq!(NormClassFilter::parse(s).as_str(), s);
        }
        assert_eq!(NormClassFilter::parse("nope"), NormClassFilter::All);
    }

    #[test]
    fn get_filtered_normalisations_combines_status_class_and_skip() {
        let conn = initialize_in_memory().unwrap();
        upsert(&conn, "W", Some("merchant"), Status::Pending, 10);
        upsert(&conn, "C", Some("merchant"), Status::Confirmed, 5);
        upsert(&conn, "P", Some("person"), Status::Pending, 3);
        upsert(&conn, "U", None, Status::Pending, 1);
        upsert(&conn, "R", Some("other"), Status::Rejected, 2);

        let mut decisions: HashMap<String, Decision> = HashMap::new();

        // All / All returns all five rows.
        assert_eq!(
            get_filtered_normalisations(&conn, NormStatusFilter::All, NormClassFilter::All, &decisions).len(),
            5
        );

        // Pending + Merchant filters out C (confirmed), P (person), U (unclassified), R (rejected).
        let rows = get_filtered_normalisations(
            &conn,
            NormStatusFilter::Pending,
            NormClassFilter::Merchant,
            &decisions,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].original_payee, "W");

        // Unclassified class isolates U.
        let rows = get_filtered_normalisations(
            &conn,
            NormStatusFilter::All,
            NormClassFilter::Unclassified,
            &decisions,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].original_payee, "U");

        // Skipping W moves it out of Pending and into Skipped.
        decisions.insert("W".to_string(), Decision::Skip);
        let pending = get_filtered_normalisations(
            &conn,
            NormStatusFilter::Pending,
            NormClassFilter::All,
            &decisions,
        );
        assert!(pending.iter().all(|r| r.original_payee != "W"));
        let skipped_view = get_filtered_normalisations(
            &conn,
            NormStatusFilter::Skipped,
            NormClassFilter::All,
            &decisions,
        );
        assert_eq!(skipped_view.len(), 1);
        assert_eq!(skipped_view[0].original_payee, "W");
    }

    #[test]
    fn next_slug_after_returns_following_row_or_stays_on_tail() {
        let conn = initialize_in_memory().unwrap();
        upsert(&conn, "A", Some("merchant"), Status::Pending, 10);
        upsert(&conn, "B", Some("merchant"), Status::Pending, 5);
        upsert(&conn, "C", Some("merchant"), Status::Pending, 1);
        let decisions: HashMap<String, Decision> = HashMap::new();
        let rows = get_filtered_normalisations(&conn, NormStatusFilter::All, NormClassFilter::All, &decisions);
        // ordered by txn_count DESC -> A, B, C.
        let a_slug = pn::slug_for("A");
        let b_slug = pn::slug_for("B");
        let c_slug = pn::slug_for("C");
        assert_eq!(next_slug_after(&rows, &a_slug), Some(b_slug.clone()));
        assert_eq!(next_slug_after(&rows, &b_slug), Some(c_slug.clone()));
        // Tail stays on tail (matches transfer next_pair_after semantics).
        assert_eq!(next_slug_after(&rows, &c_slug), Some(c_slug));
        // Unknown slug returns None.
        assert_eq!(next_slug_after(&rows, "deadbeefdeadbeef"), None);
    }
}
