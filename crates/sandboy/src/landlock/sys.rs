//! The ENTIRE `unsafe` surface of sandboy: four thin, individually-audited wrappers over the three
//! Landlock syscalls plus `prctl(PR_SET_NO_NEW_PRIVS)`. Nothing else in the crate contains `unsafe`
//! (the gate greps for that). Every wrapper preserves `errno` immediately (via `io::Error`), owns
//! exactly one documented `unsafe` block, and does NO policy logic — the safe `super` module builds
//! rulesets and decides enforcement.
//!
//! Kernel UAPI structs are declared here as `#[repr(C)]` with explicit widths and size/alignment
//! assertions, so a layout drift is a COMPILE error, not a silent mis-sized syscall. Syscall numbers
//! come from `libc::SYS_*` (never hard-coded); a non-Linux target fails to compile.

#[cfg(not(target_os = "linux"))]
compile_error!("sandboy Landlock confinement is Linux-only (no SYS_landlock_* on this target)");

use std::io;
use std::os::fd::{AsRawFd as _, FromRawFd as _, OwnedFd, RawFd};

/// Landlock filesystem access rights (`linux/landlock.h`), through ABI 3. The frozen filesystem
/// oracle needs `TRUNCATE` (ABI 3), so this is the exact minimum handled set — no ABI-4+ (net) or
/// ABI-5+ (ioctl-dev) bits, which would demand a higher kernel for no oracle benefit.
pub(crate) mod access {
    pub(crate) const EXECUTE: u64 = 1 << 0;
    pub(crate) const WRITE_FILE: u64 = 1 << 1;
    pub(crate) const READ_FILE: u64 = 1 << 2;
    pub(crate) const READ_DIR: u64 = 1 << 3;
    pub(crate) const REMOVE_DIR: u64 = 1 << 4;
    pub(crate) const REMOVE_FILE: u64 = 1 << 5;
    pub(crate) const MAKE_CHAR: u64 = 1 << 6;
    pub(crate) const MAKE_DIR: u64 = 1 << 7;
    pub(crate) const MAKE_REG: u64 = 1 << 8;
    pub(crate) const MAKE_SOCK: u64 = 1 << 9;
    pub(crate) const MAKE_FIFO: u64 = 1 << 10;
    pub(crate) const MAKE_BLOCK: u64 = 1 << 11;
    pub(crate) const MAKE_SYM: u64 = 1 << 12;
    pub(crate) const REFER: u64 = 1 << 13; // ABI 2
    pub(crate) const TRUNCATE: u64 = 1 << 14; // ABI 3
}

/// Every filesystem right through ABI 3, OR-ed from the named bits above. Handling the COMPLETE set
/// is what makes a path NOT covered by a rule fully denied; a partial handled-set would leak the
/// unhandled rights. (Equals `(1 << 15) - 1`, but spelled out so each right is explicit and used.)
pub(crate) const HANDLED_FS_ABI3: u64 = access::EXECUTE
    | access::WRITE_FILE
    | access::READ_FILE
    | access::READ_DIR
    | access::REMOVE_DIR
    | access::REMOVE_FILE
    | access::MAKE_CHAR
    | access::MAKE_DIR
    | access::MAKE_REG
    | access::MAKE_SOCK
    | access::MAKE_FIFO
    | access::MAKE_BLOCK
    | access::MAKE_SYM
    | access::REFER
    | access::TRUNCATE;

/// Rights that apply to a REGULAR FILE. Directory-only rights (READ_DIR, the MAKE_*/REMOVE_* set,
/// REFER) on a file rule make `landlock_add_rule` fail with EINVAL, so a file rule's mask must be
/// intersected with this set. (Directories may hold every handled right.)
pub(crate) const FILE_APPLICABLE: u64 =
    access::EXECUTE | access::WRITE_FILE | access::READ_FILE | access::TRUNCATE;

/// The worktree is the single WRITABLE root — read/write/create/remove/truncate — but NOT an
/// executable root. It gets every handled FS right EXCEPT `EXECUTE`, so an executable created or
/// copied inside the writable worktree is NOT kernel-executable unless the exact path is ALSO named
/// by `allow_exec`. (`EXECUTE` stays in the ruleset's HANDLED set, so it is denied where not granted.)
pub(crate) const WORKTREE_RIGHTS: u64 = HANDLED_FS_ABI3 & !access::EXECUTE;

