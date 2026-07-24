//! The state root — the owner-only directory tree, OUTSIDE any source repository,
//! where materialized worktrees live.
//!
//! Everything the substrate writes lives under one absolute, `0o700`, self-owned root.
//! A worktree's location is derived from its identity digest, so the path itself
//! encodes which identity it must attest to. The root is never allowed to be inside a
//! source repository — an operator's checkout must never contain the agent's copies.
//!
//! The state root is a **descriptor-bound capability**: on open it resolves the path
//! ONCE, then holds an `O_DIRECTORY | O_NOFOLLOW` file descriptor to the root inode.
//! Every subsequent operation — creating a worktree directory, taking the `.lock`,
//! reading the state file, writing it atomically — is performed with `*at` syscalls
//! RELATIVE to that descriptor and re-verifies the inode on the fd first. So once the
//! root is bound, swapping an ancestor directory for a symlink, renaming the root away,
//! or substituting a control file for a symlink cannot redirect any operation at a
//! victim: the fd pins the exact inode, `O_NOFOLLOW`/`O_EXCL` reject substituted
//! children, and identity is re-proven on the descriptor, not on a re-resolved path.

use std::ffi::CString;
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use rustix::fs::{self, Mode, OFlags};

use crate::attest::{effective_uid, FsIdentity, OWNER_ONLY};
use crate::identity::IdentityDigest;
use crate::reap::{remove_verified_child, ReapError};

/// Process-unique counter so each atomic-write temp file has a distinct name (it is
/// created `O_EXCL`, so a name collision would fail rather than adopt a foreign file).
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    #[error("a worktree directory already exists at {0:?}")]
    WorktreeExists(PathBuf),
    #[error("control file {path:?} is not a regular owner-only ({expected:#o}) file")]
    UnsafeControlFile { path: PathBuf, expected: u32 },
    #[error("path component {0:?} contains a NUL byte")]
    BadName(PathBuf),
    #[error("i/o error at {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// An established, verified, descriptor-bound state root.
#[derive(Debug, Clone)]
pub struct StateRoot {
    path: PathBuf,
    /// The bound root inode. `Arc` so [`StateRoot`] stays `Clone` (the fd is shared, not
    /// duplicated); every clone addresses the exact same inode.
    fd: Arc<OwnedFd>,
}

