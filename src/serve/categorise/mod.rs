//! The `/categorise/*` serve tab: a review queue over `category_proposals`
//! (the final pipeline stage). Mirrors the normalise tab; the hardcoded
//! taxonomy means this is a proposal reviewer, not a rule editor.

pub mod handlers;
pub mod helpers;
pub mod views;
