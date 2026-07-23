//! The identities a worktree is bound to.
//!
//! A worktree's authority is NOT its path and NOT a serialized record — it is the
//! triple `(run_id, canonical repository identity, committed revision)`. This module
//! defines those three, plus the [`WorktreeIdentity`] that combines them and the
//! [`IdentityDigest`] used to bind a worktree on disk to exactly that triple. Nothing
//! here touches the filesystem — [`crate::attest`] proves the on-disk identity, and
//! [`crate::git`] reads the repository identity.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// A problem constructing one of the identity primitives.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("run id must be 1..={max} bytes; got {len}", max = MAX_RUN_ID_LEN)]
    RunIdLength { len: usize },
    #[error(
        "run id {0:?} is not a safe path component (allowed: ASCII letters, digits, '.', '_', '-'; \
         never \".\", \"..\", or a leading '.'/'-')"
    )]
    RunIdCharset(String),
    #[error("committed revision {0:?} is not a lowercase-hex object id of length 40 or 64")]
    RevisionShape(String),
}

/// Max length of a [`RunId`] — bounded so it is always a legal path component.
pub const MAX_RUN_ID_LEN: usize = 128;

/// A validated run identifier assigned by o7d, one per run.
///
/// It becomes a path component under the state root, so it is restricted to a safe
/// charset with no separators, no `.`/`..`, and no leading `.`/`-`. This makes path
/// traversal (`../`) and option-injection (`-x`) impossible by construction; an
/// invalid id is rejected here rather than sanitized silently.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(String);

