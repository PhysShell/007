//! Independent replay: re-derive a stored run's verdict and prove its artifacts.
//!
//! CONTRACT-ONLY (RED). This commit fixes the replay SURFACE and its acceptance criteria
//! (encoded in `tests/replay_acceptance.rs`) but does NOT implement verification yet;
//! [`replay`] and [`replay_verify`] return [`ReplayError::Unimplemented`], so a legacy or
//! tampered run can never be silently reported as replay-verified.
//!
//! Replay is where the tamper-evidence becomes an assertion: it (1) validates event-chain
//! continuity and per-event digests, (2) resolves every referenced artifact and checks its
//! content digest, (3) folds the stream through the pure [`crate::reduce`] reducer, and
//! (4) recomputes the verdict and — for [`replay_verify`] — compares it to the stored one.
//! Any unexplained mismatch fails loudly. A run with no events is explicitly
//! `LegacyNonReplayable`, never "verified".

use serde::{Deserialize, Serialize};

use crate::event::{ArtifactRef, RunEvent};
use crate::reduce::ReduceError;
use crate::state::Verdict;

/// Resolves an [`ArtifactRef`]'s locator to its current bytes. Replay hashes the result
/// and compares to the reference's digest, so a post-seal edit to `diff.patch` or a gate
/// log is caught. Kept as a trait so the pure core stays I/O-free and tests can inject
/// tampered content deterministically.
pub trait ArtifactResolver {
    /// Return the current bytes for `artifact`, or an error if it cannot be resolved.
    ///
    /// # Errors
    /// [`ArtifactError`] if the locator is unknown or unreadable.
    fn resolve(&self, artifact: &ArtifactRef) -> Result<Vec<u8>, ArtifactError>;
}

/// A failure resolving an artifact's bytes (missing / unreadable). Distinct from a digest
/// MISMATCH, which replay raises itself after a successful resolve.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("artifact could not be resolved: {locator} ({reason})")]
pub struct ArtifactError {
    pub locator: String,
    pub reason: String,
}

/// Whether a stored run can be independently replayed at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Replayability {
    /// A canonical event stream is present and can be verified.
    Replayable,
    /// A pre-protocol run with no canonical events: readable as a forensic archive but
    /// NOT independently replay-verifiable. Never presented as verified.
    LegacyNonReplayable,
}

/// Classify a stored run by whether it carries a replayable event stream. An empty stream
/// (or one lacking `RunStarted`) is legacy.
#[must_use]
pub fn classify(events: &[RunEvent]) -> Replayability {
    match events.first() {
        Some(_) => Replayability::Replayable,
        None => Replayability::LegacyNonReplayable,
    }
}

/// The machine-readable result of a successful replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayReport {
    /// The independently recomputed verdict.
    pub verdict: Verdict,
    /// How many events were chain- and digest-verified.
    pub events_verified: u64,
    /// How many referenced artifacts had their content digest verified.
    pub artifacts_verified: u64,
}

/// Everything that can make a replay fail loudly.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    /// The stream is structurally invalid (propagated from the reducer).
    #[error("reduce failed during replay: {0}")]
    Reduce(#[from] ReduceError),

    /// The chain link (`previous_event_digest`) at this sequence does not match its
    /// predecessor — an event was deleted, reordered, or inserted.
    #[error("event chain broken at sequence {sequence}")]
    ChainBroken { sequence: u64 },

    /// An event's stored `event_digest` does not match a recomputation — the event was
    /// tampered with in place.
    #[error("event digest mismatch at sequence {sequence}")]
    EventDigestMismatch { sequence: u64 },

    /// A referenced artifact could not be resolved.
    #[error("artifact unresolved: {0}")]
    ArtifactUnresolved(#[from] ArtifactError),

    /// A referenced artifact resolved, but its content digest does not match the event's
    /// reference — the artifact (e.g. `diff.patch`, a gate log) changed after sealing.
    #[error("artifact content changed after sealing: {locator}")]
    ArtifactDigestMismatch { locator: String },

    /// Replay was asked to verify a run that was never sealed (no fixed verdict).
    #[error("run is not sealed; there is no fixed verdict to verify")]
    NotSealed,

    /// The independently recomputed verdict disagrees with the stored one — the stored
    /// summary is not trustworthy.
    #[error("verdict mismatch: recomputed {recomputed:?}, stored {stored:?}")]
    VerdictMismatch {
        recomputed: Verdict,
        stored: Verdict,
    },

    /// The run has no canonical events; it is legacy and cannot be replay-verified.
    #[error("run is legacy (no canonical events) and cannot be replayed")]
    LegacyNonReplayable,

    /// Verification is not implemented in this (contract-only) commit.
    #[error("replay verification is not implemented in this contract-only build")]
    Unimplemented,
}

/// Independently replay a stored run: verify the chain and artifacts and recompute the
/// verdict.
///
/// # Errors
/// [`ReplayError`] on any chain break, digest mismatch, unresolved/altered artifact,
/// unsealed run, or legacy record.
pub fn replay(
    events: &[RunEvent],
    artifacts: &dyn ArtifactResolver,
) -> Result<ReplayReport, ReplayError> {
    // CONTRACT-ONLY: see the module doc. Verification is specified by the acceptance
    // tests in this commit and implemented next; until then replay is unavailable so a
    // tampered or legacy run is never reported as verified.
    let _ = (events, artifacts);
    Err(ReplayError::Unimplemented)
}

/// Replay a stored run AND assert the recomputed verdict equals `stored_verdict`.
///
/// # Errors
/// [`ReplayError`] from [`replay`], plus [`ReplayError::VerdictMismatch`] if the
/// recomputation disagrees with the stored summary.
pub fn replay_verify(
    events: &[RunEvent],
    artifacts: &dyn ArtifactResolver,
    stored_verdict: Verdict,
) -> Result<ReplayReport, ReplayError> {
    let _ = (events, artifacts, stored_verdict);
    Err(ReplayError::Unimplemented)
}
