//! Materialize a committed revision into an owned directory, straight from the
//! object store.
//!
//! This is the operation that makes "only committed bytes, and no repository-controlled
//! helper" true: files are written from `git cat-file blob` output verbatim, so no
//! smudge filter, `post-checkout` hook, fsmonitor, or external diff is ever involved —
//! there is no `git checkout` at all. An operator's dirty working copy is never read,
//! so uncommitted bytes cannot leak in.
//!
//! Path safety: every entry path is validated to be relative with no `.`/`..`/root
//! component, and every parent directory is created and re-checked to be a real
//! directory (never followed through a symlink). Because a git tree cannot contain
//! both a symlink `a` and an entry under `a/`, an in-tree symlink can never become the
//! ancestor of another entry — but the write path defends against it regardless.

use std::io::Write as _;
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Component, Path, PathBuf};

use crate::git::{GitError, HardenedGit, TreeEntry};
use crate::identity::CommittedRevision;

/// Owner+group-restricted directory mode for materialized subdirectories. The state
/// root itself is `0o700`; subdirectories under it match.
const DIR_MODE: u32 = 0o700;

/// A failure materializing a revision.
#[derive(Debug, thiserror::Error)]
pub enum MaterializeError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("unsafe tree path {path:?}: {reason}")]
    UnsafePath { path: PathBuf, reason: String },
    #[error("tree entry {path:?} targets reserved git metadata (a `.git` component); refusing")]
    ReservedGitPath { path: PathBuf },
    #[error("tree path {path:?} is {len} bytes, exceeding the maximum {max}")]
    PathTooLong {
        path: PathBuf,
        len: usize,
        max: usize,
    },
    #[error("tree entry {path:?} has an unsupported mode {mode:#o}")]
    UnsupportedMode { path: PathBuf, mode: u32 },
    #[error("tree has {count} entries, exceeding the maximum {max}")]
    TooManyEntries { count: usize, max: usize },
    #[error("blob {path:?} is {size} bytes, exceeding the per-blob maximum {max}")]
    BlobTooLarge { path: PathBuf, size: u64, max: u64 },
    #[error("cumulative materialized bytes would exceed the maximum {max}")]
    TotalTooLarge { max: u64 },
    #[error("tree has {count} {kind}, exceeding the maximum {max}")]
    TooMany {
        kind: &'static str,
        count: usize,
        max: usize,
    },
    #[error("blob {oid} for {path:?} was {actual} bytes but the tree recorded {expected}")]
    BlobSizeDrift {
        oid: String,
        path: PathBuf,
        expected: u64,
        actual: usize,
    },
    #[error("i/o error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("blob {oid} for {path:?} was {actual} bytes but git recorded a symlink target that is empty")]
    EmptySymlink {
        oid: String,
        path: PathBuf,
        actual: usize,
    },
}

/// Hard bounds enforced on attacker-controlled committed data BEFORE any bytes are
/// written, so a hostile tree cannot exhaust disk, inodes, or memory.
#[derive(Debug, Clone, Copy)]
pub struct MaterializeLimits {
    pub max_entries: usize,
    pub max_path_len: usize,
    pub max_blob_bytes: u64,
    pub max_total_bytes: u64,
    pub max_files: usize,
    pub max_symlinks: usize,
    /// Ceiling on the on-disk size of the object CLOSURE copied into the worktree's own
    /// objectdb (the commit's full reachable history, not just its working tree). The
    /// working-tree budgets above do not bound history, so a repo with a tiny current tree
    /// but huge old blobs is refused here before any objects are copied.
    pub max_closure_bytes: u64,
}

impl Default for MaterializeLimits {
    fn default() -> Self {
        Self {
            max_entries: 1_000_000,
            max_path_len: 4096,
            max_blob_bytes: 512 * 1024 * 1024,
            max_total_bytes: 8 * 1024 * 1024 * 1024,
            max_files: 1_000_000,
            max_symlinks: 1_000_000,
            max_closure_bytes: 8 * 1024 * 1024 * 1024,
        }
    }
}

