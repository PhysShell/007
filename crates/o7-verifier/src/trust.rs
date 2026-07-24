//! Trust binding and the trust store.
//!
//! Trust is bound to the **canonical repository identity**, the **executable identity**
//! (a content hash of the binary at its absolute path), and the **whole command
//! specification** — executable path, argv, cwd policy, environment allowlist, exit
//! policy, timeout, and output budget — folded into a single **command digest**. Any
//! drift in any of them yields a different digest, so the command is no longer trusted:
//! a swapped binary, a changed argument, a different working directory, a different
//! environment, a widened exit policy, a longer timeout, a larger output budget, or a
//! different repository all invalidate trust automatically.
//!
//! Trust is NEVER sourced from the repository. The [`TrustStore`] is populated by o7d
//! from a source outside the repository (an operator decision), and nothing in this
//! module reads repository config, `.git`, or the worktree to decide what is trusted.

use std::collections::BTreeSet;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use o7_worktree::CanonicalRepoId;

use crate::command::{CwdPolicy, TrustedCommand};

/// A content hash of an executable — the "executable identity". Re-hashing at run time
/// is what makes a swapped-out binary at the same path invalidate trust.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutableIdentity(String);

impl ExecutableIdentity {
    /// Hash the file at `path`.
    ///
    /// # Errors
    /// [`TrustError::Executable`] if the file cannot be read.
    pub fn of_file(path: &Path) -> Result<Self, TrustError> {
        let bytes = std::fs::read(path).map_err(|source| TrustError::Executable {
            path: path.to_path_buf(),
            source,
        })?;
        let mut hasher = Sha256::new();
        hasher.update(b"o7-verifier-exe\0v1\0");
        hasher.update(&bytes);
        Ok(Self(hex_lower(&hasher.finalize())))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The single digest that identifies a trusted command in a repository. Two commands
/// with the same digest are the same trusted command; any drift changes it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CommandDigest(String);

impl CommandDigest {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Everything trust is bound to, plus the resulting [`CommandDigest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustAnchor {
    pub repo: CanonicalRepoId,
    pub executable_identity: ExecutableIdentity,
    digest: CommandDigest,
}

impl TrustAnchor {
    /// Compute the anchor for `command` in `repo`, reading the executable to bind its
    /// identity. Because the executable is re-read here, any drift in the binary,
    /// argv, cwd policy, or repository produces a different [`CommandDigest`].
    ///
    /// # Errors
    /// [`TrustError`] if the executable cannot be read.
    pub fn compute(repo: &CanonicalRepoId, command: &TrustedCommand) -> Result<Self, TrustError> {
        let exe_identity = ExecutableIdentity::of_file(&command.executable)?;
        let digest = command_digest(repo, command, &exe_identity);
        Ok(Self {
            repo: repo.clone(),
            executable_identity: exe_identity,
            digest,
        })
    }

    #[must_use]
    pub fn digest(&self) -> &CommandDigest {
        &self.digest
    }
}

/// The set of trusted command digests. Populated by o7d from OUTSIDE the repository —
/// never from repository-controlled data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustStore {
    trusted: BTreeSet<CommandDigest>,
}

impl TrustStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Explicitly trust the command an anchor identifies (an o7d/operator decision).
    pub fn trust(&mut self, anchor: &TrustAnchor) {
        self.trusted.insert(anchor.digest().clone());
    }

    /// Whether the command an anchor identifies is trusted. A drifted anchor (recomputed
    /// after any change) is simply absent, so it is not trusted.
    #[must_use]
    pub fn is_trusted(&self, anchor: &TrustAnchor) -> bool {
        self.trusted.contains(anchor.digest())
    }

    /// Revoke a previously-trusted command.
    pub fn revoke(&mut self, digest: &CommandDigest) {
        self.trusted.remove(digest);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.trusted.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.trusted.is_empty()
    }
}

/// A failure computing a trust anchor.
#[derive(Debug, thiserror::Error)]
pub enum TrustError {
    #[error("cannot read executable {path:?} to bind its identity: {source}")]
    Executable {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The STRUCTURAL command digest: the canonical repository identity plus the WHOLE
/// command specification EXCEPT the executable's content — executable path, argv, cwd
/// policy, environment allowlist, exit policy, timeout, and output budget. It identifies
/// the command for evidence and is computable without reading the binary (so evidence
/// exists even when the executable is missing). It is NOT the trust key — trust
/// additionally binds the executable identity (see [`TrustAnchor`]).
#[must_use]
pub fn structural_command_digest(
    repo: &CanonicalRepoId,
    command: &TrustedCommand,
) -> CommandDigest {
    let mut hasher = Sha256::new();
    hasher.update(b"o7-verifier-command-structural\0v2\0");
    fold_structural(&mut hasher, repo, command);
    CommandDigest(hex_lower(&hasher.finalize()))
}

/// The trust key: the structural digest PLUS the executable content identity, so a
/// swapped binary at the same path invalidates trust.
fn command_digest(
    repo: &CanonicalRepoId,
    command: &TrustedCommand,
    exe_identity: &ExecutableIdentity,
) -> CommandDigest {
    let mut hasher = Sha256::new();
    hasher.update(b"o7-verifier-command\0v2\0");
    fold_structural(&mut hasher, repo, command);
    framed(&mut hasher, exe_identity.as_str().as_bytes());
    CommandDigest(hex_lower(&hasher.finalize()))
}

/// Fold the WHOLE command spec except the executable's content into `hasher`: repo
/// identity, exe path, argv, cwd policy, environment allowlist, exit policy, timeout,
/// and output budget. Binding all of it means trust cannot be reused with a different
/// environment, a widened exit policy, a longer timeout, or a larger output budget —
/// any of which would change what "the trusted command" actually does.
fn fold_structural(hasher: &mut Sha256, repo: &CanonicalRepoId, command: &TrustedCommand) {
    framed(hasher, repo.git_common_dir.as_os_str().as_bytes());
    framed(hasher, &repo.dev.to_le_bytes());
    framed(hasher, &repo.ino.to_le_bytes());
    framed(hasher, command.executable.as_os_str().as_bytes());
    // argv, each element framed (so `["ab","c"]` != `["a","bc"]`).
    framed(hasher, &(command.arguments.len() as u64).to_le_bytes());
    for arg in &command.arguments {
        framed(hasher, arg.as_bytes());
    }
    match &command.cwd_policy {
        CwdPolicy::WorktreeRoot => framed(hasher, b"worktree-root"),
        CwdPolicy::Absolute(path) => {
            framed(hasher, b"absolute");
            framed(hasher, path.as_os_str().as_bytes());
        }
    }
    // Environment allowlist. A `BTreeMap` iterates in sorted key order, so the binding
    // is deterministic and independent of insertion order.
    framed(hasher, &(command.environment.len() as u64).to_le_bytes());
    for (key, value) in &command.environment {
        framed(hasher, key.as_bytes());
        framed(hasher, value.as_bytes());
    }
    // Exit policy (ascending codes), timeout, and output budget.
    let codes = command.exit_policy.success_codes_sorted();
    framed(hasher, &(codes.len() as u64).to_le_bytes());
    for code in codes {
        framed(hasher, &code.to_le_bytes());
    }
    framed(hasher, &command.timeout.as_nanos().to_le_bytes());
    framed(
        hasher,
        &(command.output_limits.max_total_bytes as u64).to_le_bytes(),
    );
}

fn framed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        if let (Some(&h), Some(&l)) = (HEX.get(usize::from(b >> 4)), HEX.get(usize::from(b & 0x0f)))
        {
            out.push(char::from(h));
            out.push(char::from(l));
        }
    }
    out
}
