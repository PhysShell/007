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

/// What was written, for the durable summary and for tests.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaterializeSummary {
    pub files: usize,
    pub symlinks: usize,
    pub total_bytes: u64,
    /// Gitlink (submodule) entries that were deliberately NOT materialized — their
    /// bytes live in another repository.
    pub skipped_gitlinks: usize,
}

/// Materialize `revision` from `repo` into `dest`.
///
/// `dest` must already exist and be an owned, `0o700` directory (the caller creates it
/// under the state root). This function only writes *within* `dest`.
///
/// # Errors
/// [`MaterializeError`] on any git failure, unsafe path, or i/o error.
pub fn materialize(
    git: &HardenedGit,
    revision: &CommittedRevision,
    dest: &Path,
) -> Result<MaterializeSummary, MaterializeError> {
    let entries = git.list_tree(revision)?;
    let mut summary = MaterializeSummary::default();
    for entry in &entries {
        materialize_entry(git, dest, entry, &mut summary)?;
    }
    Ok(summary)
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
