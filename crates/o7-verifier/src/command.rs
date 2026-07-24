//! The shape of a verifier command.
//!
//! A verifier is run as an ABSOLUTE executable plus an argv ARRAY — never a shell
//! string, never a PATH search. Its working directory is explicit, its environment is
//! an explicit allowlist (the child inherits nothing else), and its timeout, output
//! size, and exit interpretation are all bounded and explicit. None of this is inferred
//! from the repository.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Hard upper bound on a verifier timeout — mirrors the worker's `MAX_TIMEOUT` so a
/// duration that would overflow a timer is rejected before anything is spawned.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(3600);

/// Hard ceiling on the retained-output budget, so an absurd policy cannot ask the
/// verifier to buffer an unbounded amount of evidence.
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;

/// Where the verifier command runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CwdPolicy {
    /// The materialized worktree root (resolved at run time). This is the normal case:
    /// a verifier inspects the agent's committed working copy.
    WorktreeRoot,
    /// An explicit absolute path (e.g. a fixed tools directory). Never repository-relative.
    Absolute(PathBuf),
}

/// Which process exit codes the verifier command treats as a *candidate* pass. This is
/// evidence policy, not a verdict: even an in-policy exit is only a candidate that o7d
/// adjudicates (see [`crate::verdict`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExitPolicy {
    success_codes: BTreeSet<i32>,
}

impl ExitPolicy {
    /// The usual policy: only exit code 0 is a success candidate.
    #[must_use]
    pub fn exactly_zero() -> Self {
        Self {
            success_codes: BTreeSet::from([0]),
        }
    }

    /// An explicit set of success codes.
    #[must_use]
    pub fn codes(codes: impl IntoIterator<Item = i32>) -> Self {
        Self {
            success_codes: codes.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn is_success(&self, code: i32) -> bool {
        self.success_codes.contains(&code)
    }

    /// The success codes in ascending order — a deterministic ordering for binding the
    /// policy into the trust digest, so a change to the accepted codes invalidates trust.
    #[must_use]
    pub fn success_codes_sorted(&self) -> Vec<i32> {
        self.success_codes.iter().copied().collect()
    }
}

/// Bounds on how much output the verifier will retain as evidence. Exceeding the cap is
/// an OUTPUT-LOSS failure (never a silent truncation, never a pass).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputLimits {
    pub max_total_bytes: usize,
}

impl Default for OutputLimits {
    fn default() -> Self {
        Self {
            max_total_bytes: 1024 * 1024,
        }
    }
}

/// A statically-detectable problem with a [`TrustedCommand`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CommandError {
    #[error("executable must be an absolute path (no PATH search, no shell): {0}")]
    RelativeExecutable(PathBuf),
    #[error("cwd policy Absolute({0}) must be an absolute path")]
    RelativeCwd(PathBuf),
    #[error("timeout must be > 0 and <= {MAX_TIMEOUT:?}; got {0:?}")]
    Timeout(Duration),
    #[error("output.max_total_bytes must be in 1..={MAX_OUTPUT_BYTES}; got {0}")]
    OutputBudget(usize),
    #[error("exit policy must list at least one success code")]
    EmptyExitPolicy,
}

/// A fully-specified verifier command. Immutable once built; validated before use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedCommand {
    /// Absolute path to the executable (never a shell, never PATH-resolved).
    pub executable: PathBuf,
    /// The argv array, passed verbatim — no shell parsing.
    pub arguments: Vec<OsString>,
    /// Where it runs.
    pub cwd_policy: CwdPolicy,
    /// The COMPLETE environment for the child; nothing else is inherited.
    pub environment: BTreeMap<OsString, OsString>,
    /// Bounded wall-clock timeout.
    pub timeout: Duration,
    /// Bounded retained-output budget.
    pub output_limits: OutputLimits,
    /// Explicit exit interpretation.
    pub exit_policy: ExitPolicy,
}

impl TrustedCommand {
    /// Validate the statically-checkable invariants (absolute exe, absolute explicit
    /// cwd, bounded timeout/output, non-empty exit policy).
    ///
    /// # Errors
    /// [`CommandError`] for any violated invariant.
    pub fn validate(&self) -> Result<(), CommandError> {
        if !self.executable.is_absolute() {
            return Err(CommandError::RelativeExecutable(self.executable.clone()));
        }
        if let CwdPolicy::Absolute(path) = &self.cwd_policy {
            if !path.is_absolute() {
                return Err(CommandError::RelativeCwd(path.clone()));
            }
        }
        if self.timeout.is_zero() || self.timeout > MAX_TIMEOUT {
            return Err(CommandError::Timeout(self.timeout));
        }
        if self.output_limits.max_total_bytes == 0
            || self.output_limits.max_total_bytes > MAX_OUTPUT_BYTES
        {
            return Err(CommandError::OutputBudget(
                self.output_limits.max_total_bytes,
            ));
        }
        if self.exit_policy == ExitPolicy::codes(std::iter::empty()) {
            return Err(CommandError::EmptyExitPolicy);
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn base() -> TrustedCommand {
        TrustedCommand {
            executable: PathBuf::from("/usr/bin/true"),
            arguments: vec![OsString::from("--check")],
            cwd_policy: CwdPolicy::WorktreeRoot,
            environment: BTreeMap::new(),
            timeout: Duration::from_secs(30),
            output_limits: OutputLimits::default(),
            exit_policy: ExitPolicy::exactly_zero(),
        }
    }

    #[test]
    fn accepts_a_valid_command() {
        assert!(base().validate().is_ok());
    }

    #[test]
    fn rejects_relative_executable_and_cwd() {
        let mut c = base();
        c.executable = PathBuf::from("true");
        assert!(matches!(
            c.validate(),
            Err(CommandError::RelativeExecutable(_))
        ));

        let mut c = base();
        c.cwd_policy = CwdPolicy::Absolute(PathBuf::from("rel/dir"));
        assert!(matches!(c.validate(), Err(CommandError::RelativeCwd(_))));
    }

    #[test]
    fn rejects_unbounded_timeout_and_output() {
        let mut c = base();
        c.timeout = Duration::ZERO;
        assert!(matches!(c.validate(), Err(CommandError::Timeout(_))));
        c.timeout = MAX_TIMEOUT + Duration::from_secs(1);
        assert!(matches!(c.validate(), Err(CommandError::Timeout(_))));

        let mut c = base();
        c.output_limits.max_total_bytes = 0;
        assert!(matches!(c.validate(), Err(CommandError::OutputBudget(_))));
        c.output_limits.max_total_bytes = MAX_OUTPUT_BYTES + 1;
        assert!(matches!(c.validate(), Err(CommandError::OutputBudget(_))));
    }

    #[test]
    fn rejects_empty_exit_policy() {
        let mut c = base();
        c.exit_policy = ExitPolicy::codes(std::iter::empty());
        assert!(matches!(c.validate(), Err(CommandError::EmptyExitPolicy)));
    }

    #[test]
    fn exit_policy_membership() {
        let p = ExitPolicy::codes([0, 2]);
        assert!(p.is_success(0));
        assert!(p.is_success(2));
        assert!(!p.is_success(1));
    }
}
