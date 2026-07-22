//! The generic worker supervisor. It owns a boundary process, streams typed
//! observations to the sink, cancels idempotently, tears down the whole owned
//! process set, and produces exactly ONE terminal [`WorkerResult`].
//!
//! The supervisor runs as its own task. A [`WorkerHandle`] controls it (and
//! requests cancellation on drop); a [`WorkerJoin`] observes its completion — the
//! task is never a detached, unobservable task.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use tokio::io::AsyncReadExt as _;
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};

use crate::boundary::{BoundaryExit, BoundaryProcess, ProcessBoundary};
use crate::observation::{ObservationSink, WorkerObservation};
use crate::output::{OutputChunk, OutputStream};
use crate::process_identity::ProcessIdentity;
use crate::spec::{WorkerId, WorkerSpec};

/// The single terminal outcome of a worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerResult {
    /// The leader exited with this code.
    ExitedNormally(i32),
    /// The leader was terminated by this signal.
    ExitedBySignal(i32),
    /// Cancelled; the leader stopped within the grace period.
    CancelledGracefully,
    /// Cancelled; the leader had to be force-killed.
    CancelledForcefully,
    /// The process never started (bad spec, spawn error, or boundary requirement
    /// not met).
    FailedToStart(String),
    /// A boundary mechanism failed during the run.
    BoundaryFailure(String),
    /// The authoritative observation sink failed; the worker was stopped.
    ObservationFailure(String),
    /// The owned process group could not be proven gone; treat as failure even if
    /// the leader exited cleanly.
    CleanupFailure(String),
}

impl WorkerResult {
    /// A stable, machine-readable tag.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ExitedNormally(_) => "EXITED_NORMALLY",
            Self::ExitedBySignal(_) => "EXITED_BY_SIGNAL",
            Self::CancelledGracefully => "CANCELLED_GRACEFULLY",
            Self::CancelledForcefully => "CANCELLED_FORCEFULLY",
            Self::FailedToStart(_) => "FAILED_TO_START",
            Self::BoundaryFailure(_) => "BOUNDARY_FAILURE",
            Self::ObservationFailure(_) => "OBSERVATION_FAILURE",
            Self::CleanupFailure(_) => "CLEANUP_FAILURE",
        }
    }
}

/// Controls a running worker. Dropping it requests cancellation (the supervisor
/// then performs full termination); observe completion via [`WorkerJoin`].
pub struct WorkerHandle {
    worker_id: WorkerId,
    request_tx: watch::Sender<bool>,
    terminal_rx: watch::Receiver<bool>,
}

impl WorkerHandle {
    #[must_use]
    pub fn worker_id(&self) -> &WorkerId {
        &self.worker_id
    }

    /// Request cancellation and wait until the supervisor has fully cleaned up.
    /// Idempotent and safe to call concurrently.
    pub async fn cancel(&self) {
        let _ = self.request_tx.send_replace(true);
        let mut rx = self.terminal_rx.clone();
        while !*rx.borrow_and_update() {
            if rx.changed().await.is_err() {
                break;
            }
        }
    }
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        // Never silently detach: ask the supervisor to cancel and clean up. The
        // supervisor task keeps running independently and is observed via
        // WorkerJoin.
        let _ = self.request_tx.send_replace(true);
    }
}

/// Observes the supervisor task to completion (independent of the handle).
pub struct WorkerJoin {
    task: JoinHandle<WorkerResult>,
}

impl WorkerJoin {
    /// Await the terminal result.
    pub async fn join(self) -> WorkerResult {
        self.task.await.unwrap_or_else(|e| {
            WorkerResult::CleanupFailure(format!("supervisor task failed: {e}"))
        })
    }
}

/// Starts and owns generic worker runs.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorkerSupervisor;

impl WorkerSupervisor {
    /// Start a worker. Returns a control handle and an independent completion
    /// observer. The supervisor runs as its own task.
    #[must_use]
    pub fn start(
        spec: WorkerSpec,
        boundary: Box<dyn ProcessBoundary>,
        sink: Arc<dyn ObservationSink>,
    ) -> (WorkerHandle, WorkerJoin) {
        let (request_tx, request_rx) = watch::channel(false);
        let (terminal_tx, terminal_rx) = watch::channel(false);
        let worker_id = spec.worker_id.clone();
        let task = tokio::spawn(run(spec, boundary, sink, request_rx, terminal_tx));
        (
            WorkerHandle {
                worker_id,
                request_tx,
                terminal_rx,
            },
            WorkerJoin { task },
        )
    }
}

