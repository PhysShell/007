//! VB-4 — the RUNTIME read policy: a SEPARATE authority class from `allow_exec`. The private
//! execution inode runs under a narrow Landlock ruleset, but a real target is a dynamically-linked
//! PIE, so its ELF interpreter and shared libraries must be reachable — WITHOUT granting `EXECUTE`
//! over whole library hierarchies (which would authorize every executable file stored there).
//!
//! The rights are split (proven on the host):
//!   - the exact ELF interpreter file: `EXECUTE | READ_FILE` (the kernel `execve`s it as `PT_INTERP`);
//!   - runtime library roots + loader support files: `READ_FILE` ONLY (loading a shared object is a
//!     read, not an execute);
//!   - ordinary programs stay governed exclusively by `allow_exec` (`EXECUTE | READ_FILE`).
//!
//! The profile is a VERSIONED, compiled-in platform constant (covered by the backend digest), NOT
//! ambient host discovery performed after confinement. It is verified against the actual staged
//! target: the target's `PT_INTERP` must be one the profile allows; a static target (no `PT_INTERP`)
//! gets an EMPTY runtime policy; an unresolved interpreter, or an unmodelled non-empty
//! `/etc/ld.so.preload`, fails closed (`NotEnforced`) — there is no fallback to granting `/`.
//! No `unsafe`: ELF parsing is bounded byte reads; object opening/identity is safe `std`.

use std::path::PathBuf;

/// A versioned platform runtime profile. Content lives in the trusted backend binary (so the backend
/// digest covers it); the launch binds the RESOLVED policy identities into `READY_TO_EXEC`.
pub(crate) struct RuntimeProfile {
    pub(crate) version: &'static str,
    /// Interpreter paths the profile permits; the target's `PT_INTERP` must match one EXACTLY.
    allowed_interpreters: &'static [&'static str],
    /// Directory roots whose files may be READ (never executed) — the shared-library search roots.
    library_read_roots: &'static [&'static str],
    /// Exact loader support files that may be READ (e.g. the loader cache), when present.
    support_read_files: &'static [&'static str],
    /// Dynamic-linker control variables that must never reach the target's environment.
    forbidden_env: &'static [&'static str],
}

/// `linux-glibc-x86_64-v1`: the platform profile for this milestone's designated host. A later,
/// stricter profile could enumerate exact resolved library FILES instead of directory roots without
/// changing the authority model.
pub(crate) const PROFILE_V1: RuntimeProfile = RuntimeProfile {
    version: "linux-glibc-x86_64-v1",
    allowed_interpreters: &[
        "/lib64/ld-linux-x86-64.so.2",
        "/usr/lib64/ld-linux-x86-64.so.2",
    ],
    library_read_roots: &["/usr/lib", "/usr/lib64", "/lib", "/lib64"],
    support_read_files: &["/etc/ld.so.cache"],
    forbidden_env: &[
        "LD_LIBRARY_PATH",
        "LD_PRELOAD",
        "LD_AUDIT",
        "LD_DEBUG",
        "LD_DEBUG_OUTPUT",
        "GLIBC_TUNABLES",
    ],
};

/// The resolved, target-specific runtime read policy. All paths are grant *inputs* — the launch opens
/// each once and binds its identity before `restrict_self`.
pub(crate) struct RuntimePolicy {
    /// Exact interpreter to grant `EXECUTE | READ_FILE`; `None` for a static target.
    pub(crate) interpreter: Option<PathBuf>,
    /// Library search roots to grant `READ_FILE` only.
    pub(crate) read_roots: Vec<PathBuf>,
    /// Exact loader support files to grant `READ_FILE` only.
    pub(crate) read_files: Vec<PathBuf>,
    /// Env var names (as bytes) that must be stripped/rejected from the target environment.
    pub(crate) forbidden_env: Vec<Vec<u8>>,
    /// The selected profile version, bound into the proof for auditability.
    pub(crate) profile_version: &'static str,
}

