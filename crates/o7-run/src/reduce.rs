//! The pure run-state reducer: `reduce(state, event) -> Result<state, ReduceError>`.
//!
//! CONTRACT-ONLY (RED). This commit fixes the reducer's SIGNATURE and its required
//! behavior (encoded exhaustively in `tests/reducer_transitions.rs`) but does NOT yet
//! implement the transition logic. Until the following commit implements it, [`reduce`]
//! returns [`ReduceError::Unimplemented`] so that no caller can ever mistake an
//! unimplemented reducer for a real verdict — the tests are RED by construction, never
//! silently green.
//!
//! The reducer is PURE: same input state + event ⇒ same output, no I/O, no clock, no
//! panics. Artifact-content validation is NOT the reducer's job (it folds only the event
//! payloads); that belongs to [`crate::replay`].

use crate::event::RunEvent;
use crate::state::RunState;

/// A structural violation of the event STREAM (as opposed to a domain verdict). These
/// make reduction fail loudly — they are never folded into a `Verdict`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReduceError {
    /// The first folded event was not `RunStarted`.
    #[error("stream does not begin with RunStarted (found {found} at sequence {sequence})")]
    MissingRunStarted { found: &'static str, sequence: u64 },

    /// A second `RunStarted`, or any run-lifecycle re-initialization.
    #[error("duplicate RunStarted at sequence {sequence}")]
    DuplicateStart { sequence: u64 },

    /// `sequence` did not increase by exactly one from the previous event.
    #[error("out-of-order event: expected sequence {expected}, found {found}")]
    OutOfOrder { expected: u64, found: u64 },

    /// The same sequence number appeared twice.
    #[error("duplicate sequence {sequence}")]
    DuplicateSequence { sequence: u64 },

    /// `previous_event_digest` did not equal the prior event's `event_digest` (or genesis
    /// for the first event) — the chain is broken.
    #[error("broken chain at sequence {sequence}: previous-digest link does not match")]
    BrokenChain { sequence: u64 },

    /// The event's stored `event_digest` does not match a recomputation of its own fields.
    #[error("inconsistent event digest at sequence {sequence}")]
    InconsistentDigest { sequence: u64 },

    /// A transition that cannot happen (e.g. `GateFinished` for a gate never `GateStarted`,
    /// or any event before `RunStarted`).
    #[error("impossible transition at sequence {sequence}: {detail}")]
    ImpossibleTransition { sequence: u64, detail: String },

    /// Any event after `RunSealed`.
    #[error("event after seal at sequence {sequence}")]
    EventAfterSeal { sequence: u64 },

    /// A newer schema version than this build understands — refuse rather than
    /// mis-reduce.
    #[error("event schema v{found} is newer than supported v{supported}")]
    SchemaTooNew { found: u32, supported: u32 },

    /// The transition logic is not implemented in this (contract-only) commit.
    #[error("reducer transition logic is not implemented in this contract-only build")]
    Unimplemented,
}

/// Fold one event into the run state.
///
/// # Errors
/// [`ReduceError`] for any structural violation of the stream. A domain outcome (pass /
/// fail / blocked / error) is NEVER an error here — it is recorded in the returned state
/// and finalized into [`crate::state::Verdict`] at `RunSealed`.
pub fn reduce(state: RunState, event: &RunEvent) -> Result<RunState, ReduceError> {
    // CONTRACT-ONLY: see the module doc. The behavior is specified by the tests in this
    // commit and implemented next; until then reduction is explicitly unavailable.
    let _ = (&state, event);
    Err(ReduceError::Unimplemented)
}

/// Fold an entire stream from the initial state, returning the terminal state.
///
/// # Errors
/// The first [`ReduceError`] encountered (reduction is fail-fast and left-to-right).
pub fn reduce_all(events: &[RunEvent]) -> Result<RunState, ReduceError> {
    let mut state = RunState::initial();
    for event in events {
        state = reduce(state, event)?;
    }
    Ok(state)
}