/// What was written, for the durable summary and for tests.
///
/// A stored summary is diagnostic only: it never confers deletion authority (that
/// comes solely from live attestation), so a forged or stale summary in a durable
/// record cannot authorize a delete.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MaterializeSummary {
    pub files: usize,
    pub symlinks: usize,
    pub total_bytes: u64,
    /// Gitlink (submodule) entries that were deliberately NOT materialized — their
    /// bytes live in another repository.
    pub skipped_gitlinks: usize,
}

/// Materialize `revision` from `repo` into `dest` under [`MaterializeLimits::default`].
///
/// # Errors
/// [`MaterializeError`] on any git failure, unsafe path, budget breach, or i/o error.
pub fn materialize(
    git: &HardenedGit,
    revision: &CommittedRevision,
    dest: &Path,
) -> Result<MaterializeSummary, MaterializeError> {
    materialize_with_limits(git, revision, dest, &MaterializeLimits::default())
}

/// Materialize `revision` from `repo` into `dest`, enforcing `limits`.
///
/// `dest` must already exist and be an owned, `0o700` directory (the caller creates it
/// under the state root). This function only writes *within* `dest`, and it runs a
/// COMPLETE preflight over every tree entry — validating paths (including rejecting any
/// reserved `.git` component), modes, path lengths, and the entry/blob/cumulative byte
/// budgets — BEFORE creating a single filesystem object, so a hostile tree fails closed
/// with nothing written.
///
/// # Errors
/// [`MaterializeError`] on any git failure, unsafe/reserved path, budget breach, or i/o
/// error.
pub fn materialize_with_limits(
    git: &HardenedGit,
    revision: &CommittedRevision,
    dest: &Path,
    limits: &MaterializeLimits,
) -> Result<MaterializeSummary, MaterializeError> {
    MaterializePlan::prepare(git, revision, limits)?.write(git, dest)
}

/// A validated materialization plan: the tree entries after a COMPLETE preflight. It is
/// produced by a read-only [`MaterializePlan::prepare`] pass (no filesystem writes), so a
/// hostile tree is rejected before any directory, `.git` metadata, or file is created.
#[derive(Debug, Clone)]
pub struct MaterializePlan {
    entries: Vec<TreeEntry>,
}

impl MaterializePlan {
    /// List the tree and run the full preflight (paths, reserved `.git`, modes, path
    /// length, entry/blob/cumulative budgets). Purely read-only.
    ///
    /// # Errors
    /// [`MaterializeError`] on any git failure, unsafe/reserved path, or budget breach.
    pub fn prepare(
        git: &HardenedGit,
        revision: &CommittedRevision,
        limits: &MaterializeLimits,
    ) -> Result<Self, MaterializeError> {
        let entries = git.list_tree(revision)?;
        preflight(&entries, limits)?;
        Ok(Self { entries })
    }

    /// Write the validated plan into `dest` (which must be an owned `0o700` directory).
    ///
    /// # Errors
    /// [`MaterializeError`] on a git or i/o failure while writing.
    pub fn write(
        &self,
        git: &HardenedGit,
        dest: &Path,
    ) -> Result<MaterializeSummary, MaterializeError> {
        let mut summary = MaterializeSummary::default();
        for entry in &self.entries {
            materialize_entry(git, dest, entry, &mut summary)?;
        }
        Ok(summary)
    }
}

