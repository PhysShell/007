//! The verdict — o7d's authority, not the verifier's.
//!
//! Evidence describes what happened; only o7d turns evidence into an accept/reject
//! decision, and only against its own **trust store**. This module is that authority.
//!
//! [`adjudicate`] takes NO independent exit-policy or boundary argument. It RE-DERIVES the
//! full trust digest from the evidence's own carried binding (repository, the exact
//! command including its bound exit policy and boundary requirement, and the executable
//! identity), requires that digest to equal the one the evidence claims AND to be present
//! in o7d's [`TrustStore`], and only then evaluates the run-time observations (the
//! attested enforcement against the command's BOUND requirement, and the exit code against
//! the command's BOUND exit policy). Consequences:
//!
//!   * a forged `trusted = true` buys nothing — the flag is never read;
//!   * a widened exit policy or a relaxed boundary requirement changes the digest, so it
//!     is no longer in the store and is rejected;
//!   * a command trusted for `RequireFullyEnforced` cannot be re-adjudicated as if it were
//!     `AllowUnconfined` (there is no boundary parameter to relax, and the requirement is
//!     bound into the digest);
//!   * revoking the digest from the store makes prior evidence reject;
//!   * a swapped executable (different identity) yields a different digest and rejects.

use crate::evidence::{AttestedEnforcement, VerifierEvidence, VerifierOutcome};
use crate::trust::{TrustAnchor, TrustStore};

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

/// Adjudicate evidence into a verdict against o7d's trust store — the step o7d owns.
///
/// Acceptance requires ALL of:
///   * the evidence carries a trust binding (executable identity + claimed trust digest);
///   * the trust digest RE-DERIVED from the evidence's own binding equals the claimed one
///     (a self-inconsistent/forged evidence is rejected);
///   * that digest is present in `trust_store` (revocation or any drift ⇒ absent ⇒ reject);
///   * the attested enforcement satisfies the command's OWN BOUND boundary requirement
///     (under `RequireFullyEnforced`, only `FullyEnforced` qualifies — no fallback);
///   * the outcome is a clean completion with an exit code in the command's OWN BOUND
///     exit policy.
///
/// Every non-completion (not-run, spawn failure, timeout, signal, output loss,
/// boundary-unavailable, fault) is rejected. This is the ONLY path to [`Verdict::Accepted`];
/// evidence can never accept itself.
#[must_use]
pub fn adjudicate(evidence: &VerifierEvidence, trust_store: &TrustStore) -> Verdict {
    // 1. There must be a trust binding at all.
    let (Some(exe_identity), Some(claimed_digest)) =
        (&evidence.executable_identity, &evidence.trust_digest)
    else {
        return Verdict::Rejected(
            "evidence carries no trust binding — nothing was trusted".to_owned(),
        );
    };

    // 2. RE-DERIVE the full trust digest from the evidence's OWN carried binding. A forged
    //    field (a widened exit policy, a relaxed boundary requirement, a swapped identity)
    //    changes this digest; a mismatch with the claimed digest is a rejected forgery.
    let anchor = TrustAnchor::from_parts(&evidence.repo, &evidence.command, exe_identity.clone());
    if anchor.digest() != claimed_digest {
        return Verdict::Rejected(
            "evidence trust digest is inconsistent with its own command binding".to_owned(),
        );
    }

    // 3. The re-derived digest must be in o7d's trust store. Revocation, exe drift, a
    //    structural (non-trust) digest, or any spec drift all fall out here.
    if !trust_store.is_trusted(&anchor) {
        return Verdict::Rejected(
            "the command is not in o7d's trust store (revoked, drifted, or never trusted)"
                .to_owned(),
        );
    }

    // 4. Boundary requirement — taken from the command's BOUND requirement, never a caller
    //    argument. Fail closed, no fallback.
    if let crate::command::RequiredBoundary::RequireFullyEnforced =
        evidence.command.boundary_requirement
    {
        match evidence.boundary_enforcement {
            Some(AttestedEnforcement::FullyEnforced) => {}
            other => {
                return Verdict::Rejected(format!(
                    "boundary requirement RequireFullyEnforced not met: attested {other:?}"
                ));
            }
        }
    }

    // 5. Outcome — only a clean completion whose exit code is in the command's OWN bound
    //    policy is a pass. Everything else rejects; enumerate so a new outcome variant
    //    forces a decision here.
    match &evidence.outcome {
        VerifierOutcome::Completed { exit_code } => {
            if evidence.command.exit_policy.is_success(*exit_code) {
                Verdict::Accepted
            } else {
                Verdict::Rejected(format!(
                    "exit code {exit_code} is not in the command's bound success policy"
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
