//! The boundary-integrated verifier runner.
//!
//! `verify` runs a trusted command through a caller-supplied [`ProcessBoundary`] and
//! returns [`VerifierEvidence`]. It NEVER constructs its own boundary and NEVER falls
//! back: a production verifier is built with [`Verifier::production`], which requires
//! `RequireFullyEnforced`, so if the supplied boundary does not attest `FullyEnforced`
//! the run fails closed BEFORE spawn (`BoundaryUnavailable`). Until a fully-enforced
//! boundary (Sandboy) exists, production execution is therefore unavailable by
//! construction.
//!
//! Ordering of the pre-spawn gates: validate the command, ACQUIRE the source bytes under a
//! hardened open (`O_NOFOLLOW`, proven a regular file, size-capped), STAGE them into a
//! fully-sealed anonymous `memfd`, then bind trust to the bytes read BACK from that sealed
//! object and require the command to be trusted (an unsafe/unreadable acquisition, a staging
//! failure, or an untrusted command ⇒ NotRun, never spawned), then check the boundary
//! requirement. Only then is anything spawned. The run is bounded by the command timeout; a
//! timeout cancels the worker, which tears down the WHOLE owned process set, and is reported
//! as `TimedOut` (never a pass). Output is bounded by the command's budget; exceeding it
//! fails the sink closed and is reported as `OutputLost` (never a silent truncation, never a
//! pass).
//!
//! ACQUISITION and EXECUTION are both fd-exact against a same-UID attacker, and TRUST is
//! bound to the bytes that will actually execute. Acquisition opens the trusted executable
//! `O_NOFOLLOW`, proves it a regular file, and reads its exact bytes. Those bytes are written
//! into a fully-SEALED, anonymous `memfd` (`F_SEAL_WRITE | GROW | SHRINK | SEAL`), and the
//! command is executed through a `/proc/<pid>/fd/<n>` path backed by a held-open descriptor.
//! Because the object is anonymous (no directory entry to swap) and immutable (writes are
//! sealed even for the owner, and the seal cannot be removed), and because the kernel resolves
//! the exec path THROUGH the descriptor to that object (the mechanism glibc's `fexecve` uses),
//! the bytes executed cannot be swapped or rewritten by a same-UID attacker.
//!
//! Crucially, the trust decision is made over the bytes READ BACK from the sealed memfd — not
//! over the source buffer that was written before sealing. There is an unavoidable window
//! between `write` and `F_ADD_SEALS` in which a same-UID process could open the still-unsealed
//! memfd (via `/proc/<pid>/fd`) and overwrite it; sealing would then freeze the attacker's
//! bytes. Binding trust to the post-seal read-back closes that pre-seal race: any such
//! mutation changes the sealed content, so the recomputed trust digest is not in the store and
//! the command is NotRun — never spawned. The bytes hashed for trust are exactly the immutable
//! bytes that run. This uses no `unsafe` (`memfd_create`/`F_ADD_SEALS`/`F_GET_SEALS` via
//! rustix's safe wrappers) and does not change the frozen o7-worker `ProcessBoundary`/
//! `BoundarySpawnSpec` seam, which still spawns from a PATH. (Requires `/proc` mounted;
//! otherwise the run fails closed at spawn — see [`stage_executable`].)

use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use o7_worker::{
    BoundaryRequirement, CancellationPolicy, HeartbeatPolicy, ObservationError, ObservationSink,
    OutputPolicy, OutputStream, ProcessBoundary, StdinMode, WorkerId, WorkerObservation,
    WorkerResult, WorkerSpec, WorkerSupervisor,
};
use o7_worktree::CanonicalRepoId;

use crate::command::{CwdPolicy, TrustedCommand};
use crate::evidence::{AttestedEnforcement, VerifierEvidence, VerifierOutcome};
use crate::trust::{
    structural_command_digest, CommandDigest, ExecutableIdentity, TrustAnchor, TrustStore,
};

/// A verifier, parameterized by the boundary requirement it enforces.
#[derive(Debug, Clone, Copy)]
pub struct Verifier {
    requirement: BoundaryRequirement,
}

