//! Shared review primitives for the staging-table paradigm used by
//! `transfers` (transfer pair detection) and `normalise` (payee proposals).
//!
//! Both flows stage candidate changes in a dedicated table, attach a
//! `Status` to each row (pending/confirmed/rejected), and drain confirmed
//! rows back into `transactions` via an `apply_confirmed` step. The types
//! here are the small contract those flows share.
//!
//! The on-disk encoding (the `statuses` lookup table referenced by both
//! staging tables' `status` column) is `0=pending`, `1=confirmed`,
//! `2=rejected`. See `db/schema.rs`.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Status {
    Pending,
    Confirmed,
    Rejected,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Confirmed => "confirmed",
            Status::Rejected => "rejected",
        }
    }

    pub fn from_str(s: &str) -> Option<Status> {
        match s {
            "pending" => Some(Status::Pending),
            "confirmed" => Some(Status::Confirmed),
            "rejected" => Some(Status::Rejected),
            _ => None,
        }
    }

    pub fn to_i32(self) -> i32 {
        match self {
            Status::Pending => 0,
            Status::Confirmed => 1,
            Status::Rejected => 2,
        }
    }

    pub fn from_i32(v: i32) -> Option<Status> {
        match v {
            0 => Some(Status::Pending),
            1 => Some(Status::Confirmed),
            2 => Some(Status::Rejected),
            _ => None,
        }
    }

    pub const ALL: [Status; 3] = [Status::Pending, Status::Confirmed, Status::Rejected];
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Counts returned from any `apply_confirmed` step. Both the transfer and
/// normalise flows drain a number of confirmed staging rows and update a
/// number of transaction rows in the same operation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ApplyStats {
    /// Confirmed staging rows deleted by this apply.
    pub rows_drained: usize,
    /// `transactions` rows whose tracked columns were updated.
    pub transactions_updated: usize,
}
