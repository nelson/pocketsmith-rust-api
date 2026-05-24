//! Transfer pair review tab — `/transfers/*` routes.
//!
//! Mirrors the structure of [`crate::normalise`]:
//! - [`helpers`]  filter helpers + the `parse_pair_id`/derive_decision plumbing
//! - [`handlers`] action handlers (act/undo/bulk_act/clear_all_skipped/apply)
//! - [`views`]    page + fragment rendering

pub mod handlers;
pub mod helpers;
pub mod views;
