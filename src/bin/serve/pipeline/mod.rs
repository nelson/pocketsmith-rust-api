//! Pipeline tab — `/pipeline/*` routes (editable-rules-v3 §4).
//!
//! Lets the user inspect and (in later PRs) edit the eight normalisation
//! pipeline stages. PR 3 is the shell only: a queue of the eight stages
//! in execution order, an empty detail panel, and an activity panel.
//!
//! Submodules:
//! - [`views`] page + fragment rendering
//! - [`form`] urlencoded body parsing + `RuleData` form decoding
//! - [`editor`] the parameterised Edit/Evaluate/New editor card
//! - [`impact`] HTML rendering of the evaluate impact buckets + tester
//! - [`handlers`] create / edit / delete / reorder / re-scan mutations

pub mod editor;
pub mod form;
pub mod handlers;
pub mod impact;
pub mod regex_hl;
pub mod views;
