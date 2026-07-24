//! `o7-worktree` — the worktree substrate for 007.
//!
//! It materializes ONLY a committed revision into an o7d-owned, owner-only directory
//! that lives OUTSIDE the source repository, binds that directory to a
//! `(run_id, canonical repository identity, committed revision)` identity, and proves
//! that identity against the live filesystem before ever deleting anything. No
//! repository-controlled helper (hook, smudge/clean filter, fsmonitor, external diff)
//! ever runs — the tree is read straight from the object store.
//!
//! A worktree is **not a sandbox** and is deliberately never called one: it bounds
//! which bytes the agent starts from and who owns them; it provides no process
//! confinement. Confinement is the ProcessBoundary seam (and, in production, Sandboy).
//!
//! Create, reopen, cleanup, and prune belong to o7d, whose durable, versioned
//! `run -> worktree` record survives restarts and is re-attested (never trusted on its
//! own) before any deletion.

pub mod attest;
pub mod git;
pub mod identity;
pub mod materialize;
pub mod reap;
pub mod stateroot;
pub mod store;
pub mod worktree;

pub use attest::{attest_owned_dir, effective_uid, AttestError, FsIdentity};
pub use git::{GitError, HardenedGit, TreeEntry};
pub use identity::{
    CanonicalRepoId, CommittedRevision, IdentityDigest, IdentityError, RunId, WorktreeIdentity,
};
pub use materialize::{
    materialize, materialize_with_limits, MaterializeError, MaterializeLimits, MaterializePlan,
    MaterializeSummary,
};
pub use reap::{remove_verified_dir, ReapError};
pub use stateroot::{StateRoot, StateRootError};
pub use store::{
    RecoveredWorktree, RecoveryOutcome, StoreError, WorktreeRecord, WorktreeStore, STATE_SCHEMA,
};
pub use worktree::{CleanupOutcome, Worktree, WorktreeError};
