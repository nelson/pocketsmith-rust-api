use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decision {
    Confirm,
    Reject,
    Skip,
}

impl Decision {
    pub fn css_class(self) -> &'static str {
        match self {
            Decision::Confirm => "decided-confirmed",
            Decision::Reject => "decided-rejected",
            Decision::Skip => "decided-skipped",
        }
    }
}

pub struct ActivityEntry {
    pub pair_id: (i64, i64),
    pub decision: Decision,
    pub amount_cents: i64,
    pub account_a: String,
    pub account_b: String,
}

pub struct AppState {
    pub conn: rusqlite::Connection,
    pub activity: Vec<ActivityEntry>,
    pub undone: usize,
    /// Cumulative count of pairs applied this session via the "Apply all
    /// changes" button. Shown in the activity header.
    pub applied: usize,
    pub status_filter: String,
    pub confidence_filter: String,
    pub decisions: HashMap<(i64, i64), Decision>,
    pub active_pair: Option<(i64, i64)>,

    // --- Normalise tab session state ---
    /// Session-only "skip" set for normalise proposals, keyed by
    /// `original_payee`. Skipped rows stay pending in the DB but are
    /// hidden from the Pending queue and surfaced under Skipped.
    pub norm_skipped: HashMap<String, ()>,
    /// Cumulative count of `transactions.payee` writes applied this
    /// session via the normalise tab's "Apply confirmed" button.
    pub norm_applied: usize,
    #[allow(dead_code)] // wired in commit 9 (views + routes)
    pub norm_status_filter: String,
    #[allow(dead_code)] // wired in commit 9
    pub norm_class_filter: String,
    #[allow(dead_code)] // wired in commit 9
    pub norm_active_slug: Option<String>,
}

impl AppState {
    /// Construct an AppState pre-populated for tests / fresh sessions.
    /// Carries no decisions, no activity, default filter strings.
    pub fn new(conn: rusqlite::Connection) -> Self {
        Self {
            conn,
            activity: Vec::new(),
            undone: 0,
            applied: 0,
            status_filter: "all".to_string(),
            confidence_filter: "all".to_string(),
            decisions: HashMap::new(),
            active_pair: None,
            norm_skipped: HashMap::new(),
            norm_applied: 0,
            norm_status_filter: "pending".to_string(),
            norm_class_filter: "all".to_string(),
            norm_active_slug: None,
        }
    }
}
