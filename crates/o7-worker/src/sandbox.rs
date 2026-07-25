//! The Sandboy confinement contract: what confinement to request ([`SandboxPolicy`])
//! and what the backend actually enforced ([`SandboxReport`]).
//!
//! This module is PURE — no I/O, no `unsafe`, no process launching. The real Landlock +
//! seccomp mechanism lives in an external `sandboy` backend binary (see
//! `docs/architecture/sandboy-boundary.md`); this module only describes the request and
//! interprets the backend's machine-readable report into an honest
//! [`BoundaryAttestation`]. Keeping it pure lets the mapping be unit-tested exhaustively
//! without launching anything.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::boundary::{BoundaryAttestation, BoundaryKind, EnforcementLevel};

/// The dimensions a Sandboy backend confines. Reported independently — there is no single
/// boolean "secure", because a real boundary enforces some dimensions and not others and
/// must say which (issue #53).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxDimension {
    /// Filesystem reads/writes/execs (Landlock).
    Filesystem,
    /// Outbound/inbound network.
    Network,
    /// The child's environment variables.
    Env,
    /// The descendant process tree / group.
    ProcessTree,
    /// A wall-clock deadline with a process-tree kill.
    Timeout,
}

impl SandboxDimension {
    /// Every dimension a fully-enforced report must account for. `FullyEnforced` requires
    /// each of these to be [`Enforcement::Enforced`].
    pub const ALL: [SandboxDimension; 5] = [
        SandboxDimension::Filesystem,
        SandboxDimension::Network,
        SandboxDimension::Env,
        SandboxDimension::ProcessTree,
        SandboxDimension::Timeout,
    ];
}

/// How thoroughly a single dimension was enforced. Three-valued on purpose: `Partial` is
/// an honest admission, not a rounding error, and must never be silently promoted to
/// `Enforced`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    /// The dimension was fully confined.
    Enforced,
    /// The dimension was only partially confined (best-effort; a real gap remains).
    Partial,
    /// The dimension was not confined at all.
    NotEnforced,
}

/// The machine-readable report a `sandboy` backend emits describing what it ACTUALLY
/// enforced. Deserialized from the backend on the untrusted path, so:
///
/// - `deny_unknown_fields` — a stray `secure: true` (or any other field) fails to parse,
///   rather than being ignored. There is deliberately no boolean success field.
/// - every dimension is a REQUIRED field — a missing dimension is a parse error, never a
///   silent "enforced".
///
/// [`SandboxReport::parse`] adds the one check serde cannot express (a non-empty backend
/// identifier).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct SandboxReport {
    /// The concrete mechanism, e.g. `landlock-seccomp`. Non-empty (checked by `parse`);
    /// an anonymous report is not trustworthy evidence.
    pub backend: String,
    pub filesystem: Enforcement,
    pub network: Enforcement,
    pub env: Enforcement,
    pub process_tree: Enforcement,
    pub timeout: Enforcement,
}

/// A report that could not be accepted from the backend.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReportError {
    /// The bytes were not the expected report shape (bad JSON, an unknown field such as a
    /// boolean `secure`, or a missing dimension).
    #[error("sandbox report is malformed: {0}")]
    Malformed(String),
    /// The report parsed but named no backend mechanism.
    #[error("sandbox report names no backend mechanism")]
    EmptyBackend,
}

impl SandboxReport {
    /// Parse and validate an untrusted backend report.
    ///
    /// # Errors
    /// [`ReportError::Malformed`] on bad shape / unknown field / missing dimension;
    /// [`ReportError::EmptyBackend`] if the backend identifier is blank.
    pub fn parse(bytes: &[u8]) -> Result<Self, ReportError> {
        let report: SandboxReport =
            serde_json::from_slice(bytes).map_err(|e| ReportError::Malformed(e.to_string()))?;
        if report.backend.trim().is_empty() {
            return Err(ReportError::EmptyBackend);
        }
        Ok(report)
    }

    /// The enforcement for one dimension.
    #[must_use]
    pub fn dimension(&self, dimension: SandboxDimension) -> Enforcement {
        match dimension {
            SandboxDimension::Filesystem => self.filesystem,
            SandboxDimension::Network => self.network,
            SandboxDimension::Env => self.env,
            SandboxDimension::ProcessTree => self.process_tree,
            SandboxDimension::Timeout => self.timeout,
        }
    }

