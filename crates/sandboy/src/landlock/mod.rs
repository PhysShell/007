//! VB-2 — real Landlock **filesystem** confinement + a DIFFERENTIAL effect-based self-check, SAFE on
//! top of the single audited [`sys`] syscall module. Scope, strictly: confine writes to the worktree
//! and read+execute to `allow_exec`; PROVE it against an unconfined baseline; report the `filesystem`
//! dimension honestly. **No seccomp, network, or env (VB-3); no `confinement_backend()` switch; no
//! RED-matrix flip (VB-4).**
//!
//! Enforcement is NOT "the kernel handed back a ruleset fd". It is the full ordered sequence —
//! `create_ruleset` → `add_rule`* → `prctl(PR_SET_NO_NEW_PRIVS)` → `restrict_self` — AND a
//! DIFFERENTIAL self-check: the exact outside-worktree write is proven to SUCCEED *before*
//! `restrict_self` (ruling out a DAC / other-LSM denial) and then observed DENIED (EACCES/EPERM)
//! *after* it, while an inside-worktree write stays allowed. A baseline that cannot even succeed
//! unconfined is a distinct `not_enforced` verdict, never a false "confined". Anything short returns
//! a typed [`InstallError`] and the caller MUST NOT launch.
//!
//! `allow_exec` rules are OBJECT-TYPE-CORRECT: a directory may hold `EXECUTE|READ_FILE|READ_DIR`, a
//! regular file only the file-applicable subset (`READ_DIR` on a file is EINVAL); other object types
//! are rejected. The ABI is probed first and the exact minimum (3, for `TRUNCATE`) required.
//!
//! Compiled ONLY under `test-harness` (like the VB-1 cgroup monitor): production `run` compiles
//! NEITHER this module NOR `libc` NOR any `unsafe`. A TEST-ONLY `O7_LL_FAULT` knob forces one install
//! stage to fail so every fail-closed path — including the self-check verdicts — is provable.

mod sys;

// The `execveat` primitive the VB-4 launch child uses to become the sealed target (its `unsafe` lives
// in `sys`). Re-exported at `crate::landlock::execveat_replace`.
pub(crate) use sys::execveat_replace;

use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::os::fd::{AsRawFd as _, RawFd};
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sys::access;

/// EACCES / EPERM — the two errnos Landlock denies with, per the frozen oracle (`ERR:13`/`ERR:1`).
fn is_denied(err: &io::Error) -> bool {
    matches!(err.raw_os_error(), Some(libc::EACCES) | Some(libc::EPERM))
}

#[cfg(feature = "test-harness")]
fn errno_of(err: &io::Error) -> i32 {
    err.raw_os_error().unwrap_or(0)
}

/// Why the Landlock filesystem policy is not a proven, fully-enforced boundary. Each variant is a
/// distinct stage with a distinct exit code, so a test (and an operator) can tell WHICH gate refused
/// — and none of them ever downgrades into a launch.
#[derive(Debug)]
pub(crate) enum InstallError {
    /// Kernel has no Landlock (ENOSYS) or it is disabled (EOPNOTSUPP).
    Unsupported(io::Error),
    /// The kernel's Landlock ABI is below the minimum the oracle needs (TRUNCATE = ABI 3).
    AbiTooLow {
        have: i32,
        need: i32,
    },
    CreateRuleset(io::Error),
    OpenParent {
        path: PathBuf,
        err: io::Error,
    },
    /// An `allow_exec` path is neither a directory nor a regular file (no correct rule mask exists),
    /// OR the OPENED object lives on `procfs` (`fstatfs` → PROC_SUPER_MAGIC, incl. a symlink/bind alias
    /// to `/proc/<pid>/fd`) — never a legitimate subprocess exec allowance (a sealed launch SOURCE is
    /// authorized separately and runs via its private execution inode, never by a `/proc/fd` grant).
    UnsupportedObjectType {
        path: PathBuf,
    },
    AddRule {
        path: PathBuf,
        err: io::Error,
    },
    NoNewPrivs(io::Error),
    RestrictSelf(io::Error),
    /// The unconfined baseline inside-worktree write failed — cannot even establish a baseline.
    SelfCheckBaselineInsideDenied(io::Error),
    /// The unconfined baseline outside-worktree write failed (DAC / read-only / other LSM), so a
    /// later denial could NOT be attributed to Landlock — report not_enforced, never "confined".
    SelfCheckBaselineOutsideDenied(io::Error),
    /// After `restrict_self`, the outside-worktree write SUCCEEDED — the ruleset is not confining.
    SelfCheckOutsideAllowed,
    /// After `restrict_self`, the outside write failed but NOT with a Landlock deny — unproven.
    SelfCheckOutsideNotDenied(io::Error),
    /// After `restrict_self`, an inside-worktree write was denied — the ruleset is misbuilt.
    SelfCheckInsideDenied(io::Error),
}

