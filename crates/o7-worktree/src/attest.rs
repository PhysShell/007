//! Filesystem attestation — the proof that must succeed before anything is deleted.
//!
//! A serialized record, a path, and a summary are all forgeable or stale. Deletion
//! authority comes from re-proving, against the live filesystem, that the directory at
//! a path is *exactly* the one we created and own:
//!
//! * it is a directory reached without following a symlink (`O_DIRECTORY | O_NOFOLLOW`);
//! * its `(dev, ino)` equal what we recorded at create time — a rename of the path to
//!   a different directory, or an inode replacement, moves these;
//! * it is owned by our effective uid;
//! * its permission bits are owner-only (`0o700`).
//!
//! Identity is read through ONE source — a rustix `fstat` on an `O_DIRECTORY | O_NOFOLLOW`
//! descriptor — the SAME source `stateroot::create_worktree_dir` records with and `reap`
//! verifies against. Using one `dev`/`ino` encoding everywhere avoids the glibc-vs-raw
//! `dev_t` mismatch that a mix of std `MetadataExt` and rustix `fstat` would risk, which
//! would otherwise fail deletion closed on a legitimate directory.
//!
//! Any failure is fail-closed: [`AttestError`] is returned and the caller must NOT
//! delete — the files are preserved for investigation.

use std::os::fd::OwnedFd;
use std::path::{Path, PathBuf};

use rustix::fs::{self, Mode, OFlags};
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
        let fd = open_dir_nofollow(path)?;
        let st = fstat(&fd, path)?;
        Ok(Self {
            dev: st.st_dev,
            ino: st.st_ino,
        })
    }
}

/// Open `path` as a directory without following a final symlink, mapping the failure
/// modes to the fail-closed [`AttestError`] variants. This is the single entry point for
/// reading a directory's identity, so creation, attestation, and reaping all agree.
fn open_dir_nofollow(path: &Path) -> Result<OwnedFd, AttestError> {
    fs::open(
        path,
        OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::RDONLY,
        Mode::empty(),
    )
    .map_err(|err| {
        if err == rustix::io::Errno::LOOP {
            // O_NOFOLLOW hit a symlink at the final component.
            AttestError::SymlinkSubstitution {
                path: path.to_path_buf(),
            }
        } else if err == rustix::io::Errno::NOTDIR {
            AttestError::NotADirectory {
                path: path.to_path_buf(),
            }
        } else {
            AttestError::Stat {
                path: path.to_path_buf(),
                source: err.into(),
            }
        }
    })
}

fn fstat(fd: &OwnedFd, path: &Path) -> Result<rustix::fs::Stat, AttestError> {
    fs::fstat(fd).map_err(|err| AttestError::Stat {
        path: path.to_path_buf(),
        source: err.into(),
    })
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

impl AttestError {
    /// Whether the failure is "nothing is at the path" (a confirmed absence), as
    /// opposed to "something is there but is not provably ours". Recovery treats a
    /// provably-absent worktree as already gone (drop the stale record); it treats an
    /// unprovable one as preserve-for-investigation (never delete).
    #[must_use]
    pub fn is_missing(&self) -> bool {
        matches!(
            self,
            AttestError::Stat { source, .. } if source.kind() == std::io::ErrorKind::NotFound
        )
    }
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
    // One identity source: fstat on an O_DIRECTORY|O_NOFOLLOW descriptor (a symlink fails
    // as ELOOP, a non-directory as ENOTDIR), matching create/reap exactly.
    let fd = open_dir_nofollow(path)?;
    let st = fstat(&fd, path)?;
    let got = FsIdentity {
        dev: st.st_dev,
        ino: st.st_ino,
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
    if st.st_uid != euid {
        return Err(AttestError::Ownership {
            path: path.to_path_buf(),
            expected: euid,
            found: st.st_uid,
        });
    }
    let bits = st.st_mode & 0o777;
    if bits != OWNER_ONLY {
        return Err(AttestError::Permissions {
            path: path.to_path_buf(),
            expected: OWNER_ONLY,
            found: bits,
        });
    }
    Ok(got)
}
