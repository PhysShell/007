//! `o7-verifier` — the trusted verifier for 007.
//!
//! It runs an explicitly-trusted command (an absolute executable plus an argv array,
//! never a shell) with an explicit working directory, an environment allowlist, a
//! bounded timeout, and a bounded output budget, and returns EVIDENCE of what happened.
//! It does NOT decide accept/reject — that verdict belongs to o7d (see [`verdict`]).
//!
//! Trust is bound to the canonical repository identity, the executable identity, the
//! argv, the cwd policy, and a command digest over all of them; any drift invalidates
//! trust. Trust is never sourced from repository config. A production run requires a
//! `RequireFullyEnforced` ProcessBoundary with no fallback to an unconfined host
//! boundary — so, until a fully-enforced boundary (Sandboy) exists, production
//! execution is unavailable by construction.
//!
//! Slice 3 ships the pure pieces (command shape, trust binding, evidence, verdict). The
//! boundary-integrated runner is slice 4.

pub mod command;
pub mod evidence;
pub mod trust;
pub mod verdict;

pub use command::{
    CommandError, CwdPolicy, ExitPolicy, OutputLimits, TrustedCommand, MAX_OUTPUT_BYTES,
    MAX_TIMEOUT,
};
pub use evidence::{AttestedEnforcement, VerifierEvidence, VerifierOutcome};
pub use trust::{CommandDigest, ExecutableIdentity, TrustAnchor, TrustError, TrustStore};
pub use verdict::{adjudicate, Verdict};
