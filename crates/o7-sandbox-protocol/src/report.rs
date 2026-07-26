//! The machine-readable enforcement report a backend emits, bound to the exact policy,
//! launch, and target it confined.
//!
//! On the untrusted deserialize path: `deny_unknown_fields` rejects a stray `secure: true`
//! (issue #53 — "how software lies to itself before a breach report"); every dimension is a
//! REQUIRED field, so a missing dimension is a parse error, not a silent "enforced"; and the
//! binding fields (`policy_digest`, `launch_nonce`, `target_digest`) are validated newtypes.
//! [`SandboxReport::parse`] adds the schema-version check serde cannot express.

use serde::{Deserialize, Serialize};

use crate::ids::{BackendIdentity, Digest256, LaunchNonce};
use crate::policy::{Enforcement, SandboxDimension};
use crate::SCHEMA_VERSION;

/// A backend's report of what it ACTUALLY enforced, for one launch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxReport {
    /// Wire-format version; [`SandboxReport::parse`] rejects anything but [`SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Which mechanism made the claim.
    pub backend: BackendIdentity,
    /// The digest of the BACKEND object that made the claim — must equal the parent's trusted
    /// [`crate::SandboxPolicy`]-adjacent backend image digest. Persisted so a later replay of the
    /// raw report knows which exact backend binary was trusted.
    pub backend_digest: Digest256,
    /// The digest of the policy that was installed — must equal the parent's own
    /// [`crate::SandboxPolicy::digest`].
    pub policy_digest: Digest256,
    /// The per-spawn nonce the parent minted — must equal what the parent sent.
    pub launch_nonce: LaunchNonce,
    /// The digest of the exact LAUNCH (target object + argv + cwd + allowlisted env + stdin),
    /// i.e. [`crate::LaunchRequest::spec_digest`] — binds the report to the specific INVOCATION,
    /// not merely to the binary.
    pub launch_spec_digest: Digest256,
    pub filesystem: Enforcement,
    pub network: Enforcement,
    pub env: Enforcement,
    pub process_tree: Enforcement,
    pub timeout: Enforcement,
}

/// A report that could not be accepted from the backend.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReportError {
    /// Bad JSON, an unknown field (e.g. a boolean `secure`), a missing dimension, or a
    /// malformed binding value.
    #[error("sandbox report is malformed: {0}")]
    Malformed(String),
    /// The report's `schema_version` is not the one this build speaks.
    #[error("sandbox report schema version {found} != supported {SCHEMA_VERSION}")]
    UnsupportedVersion { found: u32 },
}

impl SandboxReport {
    /// Parse and validate an untrusted backend report body (the JSON inside a frame).
    ///
    /// # Errors
    /// [`ReportError::Malformed`] on bad shape / unknown field / missing dimension /
    /// malformed binding; [`ReportError::UnsupportedVersion`] on a version mismatch.
    pub fn parse(bytes: &[u8]) -> Result<Self, ReportError> {
        let report: SandboxReport =
            serde_json::from_slice(bytes).map_err(|e| ReportError::Malformed(e.to_string()))?;
        if report.schema_version != SCHEMA_VERSION {
            return Err(ReportError::UnsupportedVersion {
                found: report.schema_version,
            });
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

    /// True only when EVERY dimension is `Enforced`. A single downgrade makes this false, so
    /// a partial report can never present as full confinement.
    ///
    /// The report is destructured EXHAUSTIVELY (no `..`): a new enforcement dimension is a new
    /// field, so this fails to compile until that dimension is named AND folded into the check —
    /// it cannot be silently omitted (a fresh `Enforcement` field left out would otherwise let a
    /// report that does not enforce it read as fully enforced). This structural check, not
    /// [`SandboxDimension::ALL`], is what guarantees completeness of the trust decision; `ALL` and
    /// its ordering anchor remain for iteration.
    #[must_use]
    pub fn is_fully_enforced(&self) -> bool {
        let SandboxReport {
            filesystem,
            network,
            env,
            process_tree,
            timeout,
            // Non-dimension metadata — deliberately not part of the enforcement decision.
            schema_version: _,
            backend: _,
            backend_digest: _,
            policy_digest: _,
            launch_nonce: _,
            launch_spec_digest: _,
        } = self;
        *filesystem == Enforcement::Enforced
            && *network == Enforcement::Enforced
            && *env == Enforcement::Enforced
            && *process_tree == Enforcement::Enforced
            && *timeout == Enforcement::Enforced
    }

    /// True when NO dimension is enforced at all. Uses the same exhaustive destructure as
    /// [`SandboxReport::is_fully_enforced`], so a new dimension cannot be silently dropped here
    /// either.
    #[must_use]
    pub fn is_none_enforced(&self) -> bool {
        let SandboxReport {
            filesystem,
            network,
            env,
            process_tree,
            timeout,
            schema_version: _,
            backend: _,
            backend_digest: _,
            policy_digest: _,
            launch_nonce: _,
            launch_spec_digest: _,
        } = self;
        *filesystem == Enforcement::NotEnforced
            && *network == Enforcement::NotEnforced
            && *env == Enforcement::NotEnforced
            && *process_tree == Enforcement::NotEnforced
            && *timeout == Enforcement::NotEnforced
    }
}
