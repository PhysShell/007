//! VB-2 — real Landlock **filesystem** confinement + an effect-based self-check, SAFE on top of the
//! single audited [`sys`] syscall module. Scope, strictly: confine writes to the worktree and
//! read+execute to `allow_exec`; prove it; report the `filesystem` dimension honestly. **No seccomp,
//! network, or env (VB-3); no `confinement_backend()` switch; no RED-matrix flip (VB-4).**
//!
//! Enforcement is NOT "the kernel handed back a ruleset fd". It is the full ordered sequence —
//! `create_ruleset` → `add_rule`* → `prctl(PR_SET_NO_NEW_PRIVS)` → `restrict_self` — AND a
//! subsequent EFFECT-BASED self-check (an outside write is observed DENIED and an inside write
//! observed ALLOWED). Anything short of that returns a typed [`InstallError`] and the caller MUST
//! NOT launch the target. The ABI is probed first and the exact minimum (3, for `TRUNCATE`) is
//! required; a lower/absent/disabled Landlock reports `not_enforced`, never a launch.
//!
//! Compiled ONLY under `test-harness` (like the VB-1 cgroup monitor): production `run` is the
//! untouched VB-0 honest bootstrap, so a production build compiles NEITHER this module NOR `libc`
//! NOR any `unsafe` (the crate keeps `forbid(unsafe_code)` when the feature is off). A TEST-ONLY
//! `O7_LL_FAULT` knob forces one install stage to fail so every fail-closed path is provable.

mod sys;

use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::os::fd::AsRawFd as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sys::access;

/// EACCES / EPERM — the two errnos Landlock denies with, per the frozen oracle (`ERR:13`/`ERR:1`).
fn is_denied(err: &io::Error) -> bool {
    matches!(err.raw_os_error(), Some(libc::EACCES) | Some(libc::EPERM))
}

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
    AddRule {
        path: PathBuf,
        err: io::Error,
    },
    NoNewPrivs(io::Error),
    RestrictSelf(io::Error),
    /// After `restrict_self`, an OUTSIDE-worktree write SUCCEEDED — the ruleset is not confining.
    SelfCheckOutsideAllowed,
    /// The outside-worktree write failed, but NOT with a Landlock deny (EACCES/EPERM), so the
    /// restriction is unproven — fail closed rather than assume it holds.
    SelfCheckOutsideNotDenied(io::Error),
    /// After `restrict_self`, an INSIDE-worktree write was denied — the ruleset is misbuilt.
    SelfCheckInsideDenied(io::Error),
}

impl InstallError {
    pub(crate) fn stage(&self) -> &'static str {
        match self {
            InstallError::Unsupported(_) => "unsupported",
            InstallError::AbiTooLow { .. } => "abi_too_low",
            InstallError::CreateRuleset(_) => "create_ruleset",
            InstallError::OpenParent { .. } => "open_parent",
            InstallError::AddRule { .. } => "add_rule",
            InstallError::NoNewPrivs(_) => "no_new_privs",
            InstallError::RestrictSelf(_) => "restrict_self",
            InstallError::SelfCheckOutsideAllowed => "self_check_outside",
            InstallError::SelfCheckOutsideNotDenied(_) => "self_check_outside_inconclusive",
            InstallError::SelfCheckInsideDenied(_) => "self_check_inside",
        }
    }
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
            InstallError::AddRule { path, err } => write!(f, "add_rule {path:?}: {err}"),
            InstallError::NoNewPrivs(e) => write!(f, "prctl(NO_NEW_PRIVS): {e}"),
            InstallError::RestrictSelf(e) => write!(f, "restrict_self: {e}"),
            InstallError::SelfCheckOutsideAllowed => {
                write!(
                    f,
                    "self-check: an outside-worktree write SUCCEEDED (not confined)"
                )
            }
            InstallError::SelfCheckOutsideNotDenied(e) => {
                write!(
                    f,
                    "self-check: outside write failed but not via a Landlock deny: {e}"
                )
            }
            InstallError::SelfCheckInsideDenied(e) => {
                write!(f, "self-check: an inside-worktree write was denied: {e}")
            }
        }
    }
}

/// TEST-ONLY forced-fault knob (`O7_LL_FAULT`), read from the control-plane env, letting the
/// confinement tests force a specific install stage to fail without needing a broken kernel.
#[derive(Debug, Clone, Copy, Default)]
struct Faults {
    abi_enosys: bool,
    abi_eopnotsupp: bool,
    abi_low: bool,
    create: bool,
    add_rule: bool,
    add_rule_partial: bool,
    no_new_privs: bool,
    restrict_self: bool,
}

impl Faults {
    fn from_env() -> Faults {
        let mut f = Faults::default();
        match std::env::var("O7_LL_FAULT").ok().as_deref() {
            Some("abi_enosys") => f.abi_enosys = true,
            Some("abi_eopnotsupp") => f.abi_eopnotsupp = true,
            Some("abi_low") => f.abi_low = true,
            Some("create") => f.create = true,
            Some("add_rule") => f.add_rule = true,
            Some("add_rule_partial") => f.add_rule_partial = true,
            Some("no_new_privs") => f.no_new_privs = true,
            Some("restrict_self") => f.restrict_self = true,
            _ => {}
        }
        f
    }
}

/// Add one `path_beneath` rule for `path`, opening it `O_PATH|O_CLOEXEC` via SAFE std (its `File`
/// closes the fd on EVERY return path, success or error).
fn add_rule(ruleset: &std::os::fd::OwnedFd, path: &Path, rights: u64) -> Result<(), InstallError> {
    let dir = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH | libc::O_CLOEXEC)
        .open(path)
        .map_err(|err| InstallError::OpenParent {
            path: path.to_path_buf(),
            err,
        })?;
    let res = sys::add_path_beneath_rule(ruleset, rights, dir.as_raw_fd()).map_err(|err| {
        InstallError::AddRule {
            path: path.to_path_buf(),
            err,
        }
    });
    // `dir` drops here regardless — the O_PATH fd is closed on success and on error.
    res
}

