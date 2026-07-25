//! The Sandboy boundary: a [`ProcessBoundary`] that confines the child with real Linux
//! isolation (Landlock + seccomp) and attests [`EnforcementLevel::FullyEnforced`].
//!
//! The confinement runs in an EXTERNAL `sandboy` backend binary (every 007 crate forbids
//! `unsafe`; see `docs/architecture/sandboy-boundary.md`). This boundary launches the backend
//! through the spawn seam. In a MONITOR topology the backend installs the confinement, starts
//! the target as a CHILD (same PID namespace, so PR 3's sealed-memfd exec via
//! `/proc/<verifier_pid>/fd/<n>` still resolves), and REMAINS ALIVE as the cgroup-owning
//! monitor enforcing the deadline + process-tree kill — it never `exec`s into the target.
//!
//! Trust is bound to OBJECTS, not paths:
//! - the BACKEND is a [`BackendImage`] that OWNS a SEALED, immutable `memfd` copy of the backend
//!   binary + its digest + the expected [`BackendIdentity`]. The launch execs the sealed object
//!   via [`BackendImage::descriptor_path`] (so neither a path swap NOR an in-place inode rewrite
//!   can substitute a different binary), and the report must echo the expected identity AND the
//!   backend digest;
//! - the report also binds the `launch_spec_digest` — a digest over the exact INVOCATION (target
//!   object + argv + cwd + allowlisted env + stdin), not merely which binary.
//!
//! Planes are SEPARATE: the backend is a TRUSTED launcher and runs from a fixed trusted cwd
//! (`/`) with a trusted control-plane environment — never the untrusted target's cwd/env
//! (LD_PRELOAD, …). The target's argv, cwd, and (allowlisted) environment travel out-of-band in
//! the [`o7_sandbox_protocol::LaunchRequest`] on `--request-fd`, never on the backend's
//! /proc-visible argv or its process environment.
//!
//! Authorization is a BARRIER: the two-phase handshake (`--report-fd` for the report,
//! `--control-fd` for the parent GO) means the backend holds the confined target BEFORE it
//! starts until the parent has verified the report and sent GO. A malformed / mis-bound /
//! downgraded report, a NACK, EOF, or a cancel makes the backend kill its cgroup and reap — the
//! target never runs on an unverified report.
//!
//! LIVE + enforced here: backend + TARGET acquisition (hardened `O_NOFOLLOW`/regular-file open
//! for ordinary paths, a followed-and-seal-checked path for the frozen `/proc/<pid>/fd/<n>`
//! executable, fail-closed on a digest mismatch), policy validation, nonce minting (fail-closed
//! on RNG failure), the confined-backend argv with plane separation, the exec-permission gate,
//! the two-phase report handshake with an end-of-message (EOF) proof so a trailing byte fails
//! closed, monitor OWNERSHIP of the target's group (a monitor that exits leaving a live owned
//! target fails closed even on a clean code), and cleanup that must be PROVEN (an unproven reap
//! dominates the triggering error).

use std::collections::BTreeMap;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{ffi::OsString, io};

use async_trait::async_trait;

use o7_sandbox_protocol::frame::{self, FrameError};
use o7_sandbox_protocol::ids::{BackendIdentity, Digest256, LaunchNonce};
use o7_sandbox_protocol::policy::{NetworkPolicy, SandboxPolicy, SandboxPolicyError};

use crate::boundary::{
    BoundaryAttestation, BoundaryError, BoundaryEvidence, BoundaryKind, BoundaryLaunch,
    BoundaryProcess, BoundarySpawnSpec, EnforcementLevel, ProcessBoundary,
};
use crate::host_boundary::{signalable_pid, HostBoundaryProcess};
use crate::process_identity::ProcessIdentity;
use crate::sealed::SealedObject;

/// Create a pipe whose ends are non-CLOEXEC (so the backend end survives `exec`); the caller
/// marks its own end CLOEXEC via [`set_cloexec`].
fn make_pipe() -> Result<(std::os::fd::OwnedFd, std::os::fd::OwnedFd), BoundaryError> {
    nix::unistd::pipe().map_err(|e| BoundaryError::Spawn(io::Error::other(format!("pipe: {e}"))))
}