/// Validate EVERY entry before anything is written: safe relative paths, no reserved
/// `.git` component, supported modes, bounded path length, and entry/blob/cumulative
/// budgets. Returns the first violation; on success the write loop cannot breach a
/// budget or touch git metadata.
fn preflight(entries: &[TreeEntry], limits: &MaterializeLimits) -> Result<(), MaterializeError> {
    if entries.len() > limits.max_entries {
        return Err(MaterializeError::TooManyEntries {
            count: entries.len(),
            max: limits.max_entries,
        });
    }
    let mut files = 0usize;
    let mut symlinks = 0usize;
    let mut total: u64 = 0;
    for entry in entries {
        let rel = safe_relative(&entry.path)?;
        if first_component_is_reserved_git(&rel) {
            return Err(MaterializeError::ReservedGitPath {
                path: entry.path.clone(),
            });
        }
        let len = os_path_len(&entry.path);
        if len > limits.max_path_len {
            return Err(MaterializeError::PathTooLong {
                path: entry.path.clone(),
                len,
                max: limits.max_path_len,
            });
        }
        if entry.is_gitlink() {
            continue; // never materialized; contributes no bytes.
        }
        if !entry.is_regular_file() && !entry.is_symlink() {
            return Err(MaterializeError::UnsupportedMode {
                path: entry.path.clone(),
                mode: entry.mode,
            });
        }
        let size = entry
            .size
            .ok_or_else(|| MaterializeError::UnsupportedMode {
                path: entry.path.clone(),
                mode: entry.mode,
            })?;
        if size > limits.max_blob_bytes {
            return Err(MaterializeError::BlobTooLarge {
                path: entry.path.clone(),
                size,
                max: limits.max_blob_bytes,
            });
        }
        total = total
            .checked_add(size)
            .filter(|t| *t <= limits.max_total_bytes)
            .ok_or(MaterializeError::TotalTooLarge {
                max: limits.max_total_bytes,
            })?;
        if entry.is_symlink() {
            symlinks += 1;
            if symlinks > limits.max_symlinks {
                return Err(MaterializeError::TooMany {
                    kind: "symlinks",
                    count: symlinks,
                    max: limits.max_symlinks,
                });
            }
        } else {
            files += 1;
            if files > limits.max_files {
                return Err(MaterializeError::TooMany {
                    kind: "files",
                    count: files,
                    max: limits.max_files,
                });
            }
        }
    }
    Ok(())
}

/// Whether the first path component is a reserved git-metadata name (`.git`, case- and
/// trailing-dot/space-insensitive, plus the NTFS 8.3 short form) — which would collide
/// with the real `.git` gitdir. Only the FIRST component matters: `.gitignore` /
/// `.gitattributes` (legitimate files) are not first-component `.git`.
fn first_component_is_reserved_git(rel: &Path) -> bool {
    let Some(Component::Normal(first)) = rel.components().next() else {
        return false;
    };
    let bytes = first.as_encoded_bytes();
    // Strip trailing dots/spaces (FAT/NTFS fold these away) and lowercase ASCII.
    let trimmed: Vec<u8> = bytes
        .iter()
        .rev()
        .skip_while(|b| **b == b'.' || **b == b' ')
        .copied()
        .collect::<Vec<u8>>()
        .into_iter()
        .rev()
        .map(|b| b.to_ascii_lowercase())
        .collect();
    trimmed == b".git" || trimmed == b"git~1"
}

fn os_path_len(path: &Path) -> usize {
    use std::os::unix::ffi::OsStrExt as _;
    path.as_os_str().as_bytes().len()
}

