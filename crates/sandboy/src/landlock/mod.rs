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

use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::os::fd::AsRawFd as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
    /// An `allow_exec` path is neither a directory nor a regular file — no correct rule mask exists.
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
struct Faults {
    abi_enosys: bool,
    abi_eopnotsupp: bool,
    abi_low: bool,
    create: bool,
    add_rule: bool,
    add_rule_partial: bool,
    omit_worktree_rule: bool,
    no_new_privs: bool,
    restrict_self: bool,
    selfcheck_outside_notdenied: bool,
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
/// launch. `outside_probe` is a directory the self-check uses for its outside write — the default is
/// a genuinely-outside temp dir; tests point it at pathological locations to exercise each verdict.
fn install_filesystem(
    worktree: &Path,
    allow_exec: &[PathBuf],
    outside_probe: &Path,
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

    // 3. Rules — TOCTOU-safe. Open EVERY rule object ONCE (O_PATH) and attach rules to those SAME fds;
    //    no pathname is ever re-resolved between the containment decision and the rule attachment. The
    //    worktree is the single WRITABLE root (WORKTREE_RIGHTS, no EXECUTE); each allow_exec object is
    //    read+execute, object-type-masked. A DESCENDANT of the worktree that is also allow-listed
    //    naturally accumulates worktree-write + allow_exec-execute along the path hierarchy (documented
    //    Landlock same-layer semantics) — no explicit union, no path classification.
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
        exec_objs.push(open_object(path)?);
    }

    // Test-only deterministic barrier: after every object is OPEN, before any rule is ATTACHED.
    race_barrier();

    let exec_desired = access::EXECUTE | access::READ_FILE | access::READ_DIR;
    // EXACT-object overlap: only when an allow_exec object IS the worktree object (same dev+ino) do we
    // combine rights — into ONE deliberate rule — instead of adding a duplicate rule for that object.
    // Identity is compared between the SAME opened objects, never between re-resolved paths.
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
        Err(ref e) if is_denied(e) => Ok(()),
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
    outside_probe: PathBuf,
    allow_exec: Vec<PathBuf>,
    op: Op,
}

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
    // is executed via this fd AFTER restrict_self, so the exec restriction observed is Landlock's own
    // — a real file's execute right is enforced by the kernel, while a sealed memfd (no path) runs.
    let exec_fd: Option<io::Result<File>> = match &args.op {
        Op::Exec { target, .. } => Some(OpenOptions::new().read(true).open(target)),
        Op::Fs { .. } => None,
    };

    if let Err(e) = install_filesystem(
        &args.worktree,
        &args.allow_exec,
        &args.outside_probe,
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