/// Mark a descriptor CLOEXEC so it does NOT leak into the spawned backend.
fn set_cloexec(fd: &std::os::fd::OwnedFd) -> Result<(), BoundaryError> {
    use std::os::fd::AsRawFd as _;
    nix::fcntl::fcntl(
        fd.as_raw_fd(),
        nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC),
    )
    .map(|_| ())
    .map_err(|e| BoundaryError::Spawn(io::Error::other(format!("cloexec: {e}"))))
}

/// A backend bound to a specific SEALED, immutable OBJECT — not a re-resolvable path, and not
/// merely a held-but-mutable inode. Acquisition stages the backend bytes into a sealed `memfd`
/// (WRITE|GROW|SHRINK|SEAL), so a same-UID process cannot rewrite the object in place after
/// construction; the launch execs the sealed object via [`BackendImage::descriptor_path`]
/// (`/proc/<owner_pid>/fd/<n>`). Fields are private; the only constructor is
/// [`BackendImage::acquire`], which fails closed on a digest mismatch or a sealing failure.
#[derive(Debug, Clone)]
pub struct BackendImage {
    object: SealedObject,
    identity: BackendIdentity,
}

impl BackendImage {
    /// Acquire the backend object into a SEALED, held memfd and bind its digest. After this
    /// returns the object is immutable — a swap OR in-place rewrite of `path` cannot change it.
    ///
    /// # Errors
    /// [`SandboyLaunchError::BackendOpen`] if the object cannot be opened/read/sealed;
    /// [`SandboyLaunchError::BackendObjectMismatch`] if the sealed bytes do not match the digest.
    pub fn acquire(
        path: &Path,
        expected_digest: Digest256,
        identity: BackendIdentity,
    ) -> Result<Self, SandboyLaunchError> {
        let object = SealedObject::stage_from_path(path, &expected_digest)?;
        Ok(Self { object, identity })
    }

    /// The exec path of the sealed backend object: `/proc/<owner_pid>/fd/<n>`.
    #[must_use]
    pub fn descriptor_path(&self) -> PathBuf {
        self.object.descriptor_path()
    }

    /// The expected backend identity a report must echo.
    #[must_use]
    pub fn identity(&self) -> &BackendIdentity {
        &self.identity
    }

    /// The digest of the sealed backend object.
    #[must_use]
    pub fn digest(&self) -> &Digest256 {
        self.object.digest()
    }
}

/// Mints a 128-bit launch nonce. Abstracted so tests can inject a deterministic source and a
/// forced RNG failure; production uses [`OsNonceSource`] (the OS CSPRNG, no fallback).
pub trait NonceSource: Send + Sync {
    /// Mint a fresh nonce, or fail (an RNG failure must fail the launch, never fall back).
    ///
    /// # Errors
    /// [`NonceError`] if randomness could not be obtained.
    fn mint(&self) -> Result<LaunchNonce, NonceError>;
}

/// The OS CSPRNG nonce source: 128 bits straight from `getrandom`, no timestamp/counter
/// fallback.
#[derive(Debug, Clone, Copy, Default)]
pub struct OsNonceSource;

impl NonceSource for OsNonceSource {
    fn mint(&self) -> Result<LaunchNonce, NonceError> {
        let mut bytes = [0u8; 16];
        getrandom::getrandom(&mut bytes).map_err(|e| NonceError(e.to_string()))?;
        Ok(LaunchNonce::from_bytes(bytes))
    }
}

/// A failure obtaining randomness for a launch nonce.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("launch nonce RNG failure: {0}")]
pub struct NonceError(pub String);

/// A boundary that confines the child through an external `sandboy` backend and attests full
/// enforcement. Constructed only with a validated full-confinement policy and an
/// acquisition-bound backend, so a constructed instance always intends `FullyEnforced` — there
/// is no weaker mode and no silent fallback.
/// TYPED backend control-plane configuration. Deliberately NOT an arbitrary environment map — a
/// production caller cannot smuggle `LD_PRELOAD`/`LD_LIBRARY_PATH`/etc. into the trusted backend
/// through it. The only knob is a TEST-ONLY fake-backend mode; a real backend ignores it, and
/// production leaves it `None`, giving the backend an empty trusted environment.
#[derive(Debug, Clone, Default)]
pub struct BackendConfig {
    /// TEST-ONLY: drives the controlled fake `sandboy` backend's misbehavior mode. A real backend
    /// ignores this; it exists so tests configure the fake via the trusted control plane rather
    /// than the untrusted target environment.
    #[doc(hidden)]
    pub fake_mode: Option<String>,
}