/// Reserved for future infrastructure failures; `verify` itself is infallible and
/// always yields evidence (a failure is an OUTCOME, never a lost result).
#[derive(Debug, thiserror::Error)]
pub enum VerifyError {}

impl Verifier {
    /// A production verifier: requires a `FullyEnforced` boundary, no fallback.
    #[must_use]
    pub fn production() -> Self {
        Self {
            requirement: BoundaryRequirement::RequireFullyEnforced,
        }
    }

    /// Build a verifier with an explicit requirement. Use `AllowUnconfined` ONLY in
    /// tests / non-production tooling — a real provider verification must use
    /// [`Verifier::production`].
    #[must_use]
    pub fn with_requirement(requirement: BoundaryRequirement) -> Self {
        Self { requirement }
    }

    /// Run `command` through `boundary` and return evidence. Infallible: every failure
    /// mode is a [`VerifierOutcome`], never a lost result.
    pub async fn verify(
        &self,
        boundary: Box<dyn ProcessBoundary>,
        repo: &CanonicalRepoId,
        worktree_root: &Path,
        command: &TrustedCommand,
        trust: &TrustStore,
    ) -> VerifierEvidence {
        self.verify_seamed(boundary, repo, worktree_root, command, trust, |_| {})
            .await
    }

    /// The core of [`Verifier::verify`], with a test seam `pre_seal` invoked on the writable
    /// memfd handle AFTER the source bytes are written but BEFORE the seals are applied.
    ///
    /// In production `pre_seal` is a no-op. A test uses it to model a SAME-UID attacker
    /// mutating the still-unsealed memfd (via `/proc/<pid>/fd`) in the pre-seal window, to
    /// prove that trust — bound to the bytes read BACK from the sealed object — detects the
    /// mutation (unrecognized digest ⇒ NotRun) and NEVER spawns. The seam is the only way to
    /// interpose deterministically in that otherwise-racy window.
    async fn verify_seamed<F>(
        &self,
        boundary: Box<dyn ProcessBoundary>,
        repo: &CanonicalRepoId,
        worktree_root: &Path,
        command: &TrustedCommand,
        trust: &TrustStore,
        pre_seal: F,
    ) -> VerifierEvidence
    where
        F: FnOnce(&std::fs::File),
    {
        let structural = structural_command_digest(repo, command);
        let attestation = boundary.attestation();
        let enforcement = Some(AttestedEnforcement::from(attestation.enforcement));
        let bound_req: BoundaryRequirement = command.boundary_requirement.into();

        // 0. The verifier's configured requirement must MATCH the command's trust-bound
        //    requirement — a production verifier never runs a command bound to a weaker
        //    boundary, and never silently strengthens one bound to a stronger boundary.
        if bound_req != self.requirement {
            return not_run(
                repo,
                command,
                &structural,
                None,
                None,
                enforcement,
                false,
                format!(
                    "verifier requirement {:?} does not match the command's trust-bound \
                     requirement {bound_req:?}",
                    self.requirement
                ),
            );
        }

        // 1. Command shape.
        if let Err(err) = command.validate() {
            return not_run(
                repo,
                command,
                &structural,
                None,
                None,
                enforcement,
                false,
                format!("invalid command: {err}"),
            );
        }

        // 2. ACQUIRE the source bytes under a hardened open (O_NOFOLLOW, proven regular file,
        //    size-capped, drift-checked). An unsafe or unreadable acquisition means NotRun.
        let source_bytes = match acquire_executable(&command.executable) {
            Ok(bytes) => bytes,
            Err(reason) => {
                return not_run(
                    repo,
                    command,
                    &structural,
                    None,
                    None,
                    enforcement,
                    false,
                    format!("command executable could not be safely acquired: {reason}"),
                );
            }
        };

        // 3. STAGE the source bytes into a fully-sealed anonymous memfd — BEFORE the trust
        //    check, so trust can bind to the immutable object rather than the source buffer.
        //    `pre_seal` (a no-op in production) is invoked in the pre-seal window; sealing
        //    then freezes whatever content the memfd holds and the seals are verified present.
        let staged = match stage_executable_seamed(&source_bytes, pre_seal) {
            Ok(staged) => staged,
            Err(err) => {
                return not_run(
                    repo,
                    command,
                    &structural,
                    None,
                    None,
                    enforcement,
                    false,
                    format!("failed to stage the trusted executable: {err}"),
                );
            }
        };

        // 4. Trust — bind it to the bytes READ BACK from the SEALED memfd (the exact immutable
        //    bytes that will execute), not the source buffer. Any pre-seal mutation therefore
        //    changes the sealed content and yields a different, untrusted digest ⇒ NotRun. An
        //    untrusted command (or an unreadable sealed object) is never run.
        let sealed_bytes = match staged.read_back() {
            Ok(bytes) => bytes,
            Err(err) => {
                return not_run(
                    repo,
                    command,
                    &structural,
                    None,
                    None,
                    enforcement,
                    false,
                    format!("failed to read back the sealed executable: {err}"),
                );
            }
        };
        let anchor = TrustAnchor::for_executable_bytes(repo, command, &sealed_bytes);
        let exe_identity = anchor.executable_identity.clone();
        let trust_digest = anchor.digest().clone();
        if !trust.is_trusted(&anchor) {
            return not_run(
                repo,
                command,
                &structural,
                Some(exe_identity),
                Some(trust_digest),
                enforcement,
                false,
                "command is not trusted for this repository".to_owned(),
            );
        }

        // 5. Boundary requirement — fail closed BEFORE spawn, no fallback.
        if !self.requirement.is_satisfied_by(&attestation) {
            return evidence_of(
                VerifierOutcome::BoundaryUnavailable {
                    reason: format!(
                        "required {:?}, boundary attests {:?}; no fallback",
                        self.requirement, attestation.enforcement
                    ),
                },
                repo,
                command,
                &structural,
                Some(exe_identity),
                Some(trust_digest),
                enforcement,
                true,
                Vec::new(),
                Vec::new(),
            );
        }

        // 6. Spawn and run under the boundary, bounded by the timeout. The staged memfd is
        //    executed through its /proc fd path and dropped after the run.
        let cwd = match &command.cwd_policy {
            CwdPolicy::WorktreeRoot => worktree_root.to_path_buf(),
            CwdPolicy::Absolute(path) => path.clone(),
        };
        let spec = WorkerSpec {
            worker_id: WorkerId::new(format!("verify-{}", structural.as_str())),
            executable: staged.path().to_path_buf(),
            arguments: command.arguments.clone(),
            working_directory: cwd,
            environment: command.environment.clone(),
            stdin: StdinMode::Null,
            output: OutputPolicy::default(),
            // After a timeout the verifier has already given up, so teardown is
            // deliberately aggressive: a short graceful window, then force-kill the
            // whole owned set. (The worker still proves the set gone before finishing.)
            cancellation: CancellationPolicy {
                graceful_timeout: std::time::Duration::from_millis(500),
            },
            heartbeat: HeartbeatPolicy {
                enabled: false,
                interval: std::time::Duration::from_secs(1),
            },
            boundary_requirement: self.requirement,
        };

        let sink = Arc::new(CollectingSink::new(command.output_limits.max_total_bytes));
        let sink_dyn: Arc<dyn ObservationSink> = sink.clone();
        let (handle, join) = WorkerSupervisor::start(spec, boundary, sink_dyn);

        let join_fut = join.join();
        tokio::pin!(join_fut);
        let (result, timed_out) = match tokio::time::timeout(command.timeout, &mut join_fut).await {
            Ok(result) => {
                // Natural completion — hold the handle so nothing is cancelled.
                drop(handle);
                (result, false)
            }
            Err(_) => {
                // Timeout: cancel drives full teardown (SIGTERM→SIGKILL the whole
                // owned process set, then prove it gone). Then resume awaiting the
                // SAME join future for the terminal result.
                handle.cancel().await;
                let result = join_fut.await;
                (result, true)
            }
        };

        // The staged executable was needed for the whole run; remove it now (the child
        // has exited). Kept explicit so the lifetime is obvious.
        drop(staged);

        let (stdout, stderr, budget_exceeded) = sink.snapshot();
        let outcome = map_outcome(&result, timed_out, budget_exceeded);
        evidence_of(
            outcome,
            repo,
            command,
            &structural,
            Some(exe_identity),
            Some(trust_digest),
            enforcement,
            true,
            stdout,
            stderr,
        )
    }
}

