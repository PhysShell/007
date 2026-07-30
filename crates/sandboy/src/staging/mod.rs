//! VB-4 — the PRIVATE EXECUTION OBJECT. Landlock cannot authorize execution of a pathless anonymous
//! `memfd` while enforcing a narrow path-exec allowlist (see
//! `docs/architecture/vb4-exec-authority-contract-correction.md`): a memfd is only covered by a
//! root-level `EXECUTE` grant, which would authorize every path exec. So instead of forcing the
//! kernel to treat the memfd as a path object, we materialize an object of the kind Landlock DOES
//! control — a regular file on a backend-private tmpfs — and prove its bytes equal the fully-sealed
//! source.
//!
//! The sealed source `memfd` remains the transport + source-identity object. This module derives a
//! second, execution-only object:
//!   private user+mount namespace → private tmpfs → unnamed `O_TMPFILE` inode → exact byte copy →
//!   digest re-verified against the sealed source AND the launch-spec target → every writable fd
//!   closed → non-dumpable → returned as an `O_RDONLY` exec fd + an `O_PATH` rule fd.
//! The caller attaches an EXACT Landlock `EXECUTE | READ_FILE` rule to the `O_PATH` fd and
//! `execveat`s the `O_RDONLY` fd. Immutability holds two ways: inside the launch, no writer survives
//! and Landlock denies a write-reopen; outside, non-dumpable blocks the only remaining reopen vector
//! (`/proc/<pid>/fd`). The `unsafe` lives entirely in [`sys`].

mod sys;

use std::ffi::CString;
use std::os::fd::RawFd;

use o7_sandbox_protocol::ids::Digest256;

/// The private tmpfs mountpoint inside the launch child's own mount namespace. `/dev/shm` is chosen
/// deliberately: it always exists, nothing the target needs lives under it, and — unlike `/tmp` — it
/// is never an ancestor of the worktree (which the parent creates under `/tmp`), so shadowing it with
/// the private tmpfs cannot hide the worktree or the self-check probe directory.
const STAGING_MOUNTPOINT: &[u8] = b"/dev/shm\0";

/// The narrow byte-copy chunk for streaming the sealed source into the execution inode.
const COPY_CHUNK: usize = 64 * 1024;

/// The materialized execution object: an `O_RDONLY` fd to `execveat`, an `O_PATH` fd for the exact
/// Landlock rule (closed by the caller before exec), and the verified digest.
pub(crate) struct ExecObject {
    pub(crate) exec_fd: RawFd,
    pub(crate) rule_fd: RawFd,
    pub(crate) digest: Digest256,
}

/// The stage a staging failure occurred at — each maps to a distinct fault point and a distinct
/// fail-closed verdict; the target never runs on a staging failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
    Namespace,
    IdMap,
    MountPrivate,
    MountTmpfs,
    Tmpfile,
    Copy,
    Digest,
    Reopen,
    CloseWriter,
    ProcIsolation,
}

impl Stage {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Stage::Namespace => "target_namespace",
            Stage::IdMap => "target_idmap",
            Stage::MountPrivate => "target_mount",
            Stage::MountTmpfs => "target_mount",
            Stage::Tmpfile => "target_tmpfile",
            Stage::Copy => "target_copy",
            Stage::Digest => "target_digest",
            Stage::Reopen => "target_identity",
            Stage::CloseWriter => "target_close_writer",
            Stage::ProcIsolation => "target_proc_isolation",
        }
    }
}

#[derive(Debug)]
pub(crate) struct StageError {
    pub(crate) stage: Stage,
    pub(crate) errno: i32,
}

impl StageError {
    fn new(stage: Stage, errno: i32) -> StageError {
        StageError { stage, errno }
    }
}

impl std::fmt::Display for StageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "private execution object failed at {}: errno {}",
            self.stage.as_str(),
            self.errno
        )
    }
}

