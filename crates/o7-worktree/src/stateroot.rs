//! The state root — the owner-only directory tree, OUTSIDE any source repository,
//! where materialized worktrees live.
//!
//! Everything the substrate writes lives under one absolute, `0o700`, self-owned root.
//! A worktree's location is derived from its identity digest, so the path itself
//! encodes which identity it must attest to. The root is never allowed to be inside a
//! source repository — an operator's checkout must never contain the agent's copies.

use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _};
use std::path::{Path, PathBuf};

use crate::attest::{effective_uid, OWNER_ONLY};
use crate::identity::IdentityDigest;

/// A failure establishing or using the state root.
#[derive(Debug, thiserror::Error)]
pub enum StateRootError {
    #[error("state root {0:?} must be an absolute path")]
    NotAbsolute(PathBuf),
    #[error("state root {root:?} is inside source repository {repo:?}; it must live outside it")]
    InsideRepo { root: PathBuf, repo: PathBuf },
    #[error("state root {path:?} is a symlink; refusing to use a substituted root")]
    Symlink { path: PathBuf },
    #[error("state root {path:?} exists but is not a directory")]
    NotADirectory { path: PathBuf },
    #[error("state root {path:?} is owned by uid {found}, not our effective uid {expected}")]
    Ownership {
        path: PathBuf,
        expected: u32,
        found: u32,
    },
    #[error("state root {path:?} has permission bits {found:#o}, not owner-only {expected:#o}")]
    Permissions {
        path: PathBuf,
        expected: u32,
        found: u32,
    },
    #[error("i/o error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// An established, verified state root.
#[derive(Debug, Clone)]
pub struct StateRoot {
    path: PathBuf,
}

impl StateRoot {
    /// Open (creating if missing) the state root at `path`.
    ///
    /// The path must be absolute. If it already exists it is verified to be a
    /// self-owned, owner-only, real directory (fail closed otherwise). If created, it
    /// is created `0o700`.
    ///
    /// # Errors
    /// [`StateRootError`] if the path is relative, exists in a wrong shape, or cannot
    /// be created/verified.
    pub fn open_or_create(path: impl Into<PathBuf>) -> Result<Self, StateRootError> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(StateRootError::NotAbsolute(path));
        }
        match std::fs::symlink_metadata(&path) {
            Ok(meta) => verify_dir(&path, &meta)?,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                std::fs::DirBuilder::new()
                    .recursive(true)
                    .mode(OWNER_ONLY)
                    .create(&path)
                    .map_err(|source| StateRootError::Io {
                        path: path.clone(),
                        source,
                    })?;
                // `recursive(true)` applies the mode only to the final component and
                // umask can still clear bits; re-assert owner-only explicitly.
                std::fs::set_permissions(
                    &path,
                    std::os::unix::fs::PermissionsExt::from_mode(OWNER_ONLY),
                )
                .map_err(|source| StateRootError::Io {
                    path: path.clone(),
                    source,
                })?;
                let meta =
                    std::fs::symlink_metadata(&path).map_err(|source| StateRootError::Io {
                        path: path.clone(),
                        source,
                    })?;
                verify_dir(&path, &meta)?;
            }
            Err(source) => {
                return Err(StateRootError::Io { path, source });
            }
        }
        Ok(Self { path })
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The directory a worktree with `digest` must live at. The path encodes the
    /// identity, so the location itself is bound to the triple it was derived from.
    #[must_use]
    pub fn worktree_path(&self, digest: &IdentityDigest) -> PathBuf {
        self.path.join(digest.as_str())
    }

    /// Reject a state root that sits inside `repo` (or vice versa). Both paths are
    /// compared after canonicalization where possible; a repo that cannot be
    /// canonicalized still gets a lexical check.
    ///
    /// # Errors
    /// [`StateRootError::InsideRepo`] if either path contains the other.
    pub fn ensure_outside_repo(&self, repo: &Path) -> Result<(), StateRootError> {
        let root = self
            .path
            .canonicalize()
            .unwrap_or_else(|_| self.path.clone());
        let repo = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
        if root.starts_with(&repo) || repo.starts_with(&root) {
            return Err(StateRootError::InsideRepo { root, repo });
        }
        Ok(())
    }
}

fn verify_dir(path: &Path, meta: &std::fs::Metadata) -> Result<(), StateRootError> {
    let file_type = meta.file_type();
    if file_type.is_symlink() {
        return Err(StateRootError::Symlink {
            path: path.to_path_buf(),
        });
    }
    if !file_type.is_dir() {
        return Err(StateRootError::NotADirectory {
            path: path.to_path_buf(),
        });
    }
    let euid = effective_uid();
    if meta.uid() != euid {
        return Err(StateRootError::Ownership {
            path: path.to_path_buf(),
            expected: euid,
            found: meta.uid(),
        });
    }
    let bits = meta.mode() & 0o777;
    if bits != OWNER_ONLY {
        return Err(StateRootError::Permissions {
            path: path.to_path_buf(),
            expected: OWNER_ONLY,
            found: bits,
        });
    }
    Ok(())
}
