//! The confinement request ([`SandboxPolicy`]) and the per-dimension enforcement vocabulary
//! it is reported against. The policy hashes to a canonical [`Digest256`] so a report can be
//! bound to the exact policy that was installed.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::ids::Digest256;

/// A confinement dimension a backend reports on independently. There is no single boolean
/// "secure": a real boundary enforces some dimensions and not others and must say which.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxDimension {
    Filesystem,
    Network,
    Env,
    ProcessTree,
    Timeout,
}

impl SandboxDimension {
    /// Every dimension a fully-enforced report must account for.
    pub const ALL: [SandboxDimension; 5] = [
        SandboxDimension::Filesystem,
        SandboxDimension::Network,
        SandboxDimension::Env,
        SandboxDimension::ProcessTree,
        SandboxDimension::Timeout,
    ];
}

/// How thoroughly one dimension was enforced. Three-valued on purpose: `Partial` is an
/// honest admission and must never be silently promoted to `Enforced`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    Enforced,
    Partial,
    NotEnforced,
}

/// Network posture requested of the backend. The first slice expresses only deny-all (the
/// safe default for a worktree run); a richer allowlist is a later widening. A backend that
/// cannot prove a real deny-all on the running kernel/ABI must report the network dimension
/// as `Partial`/`NotEnforced`, not `Enforced`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicy {
    DenyAll,
}

/// The smallest and largest wall-clock deadline a policy may request. A sub-millisecond
/// timeout would serialize to `--timeout-ms 0` (i.e. "no deadline"); an unbounded one is a
/// foot-gun. Both fail validation.
pub const MIN_TIMEOUT: Duration = Duration::from_millis(1);
/// 24h — generous, but a finite ceiling so `timeout: enforced` means a real deadline.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(24 * 60 * 60);

/// What confinement a caller asks the backend to install. Validated by
/// [`SandboxPolicy::validate`]; a degenerate policy that would confine nothing never
/// becomes a `FullyEnforced` boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPolicy {
    /// The single writable root (the run's worktree).
    pub worktree: PathBuf,
    /// Absolute paths the confined child may READ+EXECUTE. MUST cover the exec target,
    /// including PR 3's sealed-memfd path `/proc/<verifier_pid>/fd/<n>`.
    pub allow_exec: Vec<PathBuf>,
    /// Network posture (deny-all in the first slice).
    pub network: NetworkPolicy,
    /// The COMPLETE set of environment variable names the child may keep.
    pub env_allowlist: Vec<std::ffi::OsString>,
    /// Wall-clock deadline; must be within `[MIN_TIMEOUT, MAX_TIMEOUT]`.
    pub timeout: Duration,
}

/// Why a [`SandboxPolicy`] cannot be trusted to fully confine.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SandboxPolicyError {
    #[error("worktree must be an absolute path: {0}")]
    RelativeWorktree(PathBuf),
    #[error("exec allowance must be an absolute path: {0}")]
    RelativeExecAllowance(PathBuf),
    #[error("policy grants no executable path; nothing could be launched")]
    NoExecAllowance,
    /// A duplicate exec allowance — allowances are a SET, so a repeat is a user error (and
    /// would otherwise give two policies the same meaning but different digests).
    #[error("duplicate exec allowance: {0}")]
    DuplicateExecAllowance(PathBuf),
    /// A duplicate env-allowlist name (same rationale).
    #[error("duplicate env allowlist name: {0:?}")]
    DuplicateEnvName(std::ffi::OsString),
    #[error(
        "timeout {0:?} is below the {MIN_TIMEOUT:?} minimum (would serialize to a zero deadline)"
    )]
    TimeoutTooSmall(Duration),
    #[error("timeout {0:?} exceeds the {MAX_TIMEOUT:?} maximum")]
    TimeoutTooLarge(Duration),
}