impl InstallError {
    #[cfg(feature = "test-harness")]
    pub(crate) fn stage(&self) -> &'static str {
        match self {
            InstallError::Unsupported(_) => "unsupported",
            InstallError::AbiTooLow { .. } => "abi_too_low",
            InstallError::CreateRuleset(_) => "create_ruleset",
            InstallError::OpenParent { .. } => "open_parent",
            InstallError::UnsupportedObjectType { .. } => "unsupported_object_type",
            InstallError::AddRule { .. } => "add_rule",
            InstallError::NoNewPrivs(_) => "no_new_privs",
            InstallError::RestrictSelf(_) => "restrict_self",
            InstallError::SelfCheckBaselineInsideDenied(_) => "baseline_inside",
            InstallError::SelfCheckBaselineOutsideDenied(_) => "baseline_outside",
            InstallError::SelfCheckOutsideAllowed => "self_check_outside",
            InstallError::SelfCheckOutsideNotDenied(_) => "self_check_outside_inconclusive",
            InstallError::SelfCheckInsideDenied(_) => "self_check_inside",
        }
    }
    #[cfg(feature = "test-harness")]
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            InstallError::CreateRuleset(_) => 80,
            InstallError::Unsupported(_) => 81,
            InstallError::AbiTooLow { .. } => 82,
            InstallError::OpenParent { .. } => 83,
            InstallError::AddRule { .. } => 84,
            InstallError::NoNewPrivs(_) => 85,
            InstallError::RestrictSelf(_) => 86,
            InstallError::SelfCheckOutsideAllowed => 87,
            InstallError::SelfCheckInsideDenied(_) => 88,
            InstallError::SelfCheckOutsideNotDenied(_) => 89,
            InstallError::SelfCheckBaselineInsideDenied(_) => 90,
            InstallError::SelfCheckBaselineOutsideDenied(_) => 91,
            InstallError::UnsupportedObjectType { .. } => 92,
        }
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Unsupported(e) => write!(f, "Landlock unsupported/disabled: {e}"),
            InstallError::AbiTooLow { have, need } => {
                write!(f, "Landlock ABI {have} < required {need} (TRUNCATE)")
            }
            InstallError::CreateRuleset(e) => write!(f, "create_ruleset: {e}"),
            InstallError::OpenParent { path, err } => write!(f, "open {path:?}: {err}"),
            InstallError::UnsupportedObjectType { path } => {
                write!(
                    f,
                    "allow_exec {path:?} is neither a directory nor a regular file"
                )
            }
            InstallError::AddRule { path, err } => write!(f, "add_rule {path:?}: {err}"),
            InstallError::NoNewPrivs(e) => write!(f, "prctl(NO_NEW_PRIVS): {e}"),
            InstallError::RestrictSelf(e) => write!(f, "restrict_self: {e}"),
            InstallError::SelfCheckBaselineInsideDenied(e) => {
                write!(
                    f,
                    "self-check baseline: inside-worktree write failed unconfined: {e}"
                )
            }
            InstallError::SelfCheckBaselineOutsideDenied(e) => {
                write!(
                    f,
                    "self-check baseline: outside write failed unconfined (DAC/other LSM): {e}"
                )
            }
            InstallError::SelfCheckOutsideAllowed => {
                write!(
                    f,
                    "self-check: an outside-worktree write SUCCEEDED post-restrict (not confined)"
                )
            }
            InstallError::SelfCheckOutsideNotDenied(e) => {
                write!(
                    f,
                    "self-check: outside write failed post-restrict but not via a deny: {e}"
                )
            }
            InstallError::SelfCheckInsideDenied(e) => {
                write!(
                    f,
                    "self-check: an inside-worktree write was denied post-restrict: {e}"
                )
            }
        }
    }
}

