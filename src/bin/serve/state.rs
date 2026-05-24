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
    /// Session decisions for normalise proposals, keyed by `original_payee`.
    /// Mirrors `decisions` for transfers. Decision::Skip is session-only;
    /// Decision::Confirm/Reject also reflect a DB write to
    /// `payee_normalisations.status`.
    pub norm_decisions: HashMap<String, Decision>,
    /// Activity log of normalise actions, in chronological order.
    pub norm_activity: Vec<NormActivityEntry>,
    /// Cumulative count of undo actions on the normalise tab.
    pub norm_undone: usize,
    /// Cumulative count of `transactions.payee` writes applied this
    /// session via the normalise tab's "Apply confirmed" button.
    pub norm_applied: usize,
    pub norm_status_filter: String,
    pub norm_class_filter: String,
    pub norm_active_slug: Option<String>,
}

/// Activity-log entry for the normalise tab. Mirrors [`ActivityEntry`] in
/// shape so the renderer can lay them out the same way.
pub struct NormActivityEntry {
    pub slug: String,
    #[allow(dead_code)] // useful for debugging / future activity-row tooltips
    pub original_payee: String,
    pub proposed_payee: String,
    pub txn_count: i64,
    pub decision: Decision,
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
            norm_decisions: HashMap::new(),
            norm_activity: Vec::new(),
            norm_undone: 0,
            norm_applied: 0,
            norm_status_filter: "all".to_string(),
            norm_class_filter: "all".to_string(),
            norm_active_slug: None,
        }
    }
}
