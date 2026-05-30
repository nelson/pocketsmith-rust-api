//! Pipeline tab — `/pipeline/*` routes (editable-rules-v3 §4).
//!
//! Lets the user inspect and (in later PRs) edit the eight normalisation
//! pipeline stages. PR 3 is the shell only: a queue of the eight stages
//! in execution order, an empty detail panel, and an activity panel.
//!
//! Submodules:
//! - [`views`] page + fragment rendering

pub mod views;
