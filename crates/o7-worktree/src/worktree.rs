//! A materialized worktree and the create / attest / cleanup operations over it.
//!
//! These are the substrate primitives; o7d's durable ownership (create/reopen/cleanup/
//! prune backed by a versioned record) is layered on top in [`crate::store`]. The one
//! non-negotiable rule enforced here: **cleanup deletes only after
//! [`crate::attest::attest_owned_dir`] proves the on-disk directory is exactly the one
//! we created.** When identity cannot be proven, nothing is deleted — the files are
//! preserved for investigation.
//!
//! A worktree is NOT a sandbox. It bounds *which bytes* the agent starts from and who
//! owns them; it provides no process confinement. Confinement is the ProcessBoundary
//! (and, in production, Sandboy).

use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};

use crate::attest::{attest_owned_dir, AttestError, FsIdentity, OWNER_ONLY};
use crate::git::{GitError, HardenedGit};
use crate::identity::{CommittedRevision, IdentityDigest, RunId, WorktreeIdentity};
use crate::materialize::{materialize, MaterializeError, MaterializeSummary};
use crate::reap::{remove_verified_dir, ReapError};
use crate::stateroot::{StateRoot, StateRootError};

/// A failure creating, attesting, or cleaning up a worktree.
#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    StateRoot(#[from] StateRootError),
    #[error(transparent)]
    Materialize(#[from] MaterializeError),
    #[error(transparent)]
    Attest(#[from] AttestError),
    #[error(transparent)]
    Reap(#[from] ReapError),
    #[error("a worktree directory already exists at {0:?}")]
    AlreadyExists(PathBuf),
    #[error("i/o error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// The outcome of a cleanup attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CleanupOutcome {
    /// Identity was proven and the directory was removed.
    Removed,
    /// Identity could NOT be proven, so nothing was deleted; the reason is retained
    /// for investigation.
    PreservedForInvestigation(String),
}

/// A materialized worktree bound to an identity and a filesystem location.
#[derive(Debug, Clone)]
pub struct Worktree {
    identity: WorktreeIdentity,
    digest: IdentityDigest,
    path: PathBuf,
    fs_identity: FsIdentity,
    summary: MaterializeSummary,
}

impl Worktree {
    /// Create a fresh worktree for `run_id` at `revision`, materializing ONLY the
    /// committed bytes into a new owner-only directory under `state_root`.
    ///
    /// The worktree path is derived from the identity digest, and the state root is
    /// verified to be outside the source repository, so an operator's checkout can
    /// never contain (or be) the agent's copy.
    ///
    /// # Errors
    /// [`WorktreeError`] on a git failure, an unsafe/occupied path, or an i/o error. A
    /// partially-materialized directory is removed before returning the error.
    pub fn create(
        git: &HardenedGit,
        state_root: &StateRoot,
        run_id: RunId,
        revision: &CommittedRevision,
    ) -> Result<Self, WorktreeError> {
        let repo = git.canonical_repo_id()?;
        let identity = WorktreeIdentity::new(run_id, repo, revision.clone());
        let digest = identity.digest();
        let path = state_root.worktree_path(&digest);

        // The agent's copy must live outside the operator's repository.
        state_root.ensure_outside_repo(&identity.repo.git_common_dir)?;
        state_root.ensure_outside_repo(git.repo())?;

        // Create the directory exclusively (never adopt a pre-existing path).
        match std::fs::DirBuilder::new().mode(OWNER_ONLY).create(&path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(WorktreeError::AlreadyExists(path));
            }
            Err(source) => return Err(WorktreeError::Io { path, source }),
        }
        // Re-assert owner-only bits (umask may have cleared some).
        std::fs::set_permissions(
            &path,
            std::os::unix::fs::PermissionsExt::from_mode(OWNER_ONLY),
        )
        .map_err(|source| WorktreeError::Io {
            path: path.clone(),
            source,
        })?;

        let fs_identity = FsIdentity::of_dir(&path)?;

        // Turn the owned directory into a REAL, self-contained detached git worktree of
        // the committed revision (init + borrowed objects + detached HEAD + index), then
        // fill the working tree from the object store — a genuine git worktree with no
        // checkout, so no hook/filter/fsmonitor runs and no operator bytes leak in.
        let build = git
            .init_detached_worktree(&path, &identity.repo, revision)
            .map_err(WorktreeError::from)
            .and_then(|()| materialize(git, revision, &path).map_err(WorktreeError::from));
        let summary = match build {
            Ok(summary) => summary,
            Err(err) => {
                // We just created and own this directory; prove that, then remove the
                // partial worktree so a failed create leaks nothing.
                if attest_owned_dir(&path, fs_identity).is_ok() {
                    let _ = std::fs::remove_dir_all(&path);
                }
                return Err(err);
            }
        };

        Ok(Self {
            identity,
            digest,
            path,
            fs_identity,
            summary,
        })
    }

    /// Reconstruct a handle for an already-materialized worktree from its recorded
    /// fields. This does NOT prove anything — the durable store proves identity via
    /// [`crate::attest::attest_owned_dir`] before calling this, and the returned handle
    /// re-proves on every [`Worktree::attest`]/[`Worktree::cleanup`].
    #[must_use]
    pub(crate) fn adopt(
        identity: WorktreeIdentity,
        digest: IdentityDigest,
        path: PathBuf,
        fs_identity: FsIdentity,
        summary: MaterializeSummary,
    ) -> Self {
        Self {
            identity,
            digest,
            path,
            fs_identity,
            summary,
        }
    }

    #[must_use]
    pub fn identity(&self) -> &WorktreeIdentity {
        &self.identity
    }

    #[must_use]
    pub fn digest(&self) -> &IdentityDigest {
        &self.digest
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn fs_identity(&self) -> FsIdentity {
        self.fs_identity
    }

    #[must_use]
    pub fn summary(&self) -> &MaterializeSummary {
        &self.summary
    }

    /// Re-prove this worktree is still exactly what we created (directory, dev/ino,
    /// owner, owner-only bits). Fail-closed.
    ///
    /// # Errors
    /// [`AttestError`] on any mismatch.
    pub fn attest(&self) -> Result<(), AttestError> {
        attest_owned_dir(&self.path, self.fs_identity)?;
        Ok(())
    }

    /// Delete this worktree — but ONLY after identity is proven on the OPEN directory
    /// descriptor, then removed relative to that descriptor (never a re-resolved path),
    /// so a swap of the path between the check and the delete cannot redirect the
    /// removal at a victim tree. If identity cannot be proven, nothing is deleted and the
    /// directory is left for investigation.
    ///
    /// # Errors
    /// [`WorktreeError::Reap`] if identity was proven but deletion then failed part way.
    pub fn cleanup(self) -> Result<CleanupOutcome, WorktreeError> {
        match remove_verified_dir(&self.path, self.fs_identity) {
            Ok(()) => Ok(CleanupOutcome::Removed),
            // Already gone counts as removed — there is nothing to preserve.
            Err(ReapError::Vanished(_)) => Ok(CleanupOutcome::Removed),
            Err(err @ ReapError::Unproven { .. }) => {
                Ok(CleanupOutcome::PreservedForInvestigation(err.to_string()))
            }
            Err(err @ ReapError::Io { .. }) => Err(WorktreeError::Reap(err)),
        }
    }
}
