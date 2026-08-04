//! 007 (`o7`) library surface.
//!
//! The modules are shared between the `o7` binary (see `src/main.rs`) and the
//! out-of-tree harnesses (`fuzz/`, Kani proofs) that need to reach the pure
//! functions and parsers. The binary is a thin CLI over these.

pub mod agent;
pub mod events;
pub mod gate;
pub mod invoke;
// The arliai HTTP layer for `o7 invoke` — crate-internal on purpose: its
// only consumer is `invoke.rs`, and nothing outside the crate should be
// able to reach the raw call/extraction seam around the status classifier.
mod invoke_arliai;
pub mod judge;
pub mod ledger_projector;
pub mod record;
pub mod recovery;
pub mod verdict;
pub mod worktree;
