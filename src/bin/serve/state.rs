use std::collections::HashMap;
use std::hash::Hash;

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

/// Per-tab session state. Both the transfers and normalise tabs maintain
/// the same shape: a map of session decisions keyed by row identifier, an
/// activity log of those decisions, plus three small counters and an
/// "active row" pointer. `K` is the row identifier (e.g. `(i64, i64)` or
/// `String`); `A` is the tab's activity-row type.
pub struct TabState<K, A> {
    /// Session decisions, keyed by row identifier.
    pub decisions: HashMap<K, Decision>,
    /// Activity log; newest at the tail. Capped at 100 entries via
    /// [`TabState::push_activity`].
    pub activity: Vec<A>,
    /// Cumulative undo count.
    pub undone: usize,
    /// Cumulative apply count.
    pub applied: usize,
    /// Identifier of the row currently shown in the detail panel.
    pub active: Option<K>,
}

impl<K: Eq + Hash, A> Default for TabState<K, A> {
    fn default() -> Self {
        Self {
            decisions: HashMap::new(),
            activity: Vec::new(),
            undone: 0,
            applied: 0,
            active: None,
        }
    }
}

impl<K: Eq + Hash, A> TabState<K, A> {
    /// Append `entry` to the activity log, trimming the oldest entry if
    /// the log would exceed 100 items. Mirrors what both handler files
    /// were doing inline.
    pub fn push_activity(&mut self, entry: A) {
        self.activity.push(entry);
        if self.activity.len() > 100 {
            self.activity.remove(0);
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

/// Activity-log entry for the normalise tab.
pub struct NormActivityEntry {
    pub slug: String,
    #[allow(dead_code)] // useful for debugging / future activity-row tooltips
    pub original_payee: String,
    pub proposed_payee: String,
    pub txn_count: i64,
    pub decision: Decision,
}

/// Activity-log entry for the transactions tab. Records a decision
/// the user made from the Transactions detail panel so the activity
/// panel can show recent actions and offer one-click undo.
pub struct TxnActivityEntry {
    pub txn_id: i64,
    pub payee: String,
    pub amount_cents: i64,
    pub decision: Decision,
    /// Which pillar's endpoint to call on undo: "norm" or "pair".
    pub pillar: TxnActionPillar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnActionPillar {
    Norm,
    Pair,
}

impl TxnActionPillar {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Norm => "norm",
            Self::Pair => "pair",
        }
    }
}

pub struct AppState {
    pub conn: rusqlite::Connection,

    // --- Transfers tab ---
    pub transfers: TabState<(i64, i64), ActivityEntry>,
    pub status_filter: String,
    pub confidence_filter: String,

    // --- Normalise tab ---
    pub normalise: TabState<String, NormActivityEntry>,
    pub norm_status_filter: String,
    pub norm_class_filter: String,

    // --- Transactions tab ---
    /// Active filter chip slug ("all", "needs-rule", "rule-pending",
    /// "orphan-transfer", "uncategorised"). Persisted across page
    /// re-renders so the user's chosen filter survives a refresh.
    pub txn_filter: String,
    /// id of the currently-selected transaction row, if any.
    pub txn_active: Option<i64>,
    /// Activity log; newest at the tail. Capped at 100 entries.
    pub txn_activity: Vec<TxnActivityEntry>,
    /// Cumulative undo count for the activity panel header.
    pub txn_undone: usize,
}

impl AppState {
    pub fn new(conn: rusqlite::Connection) -> Self {
        Self {
            conn,
            transfers: TabState::default(),
            status_filter: "all".to_string(),
            confidence_filter: "all".to_string(),
            normalise: TabState::default(),
            norm_status_filter: "all".to_string(),
            norm_class_filter: "all".to_string(),
            txn_filter: "all".to_string(),
            txn_active: None,
            txn_activity: Vec::new(),
            txn_undone: 0,
        }
    }

    /// Append `entry` to the transactions activity log, trimming the
    /// oldest if it would exceed 100 items. Mirrors `TabState::push_activity`.
    pub fn push_txn_activity(&mut self, entry: TxnActivityEntry) {
        self.txn_activity.push(entry);
        if self.txn_activity.len() > 100 {
            self.txn_activity.remove(0);
        }
    }
}