/// TEST-ONLY forced-fault knob (`O7_LL_FAULT`), letting the confinement tests force a specific
/// install stage — including the hard-to-stage self-check verdicts — to fail without a broken kernel.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Faults {
    pub(crate) abi_enosys: bool,
    pub(crate) abi_eopnotsupp: bool,
    pub(crate) abi_low: bool,
    pub(crate) create: bool,
    pub(crate) add_rule: bool,
    pub(crate) add_rule_partial: bool,
    pub(crate) omit_worktree_rule: bool,
    pub(crate) no_new_privs: bool,
    pub(crate) restrict_self: bool,
    pub(crate) selfcheck_outside_notdenied: bool,
    /// VB-4: force the private execution-object rule attach to fail.
    pub(crate) exec_object_rule: bool,
    /// VB-4: force a runtime read-object open (interpreter / library root) to fail.
    pub(crate) runtime_open: bool,
    /// VB-4: force a runtime read-object rule attach to fail.
    pub(crate) runtime_rule: bool,
}

impl Faults {
    #[cfg(feature = "test-harness")]
    fn from_env() -> Faults {
        let mut f = Faults::default();
        match std::env::var("O7_LL_FAULT").ok().as_deref() {
            Some("abi_enosys") => f.abi_enosys = true,
            Some("abi_eopnotsupp") => f.abi_eopnotsupp = true,
            Some("abi_low") => f.abi_low = true,
            Some("create") => f.create = true,
            Some("add_rule") => f.add_rule = true,
            Some("add_rule_partial") => f.add_rule_partial = true,
            Some("omit_worktree_rule") => f.omit_worktree_rule = true,
            Some("no_new_privs") => f.no_new_privs = true,
            Some("restrict_self") => f.restrict_self = true,
            Some("selfcheck_outside_notdenied") => f.selfcheck_outside_notdenied = true,
            _ => {}
        }
        f
    }
}

/// A rule object opened EXACTLY ONCE via `O_PATH|O_CLOEXEC`: the fd (closed on drop), its path (for
/// diagnostics), its type, and its stable identity (`dev`,`ino`) read from that SAME fd. Every later
/// decision — overlap detection, type masking, rule attachment — uses THIS object, never a
/// re-resolved pathname, so a concurrent rename/symlink swap cannot redirect a rule to another object.
struct RuleObject {
    path: PathBuf,
    file: File,
    file_type: std::fs::FileType,
    id: (u64, u64),
}

/// Open `path` once (O_PATH) and capture its type + identity from that fd. A failure to open or prove
/// identity fails closed (no lexical guessing).
fn open_object(path: &Path) -> Result<RuleObject, InstallError> {
    use std::os::unix::fs::MetadataExt as _;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_CLOEXEC)
        .open(path)
        .map_err(|err| InstallError::OpenParent {
            path: path.to_path_buf(),
            err,
        })?;
    let md = file.metadata().map_err(|err| InstallError::OpenParent {
        path: path.to_path_buf(),
        err,
    })?;
    Ok(RuleObject {
        path: path.to_path_buf(),
        file_type: md.file_type(),
        id: (md.dev(), md.ino()),
        file,
    })
}

/// What a proven `install_filesystem` returns to the launch child: the runtime read-policy binding,
/// computed over the profile version plus the EXACT opened [`RuleObject`] identities the Landlock
/// rules were attached to. Because it is derived from the SAME fds (never a re-resolved pathname),
/// `READY_TO_EXEC` commits to the objects that were actually ruled — closing the window where a path
/// swap between rule-attach and proof could bind object B while the rule protects object A.
pub(crate) struct FilesystemInstallProof {
    pub(crate) runtime_binding: o7_sandbox_protocol::ids::Digest256,
}

