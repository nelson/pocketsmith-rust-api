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

/// Activity-log entry for the categorise tab. Keyed by `merchant_key`.
pub struct CatActivityEntry {
    pub merchant_key: String,
    pub category_title: String,
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

/// Activity-log entry for the Pipeline tab: one committed rule change,
/// pre-formatted via `rules::activity::RuleChange::describe`. `kind`
/// drives the add/edit/delete colour vocabulary in the activity panel.
pub struct RuleChangeEntry {
    /// The full activity line, e.g. "+ added Bunnings (?i)BUNNINGS".
    pub line: String,
    /// Coarse category for colouring.
    pub kind: RuleChangeKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleChangeKind {
    Added,
    Edited,
    Deleted,
    Moved,
}

impl RuleChangeKind {
    /// Infer the category from the leading glyph of a `RuleChange` line.
    pub fn from_line(line: &str) -> RuleChangeKind {
        match line.chars().next() {
            Some('+') => RuleChangeKind::Added,
            Some('~') => RuleChangeKind::Edited,
            Some('\u{2212}') | Some('-') => RuleChangeKind::Deleted,
            _ => RuleChangeKind::Moved,
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            RuleChangeKind::Added => "rc-added",
            RuleChangeKind::Edited => "rc-edited",
            RuleChangeKind::Deleted => "rc-deleted",
            RuleChangeKind::Moved => "rc-moved",
        }
    }
}

/// A cached full-pipeline pass over every distinct payee on the
/// **committed** rules — the expensive "base" of an evaluate. Reused
/// across re-evaluates within an editing session (the committed rules
/// don't change between them) so each re-evaluate runs only the scratch
/// pass. Invalidated (dropped) on any commit / re-scan.
pub struct PipelineBase {
    pub payees: Vec<pocketsmith_sync::rules::impact::PayeeSample>,
    pub results: Vec<pocketsmith_sync::normalise::NormalisationResult>,
}

pub struct AppState {
    pub conn: rusqlite::Connection,

    /// Process-lifetime compiled-rule cache for the normalisation
    /// pipeline (editable-rules-v3 §7). Borrowed alongside `conn` to
    /// build a `PipelineCtx` when rendering pipeline traces.
    pub rule_cache: pocketsmith_sync::normalise::RuleCache,

    // --- Transfers tab ---
    pub transfers: TabState<(i64, i64), ActivityEntry>,
    pub status_filter: String,
    pub confidence_filter: String,

    // --- Normalise tab ---
    pub normalise: TabState<String, NormActivityEntry>,
    pub norm_status_filter: String,
    pub norm_class_filter: String,

    // --- Categorise tab ---
    pub categorise: TabState<String, CatActivityEntry>,
    pub cat_status_filter: String,

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

    // --- Dashboard tab ---
    /// Currently-selected month on the Dashboard tab, formatted as
    /// `YYYY-MM`. `None` means "use the most recent month with data",
    /// which is what the shell falls back to on first render.
    pub dash_active_month: Option<String>,

    // --- Pipeline tab ---
    /// `name` of the currently-selected pipeline stage, if any.
    pub pipeline_active: Option<String>,
    /// Row id of the rule whose editor card is open in the detail panel,
    /// if any. `None` means no card is open (list-only detail).
    pub pipeline_active_rule: Option<i64>,
    /// Rule-change activity log (newest at the tail), capped at 100 via
    /// [`push_rule_change`](Self::push_rule_change).
    pub pipeline_activity: Vec<RuleChangeEntry>,
    /// Cached committed-rules pipeline pass for fast re-evaluates
    /// (editable-rules-ui §4). `None` until first computed; dropped on any
    /// rule commit / re-scan.
    pub pipeline_base: Option<PipelineBase>,
    /// SQLite file path, for the background `.sql` re-dump after a rule
    /// commit (`DumpPolicy::Background`). `None` for in-memory test DBs,
    /// where the handlers fall back to a synchronous dump into
    /// [`rules_dir_override`](Self::rules_dir_override).
    pub db_path: Option<String>,
    /// Test-only override for where committed rules are dumped. `None` in
    /// production (uses `rules::rules_dir()`); tests set a temp dir so the
    /// dump is synchronous + isolated + parallel-safe.
    pub rules_dir_override: Option<std::path::PathBuf>,
}

impl AppState {
    pub fn new(conn: rusqlite::Connection) -> Self {
        Self {
            conn,
            rule_cache: pocketsmith_sync::normalise::RuleCache::new(),
            transfers: TabState::default(),
            status_filter: "all".to_string(),
            confidence_filter: "all".to_string(),
            normalise: TabState::default(),
            norm_status_filter: "all".to_string(),
            norm_class_filter: "all".to_string(),
            categorise: TabState::default(),
            cat_status_filter: "all".to_string(),
            txn_filter: "all".to_string(),
            txn_active: None,
            txn_activity: Vec::new(),
            txn_undone: 0,
            dash_active_month: None,
            pipeline_active: None,
            pipeline_active_rule: None,
            pipeline_activity: Vec::new(),
            pipeline_base: None,
            db_path: None,
            rules_dir_override: None,
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

    /// Drop the cached base pipeline pass — call after any committed rule
    /// change or re-scan so the next evaluate recomputes against the new
    /// committed rules.
    pub fn invalidate_pipeline_base(&mut self) {
        self.pipeline_base = None;
    }

    /// Compute the committed-rules base pass once and cache it
    /// (editable-rules-ui §4). Reused by evaluate and by the loop-stage
    /// "matching payees" panel until a commit / re-scan drops it.
    pub fn ensure_pipeline_base(&mut self) {
        if self.pipeline_base.is_none() {
            let payees =
                pocketsmith_sync::rules::impact::load_payees(&self.conn).unwrap_or_default();
            let results = pocketsmith_sync::rules::impact::run_base(&self.conn, &payees);
            self.pipeline_base = Some(PipelineBase { payees, results });
        }
    }

    /// Push a committed rule-change line onto the Pipeline activity log
    /// (newest at the tail), trimming the oldest beyond 100.
    pub fn push_rule_change(&mut self, line: String) {
        let kind = RuleChangeKind::from_line(&line);
        self.pipeline_activity.push(RuleChangeEntry { line, kind });
        if self.pipeline_activity.len() > 100 {
            self.pipeline_activity.remove(0);
        }
    }

    /// How a committed rule mutation re-dumps its `rules/<stage>.sql`
    /// mirror. Production has a file-backed DB → background dump (no
    /// blocked HTTP response); in-memory test DBs dump synchronously into
    /// an injected dir so the assertion is deterministic + isolated.
    pub fn rule_dump_policy(&self) -> pocketsmith_sync::rules::DumpPolicy {
        use pocketsmith_sync::rules::DumpPolicy;
        match &self.db_path {
            Some(p) => DumpPolicy::Background { db_path: p.clone() },
            None => DumpPolicy::Sync(
                self.rules_dir_override
                    .clone()
                    .unwrap_or_else(pocketsmith_sync::rules::rules_dir),
            ),
        }
    }
}
