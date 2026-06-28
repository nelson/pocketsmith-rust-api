//! Subcommand entrypoints for the unified `pocketsmith` binary.
//!
//! Each module here owns one verb's thin CLI shell (argument handling +
//! human-readable output); all real logic lives in the `pocketsmith`
//! library. The dispatcher in `main.rs` routes `argv[1]` to one of these
//! `run(args)` functions.

pub mod dump;
pub mod normalise;
pub mod push;
pub mod sync;
pub mod transfers;
