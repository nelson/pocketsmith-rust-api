//! `/dashboard/*` tab: monthly-overview answers to "what's happening
//! with my money this month?".
//!
//! MVP scope (this commit):
//!
//! - Queue panel: one row per month with the in / out / net summary
//!   and a hygiene meter. Newest month at the top, the selected month
//!   gets the standard `.selected` class so arrow-key navigation
//!   works for free.
//! - Detail panel: month header, a server-rendered SVG Sankey on the
//!   left (top income categories \u2192 inflow node \u2192 top expense
//!   categories), and an equivalent breakdown table on the right.
//!
//! Bigger ambitions from the original dashboard plan (daily cashflow
//! strip, cumulative-net line, year toggle, hygiene scorecard,
//! activity panel) are deliberately deferred until this baseline is
//! in anger.

pub mod helpers;
pub mod views;