/// Assemble a [`VerifierEvidence`] carrying the full trust binding so adjudication can
/// re-derive and check the trust digest.
#[allow(clippy::too_many_arguments)]
fn evidence_of(
    outcome: VerifierOutcome,
    repo: &CanonicalRepoId,
    command: &TrustedCommand,
    structural: &CommandDigest,
    executable_identity: Option<ExecutableIdentity>,
    trust_digest: Option<CommandDigest>,
    enforcement: Option<AttestedEnforcement>,
    trusted: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
) -> VerifierEvidence {
    VerifierEvidence {
        outcome,
        trusted,
        boundary_enforcement: enforcement,
        repo: repo.clone(),
        command: command.clone(),
        executable_identity,
        trust_digest,
        structural_digest: structural.clone(),
        stdout,
        stderr,
    }
}

#[allow(clippy::too_many_arguments)]
fn not_run(
    repo: &CanonicalRepoId,
    command: &TrustedCommand,
    structural: &CommandDigest,
    executable_identity: Option<ExecutableIdentity>,
    trust_digest: Option<CommandDigest>,
    enforcement: Option<AttestedEnforcement>,
    trusted: bool,
    reason: String,
) -> VerifierEvidence {
    evidence_of(
        VerifierOutcome::NotRun { reason },
        repo,
        command,
        structural,
        executable_identity,
        trust_digest,
        enforcement,
        trusted,
        Vec::new(),
        Vec::new(),
    )
}

