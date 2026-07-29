//! The `unsafe` surface for VB-3 seccomp EFFECT-PROBES + the fail-closed inherited-fd scrub. The BPF
//! filter itself is built by `seccompiler` (typed Rust) in the safe `super` module; this module only
//! issues the raw probe syscalls that OBSERVE the filter's effect and enumerate/close fds. Every
//! wrapper preserves `errno` and does no policy. Syscall numbers come from `libc::SYS_*`; a non-Linux
//! target fails to compile.

#[cfg(not(target_os = "linux"))]
compile_error!("sandboy seccomp confinement is Linux-only");

use std::io;
use std::os::fd::RawFd;

/// x86_64's x32 ABI issues a syscall as `native_nr | X32_SYSCALL_BIT`. The seccomp arch gate cannot
/// distinguish x32 from x86_64 (same `AUDIT_ARCH_X86_64`), so the deny rules must also cover these
/// numbers — and this constant lets the adversarial oracle call through the x32 namespace.
#[cfg(target_arch = "x86_64")]
pub(crate) const X32_SYSCALL_BIT: i64 = 0x4000_0000;

fn last_errno() -> i32 {
    io::Error::last_os_error().raw_os_error().unwrap_or(0)
}

fn set_errno(v: i32) {
    // SAFETY: `__errno_location` returns a valid, thread-local pointer to this thread's errno.
    let loc = unsafe { libc::__errno_location() };
    // SAFETY: `loc` is that valid thread-local errno pointer; write through it.
    unsafe {
        *loc = v;
    }
}

/// Attempt `socket(domain, SOCK_STREAM, 0)`. `Ok(())` if a socket was created (then closed); `Err`
/// with the errno otherwise. `via_x32` issues the call through the x32 syscall-number namespace.
pub(crate) fn probe_socket(domain: libc::c_int, via_x32: bool) -> Result<(), i32> {
    let nr: libc::c_long = if via_x32 {
        #[cfg(target_arch = "x86_64")]
        {
            libc::SYS_socket | X32_SYSCALL_BIT
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            return Err(libc::ENOSYS);
        }
    } else {
        libc::SYS_socket
    };
    // SAFETY: socket(2) takes three plain integer arguments and reads no user memory; it returns a
    // new fd or -1 with errno set.
    let ret = unsafe { libc::syscall(nr, domain, libc::SOCK_STREAM, 0) };
    if ret < 0 {
        return Err(last_errno());
    }
    let _ = close_fd(ret as RawFd);
    Ok(())
}

/// Attempt `io_uring_setup(1, &params)` — the io_uring path to socket creation (`IORING_OP_SOCKET`)
/// starts here, so denying it (plus `enter`/`register` and the fd scrub) is what makes network
/// deny-all real. `Ok(())` if a ring was created (then closed); `Err(errno)` otherwise. `via_x32`
/// issues it through the x32 namespace.
#[cfg(feature = "test-harness")]
pub(crate) fn probe_io_uring_setup(via_x32: bool) -> Result<(), i32> {
    let nr: libc::c_long = if via_x32 {
        #[cfg(target_arch = "x86_64")]
        {
            libc::SYS_io_uring_setup | X32_SYSCALL_BIT
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            return Err(libc::ENOSYS);
        }
    } else {
        libc::SYS_io_uring_setup
    };
    // A zeroed buffer >= sizeof(struct io_uring_params) (120 bytes): flags 0, resv 0 → a basic ring.
    let mut params = [0u8; 256];
    // SAFETY: io_uring_setup(entries, params*) with a valid writable buffer the kernel reads/writes
    // in place; returns a ring fd or -1/errno.
    let ret = unsafe { libc::syscall(nr, 1_u32, params.as_mut_ptr()) };
    if ret < 0 {
        return Err(last_errno());
    }
    let _ = close_fd(ret as RawFd);
    Ok(())
}

/// Attempt `setsid()`. `Ok(())` on success (a new session — a real side effect, so run in a
/// disposable child), `Err(errno)` otherwise.
#[cfg(feature = "test-harness")]
pub(crate) fn probe_setsid() -> Result<(), i32> {
    // SAFETY: setsid(2) takes no arguments and returns the new session id or -1/errno.
    let ret = unsafe { libc::setsid() };
    if ret < 0 {
        Err(last_errno())
    } else {
        Ok(())
    }
}

/// Attempt `setpgid(0, 0)`. `Ok(())`/`Err(errno)`. A side effect (pgid change) — run in a child.
#[cfg(feature = "test-harness")]
pub(crate) fn probe_setpgid() -> Result<(), i32> {
    // SAFETY: setpgid(2) with two plain integer arguments; returns 0 or -1/errno.
    let ret = unsafe { libc::setpgid(0, 0) };
    if ret == 0 {
        Ok(())
    } else {
        Err(last_errno())
    }
}

