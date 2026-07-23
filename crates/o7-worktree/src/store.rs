//! Durable ownership and recovery — the versioned `run -> worktree` record and the
//! o7d-owned lifecycle (`create` / `reopen` / `cleanup` / `prune`) over it.
//!
//! The record is a convenience, never an authority. Deletion authority always comes
//! from re-proving, against the live filesystem, that the directory a record names is
//! exactly the one we created:
//!
//!   * the record's own fields must re-hash to its stored identity digest (a tampered
//!     or forged record is refused, never actioned);
//!   * the live repository identity must equal the one recorded (repo drift is
//!     refused);
//!   * [`crate::attest::attest_owned_dir`] must prove the directory's `(dev, ino)`,
//!     owner, and owner-only bits.
//!
//! A serialized record, a path, and a materialization summary are all forgeable or
//! stale, so none of them — alone or together — authorizes a delete. Every operation
//! takes an exclusive advisory lock on the state root, so concurrent
//! create/reopen/cleanup/prune can never produce two owners. `prune` is deliberately
//! conservative: it reconciles records against disk (dropping records whose directory
//! is *provably* gone, preserving everything it cannot prove) and it NEVER deletes a
//! directory it cannot prove ownership of, so there is no automatic repo-global purge.

use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};

use crate::attest::{attest_owned_dir, AttestError, FsIdentity};
use crate::git::{GitError, HardenedGit};
use crate::identity::{
    CanonicalRepoId, CommittedRevision, IdentityDigest, RunId, WorktreeIdentity,
};
use crate::materialize::MaterializeSummary;
use crate::stateroot::StateRoot;
use crate::worktree::{CleanupOutcome, Worktree, WorktreeError};

/// The durable state schema version. Bumped only on a breaking change; a mismatch is
/// an error, never a silent migration.
pub const STATE_SCHEMA: u32 = 1;

/// The on-disk `run -> worktree` state, versioned.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateFile {
    schema: u32,
    records: Vec<WorktreeRecord>,
}

/// One durable `run -> worktree` record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRecord {
    pub run_id: RunId,
    pub repo: CanonicalRepoId,
    pub revision: CommittedRevision,
    pub identity_digest: IdentityDigest,
    pub worktree_path: PathBuf,
    pub fs_identity: FsIdentity,
    /// Diagnostic only; NEVER an authority (see the module docs).
    pub summary: MaterializeSummary,
    pub created_unix: u64,
}

/// What a [`WorktreeStore::prune`] reconciliation did. It reports; it does not purge.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryOutcome {
    /// Records whose worktree was re-proven present and ours; kept.
    pub kept: Vec<RunId>,
    /// Records whose worktree was PROVABLY gone (nothing at the path); the stale record
    /// was dropped. Nothing was deleted (there was nothing to delete).
    pub dropped_missing: Vec<RunId>,
    /// Records that could NOT be proven (tampered digest, symlink/inode/owner/perms
    /// mismatch); kept and flagged. Never deleted, never dropped.
    pub preserved: Vec<(RunId, String)>,
    /// Directories under the state root that no record references. Reported for
    /// investigation and NEVER auto-deleted — an unreferenced directory has no proven
    /// owner, so purging it would be exactly the repo-global prune this forbids.
    pub unreferenced: Vec<PathBuf>,
}

/// A reopened, re-proven worktree plus its durable record.
#[derive(Debug, Clone)]
pub struct RecoveredWorktree {
    pub worktree: Worktree,
    pub record: WorktreeRecord,
}

