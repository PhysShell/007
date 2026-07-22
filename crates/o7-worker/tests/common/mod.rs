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
use o7_worker::{
    BoundaryRequirement, CancellationPolicy, HeartbeatPolicy, ObservationError, ObservationSink,
    OutputPolicy, OutputStream, ProcessBoundary, ProcessIdentity, StdinMode,
    UnconfinedHostBoundary, WorkerId, WorkerJoin, WorkerObservation, WorkerResult, WorkerSpec,
    WorkerSupervisor,
};

/// The PR-2 host boundary as a trait object.
pub fn host_boundary() -> Box<dyn ProcessBoundary> {
    Box::new(UnconfinedHostBoundary)
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

pub const ENV_MODE: &str = "O7_WORKER_CHILD_MODE";
pub const ENV_CODE: &str = "O7_WORKER_CHILD_CODE";
pub const ENV_PAYLOAD: &str = "O7_WORKER_CHILD_PAYLOAD";
pub const ENV_CHECK_VAR: &str = "O7_WORKER_CHECK_VAR";

pub const BEGIN: &[u8] = b"\x1e\x1e<<<O7BEGIN>>>\x1e\x1e";
pub const END: &[u8] = b"\x1e\x1e<<<O7END>>>\x1e\x1e";
/// A fixed non-UTF-8 payload used by the `print_nonutf8` child mode.
pub const NON_UTF8: &[u8] = &[0x00, 0xFF, 0xFE, 0x80, 0x41, 0x00, 0xC0];

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
}

#[derive(Clone)]
pub struct RecordingSink {
    observations: Arc<Mutex<Vec<WorkerObservation>>>,
    fail_mode: FailMode,
    failed: Arc<AtomicBool>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self {
            observations: Arc::new(Mutex::new(Vec::new())),
            fail_mode: FailMode::Never,
            failed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn failing_on_output() -> Self {
        let mut sink = Self::new();
        sink.fail_mode = FailMode::OnFirstOutput;
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
}

impl Default for RecordingSink {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ObservationSink for RecordingSink {
    async fn publish(&self, observation: WorkerObservation) -> Result<(), ObservationError> {
        if self.fail_mode == FailMode::OnFirstOutput
            && matches!(observation, WorkerObservation::OutputChunk(_))
            && !self.failed.swap(true, Ordering::SeqCst)
        {
            return Err(ObservationError("forced test sink failure".to_owned()));
        }
        self.observations.lock().unwrap().push(observation);
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
        "grandchild_then_exit" => {
            spawn_grandchild();
            // Give the grandchild a moment to appear in /proc, then exit.
            std::thread::sleep(Duration::from_millis(150));
            std::process::exit(0);
        }
        "grandchild_then_sleep" => {
            spawn_grandchild();
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

fn sleep_forever() -> ! {
    loop {
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn spawn_grandchild() {
    let exe = std::env::current_exe().expect("current_exe");
    // No process_group() call → the grandchild inherits the leader's group, so it
    // is part of the owned set the supervisor must clean up.
    let _child = std::process::Command::new(exe)
        .args([
            "--ignored",
            "--exact",
            "--nocapture",
            "common::worker_child_entry",
        ])
        .env(ENV_MODE, "sleep")
        .spawn();
    // The std Child is dropped without wait: std does not kill on drop, so the
    // grandchild keeps running (reparented to init when the leader exits).
}
