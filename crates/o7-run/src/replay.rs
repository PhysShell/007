//! Independent replay: re-derive a stored run's verdict and prove its artifacts.
//!
//! Replay is where the tamper-evidence becomes an assertion: it (1) classifies the record
//! (legacy / canonical / malformed), (2) validates per-event digests and chain-link
//! continuity, (3) folds the stream through the pure [`crate::reduce`](mod@crate::reduce) reducer (structural
//! validation), (4) resolves every referenced artifact and checks its content digest, (5)
//! requires a sealed run for a fixed verdict, and (6) returns the recomputed verdict plus
//! the anchor digests. [`replay_verify`] additionally compares the recomputed verdict to a
//! stored one. Any unexplained mismatch fails loudly; a legacy record is never reported as
//! verified. Steps (1)-(4) are also exposed standalone as [`verify_prefix`] — everything
//! above EXCEPT the sealed requirement — for a caller (Q-Deck R0.7's crash-prefix recovery)
//! that must verify a not-yet-sealed stream just as strictly, without demanding a verdict.
//!
//! What the chain proves, precisely: it detects any modification NOT accompanied by a
//! consistent recomputation of every downstream digest. It does NOT prove authenticity
//! against an actor who can rewrite the WHOLE stream and recompute the chain — that is a
//! non-goal (no remote attestation). To anchor a stream against such an actor,
//! [`ReplayReport`] exposes `final_event_digest` and `normalized_state_digest` for an
//! external journal or signature.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::event::{ArtifactRef, Digest256, RunEvent, RunEventKind};
use crate::reduce::{reduce_all, ReduceError};
use crate::state::Verdict;

/// Resolves an [`ArtifactRef`]'s locator to its current bytes. Replay hashes the result and
/// compares to the reference's digest, so a post-seal edit to `diff.patch` or a gate log is
/// caught. Kept as a trait so the pure core stays I/O-free and tests can inject tampered
/// content deterministically.
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

/// Resolves an [`ArtifactRef`] locator against a run-record directory on
/// disk (`<runs_dir>/<target>/<run_id>/`) — the ONE [`ArtifactResolver`]
/// every real caller in this codebase uses, whether that's `o7 replay`/`o7
/// recover --run-dir` (the root `o7` binary) or `o7d`'s own pre-redrive
/// canonical-record classification (Q-Deck R1 fourth corrective round):
/// both must resolve artifacts identically, so this lives here, in the one
/// crate both depend on, rather than being duplicated per caller.
///
/// The locator must be a plain name — an absolute locator, a `..`, a `.`,
/// or a prefix component is rejected, so a crafted record can neither read
/// files outside the record (`/etc/...`) nor stall replay on a device file
/// (`/dev/zero`). The check is lexical; a symlink planted INSIDE a record
/// can still point out, which is out of the MVP threat model (records are
/// produced by this harness) and recorded here so it is a decision, not an
/// oversight.
pub struct RecordDirResolver {
    pub base: PathBuf,
}

fn confined(locator: &str) -> bool {
    let p = Path::new(locator);
    !locator.is_empty()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

impl ArtifactResolver for RecordDirResolver {
    fn resolve(&self, a: &ArtifactRef) -> Result<Vec<u8>, ArtifactError> {
        if !confined(&a.locator) {
            return Err(ArtifactError {
                locator: a.locator.clone(),
                reason: "locator escapes the record directory (absolute, `..`, or empty) — refused before I/O".to_string(),
            });
        }
        std::fs::read(self.base.join(&a.locator)).map_err(|e| ArtifactError {
            locator: a.locator.clone(),
            reason: e.to_string(),
        })
    }
}

/// Whether — and how — a stored run can be independently replayed. Three states, so a
/// non-empty but broken file is never called "replayable".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Replayability {
    /// A pre-protocol run with no canonical events: readable as a forensic archive but NOT
    /// independently replay-verifiable. Never presented as verified.
    LegacyNonReplayable,
    /// The stream begins like a canonical run (a genesis `RunStarted` at sequence 0); it is
    /// a replay CANDIDATE whose full validity [`replay`] then proves or rejects.
    CanonicalReplayable,
    /// Events are present but the stream does not even begin canonically (no genesis
    /// `RunStarted` at sequence 0) — corrupt, not replayable.
    CanonicalMalformed,
}