// ---- supervisor task ----

async fn run(
    spec: WorkerSpec,
    boundary: Box<dyn ProcessBoundary>,
    sink: Arc<dyn ObservationSink>,
    request_rx: watch::Receiver<bool>,
    terminal_tx: watch::Sender<bool>,
) -> WorkerResult {
    let result = run_inner(spec, boundary, sink, request_rx).await;
    // Terminal reached (including cleanup): unblock WorkerHandle::cancel awaiters.
    let _ = terminal_tx.send_replace(true);
    result
}

struct Publisher {
    sink: Arc<dyn ObservationSink>,
    timeout: Duration,
    alive: bool,
    last_error: Option<String>,
}

impl Publisher {
    fn error(&self) -> String {
        self.last_error
            .clone()
            .unwrap_or_else(|| "observation sink failure".to_owned())
    }

    async fn emit(&mut self, observation: WorkerObservation) -> bool {
        if !self.alive {
            return false;
        }
        match tokio::time::timeout(self.timeout, self.sink.publish(observation)).await {
            Ok(Ok(())) => true,
            Ok(Err(err)) => {
                self.alive = false;
                self.last_error = Some(format!("sink error: {err}"));
                false
            }
            Err(_) => {
                self.alive = false;
                self.last_error = Some(format!("sink publish exceeded {:?}", self.timeout));
                false
            }
        }
    }
}

async fn run_inner(
    spec: WorkerSpec,
    boundary: Box<dyn ProcessBoundary>,
    sink: Arc<dyn ObservationSink>,
    mut request_rx: watch::Receiver<bool>,
) -> WorkerResult {
    let mut pubr = Publisher {
        sink,
        timeout: spec.output.sink_backpressure_timeout,
        alive: true,
        last_error: None,
    };

    let attestation = boundary.attestation();
    if !pubr
        .emit(WorkerObservation::BoundaryAttested(attestation))
        .await
    {
        return WorkerResult::ObservationFailure(pubr.error());
    }

    // Fail closed BEFORE spawn if the boundary cannot satisfy the requirement.
    if !spec.boundary_requirement.is_satisfied_by(&attestation) {
        return WorkerResult::FailedToStart(format!(
            "boundary requirement not met: required {:?}, boundary attests {:?}",
            spec.boundary_requirement, attestation.enforcement
        ));
    }

    if let Err(err) = spec.validate() {
        return WorkerResult::FailedToStart(err.to_string());
    }

    if !pubr.emit(WorkerObservation::SpawnRequested).await {
        return WorkerResult::ObservationFailure(pubr.error());
    }

    let spawn_spec = crate::boundary::BoundarySpawnSpec {
        executable: spec.executable.clone(),
        arguments: spec.arguments.clone(),
        working_directory: spec.working_directory.clone(),
        environment: spec.environment.clone(),
        stdin: spec.stdin,
    };
    let mut process = match boundary.spawn(spawn_spec).await {
        Ok(process) => process,
        Err(err) => return WorkerResult::FailedToStart(err.to_string()),
    };

    let identity = process.identity();
    if !pubr
        .emit(WorkerObservation::Spawned(identity.clone()))
        .await
    {
        // Sink lost right after spawn: we still own a live process; kill it.
        force_cleanup(process.as_mut()).await;
        return WorkerResult::ObservationFailure(pubr.error());
    }

    manage(spec, process, &mut pubr, &mut request_rx, identity).await
}

/// How the select loop exited (only `wait` touched the process; cancellation is
/// handled after the loop).
enum Phase {
    Natural(BoundaryExit),
    CancelRequested,
    SinkFailed(String),
    BoundaryFailed(String),
}

/// Reasons the run phase ended, before final cleanup.
enum Termination {
    Natural(BoundaryExit),
    Cancelled {
        forceful: bool,
        exit: Option<BoundaryExit>,
    },
    SinkFailed(String),
    BoundaryFailed(String),
}

