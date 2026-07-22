//! The generic worker supervisor. It owns a boundary process GROUP, streams typed
//! observations to the authoritative sink, cancels idempotently, tears the whole
//! owned group down, and produces exactly ONE terminal [`WorkerResult`].
//!
//! Fault-closed by design: a lost sink, an unprovable cleanup, a pipe read error,
//! or a boundary error each yields a failure terminal — never a false success.
//!
//! The supervisor runs as its own task. A [`WorkerHandle`] controls it (and
//! requests cancellation on drop); a [`WorkerJoin`] observes its completion.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use tokio::io::AsyncReadExt as _;
use tokio::sync::{mpsc, watch};
use tokio::task::{JoinHandle, JoinSet};

use crate::boundary::{BoundaryExit, BoundaryProcess, BoundaryStream, ProcessBoundary};
use crate::observation::{ObservationSink, WorkerObservation};
use crate::output::{OutputChunk, OutputStream};
use crate::process_identity::ProcessIdentity;
use crate::spec::{WorkerId, WorkerSpec};
use crate::state::WorkerState;

/// Poll interval while waiting for a process group to drain during cancellation.
const GROUP_POLL_INTERVAL: Duration = Duration::from_millis(50);
/// How long, after SIGKILL, we allow the kernel to reap the group before declaring
/// a cleanup failure.
const CLEANUP_GRACE: Duration = Duration::from_millis(500);
/// How long to wait for trailing output to drain (pipes closing) after cleanup has
/// verified the owned group is gone. A descendant that ESCAPED the owned group can
/// still hold an inherited pipe open, so this wait is bounded and its expiry is a
/// failure — the drain must never hang, and never silently pass on a timeout.
const DRAIN_GRACE: Duration = Duration::from_millis(500);

/// The single terminal outcome of a worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerResult {
    /// The leader exited with this code.
    ExitedNormally(i32),
    /// The leader was terminated by this signal.
    ExitedBySignal(i32),
    /// Cancelled; the whole owned group drained within the grace period.
    CancelledGracefully,
    /// Cancelled; at least one group member required a force-kill.
    CancelledForcefully,
    /// The process never started (bad spec/policy, spawn error, or boundary
    /// requirement not met).
    FailedToStart(String),
    /// A boundary mechanism failed during the run.
    BoundaryFailure(String),
    /// The authoritative observation sink failed; the worker was stopped.
    ObservationFailure(String),
    /// A stdout/stderr read failed; output faithfulness was lost.
    OutputFailure(String),
    /// The owned group could not be proven gone; treated as failure even if the
    /// leader exited cleanly.
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
            Self::OutputFailure(_) => "OUTPUT_FAILURE",
            Self::CleanupFailure(_) => "CLEANUP_FAILURE",
        }
    }

    fn is_failure(&self) -> bool {
        matches!(
            self,
            Self::FailedToStart(_)
                | Self::BoundaryFailure(_)
                | Self::ObservationFailure(_)
                | Self::OutputFailure(_)
                | Self::CleanupFailure(_)
        )
    }

    fn message(&self) -> String {
        match self {
            Self::FailedToStart(m)
            | Self::BoundaryFailure(m)
            | Self::ObservationFailure(m)
            | Self::OutputFailure(m)
            | Self::CleanupFailure(m) => m.clone(),
            other => other.kind().to_owned(),
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
        // supervisor task keeps running independently and its completion is still
        // observable through the terminal watch (and WorkerJoin).
        let _ = self.request_tx.send_replace(true);
    }
}

/// Observes the supervisor task to completion (independent of the handle).
///
/// Dropping a `WorkerJoin` without calling [`WorkerJoin::join`] DETACHES the
/// supervisor task — exactly as dropping any Tokio [`JoinHandle`] does — and
/// discards its terminal [`WorkerResult`]. `#[must_use]` only nudges you not to do
/// that by accident; it does not change the detach semantics. Crucially, detaching
/// does NOT cancel or orphan the run: the supervisor task keeps running, still owns
/// the boundary process, and performs its own verified cleanup, so a dropped join
/// loses the RESULT VALUE, not the cleanup. Its completion also remains observable
/// through [`WorkerHandle::cancel`]'s terminal signal.
#[must_use = "dropping WorkerJoin detaches the task and discards its terminal result; join() it or keep it"]
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

    /// Publish, tracking sink health. Returns whether the sink is still alive.
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

