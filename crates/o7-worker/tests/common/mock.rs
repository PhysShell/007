//! An in-memory [`ProcessBoundary`] for fault injection.
//!
//! The real subprocess suite proves the happy path of the host boundary; it
//! cannot, on demand, make a membership query fail, make a pipe read error mid
//! stream, or make `spawn` hang. This mock drives exactly those generic-seam
//! faults so the supervisor's fail-closed semantics can be asserted
//! deterministically. It never touches the OS.
//!
//! Lint levels cascade from `common`'s inner `#![allow(...)]`, so test-grade
//! `unwrap`/indexing/`dead_code` are permitted here too.

use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::Notify;

use o7_worker::{
    BoundaryAttestation, BoundaryError, BoundaryExit, BoundaryKind, BoundaryProcess,
    BoundarySpawnSpec, BoundaryStream, EnforcementLevel, ProcessBoundary, ProcessIdentity,
};

/// Observable record of what the mock boundary was asked to do. Cloned out via
/// [`MockBoundary::state`] before the boundary is handed to the supervisor.
#[derive(Debug, Default)]
pub struct MockState {
    spawn_entered: AtomicBool,
    spawn_committed: AtomicBool,
    spawn_dropped_before_commit: AtomicBool,
    graceful_stops: AtomicUsize,
    force_stops: AtomicUsize,
    membership_queries: AtomicUsize,
}

impl MockState {
    /// The spawn future began running (entered the boundary's `spawn`).
    pub fn entered_spawn(&self) -> bool {
        self.spawn_entered.load(Ordering::SeqCst)
    }

    /// The spawn future ran to completion and produced a process the supervisor
    /// took ownership of.
    pub fn committed_spawn(&self) -> bool {
        self.spawn_committed.load(Ordering::SeqCst)
    }

    /// The spawn future was dropped BEFORE it committed a process (the cancel-safe
    /// drop path). Proves a cancel racing a slow spawn leaves nothing ownerless.
    pub fn dropped_before_commit(&self) -> bool {
        self.spawn_dropped_before_commit.load(Ordering::SeqCst)
    }

    pub fn graceful_stops(&self) -> usize {
        self.graceful_stops.load(Ordering::SeqCst)
    }

    pub fn force_stops(&self) -> usize {
        self.force_stops.load(Ordering::SeqCst)
    }

    pub fn membership_queries(&self) -> usize {
        self.membership_queries.load(Ordering::SeqCst)
    }
}

/// When the mock leader's `wait()` resolves.
#[derive(Clone, Copy)]
enum WaitGate {
    /// Resolves immediately — the leader has already exited.
    Immediate,
    /// Resolves only once `force_stop()` has been called — models a live leader
    /// that dies only when killed (so another select branch, e.g. a read error,
    /// can win first).
    AfterForceStop,
}

/// How the mock answers the AUTHORITATIVE membership query.
#[derive(Clone)]
enum Membership {
    /// No surviving members — cleanup can prove the set empty.
    Empty,
    /// Every query fails — the boundary can never prove the set's state, so
    /// cleanup must fail closed rather than assume "empty".
    Error(String),
}

/// A configurable, OS-free boundary. Defaults to the most boring possible run: a
/// leader that has already exited cleanly, no output, a provably-empty set.
pub struct MockBoundary {
    attestation: BoundaryAttestation,
    spawn_delay: Duration,
    stdout_delay: Duration,
    stdout_chunks: Vec<Vec<u8>>,
    stdout_error: Option<String>,
    /// stdout is a reader that never yields and never closes (always `Pending`),
    /// modelling a descendant that escaped the owned group but still holds the
    /// inherited pipe open, so the drain can never observe EOF.
    stdout_pending: bool,
    leader_exit: BoundaryExit,
    wait_gate: WaitGate,
    membership: Membership,
    graceful_stop_error: Option<String>,
    force_stop_error: Option<String>,
    state: Arc<MockState>,
}

impl MockBoundary {
    pub fn new() -> Self {
        Self {
            attestation: BoundaryAttestation {
                implementation: BoundaryKind::UnconfinedHost,
                enforcement: EnforcementLevel::None,
            },
            spawn_delay: Duration::ZERO,
            stdout_delay: Duration::ZERO,
            stdout_chunks: Vec::new(),
            stdout_error: None,
            stdout_pending: false,
            leader_exit: BoundaryExit::Code(0),
            wait_gate: WaitGate::Immediate,
            membership: Membership::Empty,
            graceful_stop_error: None,
            force_stop_error: None,
            state: Arc::new(MockState::default()),
        }
    }

    /// A handle to inspect the boundary's effects after the run.
    pub fn state(&self) -> Arc<MockState> {
        Arc::clone(&self.state)
    }

    /// Delay every spawn by `delay`. Per the `ProcessBoundary` cancel-safety
    /// contract, dropping the spawn future before it completes must not leak a
    /// process — the mock records that drop as [`MockState::dropped_before_commit`].
    pub fn with_spawn_delay(mut self, delay: Duration) -> Self {
        self.spawn_delay = delay;
        self
    }