/// Classify a stored run cheaply, without a full replay: empty → legacy; a genesis
/// `RunStarted` at sequence 0 → replay candidate; anything else non-empty → malformed.
#[must_use]
pub fn classify(events: &[RunEvent]) -> Replayability {
    let Some(first) = events.first() else {
        return Replayability::LegacyNonReplayable;
    };
    let starts_canonically = matches!(first.kind, RunEventKind::RunStarted { .. })
        && first.sequence == 0
        && first.previous_event_digest == Digest256::genesis();
    if starts_canonically {
        Replayability::CanonicalReplayable
    } else {
        Replayability::CanonicalMalformed
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
    /// The `event_digest` of the last (sealed) event — an external anchor point.
    pub final_event_digest: Digest256,
    /// A digest over the byte-stable normalized reduced state — anchorable in an external
    /// journal/signature to detect a fully-recomputed rewrite the chain alone cannot.
    pub normalized_state_digest: Digest256,
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

    /// The reduced state could not be serialized to its normalized form (should be
    /// impossible for a well-formed run; surfaced rather than panicked).
    #[error("normalized state digest unavailable")]
    StateDigestUnavailable,
}

/// Everything [`replay`] checks EXCEPT the sealed/verdict requirement: per-event digest
/// self-consistency, chain-link continuity, reducer structural validation, AND full
/// artifact resolution + content-digest verification — run unconditionally, regardless of
/// whether the stream is sealed yet. Returns the reduced state (whose `verdict` is `None`
/// for a not-yet-sealed run) and the count of artifacts verified.
///
/// This is the shared primitive behind both [`replay`] (which additionally requires a
/// sealed verdict) and Q-Deck R0.7's crash-prefix recovery (`o7 recover --run-dir`'s
/// catch-up, `007`'s root crate), which must verify a still-running or crashed-mid-run
/// prefix exactly as strictly as a sealed run — chain, digests, reducer, AND every
/// referenced artifact's actual content — without requiring it to already be sealed.
/// Skipping artifact verification for an unsealed prefix (as re-projecting via
/// `reduce_all` alone would) would let catch-up accept — and the ledger then report as
/// terminal — a record `replay` itself would reject once sealed.
///
/// # Errors
/// [`ReplayError::LegacyNonReplayable`] for an empty stream; [`ReplayError::EventDigestMismatch`]
/// / [`ReplayError::ChainBroken`] for a tampered/broken chain; [`ReplayError::Reduce`] for a
/// structurally invalid stream; [`ReplayError::ArtifactUnresolved`] /
/// [`ReplayError::ArtifactDigestMismatch`] for a missing or altered referenced artifact.
/// Never returns [`ReplayError::NotSealed`] — that is [`replay`]'s own, additional check.
pub fn verify_prefix(
    events: &[RunEvent],
    artifacts: &dyn ArtifactResolver,
) -> Result<(crate::state::RunState, u64), ReplayError> {
    // 1. A record with no canonical events is legacy — never "verified".
    if events.is_empty() {
        return Err(ReplayError::LegacyNonReplayable);
    }

    // 2. Per-event digest self-consistency and chain-link continuity. A deletion, reorder,
    //    insertion, or in-place edit is caught here before anything is trusted.
    let mut prev = Digest256::genesis();
    for event in events {
        if !event.digest_is_consistent() {
            return Err(ReplayError::EventDigestMismatch {
                sequence: event.sequence,
            });
        }
        if event.previous_event_digest != prev {
            return Err(ReplayError::ChainBroken {
                sequence: event.sequence,
            });
        }
        prev = event.event_digest.clone();
    }

    // 3. Structural validation via the pure reducer (sealed or not).
    let state = reduce_all(events)?;

    // 4. Resolve every referenced artifact and verify its content digest in place —
    //    UNCONDITIONALLY, whether or not the run is sealed yet. Distinct (locator, digest)
    //    pairs are resolved once — a policy declared in the contract and later
    //    re-referenced by a PolicyChecked is not double-resolved or double-counted.
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    let mut unique: Vec<&ArtifactRef> = Vec::new();
    for event in events {
        for artifact in referenced_artifacts(&event.kind) {
            if seen.insert((artifact.locator.as_str(), artifact.digest.as_str())) {
                unique.push(artifact);
            }
        }
    }
    let artifacts_verified = unique.len() as u64;
    for artifact in unique {
        let bytes = artifacts.resolve(artifact)?;
        if Digest256::of_bytes(&bytes) != artifact.digest {
            return Err(ReplayError::ArtifactDigestMismatch {
                locator: artifact.locator.clone(),
            });
        }
    }

    Ok((state, artifacts_verified))
}

/// The result of classifying a stored (or not-yet-existing) canonical
/// record — Q-Deck R1 fourth corrective round: a caller deciding whether
/// re-invoking a provider for some already-bound run id is safe must never
/// answer that question from a ledger projection alone (a ledger row can
/// be stale, absent, or simply not yet caught up) — it must classify the
/// record itself, via the SAME chain/digest/reducer/artifact verification
/// [`verify_prefix`] and [`replay`] are built on. Four states, so a
/// tampered-but-nonempty stream can never be confused with a merely
/// not-yet-started one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordVerdict {
    /// No canonical stream at all (missing or empty `events.jsonl`) — not
    /// sealed, not even a replay candidate; safe to treat as "never
    /// started".
    NoCanonicalStream,
    /// A valid, fully chain/digest/artifact-verified prefix with no fixed
    /// verdict yet — genuinely still in progress (or crashed before
    /// sealing).
    ValidUnsealed,
    /// A valid, fully verified, SEALED record — the provider already ran
    /// to completion. Carries the same evidence [`replay`] would report.
    ValidSealed(ReplayReport),
    /// The stream is non-empty but fails verification (tampered, corrupt,
    /// or otherwise structurally invalid). MUST fail closed — never
    /// treated as either "never started" or "already sealed".
    Invalid(ReplayError),
}

