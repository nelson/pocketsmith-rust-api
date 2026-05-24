//! Normalise review tab — `/normalise/*` routes. See submodules for the
//! split between filter helpers, handlers, and view rendering.
//!
//! `#[allow(dead_code)]` is applied module-wide while the tab is being built
//! up commit-by-commit (helpers → handlers → views). The allow goes away
//! once `mod.rs` wires the routes in.
#![allow(dead_code)]

pub mod helpers;
