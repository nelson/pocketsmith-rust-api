//! The `Transactions` tab â a reverse-chronological river of every
//! transaction with cleaning-state visible at a glance. See
//! `.claude/plans/PLAN-transactions-and-dashboard.md` (\u00a73) for the
//! design.
//!
//! v1 is staging-only: this tab does not directly mutate transaction
//! columns. Cleaning actions (confirm/reject/skip a normalisation
//! proposal, confirm/reject a pair, snooze an orphan) all delegate
//! into existing `/normalise/*` and `/transfers/*` endpoints, plus the
//! new `/transfer-decisions/*` endpoints for the orphan flow.

pub mod helpers;
pub mod state;
