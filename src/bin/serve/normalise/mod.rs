//! Normalise review tab — `/normalise/*` routes.
//!
//! Submodules:
//! - [`helpers`]  filter enums + `get_filtered_normalisations`
//! - [`handlers`] action handlers (confirm/reject/skip/unskip/apply)
//! - [`views`]    page + fragment rendering

pub mod handlers;
pub mod helpers;
pub mod views;