/// TEST-ONLY (`fault-injection`) forced-failure knobs for the staging stages. Production is the unit
/// default; every method is a no-op. The mapping from the typed launch `FaultId` is filled in by the
/// fault seam.
#[derive(Clone, Copy, Default)]
pub(crate) struct Faults {
    #[cfg(feature = "fault-injection")]
    pub(crate) fail_at: Option<Stage>,
}

impl Faults {
    #[cfg(feature = "fault-injection")]
    fn check(&self, stage: Stage) -> Result<(), StageError> {
        if self.fail_at == Some(stage) {
            return Err(StageError::new(stage, libc::EIO));
        }
        Ok(())
    }
    #[cfg(not(feature = "fault-injection"))]
    fn check(&self, _stage: Stage) -> Result<(), StageError> {
        Ok(())
    }
}

/// Materialize the private execution object from `source` — the exact bytes of the inherited sealed
/// source `memfd`, read once by the caller (which also uses them to parse the target's ELF). The
/// resulting inode's bytes are proven EQUAL to `source` AND to hash to `target_digest` (the
/// launch-spec target binding). Fails closed at the exact stage on any error.
pub(crate) fn materialize(
    source: &[u8],
    target_digest: &Digest256,
    faults: &Faults,
) -> Result<ExecObject, StageError> {
    let (uid, gid) = sys::current_ids();

    // 1) Private user + mount namespace.
    faults.check(Stage::Namespace)?;
    sys::unshare_user_mount().map_err(|e| StageError::new(Stage::Namespace, e))?;

    // 2) Identity map (uid/gid 0-in-ns → outer id) so worktree DAC still resolves; deny setgroups.
    faults.check(Stage::IdMap)?;
    write_id_maps(uid, gid).map_err(|e| StageError::new(Stage::IdMap, e))?;

    // 3) Make the mount tree private, then mount a fresh (own-superblock) tmpfs for staging — mounted
    //    WITHOUT `noexec`, so the execution inode is always executable regardless of how the host
    //    mounts `/dev/shm` or `/tmp`.
    faults.check(Stage::MountPrivate)?;
    sys::make_root_private().map_err(|e| StageError::new(Stage::MountPrivate, e))?;
    faults.check(Stage::MountTmpfs)?;
    let mount_c = CString::from_vec_with_nul(STAGING_MOUNTPOINT.to_vec())
        .map_err(|_| StageError::new(Stage::MountTmpfs, libc::EINVAL))?;
    sys::mount_private_tmpfs(&mount_c).map_err(|e| StageError::new(Stage::MountTmpfs, e))?;

    // 4) Unnamed writable execution inode on the private tmpfs.
    faults.check(Stage::Tmpfile)?;
    let wfd = sys::open_tmpfile(&mount_c).map_err(|e| StageError::new(Stage::Tmpfile, e))?;

    // 5) Exact byte copy of the sealed source into the execution inode.
    faults.check(Stage::Copy)?;
    write_all_to(wfd, source).map_err(|e| close_and(wfd, StageError::new(Stage::Copy, e)))?;

    // 6) Digest re-verification — the DESTINATION inode is hashed ONCE (read back from the inode, so a
    //    truncated/corrupted write is caught) and required to equal the launch-spec target digest.
    //    That equality already implies the inode bytes match the sealed source by collision-resistance
    //    — no redundant full-file `source` re-hash or `memcmp` is done (the sealed source was hashed
    //    once by the boundary, and that digest is reused here as `target_digest`).
    faults.check(Stage::Digest).map_err(|e| close_and(wfd, e))?;
    let back =
        read_fd_contents(wfd).map_err(|e| close_and(wfd, StageError::new(Stage::Digest, e)))?;
    let digest = Digest256::of_bytes(&back);
    if digest != *target_digest {
        return Err(close_and(wfd, StageError::new(Stage::Digest, libc::EILSEQ)));
    }

    // 7) Reopen the SAME inode read-only (exec fd) and O_PATH (rule fd) via its magic symlink.
    faults.check(Stage::Reopen).map_err(|e| close_and(wfd, e))?;
    let proc = CString::new(format!("/proc/self/fd/{wfd}"))
        .map_err(|_| close_and(wfd, StageError::new(Stage::Reopen, libc::EINVAL)))?;
    let exec_fd = sys::open_path(&proc, libc::O_RDONLY)
        .map_err(|e| close_and(wfd, StageError::new(Stage::Reopen, e)))?;
    let rule_fd = match sys::open_path(&proc, libc::O_PATH) {
        Ok(f) => f,
        Err(e) => {
            let _ = crate::seccomp::close_fd(exec_fd);
            return Err(close_and(wfd, StageError::new(Stage::Reopen, e)));
        }
    };

    // 8) Close EVERY writable fd to the inode — no writer may survive into confinement (else the
    //    immutability argument is void and `execveat` would ETXTBSY).
    faults.check(Stage::CloseWriter).map_err(|e| {
        let _ = crate::seccomp::close_fd(exec_fd);
        let _ = crate::seccomp::close_fd(rule_fd);
        close_and(wfd, e)
    })?;
    if crate::seccomp::close_fd(wfd).is_err() {
        let _ = crate::seccomp::close_fd(exec_fd);
        let _ = crate::seccomp::close_fd(rule_fd);
        return Err(StageError::new(Stage::CloseWriter, last_errno()));
    }

    // 9) Non-dumpable: close the /proc/<pid>/fd reopen vector against a same-UID external process.
    faults.check(Stage::ProcIsolation).inspect_err(|_| {
        let _ = crate::seccomp::close_fd(exec_fd);
        let _ = crate::seccomp::close_fd(rule_fd);
    })?;
    if let Err(e) = sys::set_nondumpable() {
        let _ = crate::seccomp::close_fd(exec_fd);
        let _ = crate::seccomp::close_fd(rule_fd);
        return Err(StageError::new(Stage::ProcIsolation, e));
    }

    Ok(ExecObject {
        exec_fd,
        rule_fd,
        digest,
    })
}