    /// The honest overall level: `FullyEnforced` ONLY when every dimension is `Enforced`;
    /// `None` when every dimension is `NotEnforced`; `Partial` for any mixture. A single
    /// non-enforced dimension can therefore never present as full confinement.
    #[must_use]
    pub fn enforcement_level(&self) -> EnforcementLevel {
        let all_enforced = SandboxDimension::ALL
            .iter()
            .all(|&d| self.dimension(d) == Enforcement::Enforced);
        if all_enforced {
            return EnforcementLevel::FullyEnforced;
        }
        let none_enforced = SandboxDimension::ALL
            .iter()
            .all(|&d| self.dimension(d) == Enforcement::NotEnforced);
        if none_enforced {
            EnforcementLevel::None
        } else {
            EnforcementLevel::Partial
        }
    }

    /// The report as a boundary attestation — always `BoundaryKind::Sandboy`, with the
    /// honestly-derived level. This is run-time EVIDENCE; a `SandboyBoundary`'s
    /// configured `attestation()` is a separate claim the evidence must re-prove.
    #[must_use]
    pub fn attestation(&self) -> BoundaryAttestation {
        BoundaryAttestation {
            implementation: BoundaryKind::Sandboy,
            enforcement: self.enforcement_level(),
        }
    }
}

/// Network posture requested of the backend. The first slice can only express deny-all
/// (the safe default for a worktree run); a richer allowlist is a later widening.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicy {
    /// No network access whatsoever.
    DenyAll,
}

/// What confinement a caller asks the backend to install. Validated by
/// [`SandboxPolicy::validate`]: a degenerate policy that would confine nothing is
/// rejected at construction, so a `SandboyBoundary` never attests full enforcement over a
/// policy that cannot deliver it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SandboxPolicy {
    /// The single writable root (the run's worktree). Landlock grants read+write here and
    /// read-only elsewhere as configured.
    pub worktree: PathBuf,
    /// Absolute paths the confined child may READ+EXECUTE. MUST cover the exec target,
    /// including PR 3's sealed-memfd path `/proc/<verifier_pid>/fd/<n>` — otherwise the
    /// `exec` is denied and the run fails closed.
    pub allow_exec: Vec<PathBuf>,
    /// Network posture (deny-all in the first slice).
    pub network: NetworkPolicy,
    /// The COMPLETE set of environment variable names the child may keep.
    pub env_allowlist: Vec<OsString>,
    /// Wall-clock deadline; must be non-zero.
    pub timeout: Duration,
}

/// Why a [`SandboxPolicy`] cannot be trusted to fully confine.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SandboxPolicyError {
    /// The worktree path is not absolute (a relative root cannot be confined reliably).
    #[error("worktree must be an absolute path: {0}")]
    RelativeWorktree(PathBuf),
    /// An exec allowance is not absolute.
    #[error("exec allowance must be an absolute path: {0}")]
    RelativeExecAllowance(PathBuf),
    /// No exec allowance was declared, so no target — not even the sealed memfd — could
    /// run.
    #[error("policy grants no executable path; nothing could be launched")]
    NoExecAllowance,
    /// The timeout is zero; a run with no deadline is not fully confined.
    #[error("timeout must be non-zero")]
    ZeroTimeout,
}

impl SandboxPolicy {
    /// Validate that this policy is capable of full confinement.
    ///
    /// # Errors
    /// [`SandboxPolicyError`] for a relative worktree/allowance, an empty exec-allowance
    /// set, or a zero timeout.
    pub fn validate(&self) -> Result<(), SandboxPolicyError> {
        if !self.worktree.is_absolute() {
            return Err(SandboxPolicyError::RelativeWorktree(self.worktree.clone()));
        }
        if self.allow_exec.is_empty() {
            return Err(SandboxPolicyError::NoExecAllowance);
        }
        for path in &self.allow_exec {
            if !path.is_absolute() {
                return Err(SandboxPolicyError::RelativeExecAllowance(path.clone()));
            }
        }
        if self.timeout.is_zero() {
            return Err(SandboxPolicyError::ZeroTimeout);
        }
        Ok(())
    }

    /// Whether `target` is permitted to execute under this policy: it must equal, or lie
    /// beneath, one of the declared exec allowances. This is the pre-spawn gate that keeps
    /// PR 3's sealed-memfd exec (`/proc/<verifier_pid>/fd/<n>`) working: grant the fd path
    /// (or its `/proc/<pid>/fd` directory) and the exec is allowed; grant nothing and the
    /// boundary fails closed rather than launching a doomed backend.
    #[must_use]
    pub fn permits_exec(&self, target: &Path) -> bool {
        self.allow_exec
            .iter()
            .any(|allowed| target == allowed || target.starts_with(allowed))
    }
}
