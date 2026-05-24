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
/// session skip decisions. Returns rows ordered by `txn_count DESC,
/// original_payee ASC` (the natural list order from `pn::list_all`).
pub fn get_filtered_normalisations(
    conn: &Connection,
    status: NormStatusFilter,
    class: NormClassFilter,
    skipped: &HashMap<String, ()>,
) -> Vec<PayeeNormalisationRow> {
    let all = pn::list_all(conn).unwrap_or_default();
    all.into_iter()
        .filter(|row| {
            let is_skipped = skipped.contains_key(&row.original_payee);
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

        let mut skipped = HashMap::new();

        // All / All returns all five rows.
        assert_eq!(
            get_filtered_normalisations(&conn, NormStatusFilter::All, NormClassFilter::All, &skipped).len(),
            5
        );

        // Pending + Merchant filters out C (confirmed), P (person), U (unclassified), R (rejected).
        let rows = get_filtered_normalisations(
            &conn,
            NormStatusFilter::Pending,
            NormClassFilter::Merchant,
            &skipped,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].original_payee, "W");

        // Unclassified class isolates U.
        let rows = get_filtered_normalisations(
            &conn,
            NormStatusFilter::All,
            NormClassFilter::Unclassified,
            &skipped,
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].original_payee, "U");

        // Skipping W moves it out of Pending and into Skipped.
        skipped.insert("W".to_string(), ());
        let pending = get_filtered_normalisations(
            &conn,
            NormStatusFilter::Pending,
            NormClassFilter::All,
            &skipped,
        );
        assert!(pending.iter().all(|r| r.original_payee != "W"));
        let skipped_view = get_filtered_normalisations(
            &conn,
            NormStatusFilter::Skipped,
            NormClassFilter::All,
            &skipped,
        );
        assert_eq!(skipped_view.len(), 1);
        assert_eq!(skipped_view[0].original_payee, "W");
    }
}