fn close_and(fd: RawFd, e: StageError) -> StageError {
    let _ = crate::seccomp::close_fd(fd);
    e
}

fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// Write the userns id maps: `setgroups=deny` (required before a `gid_map` in an unprivileged
/// userns), then map ns-id 0 to the outer (uid, gid). Plain `/proc/self` file writes — no `unsafe`.
fn write_id_maps(uid: u32, gid: u32) -> Result<(), i32> {
    let put = |path: &str, val: String| -> Result<(), i32> {
        std::fs::write(path, val).map_err(|e| e.raw_os_error().unwrap_or(libc::EIO))
    };
    put("/proc/self/setgroups", "deny".to_owned())?;
    put("/proc/self/uid_map", format!("0 {uid} 1\n"))?;
    put("/proc/self/gid_map", format!("0 {gid} 1\n"))?;
    Ok(())
}

/// Read a descriptor's full contents by its `/proc/self/fd` magic symlink (pre-Landlock, so `/proc`
/// is reachable). Used to read the sealed source and to read the execution inode BACK for the digest
/// recheck.
fn read_fd_contents(fd: RawFd) -> Result<Vec<u8>, i32> {
    std::fs::read(format!("/proc/self/fd/{fd}")).map_err(|e| e.raw_os_error().unwrap_or(libc::EIO))
}

/// Stream `bytes` into `fd` in bounded chunks via the audited raw-write primitive.
fn write_all_to(fd: RawFd, bytes: &[u8]) -> Result<(), i32> {
    let mut off = 0;
    while off < bytes.len() {
        let end = (off + COPY_CHUNK).min(bytes.len());
        let chunk = bytes.get(off..end).ok_or(libc::EIO)?;
        match crate::seccomp::write_fd(fd, chunk) {
            Ok(0) => return Err(libc::EIO),
            Ok(n) => off += n,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