/// The minimum Landlock ABI VB-2 requires: 3, for `LANDLOCK_ACCESS_FS_TRUNCATE`.
pub(crate) const MIN_ABI: i32 = 3;

/// `LANDLOCK_CREATE_RULESET_VERSION` — probe the supported ABI instead of creating a ruleset.
const CREATE_RULESET_VERSION: libc::c_uint = 1 << 0;
/// `LANDLOCK_RULE_PATH_BENEATH`.
const RULE_PATH_BENEATH: libc::c_int = 1;

/// `struct landlock_ruleset_attr` — ABI-1 / FS-only layout (`__u64 handled_access_fs;`). Passing this
/// 8-byte size to `create_ruleset` yields an FS-only ruleset on every kernel ≥ 5.13.
#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
}
const _: () = assert!(core::mem::size_of::<RulesetAttr>() == 8);
const _: () = assert!(core::mem::align_of::<RulesetAttr>() == 8);

/// `struct landlock_path_beneath_attr { __u64 allowed_access; __s32 parent_fd; } __packed;` — the
/// kernel struct is `__attribute__((packed))`, so it is exactly 12 bytes (NOT 16). `#[repr(C, packed)]`
/// + the assertions below pin that; a mismatch would mis-align `parent_fd` and pass a garbage fd.
#[repr(C, packed)]
struct PathBeneathAttr {
    allowed_access: u64,
    parent_fd: i32,
}
const _: () = assert!(core::mem::size_of::<PathBeneathAttr>() == 12);
const _: () = assert!(core::mem::align_of::<PathBeneathAttr>() == 1);

/// The supported Landlock ABI version (`≥ 1`), or an `io::Error` carrying `errno` — `ENOSYS` means
/// the kernel has no Landlock, `EOPNOTSUPP` means it is disabled.
pub(crate) fn abi_version() -> io::Result<i32> {
    // SAFETY: the documented ABI probe — a NULL attr, 0 size, and the VERSION flag. The kernel reads
    // no user memory in this mode and returns the version (≥1) or -1 with errno set.
    let ret = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            core::ptr::null::<RulesetAttr>(),
            0_usize,
            CREATE_RULESET_VERSION,
        )
    };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(ret as i32)
    }
}

/// Create an FS-only ruleset handling `handled_access_fs`; returns an owned fd (RAII-closed).
pub(crate) fn create_ruleset(handled_access_fs: u64) -> io::Result<OwnedFd> {
    let attr = RulesetAttr { handled_access_fs };
    // SAFETY: `&attr` is a live `RulesetAttr`; we pass exactly its 8-byte size and flags 0, so the
    // kernel reads one valid attr and creates a ruleset, returning a fresh fd or -1/errno.
    let ret = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &attr as *const RulesetAttr,
            core::mem::size_of::<RulesetAttr>(),
            0_u32,
        )
    };
    if ret < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `ret` is a fresh ruleset fd the kernel just handed us and nobody else owns; wrapping it
    // in `OwnedFd` gives it a single owner that closes it on drop.
    Ok(unsafe { OwnedFd::from_raw_fd(ret as RawFd) })
}