    /// stdout yields `chunks` and then a FATAL read error; the leader stays alive
    /// until force-stopped so the read-error branch is what ends the run. Proves a
    /// pipe read error is surfaced, never a silent truncation.
    pub fn with_stdout_then_read_error(mut self, chunks: Vec<Vec<u8>>, error: &str) -> Self {
        self.stdout_chunks = chunks;
        self.stdout_error = Some(error.to_owned());
        self.wait_gate = WaitGate::AfterForceStop;
        self
    }

    /// The leader stays alive until it is force-stopped — a live process to cancel
    /// or abandon (so the run doesn't end via a "leader already exited").
    pub fn with_live_leader(mut self) -> Self {
        self.wait_gate = WaitGate::AfterForceStop;
        self
    }

    /// stdout stays quiet for `delay` (past the leader's exit), then yields `chunks`
    /// and a fatal read error — so the error is seen during the POST-EXIT drain, not
    /// the run loop. The leader exits immediately.
    pub fn with_stdout_error_after_exit(
        mut self,
        chunks: Vec<Vec<u8>>,
        error: &str,
        delay: Duration,
    ) -> Self {
        self.stdout_chunks = chunks;
        self.stdout_error = Some(error.to_owned());
        self.stdout_delay = delay;
        self
    }

    /// stdout is a pipe that NEVER closes and never yields — the reader task stays
    /// pending forever. Models a descendant that escaped the owned group yet still
    /// holds the inherited stdout pipe open, so `out_rx.recv()` would block forever
    /// unless the drain is bounded. The leader exits immediately (default), so the run
    /// reaches the drain/cleanup phase with a still-open pipe.
    pub fn with_pending_stdout(mut self) -> Self {
        self.stdout_pending = true;
        self
    }

    /// Every membership query fails: the boundary can never prove the owned set is
    /// gone, so cleanup must fail closed.
    pub fn with_membership_error(mut self, error: &str) -> Self {
        self.membership = Membership::Error(error.to_owned());
        self
    }

    /// `request_graceful_stop()` fails; the leader stays alive until force-stopped so
    /// there is a live process at cancel time.
    pub fn with_graceful_stop_error(mut self, error: &str) -> Self {
        self.graceful_stop_error = Some(error.to_owned());
        self.wait_gate = WaitGate::AfterForceStop;
        self
    }

    /// `force_stop()` fails (still counted + reaped), so verified cleanup cannot be
    /// proven and `CleanupFailure` must dominate.
    pub fn with_force_stop_error(mut self, error: &str) -> Self {
        self.force_stop_error = Some(error.to_owned());
        self
    }

    pub fn boxed(self) -> Box<dyn ProcessBoundary> {
        Box::new(self)
    }
}

impl Default for MockBoundary {
    fn default() -> Self {
        Self::new()
    }
}

/// RAII marker: `enter` records that the spawn future started; `commit` records a
/// successfully handed-over process; being dropped without a commit records the
/// cancel-safe cleanup of a partially-created process.
struct SpawnGuard {
    state: Arc<MockState>,
    committed: bool,
}

impl SpawnGuard {
    fn enter(state: Arc<MockState>) -> Self {
        state.spawn_entered.store(true, Ordering::SeqCst);
        Self {
            state,
            committed: false,
        }
    }

    fn commit(mut self) {
        self.committed = true;
        self.state.spawn_committed.store(true, Ordering::SeqCst);
    }
}

impl Drop for SpawnGuard {
    fn drop(&mut self) {
        if !self.committed {
            self.state
                .spawn_dropped_before_commit
                .store(true, Ordering::SeqCst);
        }
    }
}

fn mock_identity() -> ProcessIdentity {
    ProcessIdentity {
        pid: 424_242,
        process_group: 424_242,
        start_time_ticks: 7,
    }
}

#[async_trait]
impl ProcessBoundary for MockBoundary {
    async fn spawn(
        &self,
        _spec: BoundarySpawnSpec,
    ) -> Result<Box<dyn BoundaryProcess>, BoundaryError> {
        let guard = SpawnGuard::enter(Arc::clone(&self.state));
        if !self.spawn_delay.is_zero() {
            // If the supervisor cancels during `Starting`, it drops this future at
            // the await below; `guard`'s Drop then records the cancel-safe cleanup.
            tokio::time::sleep(self.spawn_delay).await;
        }
        let stdout = if self.stdout_pending {
            // A stream that never yields and never closes — the reader stays pending,
            // so only a BOUNDED drain can keep the supervisor from hanging on it.
            Some(Box::pin(PendingReader) as BoundaryStream)
        } else if self.stdout_chunks.is_empty() && self.stdout_error.is_none() {
            None
        } else {
            Some(Box::pin(ScriptedReader::new(
                self.stdout_delay,
                self.stdout_chunks.clone(),
                self.stdout_error.clone(),
            )) as BoundaryStream)
        };
        let process = MockProcess {
            identity: mock_identity(),
            // A boxed `dyn AsyncRead + Send` is not `Sync`, but `BoundaryProcess`'s
            // `&self` async methods require `Self: Sync` (via async_trait). The real
            // host process sidesteps this by storing `Child` and taking the pipe out
            // of it; the mock holds the stream directly, so a `Mutex` restores Sync.
            stdout: Mutex::new(stdout),
            leader_exit: self.leader_exit,
            wait_gate: self.wait_gate,
            membership: self.membership.clone(),
            graceful_stop_error: self.graceful_stop_error.clone(),
            force_stop_error: self.force_stop_error.clone(),
            force_notify: Arc::new(Notify::new()),
            state: Arc::clone(&self.state),
        };
        guard.commit();
        Ok(Box::new(process))
    }

