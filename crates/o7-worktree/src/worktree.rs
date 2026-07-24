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

use std::path::{Path, PathBuf};

use crate::attest::{attest_owned_dir, AttestError, FsIdentity};
use crate::git::{GitError, HardenedGit};
use crate::identity::{CommittedRevision, IdentityDigest, RunId, WorktreeIdentity};
use crate::materialize::{
    MaterializeError, MaterializeLimits, MaterializePlan, MaterializeSummary,
};
use crate::reap::ReapError;
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
    /// The descriptor-bound state root the worktree lives under. Held so deletion runs
    /// RELATIVE to the proven root fd (never a re-resolved parent path).
    state_root: StateRoot,
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

        // The agent's copy must live outside the operator's repository. Check against the
        // common git dir, the path git was bound to, AND the resolved top-level working
        // tree — so a state root inside the checkout is rejected even when git is bound to
        // a subdirectory (where the bound path alone would not contain the state root).
        state_root.ensure_outside_repo(&identity.repo.git_common_dir)?;
        state_root.ensure_outside_repo(git.repo())?;
        if let Some(toplevel) = git.worktree_toplevel() {
            state_root.ensure_outside_repo(&toplevel)?;
        }

        // Preflight the WHOLE tree read-only, BEFORE creating any directory or git
        // metadata: a hostile tree (reserved `.git` path, unsupported mode, or an
        // entry/blob/cumulative budget breach) fails closed here with nothing written.
        let limits = MaterializeLimits::default();
        let plan = MaterializePlan::prepare(git, revision, &limits)?;

        // Create the worktree directory RELATIVE to the state root's bound descriptor:
        // mkdirat under the proven root inode (never adopting a pre-existing path), then
        // re-open O_NOFOLLOW and prove owner-only, returning the identity taken from the
        // descriptor. An ancestor swap cannot redirect this at a victim location.
        let (path, fs_identity) = match state_root.create_worktree_dir(&digest) {
            Ok(pair) => pair,
            Err(StateRootError::WorktreeExists(p)) => return Err(WorktreeError::AlreadyExists(p)),
            Err(err) => return Err(WorktreeError::StateRoot(err)),
        };

        // Turn the owned directory into a REAL, self-contained detached git worktree of
        // the committed revision (init + borrowed objects + detached HEAD + index), then
        // write the validated plan — a genuine git worktree with no checkout, so no
        // hook/filter/fsmonitor runs and no operator bytes leak in.
        let build = git
            .init_detached_worktree(&path, &identity.repo, revision, limits.max_closure_bytes)
            .map_err(WorktreeError::from)
            .and_then(|()| plan.write(git, &path).map_err(WorktreeError::from));
        let summary = match build {
            Ok(summary) => summary,
            Err(err) => {
                // We just created and own this directory; remove it race-safely RELATIVE to
                // the state root's bound fd — identity is proven on the open fd, the
                // directory is quarantined under a private name, re-proven, cleared, and its
                // removal is PROVEN (link count 0) — so a failed create leaks nothing and the
                // cleanup can never be redirected at a victim tree.
                let _ = state_root.remove_worktree_dir(&digest, fs_identity);
                return Err(err);
            }
        };

        Ok(Self {
            identity,
            digest,
            path,
            fs_identity,
            summary,
            state_root: state_root.clone(),
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
        state_root: StateRoot,
    ) -> Self {
        Self {
            identity,
            digest,
            path,
            fs_identity,
            summary,
            state_root,
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
    /// descriptor (addressed RELATIVE to the state root's bound fd, never a re-resolved
    /// parent path), quarantined, re-proven, cleared, and its removal PROVEN (link count 0).
    /// A swap of the name between any check and the delete cannot redirect the removal at a
    /// victim tree, and an empty-decoy swap can never yield a false success. If identity or
    /// the removal cannot be proven, nothing is dropped and the directory is left for
    /// investigation.
    ///
    /// # Errors
    /// [`WorktreeError::Reap`] if identity was proven but deletion then failed part way.
    pub fn cleanup(self) -> Result<CleanupOutcome, WorktreeError> {
        match self
            .state_root
            .remove_worktree_dir(&self.digest, self.fs_identity)
        {
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