/// Fork, run `f` in the child and `_exit` with its return code; return the child's exit code in the
/// parent (or a negative sentinel on fork/wait failure). Used to probe side-effecting syscalls in a
/// disposable process and to prove the seccomp filter is inherited across fork.
#[cfg(feature = "test-harness")]
pub(crate) fn run_in_child<F: FnOnce() -> i32>(f: F) -> i32 {
    // SAFETY: fork(2) in this single-threaded harness; the parent gets the child pid, the child 0.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        let code = f();
        // SAFETY: _exit terminates the child immediately (no atexit/destructors, no stdio flush).
        unsafe { libc::_exit(code) };
    }
    if pid < 0 {
        return -1;
    }
    let mut status: libc::c_int = 0;
    // SAFETY: waitpid on our own child with a valid status out-pointer.
    let w = unsafe { libc::waitpid(pid, &mut status, 0) };
    if w < 0 {
        return -1;
    }
    if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else {
        -2
    }
}

/// Terminate the current process IMMEDIATELY (`_exit`), without running atexit handlers, destructors,
/// or stdio flushes — the correct primitive for a forked launch child that must not run the parent's
/// cleanup after a failure.
pub(crate) fn exit_now(code: i32) -> ! {
    // SAFETY: `_exit(2)` never returns and touches no user memory.
    unsafe { libc::_exit(code) }
}

/// The result of [`fork_raw`].
pub(crate) enum ForkResult {
    Parent(libc::pid_t),
    Child,
}

/// `fork(2)` returning which side we are on (or an errno). The VB-4 launch monitor forks exactly ONE
/// child, supervises it by pid, and enforces an absolute deadline — so it cannot use the
/// waitpid-inline [`run_in_child`] helper.
pub(crate) fn fork_raw() -> Result<ForkResult, i32> {
    // SAFETY: fork(2) in a single-threaded process; returns the child pid to the parent, 0 to the
    // child, or -1/errno.
    let pid = unsafe { libc::fork() };
    match pid {
        -1 => Err(last_errno()),
        0 => Ok(ForkResult::Child),
        n => Ok(ForkResult::Parent(n)),
    }
}

/// Create a `pipe2(O_CLOEXEC)` and return its raw (read, write) fds. CLOEXEC so neither end leaks
/// across an unrelated exec; the VB-4 launch code closes each end explicitly.
pub(crate) fn make_pipe_cloexec() -> Result<(RawFd, RawFd), i32> {
    let mut fds = [0 as libc::c_int; 2];
    // SAFETY: pipe2 with a valid 2-element out array and O_CLOEXEC; returns 0 or -1/errno.
    let r = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
    if r != 0 {
        return Err(last_errno());
    }
    Ok((fds[0], fds[1]))
}

/// `read(2)` into `buf`; `Ok(0)` is EOF. Raw so the launch code needs no fd→File `unsafe` of its own.
pub(crate) fn read_fd(fd: RawFd, buf: &mut [u8]) -> Result<usize, i32> {
    // SAFETY: read into a valid buffer of `buf.len()` bytes on an open fd; returns count or -1/errno.
    let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast::<libc::c_void>(), buf.len()) };
    if n < 0 {
        Err(last_errno())
    } else {
        Ok(n as usize)
    }
}

/// `write(2)` from `buf`.
pub(crate) fn write_fd(fd: RawFd, buf: &[u8]) -> Result<usize, i32> {
    // SAFETY: write from a valid buffer of `buf.len()` bytes on an open fd; returns count or -1/errno.
    let n = unsafe { libc::write(fd, buf.as_ptr().cast::<libc::c_void>(), buf.len()) };
    if n < 0 {
        Err(last_errno())
    } else {
        Ok(n as usize)
    }
}

/// Borrow an existing fd as a `File` WITHOUT owning it — its `Drop` never closes the fd (wrapped in
/// `ManuallyDrop`). Used to hand the inherited sealed-target fd to Landlock's install (which fstats +
/// reads its seals) while the launch child keeps the fd for its `execveat`.
pub(crate) fn borrow_fd_as_file(fd: RawFd) -> std::mem::ManuallyDrop<std::fs::File> {
    use std::os::fd::FromRawFd as _;
    // SAFETY: `fd` is an open fd owned elsewhere; ManuallyDrop ensures this `File` never closes it.
    std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(fd) })
}

/// `dup2(old, new)` — used by the launch child to place `/dev/null` on the target's stdin.
pub(crate) fn dup2_fd(old: RawFd, new: RawFd) -> Result<(), i32> {
    // SAFETY: dup2 with two integer fds; returns the new fd or -1/errno.
    let r = unsafe { libc::dup2(old, new) };
    if r < 0 {
        Err(last_errno())
    } else {
        Ok(())
    }
}

