//! VB-4 — the `unsafe` syscall boundary for the PRIVATE EXECUTION OBJECT. This is the THIRD audited
//! `unsafe` module (alongside `landlock/sys.rs` and `seccomp/sys.rs`), created explicitly rather than
//! smuggling namespace/mount/`prctl` operations into `seccomp/sys.rs` — the unsafe-containment gate
//! names all three. It owns ONLY: user+mount namespace setup, `mount`/private-propagation, an
//! `O_TMPFILE` staging-inode open, `PR_SET_DUMPABLE`, and the staging-object fd reopen. The safe
//! orchestration (id-maps, byte copy, digest recheck, fail-closed sequencing) lives in `super`.
//!
//! Every wrapper preserves `errno`, does no policy, and returns a typed `Result`. Syscall arguments
//! are plain integers or a single `&CStr` NUL-terminated path — no user structs are read by the
//! kernel through a pointer this module constructs, except the two `MS_*` mounts (all-integer flags,
//! NULL data).

#[cfg(not(target_os = "linux"))]
compile_error!("sandboy private-execution-object staging is Linux-only");

use std::ffi::CStr;
use std::io;
use std::os::fd::RawFd;

fn last_errno() -> i32 {
    io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

/// This process's real (uid, gid), captured BEFORE `unshare(CLONE_NEWUSER)` so the id-map can name
/// the outer identity (after the unshare, before a map is written, the ids read as the overflow uid).
pub(crate) fn current_ids() -> (u32, u32) {
    // SAFETY: getuid takes no arguments, reads no memory, and cannot fail.
    let uid = unsafe { libc::getuid() };
    // SAFETY: getgid takes no arguments, reads no memory, and cannot fail.
    let gid = unsafe { libc::getgid() };
    (uid, gid)
}

/// Detach into a fresh USER + MOUNT namespace (`unshare(CLONE_NEWUSER | CLONE_NEWNS)`), so the child
/// can mount a private staging filesystem no other process shares. Requires unprivileged user
/// namespaces on the host; a refusal fails closed at the caller.
pub(crate) fn unshare_user_mount() -> Result<(), i32> {
    // SAFETY: unshare(2) takes a single integer flag word and reads no user memory; returns 0 or -1.
    let r = unsafe { libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS) };
    if r == 0 {
        Ok(())
    } else {
        Err(last_errno())
    }
}

/// Make the whole mount tree in this namespace `MS_PRIVATE` recursively, so the private tmpfs mount
/// below neither propagates to nor is affected by any peer/shared mount.
pub(crate) fn make_root_private() -> Result<(), i32> {
    let root = c"/";
    // SAFETY: mount(2) with a static NUL-terminated target, NULL source/fstype/data, and an
    // all-integer flag word; reads only the target path string. Returns 0 or -1.
    let r = unsafe {
        libc::mount(
            std::ptr::null(),
            root.as_ptr(),
            std::ptr::null(),
            libc::MS_REC | libc::MS_PRIVATE,
            std::ptr::null(),
        )
    };
    if r == 0 {
        Ok(())
    } else {
        Err(last_errno())
    }
}

/// Mount a FRESH (own-superblock) `tmpfs` at `target` — the backend-private staging filesystem the
/// execution inode is created on. A fresh superblock (not a shared `/tmp`/`/dev/shm`) is what makes
/// the staging inode private to this launch.
pub(crate) fn mount_private_tmpfs(target: &CStr) -> Result<(), i32> {
    let src = c"o7staging";
    let fstype = c"tmpfs";
    // SAFETY: mount(2) with static NUL-terminated source/fstype and the caller's NUL-terminated
    // target path, no flags, NULL data; reads only those path strings. Returns 0 or -1.
    let r = unsafe {
        libc::mount(
            src.as_ptr(),
            target.as_ptr(),
            fstype.as_ptr(),
            0,
            std::ptr::null(),
        )
    };
    if r == 0 {
        Ok(())
    } else {
        Err(last_errno())
    }
}

/// Create an UNNAMED, writable regular inode on the tmpfs mounted at `dir` (`open(dir, O_TMPFILE |
/// O_RDWR | O_CLOEXEC)`). Being unnamed, it has no path in any namespace — the only reopen vector is
/// `/proc/<pid>/fd`, which [`set_nondumpable`] closes.
pub(crate) fn open_tmpfile(dir: &CStr) -> Result<RawFd, i32> {
    // SAFETY: open(2) with the caller's NUL-terminated dir path, an integer flag word, and a mode;
    // reads only the path string. Returns a new fd or -1.
    let fd = unsafe {
        libc::open(
            dir.as_ptr(),
            libc::O_TMPFILE | libc::O_RDWR | libc::O_CLOEXEC,
            0o700 as libc::c_uint,
        )
    };
    if fd < 0 {
        Err(last_errno())
    } else {
        Ok(fd)
    }
}

/// Open `path` with exactly `flags` (used to reopen the staging inode `O_RDONLY` as the exec fd and
/// `O_PATH` as the Landlock rule fd, via its `/proc/self/fd/<n>` magic symlink).
pub(crate) fn open_path(path: &CStr, flags: libc::c_int) -> Result<RawFd, i32> {
    // SAFETY: open(2) with the caller's NUL-terminated path and an integer flag word; reads only the
    // path string. Returns a new fd or -1.
    let fd = unsafe { libc::open(path.as_ptr(), flags | libc::O_CLOEXEC) };
    if fd < 0 {
        Err(last_errno())
    } else {
        Ok(fd)
    }
}

/// Mark this process NON-DUMPABLE (`prctl(PR_SET_DUMPABLE, 0)`), so `/proc/<pid>` becomes
/// root-owned and a SAME-UID external process can no longer reopen the staging descriptors through
/// `/proc/<pid>/fd/<n>` (proven by the `target_proc_isolation` oracle). Set BEFORE any staging fd is
/// created, so no reopen window ever exists; a `unshare(CLONE_NEWUSER)`/id-map transition can reset
/// dumpability, so the caller sets this AFTER that transition and then [`is_nondumpable`]-verifies it.
pub(crate) fn set_nondumpable() -> Result<(), i32> {
    // SAFETY: prctl(PR_SET_DUMPABLE, 0) takes integer arguments only and reads no user memory;
    // returns 0 or -1.
    let r = unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
    if r == 0 {
        Ok(())
    } else {
        Err(last_errno())
    }
}

/// Whether this process is CONFIRMED non-dumpable (`prctl(PR_GET_DUMPABLE) == 0`). Used to VERIFY
/// that [`set_nondumpable`] actually took effect before any writable staging inode is opened — a
/// successful `prctl` call is not trusted blindly; the state is read back.
pub(crate) fn is_nondumpable() -> bool {
    // SAFETY: prctl(PR_GET_DUMPABLE) takes integer arguments only and reads no user memory; it
    // returns the current dumpable flag (0/1/2) or -1 on error. Any non-zero (incl. -1) is treated
    // as "not confirmed non-dumpable" and fails closed at the caller.
    let r = unsafe { libc::prctl(libc::PR_GET_DUMPABLE, 0, 0, 0, 0) };
    r == 0
}
