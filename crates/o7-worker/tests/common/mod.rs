//! Shared test scaffolding.
//!
//! Subprocess fixtures re-exec THIS test binary into the `#[ignore]`d
//! `worker_child_entry` test (dispatched by the `O7_WORKER_CHILD_MODE` env var),
//! exactly like PR 1's crash test — no shipped helper executable.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    unreachable_pub,
    dead_code
)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Notify;

use o7_worker::{
    BoundaryRequirement, CancellationPolicy, HeartbeatPolicy, ObservationError, ObservationSink,
    OutputPolicy, OutputStream, ProcessBoundary, ProcessIdentity, StdinMode,
    UnconfinedHostBoundary, WorkerId, WorkerJoin, WorkerObservation, WorkerResult, WorkerSpec,
    WorkerSupervisor,
};

pub mod mock;

/// The PR-2 host boundary as a trait object.
pub fn host_boundary() -> Box<dyn ProcessBoundary> {
    Box::new(UnconfinedHostBoundary)
}

/// Start a worker on an ARBITRARY boundary (e.g. a fault-injecting mock). [`start`]
/// uses the real host boundary; this drives the generic seam that the fault tests
/// exercise.
pub fn start_with(
    spec: WorkerSpec,
    boundary: Box<dyn ProcessBoundary>,
    sink: &RecordingSink,
) -> (o7_worker::WorkerHandle, WorkerJoin) {
    WorkerSupervisor::start(spec, boundary, sink.arc())
}

/// Start a worker on the host boundary. The handle is returned so the caller can
/// cancel/drop it deliberately.
pub fn start(spec: WorkerSpec, sink: &RecordingSink) -> (o7_worker::WorkerHandle, WorkerJoin) {
    WorkerSupervisor::start(spec, host_boundary(), sink.arc())
}

/// Run a worker to its natural terminal result (the handle is held until then, so
/// no cancellation is triggered).
pub async fn run_to_completion(spec: WorkerSpec, sink: &RecordingSink) -> WorkerResult {
    let (_handle, join) = start(spec, sink);
    join.join().await
}

/// Like [`run_to_completion`] but on an arbitrary boundary (e.g. a mock). The
/// handle is held until the terminal result, so nothing is cancelled — dropping it
/// early would request cancellation.
pub async fn run_with(
    spec: WorkerSpec,
    boundary: Box<dyn ProcessBoundary>,
    sink: &RecordingSink,
) -> WorkerResult {
    let (_handle, join) = start_with(spec, boundary, sink);
    join.join().await
}

/// Live members of process group `pgid`. `enumerate_group` is now authoritative
/// (`/proc` unreadable is an error, never "empty"); in tests `/proc` is always
/// readable, so a query error is itself a test failure — never a silent "empty".
pub fn group_members(pgid: i32) -> Vec<ProcessIdentity> {
    ProcessIdentity::enumerate_group(pgid).expect("/proc enumeration must succeed in tests")
}

/// Whether process group `pgid` has no live members.
pub fn group_is_empty(pgid: i32) -> bool {
    group_members(pgid).is_empty()
}

/// Whether `/proc/<pid>` exists AT ALL — a RAW existence check that does NOT go
/// through the live-members scan. A REAPED process has no `/proc/<pid>` entry; an
/// unreaped zombie still does (with state `Z`). Proving a direct child was reaped
/// therefore requires this raw check, because [`group_is_empty`] deliberately treats a
/// zombie as gone and so could not distinguish "reaped" from "zombie".
pub fn proc_pid_exists(pid: i32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

/// Poll (bounded) until `/proc/<pid>` disappears, returning whether it did. Used to
/// assert a direct child is genuinely REAPED, not merely killed-and-zombified.
pub async fn proc_pid_gone_within(pid: i32, timeout: Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    while proc_pid_exists(pid) {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    true
}

pub const ENV_MODE: &str = "O7_WORKER_CHILD_MODE";
pub const ENV_CODE: &str = "O7_WORKER_CHILD_CODE";
pub const ENV_PAYLOAD: &str = "O7_WORKER_CHILD_PAYLOAD";
pub const ENV_CHECK_VAR: &str = "O7_WORKER_CHECK_VAR";

pub const BEGIN: &[u8] = b"\x1e\x1e<<<O7BEGIN>>>\x1e\x1e";
pub const END: &[u8] = b"\x1e\x1e<<<O7END>>>\x1e\x1e";
/// A fixed non-UTF-8 payload used by the `print_nonutf8` child mode.
pub const NON_UTF8: &[u8] = &[0x00, 0xFF, 0xFE, 0x80, 0x41, 0x00, 0xC0];

/// Unique readiness markers a child fixture writes to stdout ONLY after a specific
/// precondition holds, so a test can await the real condition (via
/// [`RecordingSink::wait_for_stdout_contains`]) instead of assuming it after a fixed
/// sleep. They travel through the same byte-preserving stdout pipe as any other output.
pub const READY_GRANDCHILD: &[u8] = b"O7_READY_GRANDCHILD\n";
pub const READY_SIGTERM_HANDLER: &[u8] = b"O7_READY_SIGTERM_HANDLER\n";

/// Size of the `print_large` child payload — larger than the default 64 KiB
/// chunk so output is split across many chunks.
pub const LARGE_LEN: usize = 200_000;

/// Deterministic byte pattern the `print_large` child emits and the parent
/// reconstructs to check nothing is lost or reordered.
pub fn large_pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8).collect()
}