/// Add one `path_beneath` rule granting `allowed_access` under the (already-open) `parent_fd`.
pub(crate) fn add_path_beneath_rule(
    ruleset: &OwnedFd,
    allowed_access: u64,
    parent_fd: RawFd,
) -> io::Result<()> {
    let attr = PathBeneathAttr {
        allowed_access,
        parent_fd,
    };
    // SAFETY: `&attr` is a live `PathBeneathAttr` whose layout matches the packed kernel struct the
    // PATH_BENEATH rule type expects; `parent_fd` is an open fd owned by the caller for the duration
    // of this call; flags are 0. The kernel copies the attr and returns 0 or -1/errno.
    let ret = unsafe {
        libc::syscall(
            libc::SYS_landlock_add_rule,
            ruleset.as_raw_fd(),
            RULE_PATH_BENEATH,
            &attr as *const PathBeneathAttr,
            0_u32,
        )
    };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Irrevocably apply `ruleset` to the calling thread (and everything it later spawns). Requires
/// `no_new_privs` (see [`set_no_new_privs`]) unless the caller holds `CAP_SYS_ADMIN`.
pub(crate) fn restrict_self(ruleset: &OwnedFd) -> io::Result<()> {
    // SAFETY: a plain application of an owned ruleset fd to the current thread with flags 0; the
    // kernel reads no user memory and returns 0 or -1/errno.
    let ret =
        unsafe { libc::syscall(libc::SYS_landlock_restrict_self, ruleset.as_raw_fd(), 0_u32) };
    if ret < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// `execveat(fd, "", argv, envp, AT_EMPTY_PATH)` — replace THIS process with the program at the
/// already-open `fd` (a sealed memfd), using a caller-built argv/envp. Returns only on failure (the
/// `io::Error`); on success the process is replaced. `argv`/`envp` must be NULL-terminated arrays of
/// valid C-string pointers that outlive the call. The VB-4 launch child calls this after it has
/// installed + self-checked the full confinement, so the target inherits it.
pub(crate) fn execveat_replace(
    fd: RawFd,
    argv: &[*const libc::c_char],
    envp: &[*const libc::c_char],
) -> io::Error {
    // SAFETY: `fd` is an open, owned fd; `argv`/`envp` are NULL-terminated pointer arrays that live
    // across the call; `""` + AT_EMPTY_PATH names the fd itself. execveat replaces the process or
    // returns -1 with errno set.
    unsafe {
        libc::syscall(
            libc::SYS_execveat,
            fd,
            c"".as_ptr(),
            argv.as_ptr(),
            envp.as_ptr(),
            libc::AT_EMPTY_PATH,
        );
    }
    io::Error::last_os_error()
}

/// Spawn a child that `execveat`s the already-open `fd` with `AT_EMPTY_PATH` (i.e. `fexecve`), passing
/// `arg` as argv[1]. Executing via an fd — opened BEFORE `restrict_self` — is how a sealed memfd (an
/// anonymous inode with no filesystem path, so un-nameable by a path rule) runs under Landlock, while
/// a real file's execute right is still enforced by the kernel at `execveat`. Returns the child on a
/// successful hand-off; a Landlock/exec denial surfaces as the spawn `io::Error` (EACCES/EPERM).
#[cfg(feature = "test-harness")]
pub(crate) fn spawn_via_execveat(
    fd: RawFd,
    arg: &std::ffi::OsStr,
) -> io::Result<std::process::Child> {
    use std::os::unix::ffi::OsStrExt as _;
    use std::os::unix::process::CommandExt as _;

    let bad = || io::Error::from(io::ErrorKind::InvalidInput);
    let argv0 = std::ffi::CString::new("o7-sealed-exec").map_err(|_| bad())?;
    let arg1 = std::ffi::CString::new(arg.as_bytes()).map_err(|_| bad())?;
    let empty = std::ffi::CString::new("").map_err(|_| bad())?;

    // Runs in the forked child before any exec: build argv/envp as STACK arrays of pointers into the
    // moved-in CStrings (no heap allocation → async-signal-safe in the single-threaded child) and
    // execveat the fd, which either replaces the process (never returning) or returns -1/errno.
    let hook = move || -> io::Result<()> {
        let argv = [argv0.as_ptr(), arg1.as_ptr(), core::ptr::null()];
        let envp = [core::ptr::null::<libc::c_char>()];
        // SAFETY: `fd` is an inherited, open fd; `empty` is a valid "" path; argv/envp are valid
        // NULL-terminated pointer arrays that live across the call.
        unsafe {
            libc::syscall(
                libc::SYS_execveat,
                fd,
                empty.as_ptr(),
                argv.as_ptr(),
                envp.as_ptr(),
                libc::AT_EMPTY_PATH,
            );
        }
        Err(io::Error::last_os_error())
    };

    // The placeholder program is NEVER exec'd: `hook` either replaces the process or errors first.
    let mut cmd = std::process::Command::new("/nonexistent-o7-execveat-placeholder");
    // SAFETY: registering a pre_exec hook; the hook above is async-signal-safe as documented.
    unsafe {
        cmd.pre_exec(hook);
    }
    cmd.spawn()
}

/// `prctl(PR_SET_NO_NEW_PRIVS, 1)` — a prerequisite for `restrict_self` without `CAP_SYS_ADMIN`.
pub(crate) fn set_no_new_privs() -> io::Result<()> {
    // SAFETY: `PR_SET_NO_NEW_PRIVS` sets a boolean thread flag; the trailing args are ignored and no
    // user memory is read. Returns 0 on success, -1/errno otherwise.
    let ret = unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) };
    if ret != 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