/// Install and PROVE the Landlock filesystem policy for this thread: writes confined to `worktree`,
/// read+execute confined to `allow_exec`. `Ok(())` means fully enforced AND self-checked; any error
/// means the caller must NOT launch the target.
fn install_filesystem(
    worktree: &Path,
    allow_exec: &[PathBuf],
    faults: Faults,
) -> Result<(), InstallError> {
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

    // 3. Rules: the worktree is the single WRITABLE root (every FS right); each allow_exec path is
    //    read+execute only.
    if faults.add_rule {
        return Err(InstallError::AddRule {
            path: worktree.to_path_buf(),
            err: io::Error::from_raw_os_error(libc::EINVAL),
        });
    }
    add_rule(&ruleset, worktree, sys::HANDLED_FS_ABI3)?;
    let exec_rights = access::EXECUTE | access::READ_FILE | access::READ_DIR;
    for (i, path) in allow_exec.iter().enumerate() {
        if faults.add_rule_partial && i == 0 {
            // A PARTIAL ruleset: the worktree rule was added, but an allow_exec rule fails. We abort
            // before restrict_self, so nothing is ever enforced.
            return Err(InstallError::AddRule {
                path: path.to_path_buf(),
                err: io::Error::from_raw_os_error(libc::EINVAL),
            });
        }
        add_rule(&ruleset, path, exec_rights)?;
    }

    // 4. no_new_privs — restrict_self needs it (or CAP_SYS_ADMIN), else EPERM.
    if faults.no_new_privs {
        return Err(InstallError::NoNewPrivs(io::Error::from_raw_os_error(
            libc::EPERM,
        )));
    }
    sys::set_no_new_privs().map_err(InstallError::NoNewPrivs)?;

    // 5. The point of no return.
    if faults.restrict_self {
        return Err(InstallError::RestrictSelf(io::Error::from_raw_os_error(
            libc::EPERM,
        )));
    }
    sys::restrict_self(&ruleset).map_err(InstallError::RestrictSelf)?;

    // 6. EFFECT-BASED self-check — the only thing that turns "syscalls returned 0" into "enforced".
    self_check(worktree)
}

/// Prove the restriction is real by OBSERVING it: an inside-worktree write must succeed and an
/// outside-worktree write must be denied. Uses a world-writable temp dir for the outside probe so a
/// success there unambiguously means Landlock is NOT confining (not merely a unix-permission deny).
fn self_check(worktree: &Path) -> Result<(), InstallError> {
    let tag = format!(".o7-landlock-selfcheck-{}", std::process::id());

    let inside = worktree.join(&tag);
    match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&inside)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&inside);
        }
        Err(e) => return Err(InstallError::SelfCheckInsideDenied(e)),
    }

    let outside = std::env::temp_dir().join(&tag);
    match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&outside)
    {
        Ok(_) => {
            // The write SUCCEEDED outside the worktree — confinement is not real.
            let _ = std::fs::remove_file(&outside);
            Err(InstallError::SelfCheckOutsideAllowed)
        }
        // The proof: the outside write is denied SPECIFICALLY by Landlock (EACCES/EPERM).
        Err(ref e) if is_denied(e) => Ok(()),
        // Failed, but not via a deny — cannot conclude the restriction holds. Fail closed.
        Err(e) => Err(InstallError::SelfCheckOutsideNotDenied(e)),
    }
}

// --- harness entry ---

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

struct Args {
    result: PathBuf,
    worktree: PathBuf,
    allow_exec: Vec<PathBuf>,
    op: Op,
}

fn parse_args() -> Option<Args> {
    let mut it = std::env::args_os().skip(2); // program + "__landlock-run"
    let mut result = None;
    let mut worktree = None;
    let mut allow_exec = Vec::new();
    loop {
        let flag = it.next()?;
        match flag.to_str()? {
            "--result" => result = Some(PathBuf::from(it.next()?)),
            "--worktree" => worktree = Some(PathBuf::from(it.next()?)),
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
        allow_exec,
        op,
    })
}

/// Open `path` under `opts`, returning the `key=OK` / `key=ERR:<errno>` token the frozen oracle reads.
fn probe(key: &str, opts: &OpenOptions, path: &Path) -> String {
    match opts.open(path) {
        Ok(_) => format!("{key}=OK\n"),
        Err(e) => format!("{key}=ERR:{}\n", errno_of(&e)),
    }
}

/// TEST-HARNESS ENTRY (`sandboy __landlock-run …`): install + prove the Landlock filesystem policy,
/// then — ONLY if fully enforced — run the requested probe op inside the confinement. On any install
/// failure, record `filesystem=not_enforced` + the stage and DO NOT run the op (never launch).
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

    if let Err(e) = install_filesystem(&args.worktree, &args.allow_exec, faults) {
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
        Op::Exec { target, secondary } => {
            // execve of a non-allowed binary must be DENIED by the kernel; if it ran, it would create
            // `secondary`.
            match Command::new(&target)
                .arg(&secondary)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    let _ = child.wait();
                    rec.push_str("exec=OK\n");
                }
                Err(e) => rec.push_str(&format!("exec=ERR:{}\n", errno_of(&e))),
            }
        }
    }

    let _ = out.write_all(rec.as_bytes());
    let _ = out.flush();
    0
}