impl SandboxPolicy {
    /// Validate that this policy is capable of full confinement.
    ///
    /// # Errors
    /// [`SandboxPolicyError`] for a relative worktree/allowance, an empty exec-allowance
    /// set, or an out-of-bounds timeout.
    pub fn validate(&self) -> Result<(), SandboxPolicyError> {
        if !self.worktree.is_absolute() {
            return Err(SandboxPolicyError::RelativeWorktree(self.worktree.clone()));
        }
        if self.allow_exec.is_empty() {
            return Err(SandboxPolicyError::NoExecAllowance);
        }
        // Allowances/env are SETS: a duplicate is rejected loudly rather than silently
        // canonicalized, so the user sees the error instead of an unnoticed policy-identity
        // change (a hashed multiset masquerading as a set).
        let mut seen_exec: BTreeSet<&[u8]> = BTreeSet::new();
        for path in &self.allow_exec {
            if !path.is_absolute() {
                return Err(SandboxPolicyError::RelativeExecAllowance(path.clone()));
            }
            if !seen_exec.insert(path.as_os_str().as_encoded_bytes()) {
                return Err(SandboxPolicyError::DuplicateExecAllowance(path.clone()));
            }
        }
        let mut seen_env: BTreeSet<&[u8]> = BTreeSet::new();
        for name in &self.env_allowlist {
            if !seen_env.insert(name.as_encoded_bytes()) {
                return Err(SandboxPolicyError::DuplicateEnvName(name.clone()));
            }
        }
        if self.timeout < MIN_TIMEOUT {
            return Err(SandboxPolicyError::TimeoutTooSmall(self.timeout));
        }
        if self.timeout > MAX_TIMEOUT {
            return Err(SandboxPolicyError::TimeoutTooLarge(self.timeout));
        }
        Ok(())
    }

    /// Whether `target` may execute under this policy: it must equal, or lie beneath, one of
    /// the declared exec allowances. This is the pre-spawn gate that keeps PR 3's sealed
    /// memfd exec (`/proc/<verifier_pid>/fd/<n>`) working: grant that path (or its
    /// `/proc/<pid>/fd` directory) and the exec is allowed; grant nothing and the boundary
    /// fails closed. NOTE: this is a LEXICAL check on the request — it proves the policy
    /// *declares* the allowance, not that Landlock actually grants the sealed object; that
    /// is proven only by the live sealed-memfd acceptance probe.
    #[must_use]
    pub fn permits_exec(&self, target: &Path) -> bool {
        self.allow_exec
            .iter()
            .any(|allowed| target == allowed || target.starts_with(allowed))
    }

    /// A canonical, order-independent digest of the policy: the same allowances and settings
    /// hash identically regardless of declaration order, so a report's `policy_digest` binds
    /// to the policy's MEANING, not its spelling.
    #[must_use]
    pub fn digest(&self) -> Digest256 {
        Digest256::of_bytes(&self.canonical_bytes())
    }

    fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(b"o7-sandbox-policy\0v1\0");
        push_field(&mut buf, b"worktree", self.worktree.as_os_str());
        buf.push(match self.network {
            NetworkPolicy::DenyAll => 0x01,
        });
        // Allowances and env names are SETS: sort their encoded bytes so order does not
        // change the digest.
        let mut execs: Vec<Vec<u8>> = self
            .allow_exec
            .iter()
            .map(|p| p.as_os_str().as_encoded_bytes().to_vec())
            .collect();
        execs.sort();
        push_len(&mut buf, execs.len() as u64);
        for e in &execs {
            push_bytes(&mut buf, e);
        }
        let mut envs: Vec<Vec<u8>> = self
            .env_allowlist
            .iter()
            .map(|s| s.as_encoded_bytes().to_vec())
            .collect();
        envs.sort();
        push_len(&mut buf, envs.len() as u64);
        for e in &envs {
            push_bytes(&mut buf, e);
        }
        // Fixed-width nanoseconds — no ambiguity between ms/ns representations.
        buf.extend_from_slice(&self.timeout.as_nanos().to_be_bytes());
        buf
    }
}

fn push_len(buf: &mut Vec<u8>, len: u64) {
    buf.extend_from_slice(&len.to_be_bytes());
}

fn push_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    push_len(buf, bytes.len() as u64);
    buf.extend_from_slice(bytes);
}

fn push_field(buf: &mut Vec<u8>, tag: &[u8], value: &OsStr) {
    push_bytes(buf, tag);
    push_bytes(buf, value.as_encoded_bytes());
}
