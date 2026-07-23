//! The verdict — o7d's authority, not the verifier's.
//!
//! Evidence describes what happened; only o7d turns evidence into an accept/reject
//! decision. This module is that authority. It exists as a separate function precisely
//! so a [`crate::evidence::VerifierEvidence`] can never "self-accept": there is no
//! method on the evidence that yields [`Verdict::Accepted`], and this adjudication
//! additionally re-checks trust and the boundary requirement, both of which the
//! evidence alone cannot satisfy.

use o7_worker::BoundaryRequirement;

use crate::command::ExitPolicy;
use crate::evidence::{AttestedEnforcement, VerifierEvidence, VerifierOutcome};

/// o7d's decision over a piece of verifier evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The run is accepted.
    Accepted,
    /// The run is rejected, with a reason.
    Rejected(String),
}

impl Verdict {
    #[must_use]
    pub fn is_accepted(&self) -> bool {
        matches!(self, Verdict::Accepted)
    }
}

/// Adjudicate evidence into a verdict — the step o7d owns.
///
/// Acceptance requires ALL of:
///   * the boundary requirement is met by the attested enforcement (under
///     `RequireFullyEnforced`, only a `FullyEnforced` boundary qualifies — no fallback);
///   * the command was trusted at run time;
///   * the outcome is a clean completion with an in-policy exit code.
///
/// Every non-completion (not-run, spawn failure, timeout, signal, output loss,
/// boundary-unavailable, fault) is rejected. Evidence can never accept itself: this
/// function is the only path to [`Verdict::Accepted`].
#[must_use]
pub fn adjudicate(
    evidence: &VerifierEvidence,
    exit_policy: &ExitPolicy,
    requirement: BoundaryRequirement,
) -> Verdict {
    // 1. Boundary requirement — fail closed, no fallback.
    if let BoundaryRequirement::RequireFullyEnforced = requirement {
        match evidence.boundary_enforcement {
            Some(AttestedEnforcement::FullyEnforced) => {}
            other => {
                return Verdict::Rejected(format!(
                    "boundary requirement RequireFullyEnforced not met: attested {other:?}"
                ));
            }
        }
    }
    // 2. Trust — a non-trusted command is never accepted.
    if !evidence.trusted {
        return Verdict::Rejected("command was not trusted at run time".to_owned());
    }
    // 3. Outcome — only a clean, in-policy completion is a pass. Everything else is a
    //    reject; enumerate so a new outcome variant forces a decision here.
    match &evidence.outcome {
        VerifierOutcome::Completed { exit_code } => {
            if exit_policy.is_success(*exit_code) {
                Verdict::Accepted
            } else {
                Verdict::Rejected(format!(
                    "exit code {exit_code} is not in the success policy"
                ))
            }
        }
        VerifierOutcome::NotRun { reason }
        | VerifierOutcome::SpawnFailed { reason }
        | VerifierOutcome::OutputLost { reason }
        | VerifierOutcome::BoundaryUnavailable { reason }
        | VerifierOutcome::Faulted { reason } => {
            Verdict::Rejected(format!("{}: {reason}", evidence.outcome.kind()))
        }
        VerifierOutcome::TimedOut => Verdict::Rejected("verifier timed out".to_owned()),
        VerifierOutcome::Signalled { signal } => {
            Verdict::Rejected(format!("verifier was terminated by signal {signal}"))
        }
    }
}