/// Why a runtime policy could not be resolved. Every variant is fail-closed: the target never runs.
#[derive(Debug)]
pub(crate) enum ResolveError {
    /// The staged bytes are not a well-formed ELF the profile understands.
    NotElf,
    /// The target's `PT_INTERP` is not one the profile allows.
    InterpreterNotAllowed(String),
    /// A non-empty `/etc/ld.so.preload` the profile does not model — its contents would silently
    /// widen the target's loaded objects, so the launch fails closed.
    UnmodelledPreload,
    /// A forced test fault at the named runtime stage.
    Fault(&'static str),
}

impl ResolveError {
    /// The fault-point stage name — consumed by the setup-failure witness (matrix wiring).
    #[allow(dead_code)]
    pub(crate) fn stage(&self) -> &'static str {
        match self {
            ResolveError::NotElf => "runtime_interpreter",
            ResolveError::InterpreterNotAllowed(_) => "runtime_interpreter",
            ResolveError::UnmodelledPreload => "runtime_preload_check",
            ResolveError::Fault(s) => s,
        }
    }
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::NotElf => write!(f, "staged target is not a recognizable ELF"),
            ResolveError::InterpreterNotAllowed(p) => {
                write!(
                    f,
                    "target PT_INTERP {p:?} is not permitted by the runtime profile"
                )
            }
            ResolveError::UnmodelledPreload => {
                write!(
                    f,
                    "host /etc/ld.so.preload is non-empty and not modelled by the profile"
                )
            }
            ResolveError::Fault(s) => write!(f, "forced runtime fault at {s}"),
        }
    }
}

/// TEST-ONLY (`fault-injection`) forced-failure knobs for runtime resolution.
#[derive(Clone, Copy, Default)]
pub(crate) struct Faults {
    #[cfg(feature = "fault-injection")]
    pub(crate) fail_profile: bool,
    #[cfg(feature = "fault-injection")]
    pub(crate) fail_interpreter: bool,
    #[cfg(feature = "fault-injection")]
    pub(crate) fail_preload: bool,
}

impl Faults {
    #[cfg(feature = "fault-injection")]
    fn tripped(&self, which: RuntimeFault) -> bool {
        match which {
            RuntimeFault::Profile => self.fail_profile,
            RuntimeFault::Interpreter => self.fail_interpreter,
            RuntimeFault::Preload => self.fail_preload,
        }
    }
    #[cfg(not(feature = "fault-injection"))]
    fn tripped(&self, _which: RuntimeFault) -> bool {
        false
    }
}

enum RuntimeFault {
    Profile,
    Interpreter,
    Preload,
}

impl RuntimePolicy {
    // NOTE: the runtime-binding digest is NO LONGER computed here from re-resolved pathnames. It is
    // bound inside `landlock::install_filesystem`, over the EXACT `RuleObject`s the rules attach to
    // (see `landlock::runtime_binding_digest`), so `READY_TO_EXEC` commits to the objects that were
    // actually ruled — closing the path-swap window between rule attachment and proof.

    /// An empty runtime policy (no interpreter, no read roots) — for a static target or a
    /// filesystem-only op that execs nothing.
    #[cfg(feature = "test-harness")]
    pub(crate) fn empty() -> RuntimePolicy {
        RuntimePolicy {
            interpreter: None,
            read_roots: Vec::new(),
            read_files: Vec::new(),
            forbidden_env: Vec::new(),
            profile_version: PROFILE_V1.version,
        }
    }
}