fn materialize_entry(
    git: &HardenedGit,
    dest: &Path,
    entry: &TreeEntry,
    summary: &mut MaterializeSummary,
) -> Result<(), MaterializeError> {
    let rel = safe_relative(&entry.path)?;
    let target = dest.join(&rel);
    // Create parent directories, verifying each is a real directory we own the shape
    // of — never write through a symlink.
    if let Some(parent) = rel.parent() {
        create_dirs_checked(dest, parent)?;
    }

    if entry.is_gitlink() {
        summary.skipped_gitlinks += 1;
        return Ok(());
    }

    let bytes = git.cat_blob(&entry.oid)?;

    // Defense in depth: the blob the preflight budgeted (via `ls-tree -l`) must be the
    // blob we read. A content-addressed object cannot change size, so a mismatch means
    // something is wrong — fail closed rather than write unbudgeted bytes.
    if let Some(expected) = entry.size {
        if bytes.len() as u64 != expected {
            return Err(MaterializeError::BlobSizeDrift {
                oid: entry.oid.clone(),
                path: entry.path.clone(),
                expected,
                actual: bytes.len(),
            });
        }
    }

    if entry.is_symlink() {
        if bytes.is_empty() {
            return Err(MaterializeError::EmptySymlink {
                oid: entry.oid.clone(),
                path: entry.path.clone(),
                actual: 0,
            });
        }
        // The blob content is the link target, verbatim.
        use std::os::unix::ffi::OsStrExt as _;
        let link_target = PathBuf::from(std::ffi::OsStr::from_bytes(&bytes));
        std::os::unix::fs::symlink(&link_target, &target).map_err(|source| {
            MaterializeError::Io {
                path: target.clone(),
                source,
            }
        })?;
        summary.symlinks += 1;
        return Ok(());
    }

    // A regular file (or an unexpected mode we conservatively treat as a plain file:
    // executable bit only distinguishes 100755 from 100644).
    let mode = if entry.is_executable() { 0o755 } else { 0o644 };
    // O_CREAT|O_EXCL: refuse to follow or overwrite anything already at the path (a
    // duplicate tree path or a pre-planted entry is a hard error, not a silent clobber).
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&target)
        .map_err(|source| MaterializeError::Io {
            path: target.clone(),
            source,
        })?;
    file.write_all(&bytes)
        .map_err(|source| MaterializeError::Io {
            path: target.clone(),
            source,
        })?;
    summary.files += 1;
    summary.total_bytes = summary.total_bytes.saturating_add(bytes.len() as u64);
    Ok(())
}

/// Validate that a tree path is a plain relative path (no root, no `.`/`..`, no
/// prefix), so `dest.join(path)` can never escape `dest`.
fn safe_relative(path: &Path) -> Result<PathBuf, MaterializeError> {
    let mut out = PathBuf::new();
    let mut any = false;
    for component in path.components() {
        match component {
            Component::Normal(part) => {
                out.push(part);
                any = true;
            }
            Component::CurDir => {}
            other => {
                return Err(MaterializeError::UnsafePath {
                    path: path.to_path_buf(),
                    reason: format!("contains a non-normal component: {other:?}"),
                });
            }
        }
    }
    if !any {
        return Err(MaterializeError::UnsafePath {
            path: path.to_path_buf(),
            reason: "empty after normalization".to_owned(),
        });
    }
    Ok(out)
}

/// Create every component of `rel` under `base`, checking after each step that the
/// path is a real directory (via `symlink_metadata`, which does not follow a final
/// symlink). A pre-existing symlink at any component fails closed.
fn create_dirs_checked(base: &Path, rel: &Path) -> Result<(), MaterializeError> {
    let mut current = base.to_path_buf();
    for component in rel.components() {
        let Component::Normal(part) = component else {
            // `rel` came from `safe_relative`, so only Normal components remain; anything
            // else is a bug, treated as unsafe rather than trusted.
            return Err(MaterializeError::UnsafePath {
                path: rel.to_path_buf(),
                reason: "non-normal component while creating directories".to_owned(),
            });
        };
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_dir() => {}
            Ok(meta) => {
                return Err(MaterializeError::UnsafePath {
                    path: current.clone(),
                    reason: format!("expected a directory, found {:?}", meta.file_type()),
                });
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                std::fs::DirBuilder::new()
                    .mode(DIR_MODE)
                    .create(&current)
                    .map_err(|source| MaterializeError::Io {
                        path: current.clone(),
                        source,
                    })?;
            }
            Err(source) => {
                return Err(MaterializeError::Io {
                    path: current.clone(),
                    source,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn safe_relative_rejects_traversal_and_absolute() {
        assert!(safe_relative(Path::new("../escape")).is_err());
        assert!(safe_relative(Path::new("/etc/passwd")).is_err());
        assert!(safe_relative(Path::new("a/../b")).is_err());
        assert!(safe_relative(Path::new("")).is_err());
        assert_eq!(
            safe_relative(Path::new("a/./b")).expect("ok"),
            PathBuf::from("a/b")
        );
    }
}