async fn manage(
    spec: WorkerSpec,
    mut process: Box<dyn BoundaryProcess>,
    pubr: &mut Publisher,
    request_rx: &mut watch::Receiver<bool>,
    identity: ProcessIdentity,
) -> WorkerResult {
    let (out_tx, mut out_rx) = mpsc::channel::<OutputChunk>(spec.output.channel_capacity);
    let mut readers = JoinSet::new();
    if let Some(stdout) = process.take_stdout() {
        readers.spawn(read_stream(
            stdout,
            OutputStream::Stdout,
            out_tx.clone(),
            spec.output.max_chunk_bytes,
        ));
    }
    if let Some(stderr) = process.take_stderr() {
        readers.spawn(read_stream(
            stderr,
            OutputStream::Stderr,
            out_tx.clone(),
            spec.output.max_chunk_bytes,
        ));
    }
    drop(out_tx); // out_rx closes once every reader is done

    let started = Instant::now();
    let heartbeat_enabled = spec.heartbeat.enabled;
    let mut heartbeat = tokio::time::interval(spec.heartbeat.interval);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut heartbeat_seq: u64 = 0;
    let mut output_open = true;

    // Phase 1: run until the leader exits, cancellation, or a fatal sink failure.
    // Only the `wait` arm touches `process`; cancellation runs AFTER the loop (so
    // the process is not borrowed twice).
    let phase = loop {
        tokio::select! {
            exit = process.wait() => {
                match exit {
                    Ok(exit) => break Phase::Natural(exit),
                    Err(err) => break Phase::BoundaryFailed(err.to_string()),
                }
            }
            maybe = out_rx.recv(), if output_open => {
                match maybe {
                    Some(chunk) => {
                        if !pubr.emit(WorkerObservation::OutputChunk(chunk)).await {
                            break Phase::SinkFailed(pubr.error());
                        }
                    }
                    None => { output_open = false; }
                }
            }
            _ = heartbeat.tick(), if heartbeat_enabled => {
                let observation = WorkerObservation::Heartbeat {
                    worker_id: spec.worker_id.clone(),
                    sequence: heartbeat_seq,
                    uptime: started.elapsed(),
                    identity: identity.clone(),
                };
                heartbeat_seq = heartbeat_seq.wrapping_add(1);
                if !pubr.emit(observation).await {
                    break Phase::SinkFailed(pubr.error());
                }
            }
            _ = wait_cancel(request_rx) => {
                break Phase::CancelRequested;
            }
        }
    };

    let termination = match phase {
        Phase::Natural(exit) => Termination::Natural(exit),
        Phase::SinkFailed(message) => Termination::SinkFailed(message),
        Phase::BoundaryFailed(message) => Termination::BoundaryFailed(message),
        Phase::CancelRequested => {
            run_cancellation(process.as_mut(), spec.cancellation.graceful_timeout, pubr).await
        }
    };

    // Reap the direct child in the failure paths (Natural/Cancelled already
    // waited on it). A killed-but-unreaped child is a zombie that cleanup would
    // otherwise mistake for a surviving group member.
    if matches!(
        termination,
        Termination::SinkFailed(_) | Termination::BoundaryFailed(_)
    ) {
        let _ = process.force_stop().await;
        let _ = process.wait().await;
    }

    // Phase 2: verified cleanup FIRST — kill any surviving group members so their
    // pipes close. Otherwise draining could block forever on a grandchild that
    // only dies during cleanup.
    let cleanup = cleanup_group(process.as_mut(), pubr).await;

    // Phase 3: drain remaining output (pipes are closing now), then join readers.
    if pubr.alive {
        while let Some(chunk) = out_rx.recv().await {
            if !pubr.emit(WorkerObservation::OutputChunk(chunk)).await {
                break;
            }
        }
    }
    readers.shutdown().await;

    // Report the leader exit (if known) and, on success, completion — only after
    // the group is confirmed gone.
    let reported_exit = match &termination {
        Termination::Natural(exit) => Some(*exit),
        Termination::Cancelled { exit, .. } => *exit,
        _ => None,
    };
    if pubr.alive {
        if let Some(exit) = reported_exit {
            let _ = pubr.emit(WorkerObservation::Exited(exit)).await;
        }
        if cleanup.is_ok() {
            let _ = pubr.emit(WorkerObservation::CleanupCompleted).await;
        }
    }

    if let Err(message) = cleanup {
        return WorkerResult::CleanupFailure(message);
    }
    match termination {
        Termination::Natural(BoundaryExit::Code(code)) => WorkerResult::ExitedNormally(code),
        Termination::Natural(BoundaryExit::Signal(signal)) => WorkerResult::ExitedBySignal(signal),
        Termination::Cancelled {
            forceful: false, ..
        } => WorkerResult::CancelledGracefully,
        Termination::Cancelled { forceful: true, .. } => WorkerResult::CancelledForcefully,
        Termination::SinkFailed(message) => WorkerResult::ObservationFailure(message),
        Termination::BoundaryFailed(message) => WorkerResult::BoundaryFailure(message),
    }
}