/// The runtime-binding digest over the profile version + every runtime RuleObject's path AND its
/// opened-object identity `(dev, ino)`. Same tagged format the old `RuntimePolicy::binding_digest`
/// produced, but sourced from the ruled fds rather than a fresh `metadata(path)`.
fn runtime_binding_digest(
    profile_version: &str,
    interp: Option<&RuleObject>,
    roots: &[RuleObject],
    files: &[RuleObject],
) -> o7_sandbox_protocol::ids::Digest256 {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"o7-runtime-binding-v1\n");
    buf.extend_from_slice(profile_version.as_bytes());
    buf.push(b'\n');
    let mut push = |tag: &str, o: &RuleObject| {
        buf.extend_from_slice(tag.as_bytes());
        buf.extend_from_slice(o.path.as_os_str().as_encoded_bytes());
        buf.extend_from_slice(format!(":{}:{}\n", o.id.0, o.id.1).as_bytes());
    };
    if let Some(i) = interp {
        push("interp=", i);
    }
    for r in roots {
        push("root=", r);
    }
    for f in files {
        push("file=", f);
    }
    o7_sandbox_protocol::ids::Digest256::of_bytes(&buf)
}

/// Mask `desired` to the rights valid for this object's TYPE (from the opened fd): a directory keeps
/// them all; a regular file keeps only the file-applicable subset (`READ_DIR` etc. on a file is
/// EINVAL); any other type is rejected fail-closed.
fn mask_for_type(obj: &RuleObject, desired: u64) -> Result<u64, InstallError> {
    if obj.file_type.is_dir() {
        Ok(desired)
    } else if obj.file_type.is_file() {
        Ok(desired & sys::FILE_APPLICABLE)
    } else {
        Err(InstallError::UnsupportedObjectType {
            path: obj.path.clone(),
        })
    }
}

/// Attach one `path_beneath` rule to the ALREADY-OPEN object (its fd) — not a re-resolved path.
fn attach_rule(
    ruleset: &std::os::fd::OwnedFd,
    obj: &RuleObject,
    rights: u64,
) -> Result<(), InstallError> {
    sys::add_path_beneath_rule(ruleset, rights, obj.file.as_raw_fd()).map_err(|err| {
        InstallError::AddRule {
            path: obj.path.clone(),
            err,
        }
    })
}