#[derive(Clone)]
pub struct SandboyBoundary {
    backend: BackendImage,
    policy: SandboxPolicy,
    nonce_source: Arc<dyn NonceSource>,
    /// TYPED control-plane config for the backend (never the untrusted target's environment).
    backend_config: BackendConfig,
}

impl std::fmt::Debug for SandboyBoundary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The nonce source is an opaque trait object; the backend + policy are the identity.
        f.debug_struct("SandboyBoundary")
            .field("backend", &self.backend)
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

/// Why a Sandboy launch cannot be trusted — at construction OR when verifying a report/object.
#[derive(Debug, thiserror::Error)]
pub enum SandboyLaunchError {
    /// The confinement policy is not capable of full confinement.
    #[error("sandbox policy is not fully confining: {0}")]
    Policy(#[from] SandboxPolicyError),
    /// The backend object could not be opened/read/sealed for acquisition.
    #[error("sandboy backend object could not be acquired: {0}")]
    BackendOpen(String),
    /// The sealed backend object's bytes do not match its pinned digest (substitution / TOCTOU).
    #[error("backend object digest does not match the trusted image")]
    BackendObjectMismatch,
    /// The report names a different backend than the expected identity.
    #[error("report backend identity does not match the expected backend")]
    BackendMismatch,
    /// The report's backend digest does not match the trusted backend object.
    #[error("report backend digest does not match the trusted backend object")]
    BackendDigestMismatch,
    /// The report frame was malformed / oversized / truncated / trailing.
    #[error("report frame invalid: {0}")]
    Frame(#[from] FrameError),
    /// The report is bound to a different policy than the one installed.
    #[error("report policy_digest does not match the installed policy")]
    PolicyDigestMismatch,
    /// The report is bound to a different (stale/replayed) launch.
    #[error("report launch_nonce does not match this launch")]
    NonceMismatch,
    /// The report is bound to a different launch spec (target/argv/cwd/env/stdin) than confined.
    #[error("report launch_spec_digest does not match the confined launch")]
    LaunchSpecMismatch,
    /// The backend did not establish full enforcement on every dimension.
    #[error("backend did not establish full enforcement (a dimension was downgraded)")]
    NotFullyEnforced,
    /// The target requested an environment variable that the policy's allowlist does not permit.
    #[error("target environment variable is not on the policy allowlist: {0:?}")]
    DisallowedEnv(OsString),
}

impl From<crate::sealed::SealError> for SandboyLaunchError {
    fn from(e: crate::sealed::SealError) -> Self {
        match e {
            crate::sealed::SealError::DigestMismatch => SandboyLaunchError::BackendObjectMismatch,
            other => SandboyLaunchError::BackendOpen(other.to_string()),
        }
    }
}

impl SandboyBoundary {
    /// Build a Sandboy boundary from an acquired (held) backend and a confinement policy,
    /// using the OS CSPRNG for launch nonces.
    ///
    /// # Errors
    /// [`SandboyLaunchError::Policy`] if the policy cannot fully confine.
    pub fn new(backend: BackendImage, policy: SandboxPolicy) -> Result<Self, SandboyLaunchError> {
        Self::with_nonce_source(backend, policy, Arc::new(OsNonceSource))
    }

    /// As [`SandboyBoundary::new`], with an injected nonce source (deterministic tests / forced
    /// RNG failure).
    ///
    /// # Errors
    /// As [`SandboyBoundary::new`].
    pub fn with_nonce_source(
        backend: BackendImage,
        policy: SandboxPolicy,
        nonce_source: Arc<dyn NonceSource>,
    ) -> Result<Self, SandboyLaunchError> {
        policy.validate()?;
        Ok(Self {
            backend,
            policy,
            nonce_source,
            backend_config: BackendConfig::default(),
        })
    }

    /// Set the TYPED backend control-plane config (default: no fake mode → empty trusted env).
    #[must_use]
    pub fn with_backend_config(mut self, config: BackendConfig) -> Self {
        self.backend_config = config;
        self
    }

    /// The trusted control-plane environment derived from the typed config. Only a TEST-ONLY
    /// fake mode ever populates it; production is empty.
    fn backend_environment(&self) -> BTreeMap<OsString, OsString> {
        let mut env = BTreeMap::new();
        if let Some(mode) = &self.backend_config.fake_mode {
            env.insert(OsString::from("O7_FAKE_MODE"), OsString::from(mode));
        }
        env
    }

    /// The policy this boundary installs.
    #[must_use]
    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    /// The trusted backend image.
    #[must_use]
    pub fn backend(&self) -> &BackendImage {
        &self.backend
    }

    /// The digest of a target object's exact bytes — the value the report's `target_digest`
    /// must equal. The launch computes this from the HELD bytes it hands the backend, binding
    /// the report to the object that actually ran.
    #[must_use]
    pub fn target_digest_of(bytes: &[u8]) -> Digest256 {
        Digest256::of_bytes(bytes)
    }

    /// Construct the confined backend invocation for a target `spec`, or fail closed. The
    /// launched executable is the HELD backend descriptor; the report and control descriptors
    /// and the launch nonce precede the target after `--`.
    ///
    /// # Errors
    /// [`BoundaryError::Spawn`] if the target executable is not permitted by the policy.
    pub fn backend_spawn_spec(
        &self,
        spec: &BoundarySpawnSpec,
        report_fd: RawFd,
        control_fd: RawFd,
        request_fd: RawFd,
        launch_nonce: &LaunchNonce,
    ) -> Result<BoundarySpawnSpec, BoundaryError> {
        if !self.policy.permits_exec(&spec.executable) {
            return Err(BoundaryError::Spawn(io::Error::other(format!(
                "sandbox policy does not permit executing {}; refusing to launch",
                spec.executable.display()
            ))));
        }

        let mut arguments: Vec<OsString> = vec![
            OsString::from("run"),
            // The dedicated report descriptor (one versioned length-bounded frame).
            OsString::from("--report-fd"),
            OsString::from(report_fd.to_string()),
            // The control descriptor: the backend holds the target until it reads the parent's
            // GO here; a NACK/EOF makes it kill the cgroup and reap.
            OsString::from("--control-fd"),
            OsString::from(control_fd.to_string()),
            // The request descriptor: the backend reads the exact execution (target argv/cwd/
            // allowlisted-env/stdin) from here — never from the backend's own argv/env.
            OsString::from("--request-fd"),
            OsString::from(request_fd.to_string()),
            // The per-launch nonce the backend must echo, binding the report to THIS spawn.
            OsString::from("--launch-nonce"),
            OsString::from(launch_nonce.as_str()),
        ];
        match self.policy.network {
            NetworkPolicy::DenyAll => arguments.push(OsString::from("--deny-net")),
        }
        arguments.push(OsString::from("--worktree"));
        arguments.push(self.policy.worktree.clone().into_os_string());
        // Enforced `timeout >= 1ms` by policy validation, so this is never `0`.
        let timeout_ms = self.policy.timeout.as_millis();
        arguments.push(OsString::from("--timeout-ms"));
        arguments.push(OsString::from(timeout_ms.to_string()));
        for path in &self.policy.allow_exec {
            arguments.push(OsString::from("--allow-exec"));
            arguments.push(path.clone().into_os_string());
        }
        for name in &self.policy.env_allowlist {
            arguments.push(OsString::from("--allow-env"));
            arguments.push(name.clone());
        }
        // Only the target EXECUTABLE path (a /proc/<pid>/fd descriptor — not sensitive) follows
        // the separator so the backend knows what object to run. The target's argv, cwd, and
        // environment travel out-of-band in the LaunchRequest on --request-fd, never on this
        // /proc-visible argv.
        arguments.push(OsString::from("--"));
        arguments.push(spec.executable.clone().into_os_string());

        Ok(BoundarySpawnSpec {
            // Exec the HELD backend object via its owned descriptor path, not a re-resolved
            // path — a swap of the source path after acquisition cannot change what runs.
            executable: self.backend.descriptor_path(),
            arguments,
            // PLANE SEPARATION: the backend is a TRUSTED launcher and must NOT inherit the
            // untrusted target's environment or cwd (LD_PRELOAD, LD_LIBRARY_PATH, attacker cwd,
            // …). It runs from a fixed trusted directory with a trusted control-plane
            // environment. The target's own cwd/env travel to the backend out-of-band (a launch
            // request), never through the backend's process environment or a /proc-visible argv.
            working_directory: PathBuf::from("/"),
            environment: self.backend_environment(),
            stdin: spec.stdin,
        })
    }

    /// Build the out-of-band [`LaunchRequest`] for a target `spec`, enforcing the environment
    /// ALLOWLIST: every variable the target asks to keep must be named in
    /// `policy.env_allowlist`, byte-exact, or the launch fails closed BEFORE the backend is
    /// asked to run anything. The request carries the exact execution (target digest, argv, cwd,
    /// allowlisted env, stdin, nonce) out-of-band — never on the backend's argv or environment.
    ///
    /// # Errors
    /// [`SandboyLaunchError::DisallowedEnv`] if the target requests a non-allowlisted variable.
    pub fn launch_request(
        &self,
        spec: &BoundarySpawnSpec,
        target_digest: Digest256,
        launch_nonce: &LaunchNonce,
    ) -> Result<o7_sandbox_protocol::LaunchRequest, SandboyLaunchError> {
        use std::os::unix::ffi::OsStrExt as _;
        let allowed: std::collections::BTreeSet<&std::ffi::OsStr> = self
            .policy
            .env_allowlist
            .iter()
            .map(std::ffi::OsString::as_os_str)
            .collect();
        let mut env = Vec::new();
        for (name, value) in &spec.environment {
            if !allowed.contains(name.as_os_str()) {
                return Err(SandboyLaunchError::DisallowedEnv(name.clone()));
            }
            env.push(o7_sandbox_protocol::EnvEntry {
                name: name.as_bytes().to_vec(),
                value: value.as_bytes().to_vec(),
            });
        }
        Ok(o7_sandbox_protocol::LaunchRequest {
            schema_version: o7_sandbox_protocol::SCHEMA_VERSION,
            target_digest,
            argv: spec
                .arguments
                .iter()
                .map(|a| a.as_bytes().to_vec())
                .collect(),
            cwd: spec.working_directory.as_os_str().as_bytes().to_vec(),
            env,
            stdin: match spec.stdin {
                crate::spec::StdinMode::Null => o7_sandbox_protocol::StdinKind::Null,
            },
            launch_nonce: launch_nonce.clone(),
        })
    }

    /// Verify a report frame and turn it into trusted [`BoundaryEvidence::Reported`], or FAIL
    /// CLOSED. The report is trusted only if it echoes the expected BACKEND identity AND digest,
    /// and is bound to THIS policy (`policy_digest`), THIS launch (`launch_nonce`), and the exact
    /// LAUNCH SPEC confined (`launch_spec_digest` — target object + argv + cwd + env + stdin),
    /// AND attests full enforcement on every dimension.
    ///
    /// # Errors
    /// [`SandboyLaunchError`] on a bad frame, a wrong backend, a binding mismatch, or a
    /// non-full report.
    pub fn verify_report(
        &self,
        frame_bytes: &[u8],
        launch_nonce: &LaunchNonce,
        launch_spec_digest: &Digest256,
    ) -> Result<BoundaryEvidence, SandboyLaunchError> {
        let report = frame::decode(frame_bytes)?;
        if report.backend != *self.backend.identity() {
            return Err(SandboyLaunchError::BackendMismatch);
        }
        if report.backend_digest != *self.backend.digest() {
            return Err(SandboyLaunchError::BackendDigestMismatch);
        }
        if report.policy_digest != self.policy.digest() {
            return Err(SandboyLaunchError::PolicyDigestMismatch);
        }
        if report.launch_nonce != *launch_nonce {
            return Err(SandboyLaunchError::NonceMismatch);
        }
        if report.launch_spec_digest != *launch_spec_digest {
            return Err(SandboyLaunchError::LaunchSpecMismatch);
        }
        if !report.is_fully_enforced() {
            return Err(SandboyLaunchError::NotFullyEnforced);
        }
        Ok(BoundaryEvidence::Reported {
            attestation: BoundaryAttestation {
                implementation: BoundaryKind::Sandboy,
                enforcement: EnforcementLevel::FullyEnforced,
            },
            report: frame_bytes.to_vec(),
        })
    }

    /// Build the confinement-flag portion of the backend argv (network/worktree/timeout/allow-*).
    fn policy_flags(&self) -> Vec<OsString> {
        let mut args: Vec<OsString> = Vec::new();
        match self.policy.network {
            NetworkPolicy::DenyAll => args.push(OsString::from("--deny-net")),
        }
        args.push(OsString::from("--worktree"));
        args.push(self.policy.worktree.clone().into_os_string());
        args.push(OsString::from("--timeout-ms"));
        args.push(OsString::from(self.policy.timeout.as_millis().to_string()));
        for path in &self.policy.allow_exec {
            args.push(OsString::from("--allow-exec"));
            args.push(path.clone().into_os_string());
        }
        for name in &self.policy.env_allowlist {
            args.push(OsString::from("--allow-env"));
            args.push(name.clone());
        }
        args
    }
}

/// The live Sandboy monitor as a [`BoundaryProcess`]. It delegates lifecycle to the group-leader
/// wrapper, but ALSO retains the sealed backend + target objects for the process's lifetime — the
/// backend execs the sealed target via `/proc/<owner_pid>/fd/<n>`, so those descriptors must stay
/// open until the run ends, not be dropped when `spawn` returns.
struct SandboyMonitorProcess {
    inner: HostBoundaryProcess,
    _backend: BackendImage,
    _target: SealedObject,
}

impl SandboyMonitorProcess {
    /// Poll the owned membership until it is empty or the bound elapses.
    async fn drained_within(&self, bound: std::time::Duration) -> bool {
        let deadline = tokio::time::Instant::now() + bound;
        loop {
            if matches!(self.inner.remaining_members().await, Ok(m) if m.is_empty()) {
                return true;
            }
            if tokio::time::Instant::now() >= deadline {
                return matches!(self.inner.remaining_members().await, Ok(m) if m.is_empty());
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
}

#[async_trait]
impl BoundaryProcess for SandboyMonitorProcess {
    fn identity(&self) -> ProcessIdentity {
        self.inner.identity()
    }
    fn take_stdout(&mut self) -> Option<crate::boundary::BoundaryStream> {
        self.inner.take_stdout()
    }
    fn take_stderr(&mut self) -> Option<crate::boundary::BoundaryStream> {
        self.inner.take_stderr()
    }
    async fn request_graceful_stop(&mut self) -> Result<(), BoundaryError> {
        self.inner.request_graceful_stop().await
    }
    async fn force_stop(&mut self) -> Result<(), BoundaryError> {
        self.inner.force_stop().await
    }
    async fn wait(&mut self) -> Result<crate::boundary::BoundaryExit, BoundaryError> {
        // Wait the monitor (the owned group leader). In a clean run the monitor OUTLIVES the
        // target and reaps it, so the owned group is EMPTY once the monitor exits. If any owned
        // member survives the monitor, the run is broken — a monitor that died leaving a live,
        // owned target is not a clean exit no matter what code it returned. Kill the owned set,
        // prove it drains, and FAIL CLOSED; an undrained set is a cleanup failure that dominates.
        let exit = self.inner.wait().await?;
        let members = self.inner.remaining_members().await?;
        if !members.is_empty() {
            self.inner.force_stop().await?;
            if !self.drained_within(std::time::Duration::from_secs(3)).await {
                return Err(BoundaryError::Signal(
                    "monitor exited but owned members survived teardown".to_owned(),
                ));
            }
            return Err(BoundaryError::Signal(format!(
                "monitor exited ({exit:?}) leaving {} owned member(s) alive",
                members.len()
            )));
        }
        Ok(exit)
    }
    async fn remaining_members(&self) -> Result<Vec<ProcessIdentity>, BoundaryError> {
        self.inner.remaining_members().await
    }
}

/// Bounded-kill and reap a spawned backend group so no error path leaks a process. Kills the
/// whole group (`killpg`), then reaps the leader under a bounded timeout. Returns `Err` if the
/// reap is NOT PROVEN (wait failed or timed out) — an unproven cleanup must dominate whatever
/// error triggered it, never be silently ignored.
async fn kill_and_reap(child: &mut tokio::process::Child, pgid: i32) -> Result<(), BoundaryError> {
    use nix::sys::signal::{killpg, Signal};
    use nix::unistd::Pid;
    if pgid > 0 {
        let _ = killpg(Pid::from_raw(pgid), Signal::SIGKILL);
    }
    let _ = child.start_kill();
    match tokio::time::timeout(std::time::Duration::from_secs(2), child.wait()).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => Err(BoundaryError::Wait(e)),
        Err(_) => Err(BoundaryError::Signal(
            "backend cleanup did not complete: reap timed out".to_owned(),
        )),
    }
}

#[async_trait]
impl ProcessBoundary for SandboyBoundary {
    async fn spawn(&self, spec: BoundarySpawnSpec) -> Result<BoundaryLaunch, BoundaryError> {
        use std::io::{Read as _, Write as _};
        use std::os::fd::AsRawFd as _;

        if !cfg!(target_os = "linux") {
            return Err(BoundaryError::UnsupportedPlatform);
        }
        // Fail closed BEFORE anything if the ORIGINAL target path is not permitted by the policy.
        if !self.policy.permits_exec(&spec.executable) {
            return Err(BoundaryError::Spawn(io::Error::other(format!(
                "sandbox policy does not permit executing {}; refusing to launch",
                spec.executable.display()
            ))));
        }
        // Mint the launch nonce from the OS CSPRNG BEFORE any backend spawn; an RNG failure
        // fails closed here — never a timestamp/counter fallback.
        let nonce = self
            .nonce_source
            .mint()
            .map_err(|e| BoundaryError::Spawn(io::Error::other(e.to_string())))?;
        // Acquire the TARGET into a sealed memfd: the parent seals exactly the object it will
        // hand the backend and binds the launch to its digest, closing the hash→exec TOCTOU.
        let target = SealedObject::stage(&spec.executable)
            .map_err(|e| BoundaryError::Spawn(io::Error::other(format!("target: {e}"))))?;
        let request = self
            .launch_request(&spec, target.digest().clone(), &nonce)
            .map_err(|e| BoundaryError::Evidence(e.to_string()))?;
        let request_bytes = o7_sandbox_protocol::request::encode(&request)
            .map_err(|e| BoundaryError::Evidence(e.to_string()))?;
        let launch_spec_digest = request.spec_digest();

        // Three channels. The BACKEND end of each stays inheritable (non-CLOEXEC); the PARENT
        // end is marked CLOEXEC so it never leaks into the backend.
        //   report : backend WRITES, parent READS
        //   request: parent WRITES, backend READS
        //   control: parent WRITES GO, backend READS
        let (report_r, report_w) = make_pipe()?;
        let (request_r, request_w) = make_pipe()?;
        let (control_r, control_w) = make_pipe()?;
        for parent_end in [&report_r, &request_w, &control_w] {
            set_cloexec(parent_end)?;
        }

        let mut arguments: Vec<OsString> = vec![
            OsString::from("run"),
            OsString::from("--report-fd"),
            OsString::from(report_w.as_raw_fd().to_string()),
            OsString::from("--control-fd"),
            OsString::from(control_r.as_raw_fd().to_string()),
            OsString::from("--request-fd"),
            OsString::from(request_r.as_raw_fd().to_string()),
            OsString::from("--launch-nonce"),
            OsString::from(nonce.as_str()),
        ];
        arguments.extend(self.policy_flags());
        arguments.push(OsString::from("--"));
        // The backend execs the SEALED target object, not the caller's mutable path.
        arguments.push(target.descriptor_path().into_os_string());

        let mut cmd = tokio::process::Command::new(self.backend.descriptor_path());
        cmd.args(&arguments)
            .current_dir("/")
            .env_clear()
            .envs(self.backend_environment())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0)
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(BoundaryError::Spawn)?;
        // The backend now holds its inherited ends; the parent drops its copies of them.
        drop(report_w);
        drop(request_r);
        drop(control_r);

        let pid = match signalable_pid(child.id()) {
            Ok(p) => p,
            Err(e) => {
                // Cleanup failure DOMINATES the original error.
                kill_and_reap(&mut child, -1).await?;
                return Err(e);
            }
        };
        let identity = ProcessIdentity::read(pid).unwrap_or(ProcessIdentity {
            pid,
            process_group: pid,
            start_time_ticks: 0,
        });

        // Write the bounded request, then read exactly one bounded report frame AND prove it is
        // the ENTIRE message: after the declared body, one more read must be EOF. The backend
        // closes its report writer after the single frame (BEFORE it blocks on GO), so this is a
        // deterministic end-of-message — not a racy nonblocking peek. A trailing byte is a
        // protocol violation and fails closed. Offloaded so the blocking pipe ops do not stall the
        // runtime, and wrapped in a wall-clock bound so a stalled backend cannot hang the launch.
        let mut req_file = std::fs::File::from(request_w);
        let mut rep_file = std::fs::File::from(report_r);
        let read_report = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
            req_file.write_all(&request_bytes)?;
            drop(req_file); // close so the backend's request read completes
            let mut len_buf = [0u8; 4];
            rep_file.read_exact(&mut len_buf)?;
            let len = u32::from_be_bytes(len_buf) as usize;
            if len > o7_sandbox_protocol::frame::MAX_REPORT_BYTES {
                return Err(std::io::Error::other(
                    "report frame body exceeds the maximum",
                ));
            }
            let mut body = vec![0u8; len];
            rep_file.read_exact(&mut body)?;
            // Confirm end-of-message: the report channel must be at EOF now. A blocking read (the
            // writer is still open until the backend closes it after the frame) returns 0 on a
            // clean close, or >0 if the backend appended a trailing byte.
            let mut extra = [0u8; 1];
            match rep_file.read(&mut extra)? {
                0 => {}
                _ => {
                    return Err(std::io::Error::other(
                        "trailing bytes after the report frame",
                    ))
                }
            }
            let mut frame = len_buf.to_vec();
            frame.extend_from_slice(&body);
            Ok(frame)
        });
        let io = tokio::time::timeout(std::time::Duration::from_secs(10), read_report).await;
        let report_bytes = match io {
            Ok(Ok(Ok(bytes))) => bytes,
            _ => {
                // The backend never delivered a well-formed single frame (it exited first, stalled,
                // or appended trailing bytes). Fail closed; cleanup failure dominates.
                kill_and_reap(&mut child, pid).await?;
                return Err(BoundaryError::Evidence(
                    "backend did not deliver a well-formed report frame".to_owned(),
                ));
            }
        };

        // Verify every binding. On failure: withhold GO (drop control_w → the backend reads EOF
        // and never starts the target) and reap the backend. Fail closed.
        let evidence = match self.verify_report(&report_bytes, &nonce, &launch_spec_digest) {
            Ok(ev) => ev,
            Err(e) => {
                drop(control_w);
                kill_and_reap(&mut child, pid).await?;
                return Err(BoundaryError::Evidence(e.to_string()));
            }
        };

        // Authorized: send GO, then hand back the live monitor as the owned group leader.
        let mut ctl = std::fs::File::from(control_w);
        if ctl.write_all(b"G").is_err() {
            kill_and_reap(&mut child, pid).await?;
            return Err(BoundaryError::Evidence(
                "failed to authorize the target (GO)".to_owned(),
            ));
        }
        drop(ctl);

        Ok(BoundaryLaunch {
            process: Box::new(SandboyMonitorProcess {
                inner: HostBoundaryProcess::new(child, identity, pid),
                _backend: self.backend.clone(),
                _target: target,
            }),
            evidence,
        })
    }

    fn attestation(&self) -> BoundaryAttestation {
        BoundaryAttestation {
            implementation: BoundaryKind::Sandboy,
            enforcement: EnforcementLevel::FullyEnforced,
        }
    }
}