/// Hard ceiling on the size of a trusted executable we will read into memory (and stage).
/// A generous bound for any real verifier binary; anything larger is refused rather than
/// buffered without limit.
const MAX_EXECUTABLE_BYTES: u64 = 256 * 1024 * 1024;

/// Safely ACQUIRE the trusted executable's bytes.
///
/// The path is opened `O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC` (so a symlink at the final
/// component fails closed and opening a FIFO/device never blocks), the OPEN descriptor is
/// proven to be a REGULAR file (never a FIFO, character/block device, socket, directory,
/// or pseudo-file such as a procfs/sysfs entry), and the content is read from that same
/// descriptor under a size cap with a drift check. Reading from the proven-regular
/// descriptor (not re-opening the path) means the bytes returned are the bytes that were
/// verified — the same bytes the runner then hashes and stages.
fn acquire_executable(path: &Path) -> Result<Vec<u8>, String> {
    use rustix::fs::{self, Mode, OFlags};
    use std::io::Read as _;

    let fd = fs::open(
        path,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|err| {
        if err == rustix::io::Errno::LOOP {
            "the path is a symlink (O_NOFOLLOW)".to_owned()
        } else {
            format!("open failed: {err}")
        }
    })?;

    let st = fs::fstat(&fd).map_err(|err| format!("fstat failed: {err}"))?;
    // Must be a REGULAR file. This rejects FIFOs, char/block devices, sockets, and
    // directories outright.
    if (st.st_mode as u32 & 0o170_000) != 0o100_000 {
        return Err("not a regular file (fifo, device, socket, or directory)".to_owned());
    }
    // A real executable has a positive, bounded size. A zero size is characteristic of a
    // procfs/sysfs pseudo-file (which reports size 0 yet yields content), so reject it.
    let size = u64::try_from(st.st_size).map_err(|_| "negative file size".to_owned())?;
    if size == 0 {
        return Err("empty or pseudo-file (reported size 0)".to_owned());
    }
    if size > MAX_EXECUTABLE_BYTES {
        return Err(format!(
            "executable is {size} bytes, over the {MAX_EXECUTABLE_BYTES}-byte cap"
        ));
    }

    // Read from the proven descriptor under a cap of size+1, so a file that GREW between
    // stat and read (or a pseudo-file whose content exceeds its reported size) is caught.
    let mut file = std::fs::File::from(fd);
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    let read = file
        .by_ref()
        .take(size + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| format!("read failed: {err}"))?;
    let read = read as u64;
    if read != size {
        return Err(format!(
            "size drift: stat reported {size} bytes but the file yielded {read}"
        ));
    }
    Ok(bytes)
}