/// TEST-ONLY deterministic barrier for the race oracle: with `O7_LL_RACE_READY`/`O7_LL_RACE_GO` set,
/// signal that all rule objects are OPEN (READY) then wait (bounded) for GO before attaching any rule.
/// This lets a test atomically swap the rule pathnames in between and prove the rules still bind to
/// the objects opened above — never to the swapped-in ones.
fn race_barrier() {
    let (Ok(ready), Ok(go)) = (
        std::env::var("O7_LL_RACE_READY"),
        std::env::var("O7_LL_RACE_GO"),
    ) else {
        return;
    };
    let _ = std::fs::write(&ready, b"1");
    let start = Instant::now();
    while !Path::new(&go).exists() {
        if start.elapsed() > Duration::from_secs(10) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Create + write + remove `path`, proving the exact write operation succeeds. Used for the
/// unconfined baseline and the post-restrict inside check.
fn write_probe(path: &Path) -> io::Result<()> {
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        f.write_all(b"o7-landlock-probe")?;
    }
    let _ = std::fs::remove_file(path);
    Ok(())
}

/// Install and PROVE the Landlock filesystem policy for this thread against an unconfined baseline.
/// `Ok(())` means fully enforced AND differentially self-checked; any error means the caller must NOT
/// launch. `outside_probe` is a directory the self-check uses for its outside write.
///
/// Rights are SPLIT by authority class (see the runtime module + the contract-correction doc):
///   - `worktree`            → write-family (no `EXECUTE`);
///   - `allow_exec`          → `EXECUTE | READ_FILE` (ordinary programs);
///   - `exec_object`         → `EXECUTE | READ_FILE` on the EXACT private, ruleable execution inode
///     (its `O_PATH` fd), the object-capability that replaces the un-ruleable sealed `memfd`;
///   - `runtime.interpreter` → `EXECUTE | READ_FILE` (the kernel `execve`s it as `PT_INTERP`);
///   - `runtime` read roots + support files → `READ_FILE` ONLY (loading a shared object is a read,
///     never an execute; an `EXECUTE` grant over a library dir would authorize every file there).
pub(crate) fn install_filesystem(
    worktree: &Path,
    allow_exec: &[PathBuf],
    outside_probe: &Path,
    exec_object: Option<RawFd>,
    runtime: &crate::runtime::RuntimePolicy,
    faults: Faults,
) -> Result<FilesystemInstallProof, InstallError> {
    // 1. Probe the ABI FIRST — an fd or a version number alone restricts nothing.
    let abi = if faults.abi_enosys {
        return Err(InstallError::Unsupported(io::Error::from_raw_os_error(
            libc::ENOSYS,
        )));
    } else if faults.abi_eopnotsupp {
        return Err(InstallError::Unsupported(io::Error::from_raw_os_error(
            libc::EOPNOTSUPP,
        )));
    } else {
        match sys::abi_version() {
            Ok(v) => v,
            Err(e) => return Err(InstallError::Unsupported(e)),
        }
    };
    let abi = if faults.abi_low {
        sys::MIN_ABI - 1
    } else {
        abi
    };
    if abi < sys::MIN_ABI {
        return Err(InstallError::AbiTooLow {
            have: abi,
            need: sys::MIN_ABI,
        });
    }

    // 2. Create a ruleset handling the COMPLETE ABI-3 filesystem set.
    if faults.create {
        return Err(InstallError::CreateRuleset(io::Error::from_raw_os_error(
            libc::EINVAL,
        )));
    }
    let ruleset = sys::create_ruleset(sys::HANDLED_FS_ABI3).map_err(InstallError::CreateRuleset)?;

    // 3. Rules — TOCTOU-safe. Open EVERY rule object ONCE (O_PATH) and attach rules to those SAME fds;
    //    no pathname is ever re-resolved between the containment decision and the rule attachment.
    if faults.add_rule {
        return Err(InstallError::AddRule {
            path: worktree.to_path_buf(),
            err: io::Error::from_raw_os_error(libc::EINVAL),
        });
    }
    let worktree_obj = open_object(worktree)?;
    let mut exec_objs = Vec::with_capacity(allow_exec.len());
    for (i, path) in allow_exec.iter().enumerate() {
        if faults.add_rule_partial && i == 0 {
            return Err(InstallError::AddRule {
                path: path.to_path_buf(),
                err: io::Error::from_raw_os_error(libc::EINVAL),
            });
        }
        // OBJECT-IDENTITY procfs rejection (NOT a pathname check). A `procfs` object is never a
        // legitimate subprocess exec allowance: a `/proc/<pid>/fd/<n>` entry is a sealed launch SOURCE
        // (authorized by its seals + digest, run via the private execution inode), and granting its
        // `/proc/<pid>/fd` DIRECTORY would authorize EXECUTE over EVERY fd the owner holds. Decide by
        // the OPENED object's superblock (`fstatfs` → PROC_SUPER_MAGIC), so a symlink/bind alias to
        // `/proc/<pid>/fd` — whose lexical path does NOT start with `/proc` — cannot smuggle authority
        // back in. Fail closed before any rule is attached.
        let obj = open_object(path)?;
        let on_procfs = sys::is_procfs(obj.file.as_raw_fd()).map_err(|err| {
            InstallError::OpenParent {
                path: path.to_path_buf(),
                err,
            }
        })?;
        if on_procfs {
            return Err(InstallError::UnsupportedObjectType {
                path: path.to_path_buf(),
            });
        }
        exec_objs.push(obj);
    }
    // Runtime read-policy objects, opened ONCE for their exact identity + rule (a separate authority
    // class from allow_exec). An `interpreter` that cannot be opened, or any read root/support file
    // the profile named but the host lacks, fails closed here — never a fallback to a broader grant.
    if faults.runtime_open {
        return Err(InstallError::OpenParent {
            path: PathBuf::from("<runtime-object>"),
            err: io::Error::from_raw_os_error(libc::ENOENT),
        });
    }
    let interp_obj = match &runtime.interpreter {
        Some(p) => Some(open_object(p)?),
        None => None,
    };
    // `read_objs` is `read_roots ++ read_files`; remember the split so the runtime binding can label
    // each with its correct tag (`root=` vs `file=`) from the SAME opened objects.
    let n_roots = runtime.read_roots.len();
    let mut read_objs = Vec::with_capacity(n_roots + runtime.read_files.len());
    for p in runtime.read_roots.iter().chain(runtime.read_files.iter()) {
        read_objs.push(open_object(p)?);
    }

    // Test-only deterministic barrier: after every object is OPEN, before any rule is ATTACHED.
    race_barrier();

    let exec_desired = access::EXECUTE | access::READ_FILE | access::READ_DIR;
    // EXACT-object overlap: only when an allow_exec object IS the worktree object (same dev+ino) do we
    // combine rights — into ONE deliberate rule — instead of adding a duplicate rule for that object.
    let mut worktree_rights = sys::WORKTREE_RIGHTS;
    for obj in &exec_objs {
        if obj.id == worktree_obj.id {
            worktree_rights |= mask_for_type(obj, exec_desired)?;
        }
    }
    if !faults.omit_worktree_rule {
        let rights = mask_for_type(&worktree_obj, worktree_rights)?;
        attach_rule(&ruleset, &worktree_obj, rights)?;
    }
    for obj in &exec_objs {
        if obj.id == worktree_obj.id {
            continue; // folded into the single worktree rule above
        }
        let rights = mask_for_type(obj, exec_desired)?;
        attach_rule(&ruleset, obj, rights)?;
    }

    // The PRIVATE EXECUTION OBJECT: `EXECUTE | READ_FILE` on the exact ruleable execution inode via
    // its `O_PATH` fd. This is the object-capability that replaces the un-ruleable sealed `memfd` —
    // a real tmpfs inode Landlock CAN name, so exec authority stays narrow (no root grant).
    if let Some(fd) = exec_object {
        if faults.exec_object_rule {
            return Err(InstallError::AddRule {
                path: PathBuf::from("<execution-object>"),
                err: io::Error::from_raw_os_error(libc::EINVAL),
            });
        }
        let rights = (access::EXECUTE | access::READ_FILE) & sys::FILE_APPLICABLE;
        sys::add_path_beneath_rule(&ruleset, rights, fd).map_err(|err| InstallError::AddRule {
            path: PathBuf::from("<execution-object>"),
            err,
        })?;
    }

    // Runtime interpreter: `EXECUTE | READ_FILE` (the kernel execs it as `PT_INTERP`).
    if faults.runtime_rule && (interp_obj.is_some() || !read_objs.is_empty()) {
        return Err(InstallError::AddRule {
            path: PathBuf::from("<runtime-object>"),
            err: io::Error::from_raw_os_error(libc::EINVAL),
        });
    }
    if let Some(obj) = &interp_obj {
        let rights = mask_for_type(obj, access::EXECUTE | access::READ_FILE)?;
        attach_rule(&ruleset, obj, rights)?;
    }
    // Runtime read roots + support files: `READ_FILE` ONLY — NEVER `EXECUTE`. A directory rule with
    // `READ_FILE` propagates the read right to the shared-library files beneath it, which is all the
    // dynamic loader needs; it does not authorize executing anything stored there.
    for obj in &read_objs {
        // READ_FILE for the loader to map shared objects; READ_DIR so a directory read root can be
        // enumerated (e.g. `/proc/self/fd`). NEVER EXECUTE.
        let rights = mask_for_type(obj, access::READ_FILE | access::READ_DIR)?;
        attach_rule(&ruleset, obj, rights)?;
    }

    // 4. UNCONFINED BASELINE (before restrict): the exact self-check ops must succeed now, so a later
    //    denial is attributable to Landlock and not to DAC / read-only / another LSM.
    let tag = format!(".o7-landlock-selfcheck-{}", std::process::id());
    let inside_file = worktree.join(&tag);
    let outside_file = outside_probe.join(&tag);
    write_probe(&inside_file).map_err(InstallError::SelfCheckBaselineInsideDenied)?;
    write_probe(&outside_file).map_err(InstallError::SelfCheckBaselineOutsideDenied)?;

    // 5. no_new_privs — restrict_self needs it (or CAP_SYS_ADMIN), else EPERM.
    if faults.no_new_privs {
        return Err(InstallError::NoNewPrivs(io::Error::from_raw_os_error(
            libc::EPERM,
        )));
    }
    sys::set_no_new_privs().map_err(InstallError::NoNewPrivs)?;

    // 6. The point of no return.
    if faults.restrict_self {
        return Err(InstallError::RestrictSelf(io::Error::from_raw_os_error(
            libc::EPERM,
        )));
    }
    sys::restrict_self(&ruleset).map_err(InstallError::RestrictSelf)?;

    // 7. DIFFERENTIAL self-check. Inside must STILL be writable; the SAME outside op that succeeded
    //    unconfined must now be denied SPECIFICALLY by Landlock (EACCES/EPERM).
    write_probe(&inside_file).map_err(InstallError::SelfCheckInsideDenied)?;
    if faults.selfcheck_outside_notdenied {
        return Err(InstallError::SelfCheckOutsideNotDenied(
            io::Error::from_raw_os_error(libc::EROFS),
        ));
    }
    match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&outside_file)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&outside_file);
            Err(InstallError::SelfCheckOutsideAllowed)
        }
        Err(ref e) if is_denied(e) => {
            // Bind the runtime read policy from the SAME opened RuleObjects the rules attached to —
            // `read_objs` is `read_roots ++ read_files`, so split it back at `n_roots`. This is the
            // ONLY runtime binding the launch commits to; no pathname is re-resolved afterwards.
            let (roots, files) = read_objs.split_at(n_roots);
            let runtime_binding = runtime_binding_digest(
                runtime.profile_version,
                interp_obj.as_ref(),
                roots,
                files,
            );
            Ok(FilesystemInstallProof { runtime_binding })
        }
        Err(e) => Err(InstallError::SelfCheckOutsideNotDenied(e)),
    }
}