impl RunId {
    /// Validate and wrap a run id.
    ///
    /// # Errors
    /// [`IdentityError::RunIdLength`] / [`IdentityError::RunIdCharset`] if the id is
    /// empty, over-long, or not a safe single path component.
    pub fn new(id: impl Into<String>) -> Result<Self, IdentityError> {
        let id = id.into();
        if id.is_empty() || id.len() > MAX_RUN_ID_LEN {
            return Err(IdentityError::RunIdLength { len: id.len() });
        }
        if id == "." || id == ".." {
            return Err(IdentityError::RunIdCharset(id));
        }
        // A leading '.' would be a hidden entry; a leading '-' could be read as an
        // option by a helper that ever received the id as an argument.
        let safe_lead = id
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        let safe_body = id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if !safe_lead || !safe_body {
            return Err(IdentityError::RunIdCharset(id));
        }
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The canonical identity of a source repository.
///
/// `git_common_dir` (the absolute, symlink-resolved common git directory — shared by
/// all of a repo's linked worktrees) anchors *which* repository this is. `dev`/`ino`
/// are that directory's filesystem identity, so a later rename or substitution of the
/// path to a *different* directory is detectable: the path can be reused, the inode
/// cannot be forged. Both must match for the repo identity to be re-proven.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CanonicalRepoId {
    pub git_common_dir: PathBuf,
    pub dev: u64,
    pub ino: u64,
}

/// A concrete committed revision — a full object id in lowercase hex.
///
/// This is never a ref, a range, or a short id: it is a specific commit that already
/// exists in the object store (resolved by [`crate::git`]). Only committed bytes are
/// ever materialized, so the agent's working copy can never contain an operator's
/// uncommitted changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommittedRevision(String);

impl CommittedRevision {
    /// Wrap an already-resolved object id, validating its shape (40-hex SHA-1 or
    /// 64-hex SHA-256, lowercase).
    ///
    /// # Errors
    /// [`IdentityError::RevisionShape`] if the string is not a lowercase-hex id of
    /// length 40 or 64.
    pub fn from_object_id(oid: impl Into<String>) -> Result<Self, IdentityError> {
        let oid = oid.into();
        let ok = matches!(oid.len(), 40 | 64)
            && oid
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        if ok {
            Ok(Self(oid))
        } else {
            Err(IdentityError::RevisionShape(oid))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CommittedRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The full identity a worktree is bound to: the run, the repository, and the exact
/// committed revision materialized into it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeIdentity {
    pub run_id: RunId,
    pub repo: CanonicalRepoId,
    pub revision: CommittedRevision,
}

impl WorktreeIdentity {
    #[must_use]
    pub fn new(run_id: RunId, repo: CanonicalRepoId, revision: CommittedRevision) -> Self {
        Self {
            run_id,
            repo,
            revision,
        }
    }

    /// A stable digest over the whole identity triple.
    ///
    /// Fields are domain-separated and length-framed so no two distinct triples can
    /// collide by concatenation ambiguity (e.g. a run id ending where a path begins).
    /// The digest is what a worktree's on-disk location is derived from and what a
    /// durable record is checked against — a mismatch means the record does not
    /// describe this worktree.
    #[must_use]
    pub fn digest(&self) -> IdentityDigest {
        let mut hasher = Sha256::new();
        hasher.update(b"o7-worktree-identity\0v1\0");
        framed(&mut hasher, self.run_id.as_str().as_bytes());
        // Bytes of the common-dir path exactly as recorded (already canonicalized).
        framed(
            &mut hasher,
            self.repo.git_common_dir.to_string_lossy().as_bytes(),
        );
        framed(&mut hasher, &self.repo.dev.to_le_bytes());
        framed(&mut hasher, &self.repo.ino.to_le_bytes());
        framed(&mut hasher, self.revision.as_str().as_bytes());
        let bytes = hasher.finalize();
        IdentityDigest(hex_lower(&bytes))
    }
}

/// A lowercase-hex SHA-256 over a [`WorktreeIdentity`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdentityDigest(String);

impl IdentityDigest {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for IdentityDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Feed a length-prefixed field into the hasher so concatenation is unambiguous.
fn framed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// Lowercase-hex encode without pulling in a hex crate.
fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        let hi = usize::from(b >> 4);
        let lo = usize::from(b & 0x0f);
        if let (Some(&h), Some(&l)) = (HEX.get(hi), HEX.get(lo)) {
            out.push(char::from(h));
            out.push(char::from(l));
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn repo() -> CanonicalRepoId {
        CanonicalRepoId {
            git_common_dir: PathBuf::from("/srv/repo/.git"),
            dev: 66,
            ino: 1234,
        }
    }

    #[test]
    fn run_id_rejects_traversal_and_unsafe_leads() {
        assert!(RunId::new("").is_err());
        assert!(RunId::new(".").is_err());
        assert!(RunId::new("..").is_err());
        assert!(RunId::new("../escape").is_err());
        assert!(RunId::new("a/b").is_err());
        assert!(RunId::new(".hidden").is_err());
        assert!(RunId::new("-rf").is_err());
        assert!(RunId::new("a".repeat(MAX_RUN_ID_LEN + 1)).is_err());
        assert!(RunId::new("run-01_A.2").is_ok());
    }

    #[test]
    fn revision_requires_lowercase_hex_40_or_64() {
        assert!(CommittedRevision::from_object_id("a".repeat(40)).is_ok());
        assert!(CommittedRevision::from_object_id("a".repeat(64)).is_ok());
        assert!(CommittedRevision::from_object_id("A".repeat(40)).is_err()); // uppercase
        assert!(CommittedRevision::from_object_id("a".repeat(39)).is_err());
        assert!(CommittedRevision::from_object_id("g".repeat(40)).is_err()); // non-hex
        assert!(CommittedRevision::from_object_id("HEAD").is_err());
    }

    #[test]
    fn digest_is_stable_and_field_sensitive() {
        let rev = CommittedRevision::from_object_id("a".repeat(40)).expect("rev");
        let base = WorktreeIdentity::new(RunId::new("r1").expect("id"), repo(), rev.clone());
        // Deterministic.
        assert_eq!(base.digest(), base.digest());

        // Every field participates: change each and the digest must move.
        let other_run = WorktreeIdentity::new(RunId::new("r2").expect("id"), repo(), rev.clone());
        assert_ne!(base.digest(), other_run.digest());

        let mut moved_repo = repo();
        moved_repo.ino = 9999;
        let other_repo =
            WorktreeIdentity::new(RunId::new("r1").expect("id"), moved_repo, rev.clone());
        assert_ne!(base.digest(), other_repo.digest());

        let other_rev = WorktreeIdentity::new(
            RunId::new("r1").expect("id"),
            repo(),
            CommittedRevision::from_object_id("b".repeat(40)).expect("rev"),
        );
        assert_ne!(base.digest(), other_rev.digest());
    }

    #[test]
    fn framing_prevents_boundary_collision() {
        // "ab" | "c"  vs  "a" | "bc": without length framing these would collide.
        let rev = CommittedRevision::from_object_id("a".repeat(40)).expect("rev");
        let mut r1 = repo();
        r1.git_common_dir = PathBuf::from("/abc");
        let a = WorktreeIdentity::new(RunId::new("ab").expect("id"), r1, rev.clone());
        let mut r2 = repo();
        r2.git_common_dir = PathBuf::from("bc");
        let b = WorktreeIdentity::new(RunId::new("a").expect("id"), r2, rev);
        assert_ne!(a.digest(), b.digest());
    }
}
