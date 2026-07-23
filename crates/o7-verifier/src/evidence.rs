//! Verifier EVIDENCE — what a verifier run observed. Never a verdict.
//!
//! A [`VerifierEvidence`] records what happened and carries no accept/reject decision.
//! Crucially, every abnormal outcome — not-run, spawn failure, timeout, signal
//! termination, output loss, boundary unavailability, supervisor fault — is a DISTINCT
//! non-completion, and none of them is ever a pass. The only outcome that can even be a
//! pass *candidate* is a clean [`VerifierOutcome::Completed`] with an in-policy exit
//! code, and even that is adjudicated by o7d (see [`crate::verdict`]).

use o7_worker::EnforcementLevel;
use serde::{Deserialize, Serialize};

use crate::command::ExitPolicy;
use crate::trust::CommandDigest;

/// A serializable mirror of [`o7_worker::EnforcementLevel`], so evidence can be durably
/// recorded (the worker enum is not itself serde-serializable). Attestation is always
/// the boundary's own honest self-description, never inferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttestedEnforcement {
    None,
    Partial,
    FullyEnforced,
}

impl From<EnforcementLevel> for AttestedEnforcement {
    fn from(level: EnforcementLevel) -> Self {
        match level {
            EnforcementLevel::None => Self::None,
            EnforcementLevel::Partial => Self::Partial,
            EnforcementLevel::FullyEnforced => Self::FullyEnforced,
        }
    }
}

/// How a verifier run ended. Only [`VerifierOutcome::Completed`] represents the process
/// running to a normal exit; every other variant is a non-completion that can never be
/// a pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerifierOutcome {
    /// The process ran and exited normally with this code.
    Completed { exit_code: i32 },
    /// The command was never spawned (e.g. it was not trusted, or the required boundary
    /// was not available). Carries why.
    NotRun { reason: String },
    /// The boundary tried but failed to spawn the process.
    SpawnFailed { reason: String },
    /// The command exceeded its bounded timeout and its whole process set was killed.
    TimedOut,
    /// The process was terminated by a signal (never a normal exit).
    Signalled { signal: i32 },
    /// Output faithfulness was lost (a read error, or the retained-output budget was
    /// exceeded). The result cannot be trusted.
    OutputLost { reason: String },
    /// The required boundary could not be provided/attested, so nothing ran.
    BoundaryUnavailable { reason: String },
    /// The supervisor faulted (e.g. an unprovable teardown) — the run cannot be trusted.
    Faulted { reason: String },
}

impl VerifierOutcome {
    /// A stable, machine-readable tag.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Completed { .. } => "COMPLETED",
            Self::NotRun { .. } => "NOT_RUN",
            Self::SpawnFailed { .. } => "SPAWN_FAILED",
            Self::TimedOut => "TIMED_OUT",
            Self::Signalled { .. } => "SIGNALLED",
            Self::OutputLost { .. } => "OUTPUT_LOST",
            Self::BoundaryUnavailable { .. } => "BOUNDARY_UNAVAILABLE",
            Self::Faulted { .. } => "FAULTED",
        }
    }
}

/// Everything a verifier run observed. This is evidence, not a decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierEvidence {
    pub outcome: VerifierOutcome,
    /// Whether the command was trusted at run time. A non-trusted command is never run,
    /// but this is carried so the evidence is self-describing.
    pub trusted: bool,
    /// The enforcement the boundary attested, when a boundary was consulted.
    pub boundary_enforcement: Option<AttestedEnforcement>,
    /// The digest of the command this evidence is about.
    pub command_digest: CommandDigest,
    /// Retained stdout (bounded by the command's output budget).
    pub stdout: Vec<u8>,
    /// Retained stderr (bounded by the command's output budget).
    pub stderr: Vec<u8>,
}

impl VerifierEvidence {
    /// Whether this evidence could, on its face, be a pass under `policy`.
    ///
    /// This is a NECESSARY condition, NEVER sufficient: it is true only for a clean
    /// completion with an in-policy exit code. Every non-completion (not-run, spawn
    /// failure, timeout, signal, output loss, boundary-unavailable, fault) is false. It
    /// is deliberately NOT called `is_pass` — the accept/reject verdict is o7d's, in
    /// [`crate::verdict`], which additionally requires trust and the boundary
    /// requirement.
    #[must_use]
    pub fn is_pass_candidate(&self, policy: &ExitPolicy) -> bool {
        matches!(&self.outcome, VerifierOutcome::Completed { exit_code } if policy.is_success(*exit_code))
    }
}
