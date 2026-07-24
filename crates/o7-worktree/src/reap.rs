//! Race-safe deletion of a worktree directory.
//!
//! The naive `attest(path)` then `remove_dir_all(path)` re-resolves the path a second
//! time, so an attacker who swaps the directory between the two steps could have a
//! victim tree deleted. This module closes that window in two ways:
//!
//! 1. **Descriptor-bound identity.** The directory is opened ONCE with `O_NOFOLLOW`, its
//!    identity is proven against the OPEN file descriptor (`fstat`), and its contents are
//!    then removed using `*at` syscalls relative to that descriptor — so every unlink
//!    acts on the exact inode that was verified, never on a path that could have been
//!    swapped. A symlink at the target fails closed (never followed).
//! 2. **Quarantine-first.** Immediately after the identity is proven, the entry is
//!    atomically renamed to a private, unique name with `renameat2(RENAME_NOREPLACE)`,
//!    and identity is RE-proven on the quarantined name. Only then are the contents
//!    cleared and the now-empty directory `rmdir`-ed. So the only name-based steps
//!    operate on a name an attacker cannot have redirected (it is fresh, and
//!    `RENAME_NOREPLACE` guarantees nothing was clobbered), and if the pre-quarantine
//!    entry had been swapped, the re-proof fails closed with nothing deleted.
//!
//! The recursive content removal is entirely descriptor-relative and `O_NOFOLLOW`, so it
//! can never escape into another tree via a symlink, and the final `rmdir` uses
//! `REMOVEDIR`, which only ever removes an EMPTY directory — so even a last-instant swap
//! of the quarantined name can never delete a victim's populated tree.

use std::ffi::{CStr, CString};
use std::os::fd::{AsFd as _, BorrowedFd, OwnedFd};
use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use rustix::fs::{self, AtFlags, Dir, FileType, Mode, OFlags, RenameFlags};

use crate::attest::{effective_uid, FsIdentity, OWNER_ONLY};

/// Process-unique counter so each quarantine name is distinct (required by
/// `RENAME_NOREPLACE`, which refuses to clobber an existing entry).
static REAP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
/// our effective uid, owner-only `0o700`), quarantine it under a private name, re-prove,
/// clear it, then remove it — PROVING the verified inode is gone before returning `Ok`.
///
/// The parent is opened by path here (for callers that hold only a path). Callers that
/// hold a descriptor to the parent — e.g. the descriptor-bound state root — should use
/// [`remove_verified_child`] so no ancestor path is re-resolved at all.
///
/// # Errors
/// [`ReapError`]: `Unproven` if identity cannot be established or the removal cannot be
/// proven (nothing is dropped), `Vanished` if already gone, `Io` if deletion fails.
pub fn remove_verified_dir(path: &Path, expected: FsIdentity) -> Result<(), ReapError> {
    remove_verified_dir_hooked(path, expected, |_, _| {}, |_, _| {}, |_, _| {})
}

/// Descriptor-bound removal: the target is a CHILD named `name` under the already-open
/// `parent_fd` (e.g. the state root's bound root descriptor). Nothing walks or re-resolves
/// the parent path, so an ancestor swap cannot redirect the deletion. `parent_path` is used
/// only for error messages.
///
/// # Errors
/// [`ReapError`] as for [`remove_verified_dir`].
pub fn remove_verified_child(
    parent_fd: BorrowedFd<'_>,
    parent_path: &Path,
    name: &str,
    expected: FsIdentity,
) -> Result<(), ReapError> {
    let full = parent_path.join(name);
    let cname = cstr(name.as_bytes()).ok_or_else(|| ReapError::Unproven {
        path: full.clone(),
        reason: "name contains a NUL byte".to_owned(),
    })?;
    remove_verified_relative(
        parent_fd,
        parent_path,
        cname.as_c_str(),
        &full,
        expected,
        |_, _| {},
        |_, _| {},
        |_, _| {},
    )
}