/// Build a spec that re-execs this test binary into `worker_child_entry` running
/// the given `mode`. The environment is minimal (the boundary clears the host env
/// anyway); tests add explicit vars as needed.
pub fn child_spec(worker_id: &str, mode: &str) -> WorkerSpec {
    child_spec_in(worker_id, mode, std::env::temp_dir())
}

pub fn child_spec_in(worker_id: &str, mode: &str, cwd: PathBuf) -> WorkerSpec {
    let exe = std::env::current_exe().expect("current_exe");
    let mut environment = BTreeMap::new();
    environment.insert(OsString::from(ENV_MODE), OsString::from(mode));
    WorkerSpec {
        worker_id: WorkerId::new(worker_id),
        executable: exe,
        arguments: [
            "--ignored",
            "--exact",
            "--nocapture",
            "common::worker_child_entry",
        ]
        .into_iter()
        .map(OsString::from)
        .collect(),
        working_directory: cwd,
        environment,
        stdin: StdinMode::Null,
        output: OutputPolicy::default(),
        cancellation: CancellationPolicy {
            graceful_timeout: Duration::from_millis(500),
        },
        heartbeat: HeartbeatPolicy {
            enabled: true,
            interval: Duration::from_millis(100),
        },
        boundary_requirement: BoundaryRequirement::AllowUnconfined,
    }
}

pub fn set_env(spec: &mut WorkerSpec, key: &str, value: &str) {
    spec.environment
        .insert(OsString::from(key), OsString::from(value));
}

// ---- recording sink ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailMode {
    Never,
    /// Fail the first time an `OutputChunk` is published (simulates a lost sink
    /// mid-run).
    OnFirstOutput,
    /// Fail the first time an observation of this exact kind is published (see
    /// [`observation_kind`]) — e.g. `"exited"`, `"cleanup_completed"`,
    /// `"descendants_remaining"`, `"graceful_stop_sent"`. Used to prove the sink
    /// is authoritative on the TERMINAL/cleanup observations, not just mid-run
    /// output.
    OnKind(&'static str),
}

