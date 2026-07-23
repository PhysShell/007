//! Filesystem attestation — the proof that must succeed before anything is deleted.
//!
//! A serialized record, a path, and a summary are all forgeable or stale. Deletion
//! authority comes from re-proving, against the live filesystem, that the directory at
//! a path is *exactly* the one we created and own:
//!
//! * it is a directory reached without following a symlink (`symlink_metadata`);
//! * its `(dev, ino)` equal what we recorded at create time — a rename of the path to
//!   a different directory, or an inode replacement, moves these;
//! * it is owned by our effective uid;
//! * its permission bits are owner-only (`0o700`).
//!
//! Any failure is fail-closed: [`AttestError`] is returned and the caller must NOT
//! delete — the files are preserved for investigation.

use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The filesystem identity of a directory: the device and inode that a rename or
/// substitution cannot forge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsIdentity {
    pub dev: u64,
    pub ino: u64,
}

impl FsIdentity {
    /// Read the identity of the directory at `path` without following a final symlink.
    ///
    /// # Errors
    /// [`AttestError`] if the path is missing, is a symlink, or is not a directory.
    pub fn of_dir(path: &Path) -> Result<Self, AttestError> {
        let meta = std::fs::symlink_metadata(path).map_err(|source| AttestError::Stat {
            path: path.to_path_buf(),
            source,
        })?;
        let file_type = meta.file_type();
        if file_type.is_symlink() {
            return Err(AttestError::SymlinkSubstitution {
                path: path.to_path_buf(),
            });
        }
        if !file_type.is_dir() {
            return Err(AttestError::NotADirectory {
                path: path.to_path_buf(),
            });
        }
        Ok(Self {
            dev: meta.dev(),
            ino: meta.ino(),
        })
    }
}

/// Required owner-only permission bits for a state directory.
pub const OWNER_ONLY: u32 = 0o700;

/// A failed ownership/identity proof. Every variant means "do not delete".
#[derive(Debug, thiserror::Error)]
pub enum AttestError {
    #[error("cannot stat {path:?}: {source}")]
    Stat {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path:?} is a symlink; refusing to treat a substituted symlink as our worktree")]
    SymlinkSubstitution { path: PathBuf },
    #[error("{path:?} is not a directory")]
    NotADirectory { path: PathBuf },
    #[error(
        "{path:?} filesystem identity changed: recorded (dev {want_dev}, ino {want_ino}), \
         found (dev {got_dev}, ino {got_ino}) — a rename or inode replacement"
    )]
    IdentityMismatch {
        path: PathBuf,
        want_dev: u64,
        want_ino: u64,
        got_dev: u64,
        got_ino: u64,
    },
    #[error("{path:?} is owned by uid {found}, not our effective uid {expected}")]
    Ownership {
        path: PathBuf,
        expected: u32,
        found: u32,
    },
    #[error("{path:?} has permission bits {found:#o}, not owner-only {expected:#o}")]
    Permissions {
        path: PathBuf,
        expected: u32,
        found: u32,
    },
}

/// Our effective uid — the owner every state directory must have.
#[must_use]
pub fn effective_uid() -> u32 {
    nix::unistd::Uid::effective().as_raw()
}

/// Prove the directory at `path` is exactly the one recorded by `expected`, owned by
/// us, and owner-only. Returns the proven identity on success.
///
/// This is the sole gate on deletion: a caller may only remove a worktree after this
/// returns `Ok`. It fails closed on every mismatch.
///
/// # Errors
/// [`AttestError`] on any identity, ownership, or permission mismatch.
pub fn attest_owned_dir(path: &Path, expected: FsIdentity) -> Result<FsIdentity, AttestError> {
    let meta = std::fs::symlink_metadata(path).map_err(|source| AttestError::Stat {
        path: path.to_path_buf(),
        source,
    })?;
    let file_type = meta.file_type();
    if file_type.is_symlink() {
        return Err(AttestError::SymlinkSubstitution {
            path: path.to_path_buf(),
        });
    }
    if !file_type.is_dir() {
        return Err(AttestError::NotADirectory {
            path: path.to_path_buf(),
        });
    }
    let got = FsIdentity {
        dev: meta.dev(),
        ino: meta.ino(),
    };
    if got != expected {
        return Err(AttestError::IdentityMismatch {
            path: path.to_path_buf(),
            want_dev: expected.dev,
            want_ino: expected.ino,
            got_dev: got.dev,
            got_ino: got.ino,
        });
    }
    let euid = effective_uid();
    if meta.uid() != euid {
        return Err(AttestError::Ownership {
            path: path.to_path_buf(),
            expected: euid,
            found: meta.uid(),
        });
    }
    let bits = meta.mode() & 0o777;
    if bits != OWNER_ONLY {
        return Err(AttestError::Permissions {
            path: path.to_path_buf(),
            expected: OWNER_ONLY,
            found: bits,
        });
    }
    Ok(got)
}