/// Build the message for an `ObservationFailure` that dominates a co-occurring
/// boundary/output fault. Losing the authoritative sink is the reported terminal, but
/// the underlying run-loop fault must not be erased — it is preserved in the message
/// so the combined failure is legible.
fn observation_failure_message(pubr: &Publisher, base: &WorkerResult) -> String {
    let sink_error = pubr.error();
    match base {
        WorkerResult::BoundaryFailure(underlying) | WorkerResult::OutputFailure(underlying) => {
            format!("{sink_error}; underlying fault preserved: {underlying}")
        }
        // A `SinkFailed` base is the SAME sink loss (no distinct underlying fault); any
        // success/cancel base carries no fault to preserve.
        _ => sink_error,
    }
}

fn advance(state: &mut WorkerState, to: WorkerState) -> Result<(), String> {
    if state.can_transition_to(to) {
        *state = to;
        Ok(())
    } else {
        Err(format!(
            "invalid worker state transition {state:?} -> {to:?}"
        ))
    }
}

async fn run_inner(
    spec: WorkerSpec,
    boundary: Box<dyn ProcessBoundary>,
    sink: Arc<dyn ObservationSink>,
    mut request_rx: watch::Receiver<bool>,
) -> WorkerResult {
    let mut state = WorkerState::Created;
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

    // Everything below "fails to start" transitions Created -> Starting first so a
    // pre-spawn rejection is a real Starting -> FailedToStart.
    if let Err(e) = advance(&mut state, WorkerState::Starting) {
        return WorkerResult::CleanupFailure(e);
    }

    if !spec.boundary_requirement.is_satisfied_by(&attestation) {
        return fail_to_start(
            &mut state,
            format!(
                "boundary requirement not met: required {:?}, boundary attests {:?}",
                spec.boundary_requirement, attestation.enforcement
            ),
        );
    }
    if let Err(err) = spec.validate() {
        return fail_to_start(&mut state, err.to_string());
    }

    // Cancel BEFORE spawn: never launch a process we were already told to abandon.
    // The sink is authoritative even here — losing it is an ObservationFailure, not
    // a graceful cancel.
    if *request_rx.borrow_and_update() {
        if !pubr.emit(WorkerObservation::CancellationRequested).await {
            return WorkerResult::ObservationFailure(pubr.error());
        }
        return cancelled_before_running(&mut state);
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

    // Cancellable spawn: a slow/hung boundary must not make cancel wait forever.
    // Dropping the spawn future is cancel-safe per the ProcessBoundary contract.
    let spawn_fut = boundary.spawn(spawn_spec);
    tokio::pin!(spawn_fut);
    let mut process = tokio::select! {
        spawned = &mut spawn_fut => match spawned {
            Ok(process) => process,
            Err(err) => return fail_to_start(&mut state, err.to_string()),
        },
        _ = wait_cancel(&mut request_rx) => {
            // The spawn future is dropped (cancel-safe: nothing is owned). The sink
            // is still authoritative — a failed publish is an ObservationFailure.
            if !pubr.emit(WorkerObservation::CancellationRequested).await {
                return WorkerResult::ObservationFailure(pubr.error());
            }
            return cancelled_before_running(&mut state);
        }
    };

    // From here a live process is owned: every early exit must go through VERIFIED
    // cleanup (force-kill, reap, prove the group gone), never a best-effort kill.
    let identity = process.identity();
    if let Err(e) = advance(&mut state, WorkerState::Running) {
        let _ = abandon_and_verify(process.as_mut(), &mut pubr).await;
        return WorkerResult::CleanupFailure(e);
    }
    if !pubr
        .emit(WorkerObservation::Spawned(identity.clone()))
        .await
    {
        // A lost sink on Spawned still owns a live process. Prove cleanup: an
        // unprovable/failed cleanup (leaked processes) DOMINATES the sink failure.
        return match abandon_and_verify(process.as_mut(), &mut pubr).await {
            Err(message) => WorkerResult::CleanupFailure(message),
            Ok(()) => WorkerResult::ObservationFailure(pubr.error()),
        };
    }

    manage(
        spec,
        process,
        &mut pubr,
        &mut request_rx,
        identity,
        &mut state,
    )
    .await
}

fn fail_to_start(state: &mut WorkerState, message: String) -> WorkerResult {
    if let Err(e) = advance(state, WorkerState::FailedToStart) {
        return WorkerResult::CleanupFailure(e);
    }
    WorkerResult::FailedToStart(message)
}

fn cancelled_before_running(state: &mut WorkerState) -> WorkerResult {
    // Starting -> Cancelling -> Exited; nothing was ever owned.
    let _ = advance(state, WorkerState::Cancelling);
    let _ = advance(state, WorkerState::Exited);
    WorkerResult::CancelledGracefully
}

// A reader message: a chunk, or a fatal read error (never a silent truncation).
enum ReaderMessage {
    Chunk(OutputChunk),
    ReadError(String),
}

/// How the select loop exited.
enum Phase {
    Natural(BoundaryExit),
    CancelRequested,
    SinkFailed(String),
    OutputFailed(String),
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
    OutputFailed(String),
    BoundaryFailed(String),
}

async fn manage(
    spec: WorkerSpec,
    mut process: Box<dyn BoundaryProcess>,
    pubr: &mut Publisher,
    request_rx: &mut watch::Receiver<bool>,
    identity: ProcessIdentity,
    state: &mut WorkerState,
) -> WorkerResult {
    let (out_tx, mut out_rx) = mpsc::channel::<ReaderMessage>(spec.output.channel_capacity);
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
    drop(out_tx);

    let started = Instant::now();
    // Construct NO timer when heartbeats are disabled: a disabled+zero interval is a
    // legal spec, and `validate()` only bounds the interval (non-zero, ≤ MAX_TIMEOUT)
    // when heartbeats are ENABLED. Building the `Interval` solely under `enabled` means
    // no unvalidated, possibly-`Duration::MAX` period ever reaches the timer.
    let heartbeat_enabled = spec.heartbeat.enabled;
    let mut heartbeat = heartbeat_enabled.then(|| {
        let mut interval = tokio::time::interval(spec.heartbeat.interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval
    });
    let mut heartbeat_seq: u64 = 0;
    let mut output_open = true;

    let phase = loop {
        tokio::select! {
            exit = process.wait() => break match exit {
                Ok(exit) => Phase::Natural(exit),
                Err(err) => Phase::BoundaryFailed(err.to_string()),
            },
            maybe = out_rx.recv(), if output_open => {
                match maybe {
                    Some(ReaderMessage::Chunk(chunk)) => {
                        if !pubr.emit(WorkerObservation::OutputChunk(chunk)).await {
                            break Phase::SinkFailed(pubr.error());
                        }
                    }
                    Some(ReaderMessage::ReadError(message)) => {
                        break Phase::OutputFailed(message);
                    }
                    None => { output_open = false; }
                }
            }
            _ = next_heartbeat(heartbeat.as_mut()), if heartbeat_enabled => {
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
            _ = wait_cancel(request_rx) => break Phase::CancelRequested,
        }
    };

    let termination = match phase {
        Phase::Natural(exit) => Termination::Natural(exit),
        Phase::SinkFailed(m) => Termination::SinkFailed(m),
        Phase::OutputFailed(m) => Termination::OutputFailed(m),
        Phase::BoundaryFailed(m) => Termination::BoundaryFailed(m),
        Phase::CancelRequested => {
            let _ = advance(state, WorkerState::Cancelling);
            run_cancellation(process.as_mut(), spec.cancellation.graceful_timeout, pubr).await
        }
    };

    // Reap the leader in the fault paths (Natural/Cancelled already waited). A
    // killed-but-unreaped child is a zombie cleanup would mistake for a survivor.
    if matches!(
        termination,
        Termination::SinkFailed(_) | Termination::OutputFailed(_) | Termination::BoundaryFailed(_)
    ) {
        // This emergency SIGKILL is a REAL teardown action, so it must not be
        // invisible to the authoritative stream (PR-4's source-of-truth adapter maps
        // it to a canonical event). Publish — and honor — `ForceStopSent` before
        // performing it. If the sink is already dead (`SinkFailed`) this is a no-op;
        // if it dies HERE, `!pubr.alive` makes `ObservationFailure` dominate the
        // boundary/output fault in the terminal-precedence check below.
        let _ = pubr.emit(WorkerObservation::ForceStopSent).await;
        let _ = process.force_stop().await;
        let _ = process.wait().await;
    }

    // Verified cleanup FIRST — kill any surviving group members so their pipes
    // close (otherwise draining could block on a grandchild that only dies now).
    let cleanup = cleanup_group(process.as_mut(), pubr).await;

    // Drain remaining output (pipes closing), then join readers. A read error seen
    // ONLY during the drain (leader already exited) must NOT be silently dropped —
    // it means output faithfulness was lost after the exit.
    //
    // The drain MUST be bounded. `out_rx.recv()` only returns `None` once every
    // reader task has ended, which happens when the pipes close. But:
    //   * if cleanup FAILED the owned set is not proven gone, so a survivor can hold
    //     its pipe open forever; and
    //   * even after a VERIFIED-empty group, a descendant that ESCAPED the host group
    //     (its own session/group) can still hold an inherited stdout/stderr pipe open.
    // Either way an unbounded `recv()` would hang and the promised failure terminal
    // (CleanupFailure / a drain fault) would never be produced. So: skip the wait
    // entirely on a cleanup error (let CleanupFailure dominate), and otherwise bound
    // the drain with a deadline whose expiry is itself a failure — never a clean pass.
    let mut drain_output_error: Option<String> = None;
    let mut drain_timed_out = false;
    if cleanup.is_ok() && pubr.alive {
        let drain = async {
            while let Some(message) = out_rx.recv().await {
                match message {
                    ReaderMessage::Chunk(chunk) => {
                        if !pubr.emit(WorkerObservation::OutputChunk(chunk)).await {
                            break;
                        }
                    }
                    ReaderMessage::ReadError(message) => {
                        return Some(message);
                    }
                }
            }
            None
        };
        match tokio::time::timeout(DRAIN_GRACE, drain).await {
            Ok(read_error) => drain_output_error = read_error,
            Err(_) => drain_timed_out = true,
        }
    }
    // Abort/join the reader tasks unconditionally, so a permanently-pending reader
    // (blocked on a pipe an escaped descendant still holds open) cannot outlive the
    // supervisor. `shutdown()` aborts each task and awaits it — bounded by construction.
    readers.shutdown().await;

    let _ = advance(state, WorkerState::Exited);

    let reported_exit = match &termination {
        Termination::Natural(exit) => Some(*exit),
        Termination::Cancelled { exit, .. } => *exit,
        _ => None,
    };
    if let Some(exit) = reported_exit {
        let _ = pubr.emit(WorkerObservation::Exited(exit)).await;
    }
    if cleanup.is_ok() {
        let _ = pubr.emit(WorkerObservation::CleanupCompleted).await;
    }

    // Terminal precedence: an unprovable/failed cleanup (possible leaked
    // processes) dominates; a lost sink is next; otherwise the run's own outcome.
    let base = match termination {
        Termination::Natural(BoundaryExit::Code(code)) => WorkerResult::ExitedNormally(code),
        Termination::Natural(BoundaryExit::Signal(signal)) => WorkerResult::ExitedBySignal(signal),
        Termination::Cancelled {
            forceful: false, ..
        } => WorkerResult::CancelledGracefully,
        Termination::Cancelled { forceful: true, .. } => WorkerResult::CancelledForcefully,
        Termination::SinkFailed(message) => WorkerResult::ObservationFailure(message),
        Termination::OutputFailed(message) => WorkerResult::OutputFailure(message),
        Termination::BoundaryFailed(message) => WorkerResult::BoundaryFailure(message),
    };
    // Terminal precedence — CleanupFailure > ObservationFailure > Boundary/Output:
    //   1. an unprovable/failed cleanup (possible leaked processes) dominates all;
    //   2. a lost authoritative sink dominates a boundary/output fault — if the run
    //      ALSO ended on a boundary/output fault, that fault is preserved in the
    //      ObservationFailure message so it is never erased;
    //   3. a boundary/output fault that ended the run loop (sink still alive);
    //   4. a bounded-drain timeout or a drain-time read error (output faithfulness
    //      lost after the exit) — always a failure, never a clean pass;
    //   5. otherwise the run's own outcome.
    let result = if let Err(message) = cleanup {
        WorkerResult::CleanupFailure(message)
    } else if !pubr.alive {
        WorkerResult::ObservationFailure(observation_failure_message(pubr, &base))
    } else if base.is_failure() {
        base
    } else if drain_timed_out {
        WorkerResult::OutputFailure(format!(
            "trailing output drain exceeded {DRAIN_GRACE:?}; output faithfulness unproven \
             (a descendant that escaped the owned group may still hold a pipe open)"
        ))
    } else if let Some(message) = drain_output_error {
        WorkerResult::OutputFailure(message)
    } else {
        base
    };

    // If the sink is still usable and we failed, tell it (SupervisorFailed).
    if pubr.alive && result.is_failure() {
        let _ = pubr
            .emit(WorkerObservation::SupervisorFailed(result.message()))
            .await;
    }
    result
}

/// Group-based cancellation: SIGTERM the group; graceful means the WHOLE owned set
/// drains within the grace period; if any member survives it, escalate to SIGKILL
/// (forceful).
async fn run_cancellation(
    process: &mut dyn BoundaryProcess,
    grace: Duration,
    pubr: &mut Publisher,
) -> Termination {
    // A lost sink here is caught by the `!pubr.alive` terminal check, but teardown
    // must still proceed, so it is not an early return.
    let _ = pubr.emit(WorkerObservation::CancellationRequested).await;
    let deadline = Instant::now() + grace;
    // If the graceful stop itself FAILS, do not wait the grace period and then claim
    // a graceful cancel: force the group now and preserve the boundary fault. manage()
    // force-reaps and verifies cleanup before the terminal result.
    if let Err(err) = process.request_graceful_stop().await {
        return Termination::BoundaryFailed(format!("graceful stop failed: {err}"));
    }
    let _ = pubr.emit(WorkerObservation::GracefulStopSent).await;

    // Reap the leader (a zombie leader would otherwise count as a live member).
    let exit = match tokio::time::timeout(grace, process.wait()).await {
        Ok(Ok(exit)) => Some(exit),
        Ok(Err(err)) => return Termination::BoundaryFailed(err.to_string()),
        Err(_) => {
            // Leader itself did not exit within grace → force the group.
            return force_after_grace(process, pubr, None).await;
        }
    };

    // Leader gone; wait for the rest of the group to drain within the remaining grace.
    loop {
        match process.remaining_members().await {
            Ok(members) if members.is_empty() => {
                return Termination::Cancelled {
                    forceful: false,
                    exit,
                };
            }
            Ok(_) => {}
            Err(err) => return Termination::BoundaryFailed(err.to_string()),
        }
        if Instant::now() >= deadline {
            return force_after_grace(process, pubr, exit).await;
        }
        tokio::time::sleep(
            GROUP_POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
        )
        .await;
    }
}

async fn force_after_grace(
    process: &mut dyn BoundaryProcess,
    pubr: &mut Publisher,
    exit: Option<BoundaryExit>,
) -> Termination {
    let _ = pubr.emit(WorkerObservation::ForceStopSent).await;
    if let Err(err) = process.force_stop().await {
        return Termination::BoundaryFailed(err.to_string());
    }
    let exit = match exit {
        Some(exit) => Some(exit),
        None => match process.wait().await {
            Ok(exit) => Some(exit),
            Err(err) => return Termination::BoundaryFailed(err.to_string()),
        },
    };
    Termination::Cancelled {
        forceful: true,
        exit,
    }
}

/// Kill the whole owned group if anything remains, then PROVE it is gone. A
/// membership-query error is a cleanup failure (never "unknown means empty").
async fn cleanup_group(
    process: &mut dyn BoundaryProcess,
    pubr: &mut Publisher,
) -> Result<(), String> {
    let remaining = process
        .remaining_members()
        .await
        .map_err(|e| format!("membership query failed: {e}"))?;
    if remaining.is_empty() {
        return Ok(());
    }
    let _ = pubr
        .emit(WorkerObservation::DescendantsRemaining(remaining))
        .await;
    if let Err(err) = process.force_stop().await {
        return Err(format!("force stop during cleanup failed: {err}"));
    }
    let deadline = Instant::now() + CLEANUP_GRACE;
    loop {
        let survivors = process
            .remaining_members()
            .await
            .map_err(|e| format!("membership query failed: {e}"))?;
        if survivors.is_empty() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "{} owned process(es) survived cleanup",
                survivors.len()
            ));
        }
        tokio::time::sleep(GROUP_POLL_INTERVAL).await;
    }
}