#[derive(Clone)]
pub struct RecordingSink {
    observations: Arc<Mutex<Vec<WorkerObservation>>>,
    /// Every observation kind `publish()` is CALLED with, recorded even when the
    /// publish then fails. Lets a test prove an observation was ATTEMPTED (published
    /// to the authoritative stream) even though the failing one is not kept in
    /// `observations`.
    attempted: Arc<Mutex<Vec<&'static str>>>,
    fail_mode: FailMode,
    failed: Arc<AtomicBool>,
    /// Woken after every SUCCESSFULLY recorded observation, so the readiness helpers
    /// (`wait_for_kind_count`, `wait_for_stdout_contains`) can await a condition instead
    /// of polling a fixed sleep. Shared across the sink's clones (the supervisor holds a
    /// clone), so a notify from the supervisor's clone wakes a waiter on the test's sink.
    notify: Arc<Notify>,
    /// If set, `publish()` sleeps for the given duration on observations of this kind
    /// BEFORE recording — a slow (but not failed) sink. `emit` wraps `publish` in
    /// `sink_backpressure_timeout`, so this exercises the backpressure contract: a
    /// delay under the timeout still delivers; a delay over it is a timeout failure.
    delay_on: Option<(&'static str, Duration)>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self {
            observations: Arc::new(Mutex::new(Vec::new())),
            attempted: Arc::new(Mutex::new(Vec::new())),
            fail_mode: FailMode::Never,
            failed: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
            delay_on: None,
        }
    }

    pub fn failing_on_output() -> Self {
        let mut sink = Self::new();
        sink.fail_mode = FailMode::OnFirstOutput;
        sink
    }

    /// A sink that fails the first time the given observation kind is published.
    pub fn failing_on_kind(kind: &'static str) -> Self {
        let mut sink = Self::new();
        sink.fail_mode = FailMode::OnKind(kind);
        sink
    }

    /// A sink that is SLOW (not failed) on observations of `kind`: each such publish
    /// sleeps `delay` before succeeding, so backpressure — not loss — is exercised.
    pub fn delaying_on_kind(kind: &'static str, delay: Duration) -> Self {
        let mut sink = Self::new();
        sink.delay_on = Some((kind, delay));
        sink
    }

    pub fn arc(&self) -> Arc<dyn ObservationSink> {
        Arc::new(self.clone())
    }

    pub fn observations(&self) -> Vec<WorkerObservation> {
        self.observations.lock().unwrap().clone()
    }

    pub fn kinds(&self) -> Vec<&'static str> {
        self.observations().iter().map(observation_kind).collect()
    }

    /// Every observation kind the sink was ASKED to publish, including one it failed
    /// on (which never lands in [`RecordingSink::kinds`]).
    pub fn attempted_kinds(&self) -> Vec<&'static str> {
        self.attempted.lock().unwrap().clone()
    }

    pub fn count(&self, kind: &str) -> usize {
        self.kinds().iter().filter(|k| **k == kind).count()
    }

    pub fn has(&self, kind: &str) -> bool {
        self.count(kind) > 0
    }

    pub fn heartbeats(&self) -> usize {
        self.count("heartbeat")
    }

    /// The identity from the `Spawned` observation, if any.
    pub fn spawned_identity(&self) -> Option<ProcessIdentity> {
        self.observations().into_iter().find_map(|o| match o {
            WorkerObservation::Spawned(id) => Some(id),
            _ => None,
        })
    }

    pub fn stream_bytes(&self, stream: OutputStream) -> Vec<u8> {
        let mut out = Vec::new();
        for observation in self.observations() {
            if let WorkerObservation::OutputChunk(chunk) = observation {
                if chunk.stream == stream {
                    out.extend_from_slice(&chunk.bytes);
                }
            }
        }
        out
    }

    pub fn stdout(&self) -> Vec<u8> {
        self.stream_bytes(OutputStream::Stdout)
    }

    pub fn stderr(&self) -> Vec<u8> {
        self.stream_bytes(OutputStream::Stderr)
    }

    /// Await until at least `minimum` observations of `kind` have been recorded, or the
    /// bounded `timeout` elapses. Race-safe: the predicate is checked first, then the
    /// notification future is armed (`enable()`) BEFORE re-checking, so a notify that
    /// lands between the check and the await cannot be lost. Never an unbounded wait.
    ///
    /// Returns the observed count on success, or a diagnostic error (including the
    /// current observation kinds) on timeout.
    pub async fn wait_for_kind_count(
        &self,
        kind: &str,
        minimum: usize,
        timeout: Duration,
    ) -> Result<usize, String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let count = self.count(kind);
            if count >= minimum {
                return Ok(count);
            }
            // Arm the notification BEFORE the final re-check: `enable()` registers this
            // future as a waiter, so a `notify_waiters()` after the re-check still wakes
            // us (no lost wakeup between check and await).
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.count(kind) >= minimum {
                return Ok(self.count(kind));
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return Err(format!(
                    "timed out after {timeout:?} waiting for >= {minimum} {kind:?} \
                     observation(s); saw {} (kinds: {:?})",
                    self.count(kind),
                    self.kinds()
                ));
            }
        }
    }

    /// Await until the recorded stdout contains `needle`, or the bounded `timeout`
    /// elapses. Same race-safe arm-before-recheck discipline as
    /// [`RecordingSink::wait_for_kind_count`]. Never an unbounded wait.
    ///
    /// Returns a diagnostic error (including the current stdout, lossily rendered) on
    /// timeout.
    pub async fn wait_for_stdout_contains(
        &self,
        needle: &[u8],
        timeout: Duration,
    ) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if contains_subslice(&self.stdout(), needle) {
                return Ok(());
            }
            let notified = self.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if contains_subslice(&self.stdout(), needle) {
                return Ok(());
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return Err(format!(
                    "timed out after {timeout:?} waiting for stdout to contain {:?}; \
                     current stdout: {:?}",
                    String::from_utf8_lossy(needle),
                    String::from_utf8_lossy(&self.stdout())
                ));
            }
        }
    }
}

