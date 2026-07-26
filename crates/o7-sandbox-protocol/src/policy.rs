//! The confinement request ([`SandboxPolicy`]) and the per-dimension enforcement vocabulary
//! it is reported against. The policy hashes to a canonical [`Digest256`] so a report can be
//! bound to the exact policy that was installed.

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::ids::Digest256;

/// The SINGLE source of truth for the confinement dimensions. One `Variant => field` list generates
/// the [`SandboxDimension`] enum, its [`SandboxDimension::ALL`], and — the point — the three
/// [`crate::report::SandboxReport`] accessors `dimension`, `is_fully_enforced`, and
/// `is_none_enforced`. A new dimension is added by adding ONE line here: it becomes an enum variant,
/// an `ALL` entry, a `dimension()` arm, AND is threaded into BOTH trust predicates. If the matching
/// `SandboxReport` field does not exist, the generated `self.<field>` access fails to compile — so a
/// dimension can never be added without every trust decision actually consulting it. This is a real
/// authority link, not two hand-maintained lists watching each other. It deliberately does NOT
/// generate `SandboxReport` itself, so the struct keeps its public fields and serde wire layout.
macro_rules! sandbox_dimensions {
    // Count the variants at compile time so `ALL` is a fixed-size array sized from the list itself.
    (@count) => { 0usize };
    (@count $head:ident $($tail:ident)*) => { 1usize + sandbox_dimensions!(@count $($tail)*) };

    ($($variant:ident => $field:ident),+ $(,)?) => {
        /// A confinement dimension a backend reports on independently. There is no single boolean
        /// "secure": a real boundary enforces some dimensions and not others and must say which.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ::serde::Serialize, ::serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum SandboxDimension {
            $($variant,)+
        }

        impl SandboxDimension {
            /// Every dimension, for iteration. Generated from the same list as the trust predicates,
            /// so it is always the complete variant set in declaration order — never a hand-kept copy
            /// that could drift.
            pub const ALL: [SandboxDimension; sandbox_dimensions!(@count $($variant)+)] =
                [$(SandboxDimension::$variant),+];
        }

        impl $crate::report::SandboxReport {
            /// The enforcement for one dimension.
            #[must_use]
            pub fn dimension(&self, dimension: SandboxDimension) -> $crate::policy::Enforcement {
                match dimension {
                    $(SandboxDimension::$variant => self.$field,)+
                }
            }

            /// True only when EVERY dimension is `Enforced`. A single downgrade makes this false, so
            /// a partial report can never present as full confinement. Generated from the dimension
            /// list, so a newly added dimension is folded into the check automatically — it cannot be
            /// merely *mentioned* and then left out (the failure mode an exhaustive destructure
            /// alone does not catch).
            #[must_use]
            pub fn is_fully_enforced(&self) -> bool {
                true $(&& self.$field == $crate::policy::Enforcement::Enforced)+
            }

            /// True when NO dimension is enforced at all. Also generated from the dimension list, so
            /// a new dimension is consulted here too.
            #[must_use]
            pub fn is_none_enforced(&self) -> bool {
                true $(&& self.$field == $crate::policy::Enforcement::NotEnforced)+
            }
        }
    };
}

sandbox_dimensions! {
    Filesystem => filesystem,
    Network => network,
    Env => env,
    ProcessTree => process_tree,
    Timeout => timeout,
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
    /// The timeout carries sub-millisecond precision. The digest binds the full-nanosecond
    /// `Duration`, but the backend argv serializes only `--timeout-ms` (whole milliseconds); a
    /// fractional-millisecond timeout would therefore digest to one value and reconstruct to
    /// another, failing every launch with `PolicyDigestMismatch`. The wire contract is
    /// milliseconds, so unsupported precision is rejected at the input rather than silently lost.
    #[error("timeout {0:?} is not a whole number of milliseconds (the wire contract is ms)")]
    TimeoutNotWholeMilliseconds(Duration),
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
        // The digest binds full-nanosecond precision but the argv serializes whole milliseconds
        // (`timeout.as_millis()`). Reject any sub-millisecond remainder so the policy the parent
        // digests is exactly the policy a backend reconstructs from `--timeout-ms` — no silent
        // truncation that would surface later as a `PolicyDigestMismatch`. (A whole-seconds part is
        // inherently whole-milliseconds, so only the sub-second remainder needs checking.)
        if !self.timeout.subsec_nanos().is_multiple_of(1_000_000) {
            return Err(SandboxPolicyError::TimeoutNotWholeMilliseconds(
                self.timeout,
            ));
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
        // `Path::starts_with` is component-wise but NOT normalizing: a non-normalized target could
        // lexically sit under an allowance yet resolve outside it (`/usr/bin/../../etc/evil`
        // starts_with `/usr/bin`). Reject any `..`/`.` component so the lexical check below actually
        // implies containment. The frozen sealed-memfd exec path (`/proc/<pid>/fd/<n>`) has no dot
        // components, so it is unaffected.
        if target
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
        {
            return false;
        }
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