/// Classify a stored run's canonical record without requiring it to be
/// sealed — the shared primitive behind both a plain "is this replayable"
/// check and Q-Deck R1's pre-redrive decision (`crates/o7d/src/routes.rs`):
/// a retry must recover an existing, already-sealed child rather than
/// invoke the provider a second time, and ledger status alone can never be
/// trusted to make that call. Reuses [`verify_prefix`] — not a second,
/// lighter-weight parser; every byte of chain/digest/reducer/artifact
/// verification a sealed run gets is applied here too.
///
/// # Errors
/// Never returns an `Err` — verification failure is reported as
/// [`RecordVerdict::Invalid`], not propagated, so a caller can pattern-match
/// a single typed result instead of threading two failure channels.
#[must_use]
pub fn classify_record(events: &[RunEvent], artifacts: &dyn ArtifactResolver) -> RecordVerdict {
    if events.is_empty() {
        return RecordVerdict::NoCanonicalStream;
    }
    let (state, artifacts_verified) = match verify_prefix(events, artifacts) {
        Ok(ok) => ok,
        Err(e) => return RecordVerdict::Invalid(e),
    };
    let Some(verdict) = state.verdict else {
        return RecordVerdict::ValidUnsealed;
    };
    let Some(last) = events.last() else {
        // Unreachable: `events.is_empty()` was already checked above, and
        // `verify_prefix` cannot produce a sealed verdict from an empty
        // slice — kept as a typed `Invalid` rather than an early-return
        // panic so this function truly never panics on any input.
        return RecordVerdict::Invalid(ReplayError::LegacyNonReplayable);
    };
    let Some(normalized_state_digest) = state.normalized_digest() else {
        return RecordVerdict::Invalid(ReplayError::StateDigestUnavailable);
    };
    RecordVerdict::ValidSealed(ReplayReport {
        verdict,
        events_verified: events.len() as u64,
        artifacts_verified,
        final_event_digest: last.event_digest.clone(),
        normalized_state_digest,
    })
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
    let (state, artifacts_verified) = verify_prefix(events, artifacts)?;

    // Only a sealed run has a fixed verdict to verify — checked AFTER, not instead of,
    // full artifact verification above, so an unsealed run with an already-altered
    // artifact is reported as tampered, not merely "not sealed yet".
    let Some(verdict) = state.verdict else {
        return Err(ReplayError::NotSealed);
    };

    // Anchors. `events` is non-empty (`verify_prefix` already rejected the empty case).
    let last = events.last().ok_or(ReplayError::LegacyNonReplayable)?;
    let normalized_state_digest = state
        .normalized_digest()
        .ok_or(ReplayError::StateDigestUnavailable)?;

    Ok(ReplayReport {
        verdict,
        events_verified: events.len() as u64,
        artifacts_verified,
        final_event_digest: last.event_digest.clone(),
        normalized_state_digest,
    })
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
    let report = replay(events, artifacts)?;
    if report.verdict != stored_verdict {
        return Err(ReplayError::VerdictMismatch {
            recomputed: report.verdict,
            stored: stored_verdict,
        });
    }
    Ok(report)
}

/// The artifact references a single event carries (resolved and digest-checked by replay).
fn referenced_artifacts(kind: &RunEventKind) -> Vec<&ArtifactRef> {
    match kind {
        RunEventKind::RunStarted { contract, task } => {
            let mut refs = vec![task];
            for p in &contract.policy_obligations {
                refs.push(&p.policy);
            }
            refs
        }
        RunEventKind::WorktreeCreated { worktree } => vec![worktree],
        RunEventKind::PatchCaptured { patch } => vec![patch],
        RunEventKind::PolicyChecked { policy, .. } => vec![policy],
        RunEventKind::GateFinished { log: Some(log), .. } => vec![log],
        RunEventKind::SandboxEvidenceCaptured { report, .. } => vec![report],
        RunEventKind::AgentStarted
        | RunEventKind::AgentExited { .. }
        | RunEventKind::GateStarted { .. }
        | RunEventKind::GateFinished { log: None, .. }
        | RunEventKind::RunSealed => Vec::new(),
    }
}