/// Whether `haystack` contains `needle` as a contiguous subslice.
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

impl Default for RecordingSink {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ObservationSink for RecordingSink {
    async fn publish(&self, observation: WorkerObservation) -> Result<(), ObservationError> {
        self.attempted
            .lock()
            .unwrap()
            .push(observation_kind(&observation));
        // A slow (but not failed) publish: sleep INSIDE the future the supervisor wraps
        // in `sink_backpressure_timeout`, so the drain's own bounds must not cancel it.
        if let Some((kind, delay)) = self.delay_on {
            if observation_kind(&observation) == kind {
                tokio::time::sleep(delay).await;
            }
        }
        let should_fail = match self.fail_mode {
            FailMode::Never => false,
            FailMode::OnFirstOutput => {
                matches!(observation, WorkerObservation::OutputChunk(_))
                    && !self.failed.swap(true, Ordering::SeqCst)
            }
            FailMode::OnKind(kind) => {
                observation_kind(&observation) == kind && !self.failed.swap(true, Ordering::SeqCst)
            }
        };
        if should_fail {
            return Err(ObservationError(format!(
                "forced test sink failure on {}",
                observation_kind(&observation)
            )));
        }
        self.observations.lock().unwrap().push(observation);
        // Wake readiness waiters AFTER the observation is durably recorded, so a waiter
        // that re-checks its predicate on wake always sees this observation.
        self.notify.notify_waiters();
        Ok(())
    }
}

pub fn observation_kind(observation: &WorkerObservation) -> &'static str {
    match observation {
        WorkerObservation::BoundaryAttested(_) => "boundary_attested",
        WorkerObservation::SpawnRequested => "spawn_requested",
        WorkerObservation::Spawned(_) => "spawned",
        WorkerObservation::OutputChunk(_) => "output",
        WorkerObservation::Heartbeat { .. } => "heartbeat",
        WorkerObservation::CancellationRequested => "cancellation_requested",
        WorkerObservation::GracefulStopSent => "graceful_stop_sent",
        WorkerObservation::ForceStopSent => "force_stop_sent",
        WorkerObservation::DescendantsRemaining(_) => "descendants_remaining",
        WorkerObservation::Exited(_) => "exited",
        WorkerObservation::CleanupCompleted => "cleanup_completed",
        WorkerObservation::SupervisorFailed(_) => "supervisor_failed",
    }
}