/// Path-based entry with test seams: opens the parent by path, then delegates to
/// [`remove_verified_relative`]. `after_open` fires once the target is opened and proven;
/// `after_quarantine` once it is renamed to its private name; `before_unlink` after the
/// contents are cleared but before the final `rmdir`. In production all three are no-ops;
/// tests use them to substitute the entry at each race seam and prove fail-closed.
fn remove_verified_dir_hooked<A, Q, B>(
    path: &Path,
    expected: FsIdentity,
    after_open: A,
    after_quarantine: Q,
    before_unlink: B,
) -> Result<(), ReapError>
where
    A: FnMut(&Path, &CStr),
    Q: FnMut(&Path, &CStr),
    B: FnMut(&Path, &CStr),
{
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

    // Open the parent (no-follow) so the target and its later rename/rmdir are addressed
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

    remove_verified_relative(
        parent_fd.as_fd(),
        parent,
        name.as_c_str(),
        path,
        expected,
        after_open,
        after_quarantine,
        before_unlink,
    )
}

/// The removal core, performed ENTIRELY relative to `parent_fd`. The target is the child
/// `name`; `full` is its path for error messages only.
///
/// After the contents are cleared and the quarantine name is `rmdir`-ed, the verified
/// inode is RE-STATTED on the descriptor we still hold and its link count is required to be
/// zero. This closes the empty-substitution race: if the quarantine name was swapped for an
/// empty decoy just before the `rmdir` (so `rmdir` removed the decoy while our inode
/// survived under another name), the retained descriptor still shows a non-zero link count,
/// and we report `Unproven` — never a false `Ok` that would let the caller drop a durable
/// record while the real inode lives on.
#[allow(clippy::too_many_arguments)]
fn remove_verified_relative<A, Q, B>(
    parent_fd: BorrowedFd<'_>,
    parent_path: &Path,
    name: &CStr,
    full: &Path,
    expected: FsIdentity,
    mut after_open: A,
    mut after_quarantine: Q,
    mut before_unlink: B,
) -> Result<(), ReapError>
where
    A: FnMut(&Path, &CStr),
    Q: FnMut(&Path, &CStr),
    B: FnMut(&Path, &CStr),
{
    // Open the target itself, NO-FOLLOW: a symlink substituted for our directory fails
    // here (ELOOP), never followed.
    let dir_fd = match fs::openat(parent_fd, name, dir_flags(), Mode::empty()) {
        Ok(fd) => fd,
        Err(err) if err == rustix::io::Errno::NOENT => {
            return Err(ReapError::Vanished(full.to_path_buf()))
        }
        Err(err) => {
            return Err(ReapError::Unproven {
                path: full.to_path_buf(),
                reason: format!("cannot open as a directory without following symlinks: {err}"),
            })
        }
    };

    // Prove the OPENED inode is exactly the one we recorded and own.
    verify_fd(&dir_fd, expected, full)?;
    drop(dir_fd);
    after_open(parent_path, name);

    // Quarantine: atomically move the entry to a fresh private name. RENAME_NOREPLACE
    // guarantees we clobber nothing. From here, no later name-based step can be redirected
    // at a victim, because the name is one an attacker cannot have arranged in advance.
    let qname = quarantine_name(&expected);
    let qname = cstr(qname.as_bytes()).ok_or_else(|| ReapError::Unproven {
        path: full.to_path_buf(),
        reason: "quarantine name contains a NUL byte".to_owned(),
    })?;
    match fs::renameat_with(
        parent_fd,
        name,
        parent_fd,
        qname.as_c_str(),
        RenameFlags::NOREPLACE,
    ) {
        Ok(()) => {}
        Err(err) if err == rustix::io::Errno::NOENT => {
            return Err(ReapError::Vanished(full.to_path_buf()))
        }
        Err(err) => {
            return Err(ReapError::Unproven {
                path: full.to_path_buf(),
                reason: format!("could not quarantine the directory before deletion: {err}"),
            })
        }
    }
    after_quarantine(parent_path, qname.as_c_str());

    // Re-open the quarantined name (no-follow) and RE-PROVE identity. This closes the
    // window where the pre-quarantine entry could have been swapped: if what we just
    // renamed is not our recorded inode, the re-proof fails and nothing is deleted.
    let q_fd = match fs::openat(parent_fd, qname.as_c_str(), dir_flags(), Mode::empty()) {
        Ok(fd) => fd,
        Err(err) => {
            return Err(ReapError::Unproven {
                path: full.to_path_buf(),
                reason: format!("cannot re-open the quarantined directory: {err}"),
            })
        }
    };
    verify_fd(&q_fd, expected, full)?;

    // Identity is proven on the quarantined inode: clear the contents via descriptors,
    // then rmdir the now-empty directory. REMOVEDIR only removes an EMPTY directory, so a
    // last-instant swap of the quarantine name to a POPULATED tree can never delete it.
    clear_dir(&q_fd, full)?;
    before_unlink(parent_path, qname.as_c_str());
    fs::unlinkat(parent_fd, qname.as_c_str(), AtFlags::REMOVEDIR).map_err(|err| ReapError::Io {
        path: full.to_path_buf(),
        source: err.into(),
    })?;

    // PROVE the verified inode is actually gone. We still hold `q_fd` to it; after a real
    // rmdir of that inode its link count is 0. If the quarantine name had been swapped for
    // an EMPTY decoy just before the rmdir, the rmdir removed the decoy and our inode
    // survives under another name with a non-zero link count — fail closed rather than
    // report a deletion that did not happen.
    let st = fs::fstat(&q_fd).map_err(|err| ReapError::Unproven {
        path: full.to_path_buf(),
        reason: format!("cannot re-stat the removed directory to prove it is gone: {err}"),
    })?;
    if st.st_nlink != 0 {
        return Err(ReapError::Unproven {
            path: full.to_path_buf(),
            reason: format!(
                "the verified directory still has {} link(s) after removal — the quarantine \
                 name was swapped and the real inode survives; refusing to report it deleted",
                st.st_nlink
            ),
        });
    }
    Ok(())
}

