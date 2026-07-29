//! The `unsafe` surface for VB-3 seccomp EFFECT-PROBES + the inherited-fd scrub. The BPF filter
//! itself is built by `seccompiler` (typed Rust) in the safe `super` module and installed via its
//! `apply_filter`; this module only issues the raw probe syscalls that OBSERVE the filter's effect
//! (and closes leaked fds). Every wrapper preserves `errno` and does no policy. Syscall numbers come
//! from `libc::SYS_*`; a non-Linux target fails to compile.

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

/// Attempt `socket(domain, SOCK_STREAM, 0)`. `Ok(())` if a socket was created (then closed); `Err`
/// with the errno otherwise. `via_x32` issues the call through the x32 syscall-number namespace so an
/// oracle can prove the deny rules are not bypassable there.
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
    // SAFETY: `ret` is a fresh fd we own; close it (the probe only needed the create to succeed).
    unsafe {
        libc::close(ret as RawFd);
    }
    Ok(())
}

/// Attempt `setsid()`. `Ok(())` on success (a new session — a real side effect, so run this in a
/// disposable child), `Err(errno)` otherwise.
pub(crate) fn probe_setsid() -> Result<(), i32> {
    // SAFETY: setsid(2) takes no arguments and returns the new session id or -1/errno.
    let ret = unsafe { libc::setsid() };
    if ret < 0 {
        Err(last_errno())
    } else {
        Ok(())
    }
}

/// Attempt `setpgid(0, 0)` (put the caller in its own process group). `Ok(())`/`Err(errno)`.
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
/// parent (or a negative sentinel on fork/wait failure). Used to probe side-effecting syscalls
/// (setsid/setpgid) in a disposable process, and to prove the seccomp filter is inherited across fork.
pub(crate) fn run_in_child<F: FnOnce() -> i32>(f: F) -> i32 {
    // SAFETY: fork(2) in this single-threaded harness; the parent gets the child pid, the child 0.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        let code = f();
        // SAFETY: _exit terminates the child immediately (no atexit/destructors, no stdio flush) —
        // the correct primitive after a fork in a would-be async-signal context.
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

/// Whether `fd` is currently open in this process (`fcntl(fd, F_GETFD) != -1`).
pub(crate) fn fd_is_open(fd: RawFd) -> bool {
    // SAFETY: F_GETFD reads the fd flags of `fd`; it takes no pointer args and returns -1/EBADF for a
    // closed fd.
    unsafe { libc::fcntl(fd, libc::F_GETFD) != -1 }
}

/// Close `fd`, ignoring the result (used by the scrub to drop a leaked inherited fd).
pub(crate) fn close_fd(fd: RawFd) {
    // SAFETY: close(2) on an integer fd; a bad fd merely returns EBADF, which we ignore.
    unsafe {
        libc::close(fd);
    }
}