/// Find the bytes between `BEGIN` and `END` markers in `haystack`.
pub fn extract_payload(haystack: &[u8]) -> Option<Vec<u8>> {
    let begin = find(haystack, BEGIN)?;
    let after = begin + BEGIN.len();
    let rest = haystack.get(after..)?;
    let end = find(rest, END)?;
    rest.get(..end).map(<[u8]>::to_vec)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// ---- the re-exec'd child ----

/// Dispatched by re-exec. Never runs in a normal `cargo test` (it is `#[ignore]`d).
#[test]
#[ignore = "spawned as a subprocess by the worker tests"]
fn worker_child_entry() {
    let mode = std::env::var(ENV_MODE).unwrap_or_default();
    match mode.as_str() {
        "exit0" => std::process::exit(0),
        "exit_code" => {
            let code = std::env::var(ENV_CODE)
                .ok()
                .and_then(|c| c.parse::<i32>().ok())
                .unwrap_or(0);
            std::process::exit(code);
        }
        "signal" => std::process::abort(), // exits by SIGABRT (signal 6)
        "print_stdout" => {
            emit(&mut std::io::stdout(), payload_bytes());
            std::process::exit(0);
        }
        "print_stderr" => {
            emit(&mut std::io::stderr(), payload_bytes());
            std::process::exit(0);
        }
        "print_both" => {
            emit(&mut std::io::stdout(), b"stdout-side".to_vec());
            emit(&mut std::io::stderr(), b"stderr-side".to_vec());
            std::process::exit(0);
        }
        "print_nonutf8" => {
            emit(&mut std::io::stdout(), NON_UTF8.to_vec());
            std::process::exit(0);
        }
        "print_large" => {
            emit(&mut std::io::stdout(), large_pattern(LARGE_LEN));
            std::process::exit(0);
        }
        "print_then_sleep" => {
            emit(&mut std::io::stdout(), payload_bytes());
            sleep_forever();
        }
        "print_cwd" => {
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            emit(&mut std::io::stdout(), cwd.into_bytes());
            std::process::exit(0);
        }
        "check_env" => {
            let var = std::env::var(ENV_CHECK_VAR).unwrap_or_default();
            let message = match std::env::var(&var) {
                Ok(value) => format!("PRESENT:{value}"),
                Err(_) => "ABSENT".to_owned(),
            };
            emit(&mut std::io::stdout(), message.into_bytes());
            std::process::exit(0);
        }
        "sleep" => sleep_forever(),
        "ignore_sigterm" => {
            let flag = Arc::new(AtomicBool::new(false));
            let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&flag));
            sleep_forever();
        }
        "ignore_sigterm_ready" => {
            let flag = Arc::new(AtomicBool::new(false));
            let registered =
                signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&flag));
            // Announce readiness ONLY after the SIGTERM handler is actually installed
            // (mirrors `grandchild_then_sleep_ready` gating its marker on a successful
            // spawn). If registration fails, do NOT emit the marker — the waiting test
            // then fails closed with a bounded-timeout diagnostic instead of signalling a
            // child that never installed the handler (which would race the default
            // SIGTERM disposition this test exists to avoid).
            if registered.is_ok() {
                emit_marker(&mut std::io::stdout(), READY_SIGTERM_HANDLER);
            }
            sleep_forever();
        }
        "grandchild_then_exit" => {
            let _ = spawn_grandchild();
            // Give the grandchild a moment to appear in /proc, then exit.
            std::thread::sleep(Duration::from_millis(150));
            std::process::exit(0);
        }
        "grandchild_then_sleep" => {
            let _ = spawn_grandchild();
            sleep_forever();
        }
        "grandchild_then_sleep_ready" => {
            // Announce readiness ONLY after the grandchild is actually spawned — never
            // infer grandchild readiness from the leader's `Spawned` observation, which
            // says nothing about the grandchild.
            if spawn_grandchild().is_ok() {
                emit_marker(&mut std::io::stdout(), READY_GRANDCHILD);
            }
            sleep_forever();
        }
        "leader_dies_grandchild_ignores_sigterm" => {
            // A same-group descendant that IGNORES SIGTERM, while the LEADER keeps
            // the default SIGTERM disposition. On the group SIGTERM the leader dies
            // immediately but the descendant survives the grace period, forcing the
            // supervisor to escalate to SIGKILL → a FORCEFUL cancellation whose set
            // must still end up empty.
            let _ = spawn_grandchild_mode("ignore_sigterm");
            sleep_forever();
        }
        other => {
            eprintln!("unknown child mode: {other}");
            std::process::exit(97);
        }
    }
}

fn payload_bytes() -> Vec<u8> {
    std::env::var(ENV_PAYLOAD)
        .unwrap_or_else(|_| "default-payload".to_owned())
        .into_bytes()
}

fn emit<W: Write>(w: &mut W, payload: Vec<u8>) {
    let mut buf = Vec::new();
    buf.extend_from_slice(BEGIN);
    buf.extend_from_slice(&payload);
    buf.extend_from_slice(END);
    let _ = w.write_all(&buf);
    let _ = w.flush();
}

/// Write a raw readiness marker and flush it immediately, so a waiting test observes it
/// with minimal latency. Markers are unique byte constants (see [`READY_GRANDCHILD`] /
/// [`READY_SIGTERM_HANDLER`]) that pass through the same byte-preserving stdout pipe as
/// any other child output.
fn emit_marker<W: Write>(w: &mut W, marker: &[u8]) {
    let _ = w.write_all(marker);
    let _ = w.flush();
}

fn sleep_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn spawn_grandchild() -> std::io::Result<std::process::Child> {
    spawn_grandchild_mode("sleep")
}

/// Spawn a grandchild in the leader's process group. Returns the spawn result so a
/// caller can announce readiness ONLY on a successful spawn. The returned `Child` is
/// dropped without `wait` — std does not kill on drop, so the grandchild keeps running
/// (reparented to init when the leader exits), which is exactly the owned-set member the
/// supervisor must clean up.
fn spawn_grandchild_mode(mode: &str) -> std::io::Result<std::process::Child> {
    let exe = std::env::current_exe().expect("current_exe");
    // No process_group() call → the grandchild inherits the leader's group, so it
    // is part of the owned set the supervisor must clean up.
    std::process::Command::new(exe)
        .args([
            "--ignored",
            "--exact",
            "--nocapture",
            "common::worker_child_entry",
        ])
        .env(ENV_MODE, mode)
        .spawn()
}