/// A fresh, process-unique hidden name for the quarantine rename. Uniqueness (not
/// secrecy) is what `RENAME_NOREPLACE` requires; the security comes from the descriptor
/// re-proof, so a predictable name is acceptable.
fn quarantine_name(id: &FsIdentity) -> String {
    let n = REAP_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(".o7-reap.{}.{}.{}.{n}", std::process::id(), id.dev, id.ino)
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
    use std::path::PathBuf;

    /// Create an owner-only directory we "own" and return its path and recorded identity.
    fn owned_dir(parent: &Path, name: &str) -> (PathBuf, FsIdentity) {
        let p = parent.join(name);
        std::fs::DirBuilder::new()
            .mode(OWNER_ONLY)
            .create(&p)
            .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(OWNER_ONLY)).unwrap();
        let id = FsIdentity::of_dir(&p).unwrap();
        (p, id)
    }

    fn fresh_dir(at: &Path) {
        std::fs::DirBuilder::new()
            .mode(OWNER_ONLY)
            .create(at)
            .unwrap();
        std::fs::set_permissions(at, std::fs::Permissions::from_mode(OWNER_ONLY)).unwrap();
    }

    fn qpath(parent: &Path, qname: &CStr) -> PathBuf {
        parent.join(std::ffi::OsStr::from_bytes(qname.to_bytes()))
    }

    /// SEAM 1 — the entry is swapped for a DIFFERENT inode after we open+verify it but
    /// before the quarantine rename. The post-quarantine re-proof catches the mismatch,
    /// nothing is deleted, and the real tree (moved aside by the attacker) is untouched.
    #[test]
    fn a_swap_after_open_fails_closed_and_preserves_the_real_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();
        let (wt, id) = owned_dir(parent, "wt");
        std::fs::write(wt.join("marker"), b"real").unwrap();
        let moved = parent.join("wt_real");

        let res = remove_verified_dir_hooked(
            &wt,
            id,
            |_p, _n| {
                // Move the real dir away and drop a decoy (fresh inode) in its place.
                std::fs::rename(&wt, &moved).unwrap();
                fresh_dir(&wt);
            },
            |_p, _n| {},
            |_p, _n| {},
        );

        assert!(
            matches!(res, Err(ReapError::Unproven { .. })),
            "expected Unproven, got {res:?}"
        );
        assert!(
            moved.join("marker").exists(),
            "the real tree must be preserved, not deleted"
        );
    }

    /// SEAM 2 — the quarantined name is swapped for a different inode after the rename but
    /// before the re-open. The descriptor re-proof rejects it; nothing is deleted.
    #[test]
    fn a_swap_after_quarantine_fails_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();
        let (wt, id) = owned_dir(parent, "wt");
        std::fs::write(wt.join("marker"), b"real").unwrap();
        let stash = parent.join("stash");

        let res = remove_verified_dir_hooked(
            &wt,
            id,
            |_p, _n| {},
            |p, qn| {
                let q = qpath(p, qn);
                // Move our quarantined dir out and leave a decoy at the quarantine name.
                std::fs::rename(&q, &stash).unwrap();
                fresh_dir(&q);
            },
            |_p, _n| {},
        );

        assert!(
            matches!(res, Err(ReapError::Unproven { .. })),
            "expected Unproven, got {res:?}"
        );
        assert!(
            stash.join("marker").exists(),
            "the real tree must be preserved, not deleted"
        );
    }

    /// SEAM 3 — the quarantined name is swapped for a NON-EMPTY victim after the contents
    /// are cleared but before the final rmdir. `REMOVEDIR` only removes an empty directory,
    /// so the victim's contents survive and the removal reports an i/o error.
    #[test]
    fn a_non_empty_swap_before_final_unlink_never_deletes_the_victim() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();
        let (wt, id) = owned_dir(parent, "wt");
        let mut victim_file = PathBuf::new();
        let vf = &mut victim_file;

        let res = remove_verified_dir_hooked(
            &wt,
            id,
            |_p, _n| {},
            |_p, _n| {},
            |p, qn| {
                let q = qpath(p, qn);
                // Replace the (now-empty) quarantined dir with a populated victim.
                std::fs::remove_dir(&q).unwrap();
                std::fs::create_dir(&q).unwrap();
                let f = q.join("victim");
                std::fs::write(&f, b"do not delete").unwrap();
                *vf = f;
            },
        );

        assert!(
            matches!(res, Err(ReapError::Io { .. })),
            "expected Io (ENOTEMPTY), got {res:?}"
        );
        assert!(
            victim_file.exists() && std::fs::read(&victim_file).unwrap() == b"do not delete",
            "REMOVEDIR must never delete a non-empty victim tree"
        );
    }

    /// SEAM 3b — the quarantined name is swapped for an EMPTY decoy after the contents are
    /// cleared but before the final rmdir. `rmdir` succeeds on the decoy, but the verified
    /// inode survives under another name; the retained-descriptor link-count proof catches
    /// it, so the removal reports Unproven (NEVER a false success) and the real inode lives.
    #[test]
    fn an_empty_swap_before_final_unlink_is_not_reported_as_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();
        let (wt, id) = owned_dir(parent, "wt");
        let stash = parent.join("stash");

        let res = remove_verified_dir_hooked(
            &wt,
            id,
            |_p, _n| {},
            |_p, _n| {},
            |p, qn| {
                let q = qpath(p, qn);
                // Move our (now-cleared) verified inode out, and drop an EMPTY decoy at the
                // quarantine name — rmdir will happily remove the decoy.
                std::fs::rename(&q, &stash).unwrap();
                std::fs::create_dir(&q).unwrap();
            },
        );

        assert!(
            matches!(res, Err(ReapError::Unproven { .. })),
            "an empty swap must fail closed (Unproven), got {res:?}"
        );
        assert!(
            stash.exists(),
            "the verified inode was lost despite the removal not being proven"
        );
    }

    /// The happy path still removes an owned tree completely.
    #[test]
    fn removes_an_owned_tree_via_quarantine() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();
        let (wt, id) = owned_dir(parent, "wt");
        std::fs::write(wt.join("f"), b"x").unwrap();
        std::fs::create_dir(wt.join("sub")).unwrap();
        std::fs::write(wt.join("sub/g"), b"y").unwrap();

        remove_verified_dir(&wt, id).unwrap();
        assert!(!wt.exists(), "the owned tree should be gone");
    }
}