/// A failure in the durable lifecycle.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Worktree(#[from] WorktreeError),
    #[error(transparent)]
    Attest(#[from] AttestError),
    #[error("run {0} is already tracked; use reopen, not create")]
    RunAlreadyTracked(String),
    #[error("run {0} is not tracked")]
    NotTracked(String),
    #[error(
        "durable record for run {run} is inconsistent with its own identity digest — \
         refusing to trust a tampered/forged record"
    )]
    RecordTampered { run: String },
    #[error(
        "repository identity drift for run {run}: the live repository is not the one the \
         record was created against"
    )]
    RepoIdentityDrift { run: String },
    #[error(
        "durable state schema {found} is not the supported version {STATE_SCHEMA}; refusing \
         to guess a migration"
    )]
    SchemaMismatch { found: u32 },
    #[error("durable state at {path:?} is corrupt: {detail} — refusing to treat it as empty")]
    Corrupt { path: PathBuf, detail: String },
    #[error("acquiring the state lock failed: {0}")]
    Lock(String),
    #[error("i/o error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("serializing durable state failed: {0}")]
    Serialize(String),
}

/// o7d's durable owner of worktrees over one state root.
#[derive(Debug, Clone)]
pub struct WorktreeStore {
    state_root: StateRoot,
}

impl WorktreeStore {
    #[must_use]
    pub fn new(state_root: StateRoot) -> Self {
        Self { state_root }
    }

    #[must_use]
    pub fn state_root(&self) -> &StateRoot {
        &self.state_root
    }

    fn state_file(&self) -> PathBuf {
        self.state_root.path().join("worktrees.json")
    }

    fn lock_file(&self) -> PathBuf {
        self.state_root.path().join(".lock")
    }