/// The trusted executable bytes materialized as a SEALED, anonymous `memfd`, plus a
/// held-open read-only descriptor to it. The command is run through a magic
/// `/proc/<pid>/fd/<n>` path, so it is executed FD-EXACTLY *and* the backing object is
/// immutable: there is no on-disk directory entry to swap, and the bytes cannot be changed
/// by ANYONE — not even the owning UID — because `F_SEAL_WRITE` is in force and
/// `F_SEAL_SEAL` forbids removing it. The descriptor is held for the whole run.
struct StagedExecutable {
    /// Held open so `/proc/<pid>/fd/<n>` keeps resolving to the sealed memfd during exec.
    _fd: std::fs::File,
    proc_path: std::path::PathBuf,
}

impl StagedExecutable {
    fn path(&self) -> &Path {
        &self.proc_path
    }

    /// Read the bytes back from the SEALED memfd through the held descriptor. Trust is bound
    /// to THESE bytes — the immutable object that will actually execute — so a pre-seal
    /// mutation (which sealing would otherwise freeze in place) changes what is trusted and
    /// falls out of the store. The descriptor is read-only and freshly opened at offset 0,
    /// and the object is sealed (no growth), so this yields exactly the executed bytes.
    fn read_back(&self) -> std::io::Result<Vec<u8>> {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        // `impl Read for &File` reads without needing a mutable handle; the fd stays held
        // for exec afterwards (exec resolves the /proc path independently of file position).
        (&self._fd).read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

/// Materialize `bytes` as a fully-sealed, anonymous `memfd` and return a
/// `/proc/<pid>/fd/<n>` path to execute it through.
///
/// This closes the hash-to-spawn TOCTOU even against a SAME-UID attacker. A regular staged
/// file is not enough: the owning UID can `chmod` its own file back to writable and rewrite
/// the content in place, so a held read-only fd would then read mutated bytes. A memfd
/// sealed with `F_SEAL_WRITE | F_SEAL_GROW | F_SEAL_SHRINK | F_SEAL_SEAL` cannot be written,
/// resized, or un-sealed by anyone (not even the owner), and has no filesystem path to
/// swap. Executing `/proc/<pid>/fd/<n>` makes the kernel resolve the program THROUGH the
/// held descriptor to that immutable object (the mechanism glibc's `fexecve` uses), so the
/// bytes executed are exactly the bytes hashed.
///
/// The seals are added while a writable handle is open (no writable mmap exists, so
/// `F_SEAL_WRITE` succeeds); a read-only descriptor is then taken for exec and the writable
/// handle dropped, so the object has no writable open at exec time (no `ETXTBSY`).
///
/// RUNTIME PREREQUISITE: `/proc` must be mounted. On a host without it the run fails closed
/// at spawn (`SpawnFailed`) rather than executing unverified bytes.
///
/// `pre_seal` is invoked on the writable memfd handle AFTER the bytes are written but BEFORE
/// the seals are applied. In production it is a no-op; the runner's trust check binds to the
/// bytes read back from the SEALED object (see [`StagedExecutable::read_back`]), so any
/// mutation performed by the seam changes the sealed content and is caught. The seam exists to
/// make the otherwise-racy pre-seal window deterministically testable.
fn stage_executable_seamed<F>(bytes: &[u8], pre_seal: F) -> std::io::Result<StagedExecutable>
where
    F: FnOnce(&std::fs::File),
{
    use std::io::Write as _;
    use std::os::fd::AsRawFd as _;

    use rustix::fs::{fcntl_add_seals, fcntl_get_seals, memfd_create, MemfdFlags, SealFlags};

    // Anonymous, sealable, memory-backed file — no on-disk inode to chmod or replace.
    let memfd = memfd_create(
        "o7-verify-exe",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )
    .map_err(std::io::Error::from)?;
    let mut rw = std::fs::File::from(memfd);
    rw.write_all(bytes)?;
    rw.flush()?;

    // Pre-seal window. In production this is a no-op; a test interposes here to model a
    // same-UID attacker mutating the still-unsealed memfd. Trust binds to the post-seal
    // read-back, so any mutation here is detected — never trusted, never spawned.
    pre_seal(&rw);

    // Seal it fully immutable: no writes (even by the owner), no size change, and no future
    // change to the seals themselves. From here the bytes can never change.
    let required = SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL;
    fcntl_add_seals(&rw, required).map_err(std::io::Error::from)?;

    // Prove the required seals are actually in force before trusting or executing the object.
    // A sealing that silently did not take hold must fail closed, not run mutable bytes.
    let seals = fcntl_get_seals(&rw).map_err(std::io::Error::from)?;
    if !seals.contains(required) {
        return Err(std::io::Error::other(
            "required memfd seals are not in force after sealing",
        ));
    }

    // Take a READ-ONLY descriptor for exec (re-open the memfd via /proc/self/fd), then drop
    // the writable handle so the object has no writable open at exec time.
    let ro = std::fs::OpenOptions::new()
        .read(true)
        .open(format!("/proc/self/fd/{}", rw.as_raw_fd()))?;
    drop(rw);

    let proc_path = std::path::PathBuf::from(format!(
        "/proc/{}/fd/{}",
        std::process::id(),
        ro.as_raw_fd()
    ));
    Ok(StagedExecutable { _fd: ro, proc_path })
}

/// Map the worker's terminal result to a verifier outcome. A timeout is `TimedOut`
/// (unless teardown could not be proven, which is a worse `Faulted`). A budget-exceeded
/// sink is `OutputLost`. Every non-clean-exit is a non-completion — never a pass.
fn map_outcome(result: &WorkerResult, timed_out: bool, budget_exceeded: bool) -> VerifierOutcome {
    if timed_out {
        return match result {
            WorkerResult::CleanupFailure(m) => VerifierOutcome::Faulted {
                reason: format!("timeout teardown could not be proven: {m}"),
            },
            _ => VerifierOutcome::TimedOut,
        };
    }
    if budget_exceeded {
        // The sink failed closed at the budget; whatever terminal the worker produced,
        // output faithfulness was lost.
        return VerifierOutcome::OutputLost {
            reason: "verifier output exceeded the configured budget".to_owned(),
        };
    }
    match result {
        WorkerResult::ExitedNormally(code) => VerifierOutcome::Completed { exit_code: *code },
        WorkerResult::ExitedBySignal(signal) => VerifierOutcome::Signalled { signal: *signal },
        // The command was pre-validated and the boundary requirement pre-checked, so a
        // FailedToStart reaching here is a spawn failure (e.g. a missing executable).
        WorkerResult::FailedToStart(m) => VerifierOutcome::SpawnFailed { reason: m.clone() },
        WorkerResult::OutputFailure(m) => VerifierOutcome::OutputLost { reason: m.clone() },
        WorkerResult::ObservationFailure(m) | WorkerResult::BoundaryFailure(m) => {
            VerifierOutcome::Faulted { reason: m.clone() }
        }
        WorkerResult::CleanupFailure(m) => VerifierOutcome::Faulted { reason: m.clone() },
        // We only cancel on timeout (handled above); a cancel here is unexpected.
        WorkerResult::CancelledGracefully | WorkerResult::CancelledForcefully => {
            VerifierOutcome::Faulted {
                reason: "unexpected cancellation".to_owned(),
            }
        }
    }
}

/// The authoritative sink for a verifier run: it accumulates stdout/stderr up to the
/// output budget and FAILS CLOSED (returns `Err`, which the supervisor treats as fatal)
/// once the budget is exceeded, so output loss is a failure — never a silent truncation.
struct CollectingSink {
    budget: usize,
    inner: Mutex<Collected>,
}

#[derive(Default)]
struct Collected {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    total: usize,
    budget_exceeded: bool,
}

impl CollectingSink {
    fn new(budget: usize) -> Self {
        Self {
            budget,
            inner: Mutex::new(Collected::default()),
        }
    }

    fn snapshot(&self) -> (Vec<u8>, Vec<u8>, bool) {
        match self.inner.lock() {
            Ok(g) => (g.stdout.clone(), g.stderr.clone(), g.budget_exceeded),
            Err(_) => (Vec::new(), Vec::new(), true),
        }
    }
}

#[async_trait]
impl ObservationSink for CollectingSink {
    async fn publish(&self, observation: WorkerObservation) -> Result<(), ObservationError> {
        let WorkerObservation::OutputChunk(chunk) = observation else {
            return Ok(());
        };
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| ObservationError("verifier sink mutex poisoned".to_owned()))?;
        let new_total = guard.total.saturating_add(chunk.bytes.len());
        if new_total > self.budget {
            guard.budget_exceeded = true;
            return Err(ObservationError(format!(
                "verifier output exceeded the {}-byte budget",
                self.budget
            )));
        }
        guard.total = new_total;
        match chunk.stream {
            OutputStream::Stdout => guard.stdout.extend_from_slice(&chunk.bytes),
            OutputStream::Stderr => guard.stderr.extend_from_slice(&chunk.bytes),
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Content-immutability proof against a SAME-UID attacker. The staged executable is a
    /// sealed memfd, so the exact same-UID mutation that defeats a regular owned file —
    /// chmod it writable, re-open for writing via /proc/<pid>/fd, and overwrite in place —
    /// fails, and the bytes behind the exec path are unchanged. This closes the residual
    /// hash-to-spawn TOCTOU that a plain read-only fd to a regular file leaves open.
    #[test]
    fn a_sealed_memfd_cannot_be_mutated_by_the_owning_uid() {
        let staged = stage_executable_seamed(b"ORIGINAL-BYTES", |_| {}).expect("stage");
        let proc_path = staged.path().to_path_buf();

        // Same-UID owner tries to make it writable and overwrite it in place.
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&proc_path, std::fs::Permissions::from_mode(0o700));
        let overwrite = std::fs::OpenOptions::new()
            .write(true)
            .open(&proc_path)
            .and_then(|mut f| {
                use std::io::Write as _;
                f.write_all(b"EVIL-REPLACEMENT-CONTENT")
            });
        assert!(
            overwrite.is_err(),
            "a sealed memfd was mutated by its owning UID: {overwrite:?}"
        );

        // The bytes behind the exec path are unchanged.
        let via_fd = std::fs::read(&proc_path).expect("read via /proc fd");
        assert_eq!(
            via_fd, b"ORIGINAL-BYTES",
            "the sealed executable's bytes changed under a same-UID write attempt"
        );
    }

    /// The staged executable is anonymous: its `/proc/<pid>/fd` target is a `memfd:` object
    /// with no directory entry an attacker could rename or replace.
    #[test]
    fn the_staged_executable_is_an_anonymous_memfd() {
        let staged = stage_executable_seamed(b"payload", |_| {}).expect("stage");
        let target = std::fs::read_link(staged.path()).expect("readlink /proc fd");
        let shown = target.to_string_lossy();
        assert!(
            shown.contains("memfd:") && shown.contains("o7-verify-exe"),
            "expected an anonymous memfd target, got {shown:?}"
        );
    }

    /// A pre-seal mutation of the sealed bytes is bound out and never spawned.
    ///
    /// This attacks the ONLY residual window: between `write` and `F_ADD_SEALS` a same-UID
    /// process can open the still-unsealed memfd (via `/proc/<pid>/fd`) and overwrite it;
    /// sealing then freezes the attacker's bytes. Because trust binds to the bytes read BACK
    /// from the SEALED object (not the source buffer), the recomputed digest is the attacker's
    /// — absent from the store — so the outcome is `NotRun` and the boundary is NEVER asked to
    /// spawn. The seam injects exactly that pre-seal mutation, deterministically.
    #[tokio::test]
    async fn a_pre_seal_mutation_of_the_sealed_bytes_is_bound_out_and_never_spawned() {
        use std::collections::BTreeMap;
        use std::ffi::OsString;
        use std::sync::atomic::{AtomicBool, Ordering};

        use o7_worker::{
            BoundaryAttestation, BoundaryError, BoundaryKind, BoundaryProcess, BoundarySpawnSpec,
            EnforcementLevel,
        };

        use crate::command::{CwdPolicy, ExitPolicy, OutputLimits, RequiredBoundary};
        use crate::trust::TrustAnchor;

        // A boundary that flags any spawn attempt and refuses — nothing may reach it here.
        struct NeverSpawnBoundary {
            spawned: Arc<AtomicBool>,
        }
        #[async_trait]
        impl ProcessBoundary for NeverSpawnBoundary {
            async fn spawn(
                &self,
                _spec: BoundarySpawnSpec,
            ) -> Result<Box<dyn BoundaryProcess>, BoundaryError> {
                self.spawned.store(true, Ordering::SeqCst);
                Err(BoundaryError::Spawn(std::io::Error::other(
                    "spawn must not be reached: a mutated executable was about to run",
                )))
            }
            fn attestation(&self) -> BoundaryAttestation {
                BoundaryAttestation {
                    implementation: BoundaryKind::Sandboy,
                    enforcement: EnforcementLevel::FullyEnforced,
                }
            }
        }

        // A trusted executable on disk: trust is computed over its ORIGINAL bytes.
        let dir = tempfile::tempdir().expect("tempdir");
        let exe = dir.path().join("verify");
        std::fs::write(&exe, b"ORIGINAL-TRUSTED-BYTES").expect("write exe");
        let repo = CanonicalRepoId {
            git_common_dir: std::path::PathBuf::from("/srv/repo/.git"),
            dev: 66,
            ino: 4242,
        };
        let mut environment = BTreeMap::new();
        environment.insert(OsString::from("PATH"), OsString::from("/usr/bin:/bin"));
        let command = TrustedCommand {
            executable: exe,
            arguments: vec![OsString::from("--verify")],
            cwd_policy: CwdPolicy::WorktreeRoot,
            environment,
            timeout: std::time::Duration::from_secs(5),
            output_limits: OutputLimits {
                max_total_bytes: 1 << 20,
            },
            exit_policy: ExitPolicy::exactly_zero(),
            boundary_requirement: RequiredBoundary::RequireFullyEnforced,
        };
        // Trust the ORIGINAL bytes (what an operator hashed and approved).
        let mut trust = TrustStore::new();
        trust.trust(&TrustAnchor::compute(&repo, &command).expect("compute anchor"));

        let spawned = Arc::new(AtomicBool::new(false));
        let boundary = Box::new(NeverSpawnBoundary {
            spawned: Arc::clone(&spawned),
        });
        let root = tempfile::tempdir().expect("root");

        // Same-UID attacker overwrites the still-unsealed memfd in the pre-seal window.
        let ev = Verifier::production()
            .verify_seamed(
                boundary,
                &repo,
                root.path(),
                &command,
                &trust,
                |rw: &std::fs::File| {
                    use std::io::Write as _;
                    use std::os::fd::AsRawFd as _;
                    if let Ok(mut attacker) = std::fs::OpenOptions::new()
                        .write(true)
                        .truncate(false)
                        .open(format!("/proc/self/fd/{}", rw.as_raw_fd()))
                    {
                        // Same length as ORIGINAL so it is a clean in-place overwrite; the
                        // hash differs regardless. This is the byte the seal will freeze.
                        let _ = attacker.write_all(b"EVIL-REPLACEMENT-BYTES");
                        let _ = attacker.flush();
                    }
                },
            )
            .await;

        // The sealed bytes are the attacker's, so trust (bound to the read-back) fails: NotRun.
        assert!(
            matches!(ev.outcome, VerifierOutcome::NotRun { .. }),
            "expected NotRun for a pre-seal-mutated executable, got {:?}",
            ev.outcome
        );
        assert!(!ev.trusted, "a mutated executable was reported trusted");
        // And the boundary was never asked to spawn — the mutated bytes never ran.
        assert!(
            !spawned.load(Ordering::SeqCst),
            "a pre-seal-mutated executable reached the boundary spawn"
        );
    }
}
