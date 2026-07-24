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
/// How long a single post-force-stop leader reap (`wait()`) may take before it is
/// abandoned. After a `force_stop()` the kernel reaps a real child promptly; a
/// boundary whose `wait()` never completes (a hung Sandboy, a stuck mock) must NOT
/// hang the supervisor. A reap that exceeds this is a teardown that cannot be proven,
/// which the caller turns into a bounded `CleanupFailure` — never an unbounded wait.
const REAP_TIMEOUT: Duration = Duration::from_millis(500);
/// How long a single boundary CONTROL/QUERY op (`request_graceful_stop`, `force_stop`,
/// `remaining_members`) may take before it is abandoned. Each is bounded so a hung
/// boundary cannot stall cancellation/cleanup: a graceful-stop timeout escalates to
/// force immediately, and a force-stop / membership timeout is an unprovable teardown
/// that yields a bounded `CleanupFailure`.
const BOUNDARY_OP_TIMEOUT: Duration = Duration::from_millis(500);
/// Absolute ceiling on the TOTAL post-exit trailing-output drain. Bounds the drain's
/// wall-clock even when each individual publish stays within `sink_backpressure_timeout`
/// and the byte budget is never reached (an escaped descendant emitting one byte per
/// message could otherwise drive effectively unbounded sequential publishes). Checked
/// BETWEEN messages, so it never cancels an already-running publish before that
/// publish's own timeout: the worst-case terminal is `MAX_TRAILING_DRAIN + one
/// sink_backpressure_timeout`.
pub const MAX_TRAILING_DRAIN: Duration = Duration::from_secs(3);
/// How long to wait for the NEXT trailing-output message (a chunk, a read error, or
/// EOF) during the post-exit drain. This bounds ONLY the wait for pipe/reader activity
/// — it is NOT wrapped around the sink publish, which has its own
/// `OutputPolicy::sink_backpressure_timeout`. A descendant that ESCAPED the owned group
/// can hold an inherited pipe open with no further output; if nothing arrives within
/// this idle window the drain concludes (a failure when the pipes never closed).
const DRAIN_IDLE_TIMEOUT: Duration = Duration::from_millis(500);

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
fn observation_failure_message(pubr: &Publisher, effective: &WorkerResult) -> String {
    let sink_error = pubr.error();
    match effective {
        WorkerResult::BoundaryFailure(underlying) | WorkerResult::OutputFailure(underlying) => {
            format!("{sink_error}; underlying fault preserved: {underlying}")
        }
        // An effective `ObservationFailure` (a run-loop `SinkFailed`) is the SAME sink
        // loss — no distinct underlying fault; any success/cancel outcome carries none.
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
        // unprovable/failed cleanup (leaked processes) DOMINATES the sink failure — but
        // the PRIMARY sink fault (the lost Spawned publish) is preserved FIRST so the
        // cause is never erased.
        let sink_fault = pubr.error();
        return match abandon_and_verify(process.as_mut(), &mut pubr).await {
            Err(message) => {
                WorkerResult::CleanupFailure(format!("primary sink fault: {sink_fault}; {message}"))
            }
            Ok(()) => WorkerResult::ObservationFailure(sink_fault),
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

/// Reasons the run phase ended, before final cleanup. The fault variants carry `reaped`:
/// `Some(exit)` when the leader's exit was ALREADY successfully observed before the fault
/// (so the emergency teardown must NOT `wait()` again — a one-shot boundary cannot repeat
/// a consumed exit), `None` when the leader is still live / unreaped.
enum Termination {
    Natural(BoundaryExit),
    Cancelled {
        forceful: bool,
        exit: Option<BoundaryExit>,
    },
    SinkFailed {
        message: String,
        reaped: Option<BoundaryExit>,
    },
    OutputFailed {
        message: String,
        reaped: Option<BoundaryExit>,
    },
    BoundaryFailed {
        message: String,
        reaped: Option<BoundaryExit>,
    },
}

impl Termination {
    /// A boundary fault whose leader is NOT yet reaped (the common case).
    fn boundary_failed(message: String) -> Self {
        Self::BoundaryFailed {
            message,
            reaped: None,
        }
    }
}

/// The PRIMARY fault message that initiated teardown, if the run ended on a fault. Used
/// to prepend the cause to a composed `CleanupFailure` so it is never erased by the later
/// force/reap/cleanup faults. A natural exit or a clean cancel has no primary fault.
fn primary_fault_message(termination: &Termination) -> Option<String> {
    match termination {
        Termination::SinkFailed { message, .. } => Some(format!("primary sink fault: {message}")),
        Termination::OutputFailed { message, .. } => {
            Some(format!("primary output fault: {message}"))
        }
        Termination::BoundaryFailed { message, .. } => {
            Some(format!("primary boundary fault: {message}"))
        }
        Termination::Natural(_) | Termination::Cancelled { .. } => None,
    }
}

/// Whether this termination is a fault (needs emergency teardown), and — if so — the
/// already-observed leader exit (so the caller can skip a redundant `wait()`).
fn fault_reaped(termination: &Termination) -> Option<Option<BoundaryExit>> {
    match termination {
        Termination::SinkFailed { reaped, .. }
        | Termination::OutputFailed { reaped, .. }
        | Termination::BoundaryFailed { reaped, .. } => Some(*reaped),
        Termination::Natural(_) | Termination::Cancelled { .. } => None,
    }
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
        Phase::SinkFailed(m) => Termination::SinkFailed {
            message: m,
            reaped: None,
        },
        Phase::OutputFailed(m) => Termination::OutputFailed {
            message: m,
            reaped: None,
        },
        Phase::BoundaryFailed(m) => Termination::boundary_failed(m),
        Phase::CancelRequested => {
            let _ = advance(state, WorkerState::Cancelling);
            run_cancellation(process.as_mut(), spec.cancellation.graceful_timeout, pubr).await
        }
    };

    // Reap the leader in the fault paths (Natural/Cancelled already waited). A
    // killed-but-unreaped child is a zombie cleanup would mistake for a survivor. Every
    // teardown fault (force, reap, then cleanup below) is ACCUMULATED in execution order
    // and composed into one `CleanupFailure`, so no underlying failure is dropped.
    let mut teardown_faults: Vec<String> = Vec::new();
    if let Some(already_reaped) = fault_reaped(&termination) {
        // This emergency SIGKILL is a REAL teardown action, so it must not be
        // invisible to the authoritative stream (PR-4's source-of-truth adapter maps
        // it to a canonical event). Publish — and honor — `ForceStopSent` before
        // performing it. If the sink is already dead (`SinkFailed`) this is a no-op;
        // if it dies HERE, `!pubr.alive` makes `ObservationFailure` dominate the
        // boundary/output fault in the terminal-precedence check below.
        let _ = pubr.emit(WorkerObservation::ForceStopSent).await;
        // BOUNDED force: a hung force delivery must not hang here.
        if let Err(err) = bounded_force_stop(process.as_mut()).await {
            teardown_faults.push(err);
        }
        // BOUNDED reap — but ONLY if the leader was not already reaped. Re-calling
        // `wait()` after a successful, consumed exit is not part of the trait contract
        // (a one-shot boundary would error), so a fault whose leader is already reaped
        // must not `wait()` again.
        if already_reaped.is_none() {
            if let Err(err) = bounded_reap(process.as_mut()).await {
                teardown_faults.push(err);
            }
        }
    }

    // Verified cleanup — kill any surviving group members so their pipes close
    // (otherwise draining could block on a grandchild that only dies now). Its fault
    // (membership/force/survivors), if any, is appended AFTER the force/reap faults so
    // the combined `CleanupFailure` preserves every underlying failure in order.
    if let Err(message) = cleanup_group(process.as_mut(), pubr).await {
        teardown_faults.push(message);
    }
    // Compose the terminal cleanup outcome. When there ARE teardown faults, the primary
    // fault that INITIATED teardown (the boundary/output/sink termination) is prepended,
    // so the dominating `CleanupFailure` preserves the CAUSE in chronological order:
    //   primary → force → reap → membership/cleanup.
    // When teardown SUCCEEDS, there is nothing to prepend and the base fault stands (it
    // already carries the cause), so a clean cancel/exit is not turned into a failure.
    let cleanup: Result<(), String> = if teardown_faults.is_empty() {
        Ok(())
    } else {
        let mut composed = Vec::with_capacity(teardown_faults.len() + 1);
        if let Some(primary) = primary_fault_message(&termination) {
            composed.push(primary);
        }
        composed.extend(teardown_faults);
        Err(composed.join("; "))
    };

    // Drain remaining output (pipes closing), then join readers. A read error seen
    // ONLY during the drain (leader already exited) must NOT be silently dropped —
    // it means output faithfulness was lost after the exit.
    //
    // The drain MUST be bounded, but no bound may cancel a healthy in-flight sink
    // publish. `out_rx.recv()` only returns `None` once every reader task ends (pipes
    // closed); an escaped descendant can hold an inherited pipe open — with no further
    // output, or by writing forever — so an unbounded drain would hang. Bounds, none of
    // which caps a legitimate publish:
    //   * per-message WAIT: bounded by `min(DRAIN_IDLE_TIMEOUT, remaining total time)`, so
    //     an idle-but-open pipe expires AND the wait never overshoots the total deadline.
    //   * per-message PUBLISH: `pubr.emit` keeps its own `sink_backpressure_timeout`;
    //     the drain never wraps it, so a slow-but-within-contract sink is not cancelled.
    //   * TOTAL-TIME deadline (`MAX_TRAILING_DRAIN`): the hard ceiling for endless output.
    //     Because the recv wait is capped by the remaining time and a publish is never
    //     interrupted, the worst case is exactly `MAX_TRAILING_DRAIN + one sink timeout`.
    //   * OPTIONAL explicit byte cap (`OutputPolicy::max_trailing_bytes`): a deliberate
    //     caller ceiling, never inferred (a BoundaryStream's max buffering is unknown).
    // On a cleanup error the owned set is not proven gone, so skip the drain entirely
    // and let `CleanupFailure` dominate.
    let mut drain_output_error: Option<String> = None;
    let mut drain_timed_out = false;
    let mut drain_budget_exceeded = false;
    let mut drain_deadline_exceeded = false;
    if cleanup.is_ok() && pubr.alive {
        // An EXPLICIT caller cap (never inferred): `None` applies no byte ceiling — a
        // BoundaryStream's maximum post-exit buffering is unknown, so legitimate finite
        // output of any size must drain to EOF cleanly. Endless output is bounded by the
        // total-time deadline below regardless.
        let byte_cap = spec.output.max_trailing_bytes;
        let total_deadline = Instant::now() + MAX_TRAILING_DRAIN;
        let mut drained_bytes: usize = 0;
        loop {
            // Bound the wait for the next message by the MINIMUM of the idle timeout and
            // the remaining total-drain time, so the total ceiling is
            // `MAX_TRAILING_DRAIN + one sink timeout` — never `+ DRAIN_IDLE_TIMEOUT` on
            // top. This also serves as the pre-message total-deadline check.
            let remaining = total_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                drain_deadline_exceeded = true;
                break;
            }
            let recv_bound = DRAIN_IDLE_TIMEOUT.min(remaining);
            // Bound the WAIT for the next message only — never the publish below.
            let message = match tokio::time::timeout(recv_bound, out_rx.recv()).await {
                Ok(Some(message)) => message,
                Ok(None) => break, // all readers ended: pipes closed, clean drain.
                Err(_) => {
                    // Distinguish the total-deadline expiry from a mere idle gap.
                    if Instant::now() >= total_deadline {
                        drain_deadline_exceeded = true;
                    } else {
                        drain_timed_out = true;
                    }
                    break;
                }
            };
            match message {
                ReaderMessage::Chunk(chunk) => {
                    let len = chunk.bytes.len();
                    // ENFORCE the explicit cap BEFORE publishing (a true ceiling, not a
                    // post-hoc alarm): if this whole chunk would push the total OVER the
                    // cap, do NOT publish it — chunks stay atomic (never a partial chunk).
                    // Reaching the cap exactly is allowed; only strictly exceeding fails.
                    if let Some(cap) = byte_cap {
                        let within = drained_bytes
                            .checked_add(len)
                            .is_some_and(|total| total <= cap);
                        if !within {
                            drain_budget_exceeded = true;
                            break; // chunk NOT published
                        }
                    }
                    // `emit` is bounded by `sink_backpressure_timeout`, NOT the drain.
                    if !pubr.emit(WorkerObservation::OutputChunk(chunk)).await {
                        break; // sink lost mid-drain → caught by `!pubr.alive` below.
                    }
                    drained_bytes = drained_bytes.saturating_add(len);
                }
                ReaderMessage::ReadError(message) => {
                    drain_output_error = Some(message);
                    break;
                }
            }
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
        // A fault whose leader was already reaped can still report that exit.
        Termination::SinkFailed { reaped, .. }
        | Termination::OutputFailed { reaped, .. }
        | Termination::BoundaryFailed { reaped, .. } => *reaped,
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
        Termination::SinkFailed { message, .. } => WorkerResult::ObservationFailure(message),
        Termination::OutputFailed { message, .. } => WorkerResult::OutputFailure(message),
        Termination::BoundaryFailed { message, .. } => WorkerResult::BoundaryFailure(message),
    };
    // Fold the run outcome and any drain fault into a single EFFECTIVE result FIRST,
    // before sink-loss precedence is applied. A bounded-drain timeout or a drain-time
    // read error means output faithfulness was lost AFTER the exit, so it is an
    // OutputFailure that overrides a clean/cancel outcome — but not a run-loop fault,
    // which is already at least as severe. Materializing it here (rather than after the
    // sink-loss check) is what stops a lost sink on `Exited`/`CleanupCompleted` from
    // silently erasing the drain fault: the effective fault is carried into the
    // ObservationFailure message below.
    let effective = if base.is_failure() {
        base
    } else if drain_timed_out {
        WorkerResult::OutputFailure(format!(
            "trailing output drain saw no further output for {DRAIN_IDLE_TIMEOUT:?} but the \
             pipes never closed; output faithfulness unproven (a descendant that escaped the \
             owned group may still hold a pipe open)"
        ))
    } else if drain_budget_exceeded {
        WorkerResult::OutputFailure(
            "trailing output exceeded the configured max_trailing_bytes cap; \
             output faithfulness unproven"
                .to_owned(),
        )
    } else if drain_deadline_exceeded {
        WorkerResult::OutputFailure(format!(
            "trailing output drain exceeded its total time budget of {MAX_TRAILING_DRAIN:?} \
             while output kept arriving; an escaped descendant is writing without end, so \
             faithful capture is unproven"
        ))
    } else if let Some(message) = drain_output_error {
        WorkerResult::OutputFailure(message)
    } else {
        base
    };

    // Terminal precedence is computed by `finalize`, which reads the CURRENT sink-alive
    // state. It must be applied AFTER the `SupervisorFailed` publish, because that publish
    // is itself an authoritative action that can lose the sink. So: compute a TENTATIVE
    // result to obtain the message, attempt the `SupervisorFailed` publish (honoring its
    // outcome via `pubr.alive`), then re-finalize with the post-publish sink state.
    let tentative = finalize(&cleanup, pubr, &effective);
    if pubr.alive && tentative.is_failure() {
        let _ = pubr
            .emit(WorkerObservation::SupervisorFailed(tentative.message()))
            .await;
    }
    finalize(&cleanup, pubr, &effective)
}

/// Terminal precedence — CleanupFailure > ObservationFailure > Boundary/Output — using
/// the CURRENT sink-alive state. When cleanup failed, `CleanupFailure` dominates but a
/// concurrently-lost sink is still preserved in the message. When cleanup succeeded but
/// the sink was lost, the effective boundary/output fault is preserved inside the
/// `ObservationFailure` message. Otherwise the effective outcome stands.
fn finalize(
    cleanup: &Result<(), String>,
    pubr: &Publisher,
    effective: &WorkerResult,
) -> WorkerResult {
    match cleanup {
        Err(message) => {
            if pubr.alive {
                WorkerResult::CleanupFailure(message.clone())
            } else {
                // The authoritative sink was ALSO lost — preserve BOTH faults.
                WorkerResult::CleanupFailure(format!(
                    "{message}; observation sink lost: {}",
                    pubr.error()
                ))
            }
        }
        Ok(()) => {
            if pubr.alive {
                effective.clone()
            } else {
                WorkerResult::ObservationFailure(observation_failure_message(pubr, effective))
            }
        }
    }
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
    // The graceful stop is BOUNDED. If it FAILS, do not wait the grace period and then
    // claim a graceful cancel: preserve the boundary fault (manage() force-reaps and
    // verifies cleanup before the terminal). If it TIMES OUT, the boundary never even
    // acknowledged the graceful request — escalate to force IMMEDIATELY rather than
    // waiting out the grace.
    match bounded_graceful_stop(process).await {
        GracefulStop::Ok => {}
        GracefulStop::Failed(err) => return Termination::boundary_failed(err),
        GracefulStop::TimedOut => return force_after_grace(process, pubr, None).await,
    }
    let _ = pubr.emit(WorkerObservation::GracefulStopSent).await;

    // Reap the leader (a zombie leader would otherwise count as a live member).
    let exit = match tokio::time::timeout(grace, process.wait()).await {
        Ok(Ok(exit)) => Some(exit),
        Ok(Err(err)) => return Termination::boundary_failed(err.to_string()),
        Err(_) => {
            // Leader itself did not exit within grace → force the group.
            return force_after_grace(process, pubr, None).await;
        }
    };

    // Leader gone; wait for the rest of the group to drain within the remaining grace.
    loop {
        // BOUNDED membership. A query error/timeout means the AUTHORITATIVE membership
        // mechanism FAILED — that fault must not vanish into a clean cancel. Route it
        // through the manage() fault path as a `BoundaryFailed`: manage force-kills the
        // group and verifies cleanup, then composes the outcome — a recovered cleanup
        // yields `BoundaryFailure` (preserving this fault), an unprovable one yields
        // `CleanupFailure` (composing both). Never a clean `CancelledForcefully`.
        match bounded_members(process).await {
            Ok(members) if members.is_empty() => {
                return Termination::Cancelled {
                    forceful: false,
                    exit,
                };
            }
            Ok(_) => {}
            Err(fault) => {
                // The leader was ALREADY reaped above (`exit`), so carry it so manage()
                // does NOT `wait()` again — a one-shot boundary cannot repeat the exit.
                return Termination::BoundaryFailed {
                    message: format!("membership query during graceful drain: {fault}"),
                    reaped: exit,
                };
            }
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
    // BOUNDED force-stop: a hung force delivery must not stall cancellation. PRESERVE the
    // already-observed exit (the drain-loop path reaps the leader BEFORE escalating), so
    // manage() does NOT `wait()` again — a one-shot boundary's consumed exit is not
    // repeatable, and re-reaping would fabricate a spurious cleanup fault.
    if let Err(err) = bounded_force_stop(process).await {
        return Termination::BoundaryFailed {
            message: err,
            reaped: exit,
        };
    }
    let exit = match exit {
        Some(exit) => Some(exit),
        // BOUNDED reap: a boundary whose `wait()` never resolves after a successful
        // force-stop must not hang cancellation. A timed-out/failed reap is a boundary
        // fault; manage() then force-reaps (also bounded) and lets cleanup precedence
        // produce a bounded terminal.
        None => match bounded_reap(process).await {
            Ok(exit) => Some(exit),
            Err(err) => return Termination::boundary_failed(err),
        },
    };
    Termination::Cancelled {
        forceful: true,
        exit,
    }
}

/// Kill the whole owned group if anything remains, then PROVE it is gone. Every
/// boundary op is BOUNDED: a membership-query or force-stop that errors OR times out is
/// a cleanup failure (never "unknown means empty", never an unbounded wait).
async fn cleanup_group(
    process: &mut dyn BoundaryProcess,
    pubr: &mut Publisher,
) -> Result<(), String> {
    let remaining = bounded_members(process).await?;
    if remaining.is_empty() {
        return Ok(());
    }
    let _ = pubr
        .emit(WorkerObservation::DescendantsRemaining(remaining))
        .await;
    bounded_force_stop(process)
        .await
        .map_err(|e| format!("force stop during cleanup: {e}"))?;
    let deadline = Instant::now() + CLEANUP_GRACE;
    loop {
        let survivors = bounded_members(process).await?;
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

/// Reap the leader after a force-stop, BOUNDED by [`REAP_TIMEOUT`]. Returns the exit
/// when observed, or an `Err` when `wait()` itself failed or did not complete in time.
/// The bound is what keeps a boundary whose `wait()` never resolves from hanging the
/// supervisor forever; the caller lets an `Err` make teardown unprovable (a bounded
/// `CleanupFailure` / `BoundaryFailure`) instead of blocking.
async fn bounded_reap(process: &mut dyn BoundaryProcess) -> Result<BoundaryExit, String> {
    match tokio::time::timeout(REAP_TIMEOUT, process.wait()).await {
        Ok(Ok(exit)) => Ok(exit),
        Ok(Err(err)) => Err(format!("leader wait failed after force-stop: {err}")),
        Err(_) => Err(format!(
            "leader did not exit within {REAP_TIMEOUT:?} of force-stop; teardown unprovable"
        )),
    }
}

/// Outcome of a BOUNDED graceful stop: it succeeded, it returned an error, or it did
/// not complete within [`BOUNDARY_OP_TIMEOUT`]. A timeout is distinct because it
/// escalates immediately to force rather than being reported as a boundary error.
enum GracefulStop {
    Ok,
    Failed(String),
    TimedOut,
}

/// Bound `request_graceful_stop` — a hung boundary must not stall cancellation.
async fn bounded_graceful_stop(process: &mut dyn BoundaryProcess) -> GracefulStop {
    match tokio::time::timeout(BOUNDARY_OP_TIMEOUT, process.request_graceful_stop()).await {
        Ok(Ok(())) => GracefulStop::Ok,
        Ok(Err(err)) => GracefulStop::Failed(format!("graceful stop failed: {err}")),
        Err(_) => GracefulStop::TimedOut,
    }
}

/// Bound `force_stop` — a hung force delivery is an unprovable teardown, not a hang.
async fn bounded_force_stop(process: &mut dyn BoundaryProcess) -> Result<(), String> {
    match tokio::time::timeout(BOUNDARY_OP_TIMEOUT, process.force_stop()).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(format!("force stop failed: {err}")),
        Err(_) => Err(format!(
            "force stop did not complete within {BOUNDARY_OP_TIMEOUT:?}; teardown unprovable"
        )),
    }
}

/// Bound `remaining_members` — a hung membership query is an unprovable teardown,
/// never "unknown means empty". Takes `&mut` (not `&`) so the awaited future stays
/// `Send`: `&dyn BoundaryProcess` would require `Sync`, which the trait does not demand.
async fn bounded_members(
    process: &mut dyn BoundaryProcess,
) -> Result<Vec<ProcessIdentity>, String> {
    match tokio::time::timeout(BOUNDARY_OP_TIMEOUT, process.remaining_members()).await {
        Ok(Ok(members)) => Ok(members),
        // `BoundaryError::Membership` already reads "membership query failed: …" — do not
        // re-prefix it.
        Ok(Err(err)) => Err(err.to_string()),
        Err(_) => Err(format!(
            "membership query did not complete within {BOUNDARY_OP_TIMEOUT:?}; teardown unprovable"
        )),
    }
}

/// Abandon a live, boundary-owned process on a post-spawn fault: force-kill the
/// set, reap the leader (so it is not mistaken for a survivor), then PROVE the group
/// is gone. Every boundary op is BOUNDED, and ALL faults (force, reap, then cleanup)
/// are accumulated in execution order into one `Err` so the caller lets a combined
/// `CleanupFailure` dominate — never a best-effort kill that leaves the group
/// unverified, never an unbounded wait, and never a single fault masking the others.
async fn abandon_and_verify(
    process: &mut dyn BoundaryProcess,
    pubr: &mut Publisher,
) -> Result<(), String> {
    let mut faults: Vec<String> = Vec::new();
    // Force-kill the set (bounded), then reap the leader regardless of the force result
    // (bounded) so a zombie leader cannot be mistaken for a live member. Collect BOTH.
    if let Err(err) = bounded_force_stop(process).await {
        faults.push(err);
    }
    if let Err(err) = bounded_reap(process).await {
        faults.push(err);
    }
    // Still attempt verification and append its fault too — more diagnostic signal, and
    // it may prove the group gone even when the reap could not be observed.
    if let Err(err) = cleanup_group(process, pubr).await {
        faults.push(err);
    }
    if faults.is_empty() {
        Ok(())
    } else {
        Err(faults.join("; "))
    }
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