// --- harness entry ---

#[cfg(feature = "test-harness")]
enum Op {
    Fs {
        create: PathBuf,
        overwrite: PathBuf,
        truncate: PathBuf,
    },
    Exec {
        target: PathBuf,
        secondary: PathBuf,
    },
}

#[cfg(feature = "test-harness")]
struct Args {
    result: PathBuf,
    worktree: PathBuf,
    outside_probe: PathBuf,
    allow_exec: Vec<PathBuf>,
    op: Op,
}

#[cfg(feature = "test-harness")]
fn parse_args() -> Option<Args> {
    let mut it = std::env::args_os().skip(2); // program + "__landlock-run"
    let mut result = None;
    let mut worktree = None;
    let mut outside_probe = None;
    let mut allow_exec = Vec::new();
    loop {
        let flag = it.next()?;
        match flag.to_str()? {
            "--result" => result = Some(PathBuf::from(it.next()?)),
            "--worktree" => worktree = Some(PathBuf::from(it.next()?)),
            "--outside-probe" => outside_probe = Some(PathBuf::from(it.next()?)),
            "--allow-exec" => allow_exec.push(PathBuf::from(it.next()?)),
            "--" => break,
            _ => return None,
        }
    }
    let op = match it.next()?.to_str()? {
        "fs" => Op::Fs {
            create: PathBuf::from(it.next()?),
            overwrite: PathBuf::from(it.next()?),
            truncate: PathBuf::from(it.next()?),
        },
        "exec" => Op::Exec {
            target: PathBuf::from(it.next()?),
            secondary: PathBuf::from(it.next()?),
        },
        _ => return None,
    };
    Some(Args {
        result: result?,
        worktree: worktree?,
        // Default outside probe: a genuinely-outside temp dir (not under worktree/allow roots).
        outside_probe: outside_probe.unwrap_or_else(std::env::temp_dir),
        allow_exec,
        op,
    })
}