    fn attestation(&self) -> BoundaryAttestation {
        self.attestation
    }
}

struct MockProcess {
    identity: ProcessIdentity,
    stdout: Mutex<Option<BoundaryStream>>,
    leader_exit: BoundaryExit,
    wait_gate: WaitGate,
    membership: Membership,
    graceful_stop_error: Option<String>,
    force_stop_error: Option<String>,
    force_notify: Arc<Notify>,
    state: Arc<MockState>,
}

#[async_trait]
impl BoundaryProcess for MockProcess {
    fn identity(&self) -> ProcessIdentity {
        self.identity.clone()
    }

    fn take_stdout(&mut self) -> Option<BoundaryStream> {
        match self.stdout.get_mut() {
            Ok(slot) => slot.take(),
            Err(_) => None,
        }
    }

    fn take_stderr(&mut self) -> Option<BoundaryStream> {
        None
    }

    async fn request_graceful_stop(&mut self) -> Result<(), BoundaryError> {
        self.state.graceful_stops.fetch_add(1, Ordering::SeqCst);
        if let Some(err) = &self.graceful_stop_error {
            return Err(BoundaryError::Signal(err.clone()));
        }
        Ok(())
    }

    async fn force_stop(&mut self) -> Result<(), BoundaryError> {
        self.state.force_stops.fetch_add(1, Ordering::SeqCst);
        // Wake any wait() that is gated on a force-stop, even when reporting an
        // error, so a gated wait() cannot hang after a failed force-stop.
        self.force_notify.notify_one();
        if let Some(err) = &self.force_stop_error {
            return Err(BoundaryError::Signal(err.clone()));
        }
        Ok(())
    }

    async fn wait(&mut self) -> Result<BoundaryExit, BoundaryError> {
        if let WaitGate::AfterForceStop = self.wait_gate {
            if self.state.force_stops.load(Ordering::SeqCst) == 0 {
                self.force_notify.notified().await;
            }
        }
        Ok(self.leader_exit)
    }

    async fn remaining_members(&self) -> Result<Vec<ProcessIdentity>, BoundaryError> {
        self.state.membership_queries.fetch_add(1, Ordering::SeqCst);
        match &self.membership {
            Membership::Empty => Ok(Vec::new()),
            Membership::Error(err) => Err(BoundaryError::Membership(err.clone())),
        }
    }
}

/// An `AsyncRead` that is ALWAYS `Pending`: it never yields bytes and never reaches
/// EOF, modelling an inherited pipe an escaped descendant holds open forever. The
/// reader task blocked on it only ends when the supervisor aborts it (via
/// `JoinSet::shutdown`), so it proves the trailing-output drain is bounded.
struct PendingReader;

impl AsyncRead for PendingReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // Never register a waker: this future is parked until the task is aborted.
        Poll::Pending
    }
}

/// An `AsyncRead` that optionally stays `Pending` for a delay, then yields a fixed
/// script of byte chunks and then, optionally, a single fatal I/O error. The delay
/// lets a test make the leader exit BEFORE the stream produces, so the read error is
/// seen during the post-exit drain.
struct ScriptedReader {
    delay: Option<Pin<Box<tokio::time::Sleep>>>,
    chunks: VecDeque<Vec<u8>>,
    error: Option<String>,
}

impl ScriptedReader {
    fn new(delay: Duration, chunks: Vec<Vec<u8>>, error: Option<String>) -> Self {
        Self {
            delay: (!delay.is_zero()).then(|| Box::pin(tokio::time::sleep(delay))),
            chunks: chunks.into_iter().collect(),
            error,
        }
    }
}

impl AsyncRead for ScriptedReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if let Some(delay) = this.delay.as_mut() {
            match delay.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(()) => this.delay = None,
            }
        }
        if let Some(chunk) = this.chunks.front_mut() {
            let n = chunk.len().min(buf.remaining());
            if let Some(head) = chunk.get(..n) {
                buf.put_slice(head);
            }
            if n >= chunk.len() {
                this.chunks.pop_front();
            } else {
                chunk.drain(..n);
            }
            return Poll::Ready(Ok(()));
        }
        if let Some(err) = this.error.take() {
            return Poll::Ready(Err(io::Error::other(err)));
        }
        // No chunks, no error → EOF.
        Poll::Ready(Ok(()))
    }
}