impl StateRoot {
    /// Open (creating if missing) the state root at `path`, and BIND a descriptor to it.
    ///
    /// The path must be absolute. The final component is opened `O_NOFOLLOW` (a symlink
    /// there fails closed), and the opened inode is proven a self-owned, owner-only
    /// directory. If missing, it is created `0o700` and then bound.
    ///
    /// # Errors
    /// [`StateRootError`] if the path is relative, has a symlink at the final component,
    /// exists in a wrong shape, or cannot be created/verified.
    pub fn open_or_create(path: impl Into<PathBuf>) -> Result<Self, StateRootError> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(StateRootError::NotAbsolute(path));
        }
        let fd = match open_dir_nofollow(&path) {
            Ok(fd) => fd,
            Err(err) if err == rustix::io::Errno::NOENT => {
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
                open_dir_nofollow(&path).map_err(|err| map_open_err(err, &path))?
            }
            Err(err) => return Err(map_open_err(err, &path)),
        };
        verify_root_fd(&fd, &path)?;
        Ok(Self {
            path,
            fd: Arc::new(fd),
        })
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

    fn root(&self) -> std::os::fd::BorrowedFd<'_> {
        use std::os::fd::AsFd as _;
        self.fd.as_fd()
    }

    /// Re-prove the bound root inode is STILL a self-owned owner-only directory. Cheap,
    /// and closes the window where the root's owner or bits changed after binding.
    fn reverify(&self) -> Result<(), StateRootError> {
        verify_root_fd(&self.fd, &self.path)
    }

    /// Create the worktree directory for `digest` `mkdirat`-relative to the bound root,
    /// re-open it `O_NOFOLLOW`, assert owner-only bits, and return its path and the
    /// filesystem identity proven on the descriptor.
    ///
    /// # Errors
    /// [`StateRootError::WorktreeExists`] if a directory is already there (never adopt an
    /// existing path); otherwise a verification or i/o failure.
    pub fn create_worktree_dir(
        &self,
        digest: &IdentityDigest,
    ) -> Result<(PathBuf, FsIdentity), StateRootError> {
        self.reverify()?;
        let full = self.worktree_path(digest);
        let name = cname(digest.as_str(), &full)?;

        match fs::mkdirat(
            self.root(),
            name.as_c_str(),
            Mode::from_raw_mode(OWNER_ONLY),
        ) {
            Ok(()) => {}
            Err(err) if err == rustix::io::Errno::EXIST => {
                return Err(StateRootError::WorktreeExists(full))
            }
            Err(err) => {
                return Err(StateRootError::Io {
                    path: full,
                    source: err.into(),
                })
            }
        }

        let dir_fd = fs::openat(
            self.root(),
            name.as_c_str(),
            OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::RDONLY,
            Mode::empty(),
        )
        .map_err(|err| StateRootError::Io {
            path: full.clone(),
            source: err.into(),
        })?;
        // mkdirat's mode is masked by umask; re-assert owner-only on the descriptor.
        fs::fchmod(&dir_fd, Mode::from_raw_mode(OWNER_ONLY)).map_err(|err| StateRootError::Io {
            path: full.clone(),
            source: err.into(),
        })?;
        verify_root_fd(&dir_fd, &full)?;
        let id = fs_identity_of(&dir_fd, &full)?;
        Ok((full, id))
    }

    /// Remove the worktree directory for `digest` RELATIVE to the bound root descriptor —
    /// the deletion capability of the descriptor-bound state root. The quarantine-first,
    /// identity-proven, removal-proven reap runs with the target addressed as a child of the
    /// bound root fd, so no ancestor path is re-resolved and an ancestor swap cannot redirect
    /// the deletion.
    ///
    /// # Errors
    /// [`ReapError`]: `Unproven` if identity or removal cannot be proven (nothing deleted),
    /// `Vanished` if already gone, `Io` if deletion fails after verification.
    pub fn remove_worktree_dir(
        &self,
        digest: &IdentityDigest,
        expected: FsIdentity,
    ) -> Result<(), ReapError> {
        // Re-prove the bound root before deleting anything relative to it.
        self.reverify().map_err(|err| ReapError::Unproven {
            path: self.path.clone(),
            reason: format!("state root no longer proves ours before delete: {err}"),
        })?;
        remove_verified_child(self.root(), &self.path, digest.as_str(), expected)
    }

    /// Open (creating `0o600` if missing) the `.lock` control file RELATIVE to the bound
    /// root, `O_NOFOLLOW` (a symlink fails closed), and prove it is a regular, self-owned,
    /// owner-only file before returning it for advisory locking.
    ///
    /// # Errors
    /// [`StateRootError`] if the file is a symlink, not regular, not self-owned, not
    /// `0o600`, or cannot be opened.
    pub fn open_lock_file(&self) -> Result<std::fs::File, StateRootError> {
        self.reverify()?;
        let full = self.path.join(".lock");
        let fd = fs::openat(
            self.root(),
            c".lock",
            OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|err| map_control_err(err, &full))?;
        verify_regular_owner_mode(&fd, 0o600, &full)?;
        Ok(std::fs::File::from(fd))
    }

    /// Read the named state file RELATIVE to the bound root, `O_NOFOLLOW`. Returns
    /// `Ok(None)` if it does not exist. A symlink or non-regular file fails closed.
    ///
    /// # Errors
    /// [`StateRootError`] on a symlink/non-regular control file or an i/o failure.
    pub fn read_state_file(&self, name: &str) -> Result<Option<Vec<u8>>, StateRootError> {
        use std::io::Read as _;
        self.reverify()?;
        let full = self.path.join(name);
        let cname = cname(name, &full)?;
        let fd = match fs::openat(
            self.root(),
            cname.as_c_str(),
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(err) if err == rustix::io::Errno::NOENT => return Ok(None),
            Err(err) => return Err(map_control_err(err, &full)),
        };
        // A regular, self-owned file — never a device, fifo, or foreign-owned object.
        verify_regular_owner_mode(&fd, 0o600, &full)?;
        let mut file = std::fs::File::from(fd);
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|source| StateRootError::Io { path: full, source })?;
        Ok(Some(bytes))
    }

    /// Atomically replace the named state file with `bytes`: write a UNIQUE `O_EXCL`,
    /// `O_NOFOLLOW`, `0o600` temp file relative to the bound root, fsync it, `renameat` it
    /// over the final name relative to the same root, then fsync the root descriptor so
    /// the rename is durable. `O_EXCL` guarantees the temp is a fresh regular file, never
    /// a substituted symlink or a pre-existing foreign file.
    ///
    /// # Errors
    /// [`StateRootError`] on any i/o failure; the final file is left untouched if the temp
    /// write fails.
    pub fn write_state_file_atomic(&self, name: &str, bytes: &[u8]) -> Result<(), StateRootError> {
        use std::io::Write as _;
        self.reverify()?;
        let final_full = self.path.join(name);
        let final_name = cname(name, &final_full)?;

        let tmp_str = format!(
            ".{name}.tmp.{}.{}",
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let tmp_full = self.path.join(&tmp_str);
        let tmp_name = cname(&tmp_str, &tmp_full)?;

        let fd = fs::openat(
            self.root(),
            tmp_name.as_c_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|err| StateRootError::Io {
            path: tmp_full.clone(),
            source: err.into(),
        })?;
        let mut file = std::fs::File::from(fd);
        let write_then_sync = file
            .write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|source| StateRootError::Io {
                path: tmp_full.clone(),
                source,
            });
        if let Err(err) = write_then_sync {
            drop(file);
            let _ = fs::unlinkat(self.root(), tmp_name.as_c_str(), fs::AtFlags::empty());
            return Err(err);
        }
        drop(file);

        if let Err(err) = fs::renameat(
            self.root(),
            tmp_name.as_c_str(),
            self.root(),
            final_name.as_c_str(),
        ) {
            let _ = fs::unlinkat(self.root(), tmp_name.as_c_str(), fs::AtFlags::empty());
            return Err(StateRootError::Io {
                path: final_full,
                source: err.into(),
            });
        }
        // Make the rename durable: fsync the DIRECTORY the entry lives in.
        fs::fsync(self.root()).map_err(|err| StateRootError::Io {
            path: self.path.clone(),
            source: err.into(),
        })?;
        Ok(())
    }

    /// List the immediate child names under the bound root (for reconciliation). Reads the
    /// directory from the descriptor, so an ancestor swap cannot redirect the listing.
    ///
    /// # Errors
    /// [`StateRootError`] on an i/o failure reading the directory.
    pub fn list_children(&self) -> Result<Vec<ChildEntry>, StateRootError> {
        self.reverify()?;
        let dir = fs::Dir::read_from(self.root()).map_err(|err| StateRootError::Io {
            path: self.path.clone(),
            source: err.into(),
        })?;
        let mut out = Vec::new();
        for entry in dir {
            let entry = entry.map_err(|err| StateRootError::Io {
                path: self.path.clone(),
                source: err.into(),
            })?;
            let raw = entry.file_name().to_bytes();
            if raw == b"." || raw == b".." {
                continue;
            }
            let is_dir = matches!(entry.file_type(), fs::FileType::Directory);
            let is_symlink = matches!(entry.file_type(), fs::FileType::Symlink);
            let name = String::from_utf8_lossy(raw).into_owned();
            out.push(ChildEntry {
                name,
                is_dir,
                is_symlink,
                path: self.path.join(std::ffi::OsStr::from_bytes(raw)),
            });
        }
        Ok(out)
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

/// One immediate child of the state root, as read from its descriptor.
#[derive(Debug, Clone)]
pub struct ChildEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_symlink: bool,
    pub path: PathBuf,
}