/// Open `path` under `opts`, returning the `key=OK` / `key=ERR:<errno>` token the frozen oracle reads.
#[cfg(feature = "test-harness")]
fn probe(key: &str, opts: &OpenOptions, path: &Path) -> String {
    match opts.open(path) {
        Ok(_) => format!("{key}=OK\n"),
        Err(e) => format!("{key}=ERR:{}\n", errno_of(&e)),
    }
}

/// TEST-HARNESS ENTRY (`sandboy __landlock-run …`): install + differentially prove the Landlock
/// filesystem policy, then — ONLY if fully enforced — run the requested probe op inside the
/// confinement. On any install failure, record `filesystem=not_enforced` + the stage and DO NOT run
/// the op (never launch).
#[cfg(feature = "test-harness")]
pub(crate) fn harness_main() -> i32 {
    let Some(args) = parse_args() else {
        eprintln!("sandboy __landlock-run: bad arguments");
        return 64;
    };
    // Open the result file BEFORE restricting: its path is OUTSIDE the worktree, so after
    // restrict_self it could not be re-opened — but writes to this already-open fd stay allowed.
    let mut out = match File::create(&args.result) {
        Ok(f) => f,
        Err(e) => {
            eprintln!(
                "sandboy __landlock-run: cannot open result {:?}: {e}",
                args.result
            );
            return 65;
        }
    };
    let faults = Faults::from_env();
    let mut rec = String::new();

    // For the exec op, open the target executable BEFORE confinement (its path is resolved now); it
    // becomes the EXECUTION OBJECT (ruled `EXECUTE | READ_FILE`) and is executed via this same fd
    // AFTER restrict_self. This harness exercises the SAME production `install_filesystem` with a
    // REAL-file execution object (not a memfd), plus the runtime read policy resolved from the target.
    let exec_fd: Option<io::Result<File>> = match &args.op {
        Op::Exec { target, .. } => Some(OpenOptions::new().read(true).open(target)),
        Op::Fs { .. } => None,
    };
    let runtime = match &args.op {
        Op::Exec { target, .. } => {
            let bytes = std::fs::read(target).unwrap_or_default();
            match crate::runtime::resolve(
                &bytes,
                &crate::runtime::PROFILE_V1,
                &crate::runtime::Faults::default(),
            ) {
                Ok(r) => r,
                Err(_) => crate::runtime::RuntimePolicy::empty(),
            }
        }
        Op::Fs { .. } => crate::runtime::RuntimePolicy::empty(),
    };
    let exec_object: Option<RawFd> = exec_fd
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(File::as_raw_fd);

    if let Err(e) = install_filesystem(
        &args.worktree,
        &args.allow_exec,
        &args.outside_probe,
        exec_object,
        &runtime,
        faults,
    ) {
        rec.push_str("filesystem=not_enforced\n");
        rec.push_str(&format!("stage={}\n", e.stage()));
        let _ = out.write_all(rec.as_bytes());
        eprintln!("sandboy __landlock-run: not enforced at {}: {e}", e.stage());
        return e.exit_code();
    }
    rec.push_str("filesystem=enforced\n");

    match args.op {
        Op::Fs {
            create,
            overwrite,
            truncate,
        } => {
            // Inside the worktree: an allowed write must SUCCEED (creates `inside.txt`).
            let inside = args.worktree.join("inside.txt");
            match OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&inside)
            {
                Ok(mut f) => {
                    let _ = f.write_all(b"OK");
                    rec.push_str("inside=OK\n");
                }
                Err(e) => rec.push_str(&format!("inside=ERR:{}\n", errno_of(&e))),
            }
            // Outside the worktree: create (MAKE_REG), overwrite (WRITE_FILE), truncate (TRUNCATE)
            // must each be DENIED. Distinct opens exercise the distinct rights.
            rec.push_str(&probe(
                "create",
                OpenOptions::new().write(true).create_new(true),
                &create,
            ));
            rec.push_str(&probe(
                "overwrite",
                OpenOptions::new().write(true),
                &overwrite,
            ));
            rec.push_str(&probe(
                "truncate",
                OpenOptions::new().write(true).truncate(true),
                &truncate,
            ));
        }
        Op::Exec { secondary, .. } => {
            // Execute the pre-opened target fd via execveat (see `exec_fd` above): an allowed target
            // runs (creating `secondary`); a non-allowed real file is DENIED by Landlock at execveat.
            match exec_fd {
                Some(Ok(exe)) => {
                    match sys::spawn_via_execveat(exe.as_raw_fd(), secondary.as_os_str()) {
                        Ok(mut child) => {
                            let ok = child.wait().map(|s| s.success()).unwrap_or(false);
                            rec.push_str(&format!(
                                "exec={}\n",
                                if ok { "OK" } else { "RANBUTFAILED" }
                            ));
                        }
                        Err(e) => rec.push_str(&format!("exec=ERR:{}\n", errno_of(&e))),
                    }
                }
                // Could not even open the target before confinement (independent of Landlock).
                Some(Err(e)) => rec.push_str(&format!("exec=ERR:{}\n", errno_of(&e))),
                None => {}
            }
        }
    }

    let _ = out.write_all(rec.as_bytes());
    let _ = out.flush();
    0
}
