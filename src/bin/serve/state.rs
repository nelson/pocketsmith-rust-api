use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
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
    pub status_filter: String,
    pub confidence_filter: String,
    pub decisions: HashMap<(i64, i64), Decision>,
    pub active_pair: Option<(i64, i64)>,
}