fn open_dir_nofollow(path: &Path) -> rustix::io::Result<OwnedFd> {
    fs::open(
        path,
        OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::RDONLY,
        Mode::empty(),
    )
}

fn map_open_err(err: rustix::io::Errno, path: &Path) -> StateRootError {
    if err == rustix::io::Errno::LOOP {
        StateRootError::Symlink {
            path: path.to_path_buf(),
        }
    } else if err == rustix::io::Errno::NOTDIR {
        StateRootError::NotADirectory {
            path: path.to_path_buf(),
        }
    } else {
        StateRootError::Io {
            path: path.to_path_buf(),
            source: err.into(),
        }
    }
}

fn map_control_err(err: rustix::io::Errno, path: &Path) -> StateRootError {
    if err == rustix::io::Errno::LOOP {
        StateRootError::Symlink {
            path: path.to_path_buf(),
        }
    } else {
        StateRootError::Io {
            path: path.to_path_buf(),
            source: err.into(),
        }
    }
}

/// Verify an open descriptor is a self-owned, owner-only DIRECTORY.
fn verify_root_fd(fd: &OwnedFd, path: &Path) -> Result<(), StateRootError> {
    let st = fs::fstat(fd).map_err(|err| StateRootError::Io {
        path: path.to_path_buf(),
        source: err.into(),
    })?;
    if (st.st_mode as u32 & 0o170_000) != 0o040_000 {
        return Err(StateRootError::NotADirectory {
            path: path.to_path_buf(),
        });
    }
    let euid = effective_uid();
    if st.st_uid as u32 != euid {
        return Err(StateRootError::Ownership {
            path: path.to_path_buf(),
            expected: euid,
            found: st.st_uid as u32,
        });
    }
    let bits = st.st_mode as u32 & 0o777;
    if bits != OWNER_ONLY {
        return Err(StateRootError::Permissions {
            path: path.to_path_buf(),
            expected: OWNER_ONLY,
            found: bits,
        });
    }
    Ok(())
}

/// Verify an open descriptor is a self-owned REGULAR file with exactly `mode` bits.
fn verify_regular_owner_mode(fd: &OwnedFd, mode: u32, path: &Path) -> Result<(), StateRootError> {
    let st = fs::fstat(fd).map_err(|err| StateRootError::Io {
        path: path.to_path_buf(),
        source: err.into(),
    })?;
    let is_regular = (st.st_mode as u32 & 0o170_000) == 0o100_000;
    let owned = st.st_uid as u32 == effective_uid();
    let bits_ok = (st.st_mode as u32 & 0o777) == mode;
    if !is_regular || !owned || !bits_ok {
        return Err(StateRootError::UnsafeControlFile {
            path: path.to_path_buf(),
            expected: mode,
        });
    }
    Ok(())
}

fn fs_identity_of(fd: &OwnedFd, path: &Path) -> Result<FsIdentity, StateRootError> {
    let st = fs::fstat(fd).map_err(|err| StateRootError::Io {
        path: path.to_path_buf(),
        source: err.into(),
    })?;
    Ok(FsIdentity {
        dev: st.st_dev as u64,
        ino: st.st_ino as u64,
    })
}

fn cname(name: &str, full: &Path) -> Result<CString, StateRootError> {
    CString::new(name.as_bytes()).map_err(|_| StateRootError::BadName(full.to_path_buf()))
}