/// Resolve the runtime read policy for `exec_bytes` (the staged execution inode's exact bytes)
/// against `profile`, verifying the loader preconditions. Fails closed on any mismatch.
pub(crate) fn resolve(
    exec_bytes: &[u8],
    profile: &RuntimeProfile,
    faults: &Faults,
) -> Result<RuntimePolicy, ResolveError> {
    if faults.tripped(RuntimeFault::Profile) {
        return Err(ResolveError::Fault("runtime_profile"));
    }
    let forbidden_env: Vec<Vec<u8>> = profile
        .forbidden_env
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect();

    let interp = match parse_pt_interp(exec_bytes) {
        Ok(i) => i,
        Err(()) => return Err(ResolveError::NotElf),
    };

    // Static target (no interpreter): empty runtime read policy — nothing to load, nothing to grant.
    let Some(interp) = interp else {
        return Ok(RuntimePolicy {
            interpreter: None,
            read_roots: Vec::new(),
            read_files: Vec::new(),
            forbidden_env,
            profile_version: profile.version,
        });
    };

    if faults.tripped(RuntimeFault::Interpreter)
        || !profile.allowed_interpreters.iter().any(|a| *a == interp)
    {
        return Err(ResolveError::InterpreterNotAllowed(interp));
    }

    // A non-empty, unmodelled /etc/ld.so.preload would silently widen the loaded object set. The
    // profile models none, so any non-whitespace content fails closed.
    if faults.tripped(RuntimeFault::Preload) || host_has_unmodelled_preload() {
        return Err(ResolveError::UnmodelledPreload);
    }

    // Only grant support files that actually exist (a missing loader cache is not required).
    let read_files: Vec<PathBuf> = profile
        .support_read_files
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();
    let read_roots: Vec<PathBuf> = profile
        .library_read_roots
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.exists())
        .collect();

    Ok(RuntimePolicy {
        interpreter: Some(PathBuf::from(interp)),
        read_roots,
        read_files,
        forbidden_env,
        profile_version: profile.version,
    })
}

/// Whether the host carries a non-empty `/etc/ld.so.preload` (non-whitespace content). A missing or
/// whitespace-only file is fine.
fn host_has_unmodelled_preload() -> bool {
    match std::fs::read("/etc/ld.so.preload") {
        Ok(bytes) => bytes.iter().any(|b| !b.is_ascii_whitespace()),
        Err(_) => false,
    }
}

// ---- bounded ELF64 little-endian PT_INTERP parse (no `unsafe`, no panics, no indexing) -----------

const PT_INTERP: u32 = 3;

fn rd_u16(b: &[u8], off: usize) -> Option<u16> {
    let s = b.get(off..off.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}
fn rd_u32(b: &[u8], off: usize) -> Option<u32> {
    let s = b.get(off..off.checked_add(4)?)?;
    let a: [u8; 4] = s.try_into().ok()?;
    Some(u32::from_le_bytes(a))
}
fn rd_u64(b: &[u8], off: usize) -> Option<u64> {
    let s = b.get(off..off.checked_add(8)?)?;
    let a: [u8; 8] = s.try_into().ok()?;
    Some(u64::from_le_bytes(a))
}

/// Return `Ok(Some(interp))` for a dynamic ELF64-LE with a `PT_INTERP`, `Ok(None)` for a well-formed
/// ELF with none (static), or `Err(())` if the bytes are not a parseable ELF64-LE object.
fn parse_pt_interp(b: &[u8]) -> Result<Option<String>, ()> {
    // e_ident: magic, class(2=64), data(1=LE).
    if b.get(0..4) != Some(&[0x7f, b'E', b'L', b'F']) {
        return Err(());
    }
    if b.get(4).copied() != Some(2) || b.get(5).copied() != Some(1) {
        return Err(());
    }
    let e_phoff = rd_u64(b, 0x20).ok_or(())? as usize;
    let e_phentsize = rd_u16(b, 0x36).ok_or(())? as usize;
    let e_phnum = rd_u16(b, 0x38).ok_or(())? as usize;
    if e_phentsize < 56 {
        return Err(());
    }
    for i in 0..e_phnum {
        let ph = e_phoff
            .checked_add(i.checked_mul(e_phentsize).ok_or(())?)
            .ok_or(())?;
        let p_type = rd_u32(b, ph).ok_or(())?;
        if p_type != PT_INTERP {
            continue;
        }
        let p_offset = rd_u64(b, ph.checked_add(8).ok_or(())?).ok_or(())? as usize;
        let p_filesz = rd_u64(b, ph.checked_add(32).ok_or(())?).ok_or(())? as usize;
        if p_filesz == 0 || p_filesz > 4096 {
            return Err(());
        }
        let end = p_offset.checked_add(p_filesz).ok_or(())?;
        let raw = b.get(p_offset..end).ok_or(())?;
        // Strip the trailing NUL(s); the interpreter is a plain path string.
        let trimmed: Vec<u8> = raw.iter().copied().take_while(|&c| c != 0).collect();
        return String::from_utf8(trimmed).map(Some).map_err(|_| ());
    }
    Ok(None)
}
