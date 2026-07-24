//! Race-safe deletion of a worktree directory.
//!
//! The naive `attest(path)` then `remove_dir_all(path)` re-resolves the path a second
//! time, so an attacker who swaps the directory between the two steps could have a
//! victim tree deleted. This module closes that window: it opens the directory ONCE with
//! `O_NOFOLLOW`, verifies identity against the OPEN file descriptor (`fstat`), and then
//! removes the tree using `*at` syscalls relative to that descriptor — so every unlink
//! acts on the exact inode that was verified, never on a path that could have been
//! swapped. A symlink at the target fails closed (never followed).
//!
//! The recursive content removal is entirely descriptor-relative and `O_NOFOLLOW`, so it
//! can never escape into another tree via a symlink. The only name-based step is the
//! final `rmdir` of the now-empty top directory from its parent's descriptor, and
//! `REMOVEDIR` only ever removes an empty directory — so the destructive part (deleting
//! files) can never touch anything but the verified inode's own contents.

use std::ffi::CString;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags};

use crate::attest::{effective_uid, FsIdentity, OWNER_ONLY};

/// The outcome of attempting a verified removal.
#[derive(Debug, thiserror::Error)]
pub enum ReapError {
    /// Nothing was deleted: the directory could not be proven ours (a symlink was found
    /// where a directory was expected, or the `(dev, ino)`, owner, or permission bits did
    /// not match what was recorded).
    #[error("refusing to delete {path:?}: {reason}")]
    Unproven {
        path: std::path::PathBuf,
        reason: String,
    },
    /// The directory (or its parent) was not present — there is nothing to delete.
    #[error("{0:?} is already gone")]
    Vanished(std::path::PathBuf),
    /// A failure occurred DURING deletion, AFTER identity was proven; some contents may
    /// already have been removed.
    #[error("deletion of {path:?} failed after identity was proven: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Open `path` with `O_NOFOLLOW`, prove the opened inode is exactly `expected` (owned by
/// our effective uid, owner-only `0o700`), then remove the whole tree relative to that
/// descriptor.
///
/// # Errors
/// [`ReapError`]: `Unproven` if identity cannot be established (nothing deleted),
/// `Vanished` if the path is already gone, `Io` if deletion fails after verification.
pub fn remove_verified_dir(path: &Path, expected: FsIdentity) -> Result<(), ReapError> {
    let parent = path.parent().ok_or_else(|| ReapError::Unproven {
        path: path.to_path_buf(),
        reason: "path has no parent directory".to_owned(),
    })?;
    let name = path.file_name().ok_or_else(|| ReapError::Unproven {
        path: path.to_path_buf(),
        reason: "path has no final component".to_owned(),
    })?;
    let name = cstr(name.as_bytes()).ok_or_else(|| ReapError::Unproven {
        path: path.to_path_buf(),
        reason: "path component contains a NUL byte".to_owned(),
    })?;

    // Open the parent (no-follow) so the target and its later rmdir are addressed
    // relative to a descriptor, not a re-resolved path.
    let parent_fd = match fs::open(parent, dir_flags(), Mode::empty()) {
        Ok(fd) => fd,
        Err(err) if err == rustix::io::Errno::NOENT => {
            return Err(ReapError::Vanished(path.to_path_buf()))
        }
        Err(err) => {
            return Err(ReapError::Unproven {
                path: parent.to_path_buf(),
                reason: format!("cannot open parent directory: {err}"),
            })
        }
    };

    // Open the target itself, NO-FOLLOW: a symlink substituted for our directory fails
    // here (ELOOP), never followed.
    let dir_fd = match fs::openat(&parent_fd, name.as_c_str(), dir_flags(), Mode::empty()) {
        Ok(fd) => fd,
        Err(err) if err == rustix::io::Errno::NOENT => {
            return Err(ReapError::Vanished(path.to_path_buf()))
        }
        Err(err) => {
            return Err(ReapError::Unproven {
                path: path.to_path_buf(),
                reason: format!("cannot open as a directory without following symlinks: {err}"),
            })
        }
    };

    // Prove the OPENED inode is exactly the one we recorded and own.
    verify_fd(&dir_fd, expected, path)?;

    // From here identity is proven: clear the contents via descriptors, then rmdir the
    // now-empty directory from its parent's descriptor.
    clear_dir(&dir_fd, path)?;
    fs::unlinkat(&parent_fd, name.as_c_str(), AtFlags::REMOVEDIR).map_err(|err| ReapError::Io {
        path: path.to_path_buf(),
        source: err.into(),
    })?;
    Ok(())
}

fn dir_flags() -> OFlags {
    OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::RDONLY
}

/// Verify the open descriptor's inode identity, owner, and permission bits.
fn verify_fd(fd: &OwnedFd, expected: FsIdentity, path: &Path) -> Result<(), ReapError> {
    let st = fs::fstat(fd).map_err(|err| ReapError::Unproven {
        path: path.to_path_buf(),
        reason: format!("fstat failed: {err}"),
    })?;
    let got = FsIdentity {
        dev: st.st_dev as u64,
        ino: st.st_ino as u64,
    };
    if got != expected {
        return Err(ReapError::Unproven {
            path: path.to_path_buf(),
            reason: format!(
                "filesystem identity changed: recorded (dev {}, ino {}), found (dev {}, ino {})",
                expected.dev, expected.ino, got.dev, got.ino
            ),
        });
    }
    let euid = effective_uid();
    if st.st_uid as u32 != euid {
        return Err(ReapError::Unproven {
            path: path.to_path_buf(),
            reason: format!("owned by uid {}, not our effective uid {euid}", st.st_uid),
        });
    }
    let bits = st.st_mode as u32 & 0o777;
    if bits != OWNER_ONLY {
        return Err(ReapError::Unproven {
            path: path.to_path_buf(),
            reason: format!("permission bits {bits:#o}, not owner-only {OWNER_ONLY:#o}"),
        });
    }
    Ok(())
}

/// Recursively remove everything inside `dirfd` using descriptor-relative, no-follow
/// operations. Entries are collected before unlinking so the directory stream is not
/// mutated while it is being read.
fn clear_dir(dirfd: &OwnedFd, path: &Path) -> Result<(), ReapError> {
    let io = |source: std::io::Error| ReapError::Io {
        path: path.to_path_buf(),
        source,
    };

    let dir = Dir::read_from(dirfd).map_err(|e| io(e.into()))?;
    let mut files: Vec<CString> = Vec::new();
    let mut subdirs: Vec<CString> = Vec::new();
    for entry in dir {
        let entry = entry.map_err(|e| io(e.into()))?;
        let name = entry.file_name();
        if name.to_bytes() == b"." || name.to_bytes() == b".." {
            continue;
        }
        let owned = name.to_owned();
        // Classify without following symlinks. If the readdir type is unknown, stat the
        // entry relative to the descriptor (no-follow) to decide.
        let is_dir = match entry.file_type() {
            FileType::Directory => true,
            FileType::Unknown => {
                let st =
                    fs::statat(dirfd, name, AtFlags::SYMLINK_NOFOLLOW).map_err(|e| io(e.into()))?;
                (st.st_mode as u32 & libc_ifmt()) == libc_ifdir()
            }
            _ => false,
        };
        if is_dir {
            subdirs.push(owned);
        } else {
            files.push(owned);
        }
    }

    for name in &files {
        fs::unlinkat(dirfd, name.as_c_str(), AtFlags::empty()).map_err(|e| io(e.into()))?;
    }
    for name in &subdirs {
        let sub = fs::openat(dirfd, name.as_c_str(), dir_flags(), Mode::empty())
            .map_err(|e| io(e.into()))?;
        clear_dir(&sub, path)?;
        drop(sub);
        fs::unlinkat(dirfd, name.as_c_str(), AtFlags::REMOVEDIR).map_err(|e| io(e.into()))?;
    }
    Ok(())
}

fn cstr(bytes: &[u8]) -> Option<CString> {
    CString::new(bytes).ok()
}

// `S_IFMT` / `S_IFDIR` without pulling in the libc crate: these values are stable on
// Linux. Used only for the rare readdir `Unknown` fallback classification.
const fn libc_ifmt() -> u32 {
    0o170_000
}
const fn libc_ifdir() -> u32 {
    0o040_000
}