async fn run_cancellation(
    process: &mut dyn BoundaryProcess,
    grace: Duration,
    pubr: &mut Publisher,
) -> Termination {
    let _ = pubr.emit(WorkerObservation::CancellationRequested).await;
    if let Err(err) = process.request_graceful_stop().await {
        // Could not even signal: escalate straight to force.
        let _ = err;
    } else {
        let _ = pubr.emit(WorkerObservation::GracefulStopSent).await;
    }
    match tokio::time::timeout(grace, process.wait()).await {
        Ok(Ok(exit)) => Termination::Cancelled {
            forceful: false,
            exit: Some(exit),
        },
        Ok(Err(err)) => Termination::BoundaryFailed(err.to_string()),
        Err(_) => {
            // Grace expired: force the whole group and reap.
            let _ = pubr.emit(WorkerObservation::ForceStopSent).await;
            if let Err(err) = process.force_stop().await {
                return Termination::BoundaryFailed(err.to_string());
            }
            match process.wait().await {
                Ok(exit) => Termination::Cancelled {
                    forceful: true,
                    exit: Some(exit),
                },
                Err(err) => Termination::BoundaryFailed(err.to_string()),
            }
        }
    }
}

/// Kill the whole owned group if anything remains, then prove it is gone.
async fn cleanup_group(
    process: &mut dyn BoundaryProcess,
    pubr: &mut Publisher,
) -> Result<(), String> {
    let remaining = process.remaining_members().await.unwrap_or_default();
    if remaining.is_empty() {
        return Ok(());
    }
    let _ = pubr
        .emit(WorkerObservation::DescendantsRemaining(remaining))
        .await;
    if let Err(err) = process.force_stop().await {
        return Err(format!("force stop during cleanup failed: {err}"));
    }
    // Give the kernel a moment to reap, then re-check.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let survivors = process.remaining_members().await.unwrap_or_default();
    if survivors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} owned process(es) survived cleanup",
            survivors.len()
        ))
    }
}

/// Best-effort force-kill + reap when the sink is already dead (no publishing).
async fn force_cleanup(process: &mut dyn BoundaryProcess) {
    let _ = process.force_stop().await;
    let _ = process.wait().await;
    let _ = cleanup_group_silent(process).await;
}

async fn cleanup_group_silent(process: &mut dyn BoundaryProcess) -> Result<(), String> {
    let remaining = process.remaining_members().await.unwrap_or_default();
    if remaining.is_empty() {
        return Ok(());
    }
    let _ = process.force_stop().await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let survivors = process.remaining_members().await.unwrap_or_default();
    if survivors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} survived", survivors.len()))
    }
}

/// Resolve once cancellation has been requested (cancel-safe).
async fn wait_cancel(request_rx: &mut watch::Receiver<bool>) {
    loop {
        if *request_rx.borrow_and_update() {
            return;
        }
        if request_rx.changed().await.is_err() {
            return;
        }
    }
}

async fn read_stream<R>(
    mut reader: R,
    stream: OutputStream,
    tx: mpsc::Sender<OutputChunk>,
    max_chunk: usize,
) where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let cap = max_chunk.max(1);
    let mut buf = vec![0u8; cap];
    let mut sequence: u64 = 0;
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                let slice = buf.get(..n).unwrap_or(&[]);
                let chunk = OutputChunk {
                    stream,
                    sequence,
                    bytes: Bytes::copy_from_slice(slice),
                };
                sequence = sequence.wrapping_add(1);
                if tx.send(chunk).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}