    /// Take the exclusive advisory lock that serializes every lifecycle operation.
    /// Held until the returned guard drops. Cross-process (flock on the state root),
    /// so two o7d instances never both mutate the state.
    fn lock(&self) -> Result<Flock<std::fs::File>, StoreError> {
        let path = self.lock_file();
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&path)
            .map_err(|source| StoreError::Io {
                path: path.clone(),
                source,
            })?;
        Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, errno)| {
            StoreError::Lock(format!("flock LOCK_EX on {path:?} failed: {errno}"))
        })
    }

    fn load(&self) -> Result<StateFile, StoreError> {
        let path = self.state_file();
        match std::fs::read(&path) {
            Ok(bytes) => {
                let state: StateFile =
                    serde_json::from_slice(&bytes).map_err(|e| StoreError::Corrupt {
                        path: path.clone(),
                        detail: e.to_string(),
                    })?;
                if state.schema != STATE_SCHEMA {
                    return Err(StoreError::SchemaMismatch {
                        found: state.schema,
                    });
                }
                Ok(state)
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(StateFile {
                schema: STATE_SCHEMA,
                records: Vec::new(),
            }),
            Err(source) => Err(StoreError::Io { path, source }),
        }
    }

    /// Atomically persist state: write a 0600 temp file, fsync it, rename over the real
    /// file, then fsync the directory so the rename is durable.
    fn persist(&self, state: &StateFile) -> Result<(), StoreError> {
        let json =
            serde_json::to_vec_pretty(state).map_err(|e| StoreError::Serialize(e.to_string()))?;
        let final_path = self.state_file();
        let tmp_path = self.state_root.path().join("worktrees.json.tmp");
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp_path)
                .map_err(|source| StoreError::Io {
                    path: tmp_path.clone(),
                    source,
                })?;
            file.write_all(&json).map_err(|source| StoreError::Io {
                path: tmp_path.clone(),
                source,
            })?;
            file.sync_all().map_err(|source| StoreError::Io {
                path: tmp_path.clone(),
                source,
            })?;
        }
        std::fs::rename(&tmp_path, &final_path).map_err(|source| StoreError::Io {
            path: final_path.clone(),
            source,
        })?;
        if let Ok(dir) = std::fs::File::open(self.state_root.path()) {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    /// Create a new worktree for `run_id` at `revision` and durably record it.
    ///
    /// # Errors
    /// [`StoreError::RunAlreadyTracked`] if the run already has a record; otherwise any
    /// git / materialization / i/o failure.
    pub fn create(
        &self,
        git: &HardenedGit,
        run_id: RunId,
        revision: &CommittedRevision,
    ) -> Result<Worktree, StoreError> {
        let _guard = self.lock()?;
        let mut state = self.load()?;
        if state.records.iter().any(|r| r.run_id == run_id) {
            return Err(StoreError::RunAlreadyTracked(run_id.to_string()));
        }
        let worktree = Worktree::create(git, &self.state_root, run_id.clone(), revision)?;
        let record = WorktreeRecord {
            run_id,
            repo: worktree.identity().repo.clone(),
            revision: worktree.identity().revision.clone(),
            identity_digest: worktree.digest().clone(),
            worktree_path: worktree.path().to_path_buf(),
            fs_identity: worktree.fs_identity(),
            summary: worktree.summary().clone(),
            created_unix: now_unix(),
        };
        state.records.push(record);
        // Persist AFTER materializing. If we crash between the two, recovery finds a
        // directory with no record — reported as `unreferenced`, never silently
        // deleted, so authority is not lost.
        self.persist(&state)?;
        Ok(worktree)
    }

    /// Reopen an already-recorded worktree, re-attesting its FULL identity. Accepts
    /// only an EXACT match: record digest integrity, live repository identity, and
    /// on-disk `(dev, ino)`/owner/perms. Any drift is rejected and nothing is deleted.
    ///
    /// # Errors
    /// [`StoreError`] if the run is untracked, the record is tampered, the repository
    /// drifted, or the on-disk directory cannot be proven ours.
    pub fn reopen(
        &self,
        git: &HardenedGit,
        run_id: &RunId,
    ) -> Result<RecoveredWorktree, StoreError> {
        let _guard = self.lock()?;
        let state = self.load()?;
        let record = state
            .records
            .iter()
            .find(|r| &r.run_id == run_id)
            .cloned()
            .ok_or_else(|| StoreError::NotTracked(run_id.to_string()))?;

        let identity = self.verify_record_integrity(&record)?;

        // The live repository must be the SAME one the record was created against.
        let live_repo = git.canonical_repo_id()?;
        if live_repo != record.repo {
            return Err(StoreError::RepoIdentityDrift {
                run: run_id.to_string(),
            });
        }

        // Prove the on-disk directory is exactly ours (fail-closed).
        attest_owned_dir(&record.worktree_path, record.fs_identity)?;

        let worktree = Worktree::adopt(
            identity,
            record.identity_digest.clone(),
            record.worktree_path.clone(),
            record.fs_identity,
            record.summary.clone(),
        );
        Ok(RecoveredWorktree { worktree, record })
    }

    /// Delete a tracked worktree — but ONLY after live attestation proves ownership.
    /// The record's presence and summary confer no authority.
    ///
    /// # Errors
    /// [`StoreError::NotTracked`] if the run is unknown, [`StoreError::RecordTampered`]
    /// if the record is inconsistent, or an i/o error during removal. A directory that
    /// cannot be proven ours yields `Ok(PreservedForInvestigation)` and is left intact.
    pub fn cleanup(&self, run_id: &RunId) -> Result<CleanupOutcome, StoreError> {
        let _guard = self.lock()?;
        let mut state = self.load()?;
        let idx = state
            .records
            .iter()
            .position(|r| &r.run_id == run_id)
            .ok_or_else(|| StoreError::NotTracked(run_id.to_string()))?;
        let record = state
            .records
            .get(idx)
            .cloned()
            .ok_or_else(|| StoreError::NotTracked(run_id.to_string()))?;

        // A tampered/forged record is never actioned — do NOT delete on its say-so.
        self.verify_record_integrity(&record)?;

        match attest_owned_dir(&record.worktree_path, record.fs_identity) {
            Ok(_) => {
                std::fs::remove_dir_all(&record.worktree_path).map_err(|source| {
                    StoreError::Io {
                        path: record.worktree_path.clone(),
                        source,
                    }
                })?;
                state.records.remove(idx);
                self.persist(&state)?;
                Ok(CleanupOutcome::Removed)
            }
            Err(err) if err.is_missing() => {
                // The directory is provably gone (e.g. an interrupted earlier cleanup):
                // there is nothing to delete, so dropping the stale record is safe.
                state.records.remove(idx);
                self.persist(&state)?;
                Ok(CleanupOutcome::Removed)
            }
            Err(err) => {
                // Cannot prove ownership — preserve everything, keep the record.
                Ok(CleanupOutcome::PreservedForInvestigation(err.to_string()))
            }
        }
    }

    /// Reconcile the durable records against the live filesystem after a restart.
    /// Drops records whose worktree is *provably* gone; preserves (keeps + flags)
    /// every record it cannot prove; reports unreferenced on-disk directories WITHOUT
    /// deleting them. It never deletes a directory.
    ///
    /// # Errors
    /// [`StoreError`] on an i/o failure reading the state root or persisting.
    pub fn prune(&self) -> Result<RecoveryOutcome, StoreError> {
        let _guard = self.lock()?;
        let mut state = self.load()?;
        let mut outcome = RecoveryOutcome::default();
        let mut remaining = Vec::with_capacity(state.records.len());
        for record in std::mem::take(&mut state.records) {
            // Integrity first: a record that does not match its own digest is never
            // actioned — preserved and flagged.
            if self.verify_record_integrity(&record).is_err() {
                outcome
                    .preserved
                    .push((record.run_id.clone(), "record digest mismatch".to_owned()));
                remaining.push(record);
                continue;
            }
            match attest_owned_dir(&record.worktree_path, record.fs_identity) {
                Ok(_) => {
                    outcome.kept.push(record.run_id.clone());
                    remaining.push(record);
                }
                Err(err) if err.is_missing() => {
                    // Provably gone — drop the stale record (nothing to delete).
                    outcome.dropped_missing.push(record.run_id.clone());
                }
                Err(err) => {
                    // Unprovable — preserve, keep, flag. Never delete.
                    outcome
                        .preserved
                        .push((record.run_id.clone(), err.to_string()));
                    remaining.push(record);
                }
            }
        }
        state.records = remaining;
        self.persist(&state)?;
        outcome.unreferenced = self.unreferenced_dirs(&state)?;
        Ok(outcome)
    }

    /// Recompute the identity from the record's own fields and require it to equal the
    /// stored digest, and require the on-disk path to be the one the digest derives.
    fn verify_record_integrity(
        &self,
        record: &WorktreeRecord,
    ) -> Result<WorktreeIdentity, StoreError> {
        let identity = WorktreeIdentity::new(
            record.run_id.clone(),
            record.repo.clone(),
            record.revision.clone(),
        );
        let digest = identity.digest();
        // The path is bound to the digest, and the digest is bound to the identity: a
        // record whose digest, path, and fields do not all agree is not trustworthy.
        let path_ok = self.state_root.worktree_path(&digest) == record.worktree_path;
        if digest != record.identity_digest || !path_ok {
            return Err(StoreError::RecordTampered {
                run: record.run_id.to_string(),
            });
        }
        Ok(identity)
    }

    /// Directories directly under the state root that no record references. Reported,
    /// never deleted.
    fn unreferenced_dirs(&self, state: &StateFile) -> Result<Vec<PathBuf>, StoreError> {
        let known: std::collections::BTreeSet<&str> = state
            .records
            .iter()
            .map(|r| r.identity_digest.as_str())
            .collect();
        let root = self.state_root.path();
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(root) {
            Ok(entries) => entries,
            Err(source) => {
                return Err(StoreError::Io {
                    path: root.to_path_buf(),
                    source,
                })
            }
        };
        for entry in entries {
            let entry = entry.map_err(|source| StoreError::Io {
                path: root.to_path_buf(),
                source,
            })?;
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            // Only real directories are candidate worktrees; the state/lock files and
            // any symlink are ignored here.
            if !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                out.push(entry.path());
                continue;
            };
            if !known.contains(name) {
                out.push(entry.path());
            }
        }
        Ok(out)
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