/// Abandon a live, boundary-owned process on a post-spawn fault: force-kill the
/// set, reap the leader (so it is not mistaken for a survivor), then PROVE the group
/// is gone. Any force/membership fault is preserved as an `Err` so the caller can
/// let `CleanupFailure` dominate — never a best-effort kill that leaves the group
/// unverified.
async fn abandon_and_verify(
    process: &mut dyn BoundaryProcess,
    pubr: &mut Publisher,
) -> Result<(), String> {
    let force = process.force_stop().await;
    // Reap the leader regardless of the force result, so a zombie leader cannot be
    // mistaken for a live member during verification.
    let _ = process.wait().await;
    if let Err(err) = force {
        return Err(format!("force stop failed during cleanup: {err}"));
    }
    cleanup_group(process, pubr).await
}

/// Tick an OPTIONAL heartbeat timer. When heartbeats are disabled there is no
/// timer, so this stays pending forever (the select branch is also gated on
/// `heartbeat_enabled`, so it is never actually polled in that case — the `pending`
/// arm just keeps the future total without an `unwrap`).
async fn next_heartbeat(heartbeat: Option<&mut tokio::time::Interval>) {
    match heartbeat {
        Some(interval) => {
            interval.tick().await;
        }
        None => std::future::pending().await,
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

async fn read_stream(
    mut reader: BoundaryStream,
    stream: OutputStream,
    tx: mpsc::Sender<ReaderMessage>,
    max_chunk: usize,
) {
    let cap = max_chunk.max(1);
    let mut buf = vec![0u8; cap];
    let mut sequence: u64 = 0;
    loop {
        match reader.read(&mut buf).await {
            Ok(0) => break, // EOF
            Ok(n) => {
                let slice = buf.get(..n).unwrap_or(&[]);
                let chunk = OutputChunk {
                    stream,
                    sequence,
                    bytes: Bytes::copy_from_slice(slice),
                };
                sequence = sequence.wrapping_add(1);
                if tx.send(ReaderMessage::Chunk(chunk)).await.is_err() {
                    break;
                }
            }
            Err(err) => {
                // A read error is FATAL, never a silent EOF/truncation.
                let _ = tx.send(ReaderMessage::ReadError(err.to_string())).await;
                break;
            }
        }
    }
}