/// Non-blocking reap: `Some(exit_status)` if the child changed state, `None` if still running.
pub(crate) fn try_wait(pid: libc::pid_t) -> Result<Option<i32>, i32> {
    let mut status: libc::c_int = 0;
    // SAFETY: waitpid with WNOHANG on our child and a valid status out-pointer; 0 = still running.
    let r = unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) };
    if r < 0 {
        Err(last_errno())
    } else if r == 0 {
        Ok(None)
    } else {
        Ok(Some(exit_status(status)))
    }
}

/// Encode a `waitpid` status as an exit-ish code: the exit status, or `128 + signal` if killed.
fn exit_status(status: libc::c_int) -> i32 {
    if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else if libc::WIFSIGNALED(status) {
        128 + libc::WTERMSIG(status)
    } else {
        -1
    }
}

/// `prctl(PR_SET_NO_NEW_PRIVS, 1)` — required before a seccomp filter can be installed without
/// `CAP_SYS_ADMIN`.
pub(crate) fn set_no_new_privs() -> Result<(), i32> {
    // SAFETY: PR_SET_NO_NEW_PRIVS sets a boolean thread flag; trailing args are ignored, no memory
    // is read. Returns 0 or -1/errno.
    let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if ret == 0 {
        Ok(())
    } else {
        Err(last_errno())
    }
}

/// Whether `fd` is currently open (`fcntl(fd, F_GETFD) != -1`).
#[cfg(feature = "test-harness")]
pub(crate) fn fd_is_open(fd: RawFd) -> bool {
    // SAFETY: F_GETFD reads the fd flags of `fd`; it takes no pointer args and returns -1/EBADF for a
    // closed fd.
    unsafe { libc::fcntl(fd, libc::F_GETFD) != -1 }
}

/// Whether `fd` refers to a socket (`fstat` → `S_ISSOCK`). Used by a child to prove it inherited no
/// socket descriptors.
pub(crate) fn fd_is_socket(fd: RawFd) -> bool {
    // SAFETY: zero-initialize a `stat` to be filled by `fstat`.
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: fstat on `fd` with a valid `stat` out-pointer; returns 0 or -1.
    let r = unsafe { libc::fstat(fd, &mut st) };
    r == 0 && (st.st_mode & libc::S_IFMT) == libc::S_IFSOCK
}

/// Close `fd`; returns `Err(errno)` on failure (the caller decides how to treat it).
pub(crate) fn close_fd(fd: RawFd) -> Result<(), i32> {
    // SAFETY: close(2) on an integer fd.
    let r = unsafe { libc::close(fd) };
    if r == 0 {
        Ok(())
    } else {
        Err(last_errno())
    }
}

/// Enumerate every open fd of this process from `/proc/self/fd`, EXCLUDING the directory stream's own
/// fd. FAIL-CLOSED: a directory-open failure, a readdir error, or a non-numeric entry (other than
/// `.`/`..`) is an `Err(errno)`, never a silently-dropped entry.
pub(crate) fn list_fds() -> Result<Vec<RawFd>, i32> {
    // SAFETY: opendir on a valid, static NUL-terminated path; returns a DIR* or NULL/errno.
    let dir = unsafe { libc::opendir(c"/proc/self/fd".as_ptr()) };
    if dir.is_null() {
        return Err(last_errno());
    }
    // SAFETY: dirfd on our open DIR*.
    let dfd = unsafe { libc::dirfd(dir) };
    let mut fds = Vec::new();
    let mut result: Result<(), i32> = Ok(());
    loop {
        set_errno(0);
        // SAFETY: readdir on our open DIR*; returns a valid dirent pointer or NULL (end or error).
        let ent = unsafe { libc::readdir(dir) };
        if ent.is_null() {
            let e = last_errno();
            if e != 0 {
                result = Err(e);
            }
            break;
        }
        // SAFETY: `ent` is a valid dirent pointer until the next readdir/closedir.
        let d_name = unsafe { (*ent).d_name };
        // SAFETY: `d_name` is a NUL-terminated C string field within the dirent.
        let name = unsafe { std::ffi::CStr::from_ptr(d_name.as_ptr()) };
        let bytes = name.to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        match std::str::from_utf8(bytes)
            .ok()
            .and_then(|s| s.parse::<RawFd>().ok())
        {
            Some(n) if n != dfd => fds.push(n),
            Some(_) => {} // the directory stream's own fd
            None => {
                result = Err(libc::EINVAL); // a non-numeric /proc/self/fd entry — fail closed
                break;
            }
        }
    }
    // SAFETY: close the DIR* (also closes `dfd`).
    unsafe {
        libc::closedir(dir);
    }
    result.map(|()| fds)
}
