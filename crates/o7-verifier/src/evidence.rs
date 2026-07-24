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

use o7_worktree::CanonicalRepoId;

use crate::command::TrustedCommand;
use crate::trust::{CommandDigest, ExecutableIdentity};

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

/// Everything a verifier run observed, plus the FULL trust binding it was run under. This
/// is evidence, not a decision: it carries no `is_accepted` and no method yields a
/// verdict. Adjudication is [`crate::verdict::adjudicate`], which RE-DERIVES the trust
/// digest from the fields below and checks it against o7d's trust store — so a forged or
/// deserialized evidence (a flipped `trusted`, a widened `command`, a substituted digest)
/// cannot accept itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierEvidence {
    pub outcome: VerifierOutcome,
    /// Self-description only: whether the runner found the command trusted. Adjudication
    /// NEVER consults this flag — it re-derives trust from the store — so flipping it to
    /// `true` on a forged evidence buys nothing.
    pub trusted: bool,
    /// The enforcement the boundary attested, when a boundary was consulted (a run-time
    /// OBSERVATION, checked against the command's bound requirement at adjudication).
    pub boundary_enforcement: Option<AttestedEnforcement>,
    /// The repository the run was bound to (part of the trust digest).
    pub repo: CanonicalRepoId,
    /// The exact command that was run — argv, cwd, env, exit policy, timeout, output
    /// budget, and the BOUND boundary requirement. Adjudication reads the exit policy and
    /// boundary requirement from HERE (never from a late caller argument) and folds the
    /// whole thing back into the trust digest.
    pub command: TrustedCommand,
    /// The executable content identity the runner bound (`None` only when the executable
    /// was never read — an invalid command or an unreadable file, which never passes).
    pub executable_identity: Option<ExecutableIdentity>,
    /// The EXACT full trust digest the runner computed over the bytes it ran (`None` when
    /// no trust binding was formed). Re-derived and checked at adjudication.
    pub trust_digest: Option<CommandDigest>,
    /// The STRUCTURAL digest (no executable content). Diagnostic; NEVER the trust key.
    pub structural_digest: CommandDigest,
    /// Retained stdout (bounded by the command's output budget).
    pub stdout: Vec<u8>,
    /// Retained stderr (bounded by the command's output budget).
    pub stderr: Vec<u8>,
}

impl VerifierEvidence {
    /// Whether this evidence could, on its face, be a pass under its OWN bound exit
    /// policy.
    ///
    /// This is a NECESSARY condition, NEVER sufficient: it is true only for a clean
    /// completion with an exit code in the command's bound policy. Every non-completion
    /// (not-run, spawn failure, timeout, signal, output loss, boundary-unavailable,
    /// fault) is false. It is deliberately NOT called `is_pass` — the accept/reject
    /// verdict is o7d's, in [`crate::verdict`], which additionally requires the trust
    /// digest to be in the store and the boundary requirement to be met.
    #[must_use]
    pub fn is_pass_candidate(&self) -> bool {
        matches!(&self.outcome, VerifierOutcome::Completed { exit_code } if self.command.exit_policy.is_success(*exit_code))
    }
}
