//! Test scaffolding for the verifier runner: a fault-injectable fake
//! [`ProcessBoundary`], honestly attested (by default) as `FullyEnforced`, plus small
//! builders. The fake never touches the OS; it drives exactly the boundary-seam faults
//! (spawn failure, a live leader with members, endless output) that a real process
//! cannot be made to produce on demand.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    dead_code,
    unreachable_pub
)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncRead, ReadBuf};
use tokio::sync::Notify;

use o7_verifier::{CwdPolicy, ExitPolicy, OutputLimits, TrustAnchor, TrustStore, TrustedCommand};
use o7_worker::{
    BoundaryAttestation, BoundaryError, BoundaryExit, BoundaryKind, BoundaryProcess,
    BoundarySpawnSpec, BoundaryStream, EnforcementLevel, ProcessBoundary, ProcessIdentity,
};
use o7_worktree::CanonicalRepoId;

/// Observable effects of the fake boundary.
#[derive(Default)]
pub struct FakeState {
    pub spawn_entered: AtomicBool,
    pub graceful_stops: AtomicUsize,
    pub force_stops: AtomicUsize,
}

impl FakeState {
    pub fn spawn_entered(&self) -> bool {
        self.spawn_entered.load(Ordering::SeqCst)
    }
    pub fn force_stops(&self) -> usize {
        self.force_stops.load(Ordering::SeqCst)
    }
}

#[derive(Clone, Copy)]
enum Behavior {
    /// The leader has already exited with this status.
    Exit(BoundaryExit),
    /// The leader lives until stopped; `usize` same-group members persist until FORCE.
    LiveWithMembers(usize),
    /// The leader lives until stopped and streams stdout forever.
    InfiniteStdout,
}

/// A fault-injectable boundary. By default it attests `FullyEnforced` (honestly — it is
/// a stand-in for the future Sandboy boundary in tests) and runs a leader that exits 0.
pub struct FakeBoundary {
    attestation: BoundaryAttestation,
    spawn_error: Option<String>,
    behavior: Behavior,
    state: Arc<FakeState>,
}

impl FakeBoundary {
    pub fn fully_enforced() -> Self {
        Self::with_enforcement(EnforcementLevel::FullyEnforced)
    }

    pub fn with_enforcement(level: EnforcementLevel) -> Self {
        Self {
            attestation: BoundaryAttestation {
                implementation: BoundaryKind::Sandboy,
                enforcement: level,
            },
            spawn_error: None,
            behavior: Behavior::Exit(BoundaryExit::Code(0)),
            state: Arc::new(FakeState::default()),
        }
    }

    pub fn state(&self) -> Arc<FakeState> {
        Arc::clone(&self.state)
    }

    pub fn spawn_failure(mut self, message: &str) -> Self {
        self.spawn_error = Some(message.to_owned());
        self
    }

    pub fn exit_code(mut self, code: i32) -> Self {
        self.behavior = Behavior::Exit(BoundaryExit::Code(code));
        self
    }

    pub fn live_with_members(mut self, n: usize) -> Self {
        self.behavior = Behavior::LiveWithMembers(n);
        self
    }

    pub fn infinite_stdout(mut self) -> Self {
        self.behavior = Behavior::InfiniteStdout;
        self
    }

    pub fn boxed(self) -> Box<dyn ProcessBoundary> {
        Box::new(self)
    }
}

#[async_trait]
impl ProcessBoundary for FakeBoundary {
    async fn spawn(
        &self,
        _spec: BoundarySpawnSpec,
    ) -> Result<Box<dyn BoundaryProcess>, BoundaryError> {
        self.state.spawn_entered.store(true, Ordering::SeqCst);
        if let Some(message) = &self.spawn_error {
            return Err(BoundaryError::Spawn(io::Error::other(message.clone())));
        }
        let stdout = match self.behavior {
            Behavior::InfiniteStdout => Some(Box::pin(InfiniteReader) as BoundaryStream),
            _ => None,
        };
        Ok(Box::new(FakeProcess {
            identity: ProcessIdentity {
                pid: 424_242,
                process_group: 424_242,
                start_time_ticks: 7,
            },
            behavior: self.behavior,
            stdout: Mutex::new(stdout),
            graceful_ok: Arc::new(AtomicBool::new(false)),
            force_ok: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
            state: Arc::clone(&self.state),
        }))
    }

    fn attestation(&self) -> BoundaryAttestation {
        self.attestation
    }
}

struct FakeProcess {
    identity: ProcessIdentity,
    behavior: Behavior,
    stdout: Mutex<Option<BoundaryStream>>,
    graceful_ok: Arc<AtomicBool>,
    force_ok: Arc<AtomicBool>,
    notify: Arc<Notify>,
    state: Arc<FakeState>,
}

#[async_trait]
impl BoundaryProcess for FakeProcess {
    fn identity(&self) -> ProcessIdentity {
        self.identity.clone()
    }

    fn take_stdout(&mut self) -> Option<BoundaryStream> {
        self.stdout.get_mut().ok().and_then(Option::take)
    }

    fn take_stderr(&mut self) -> Option<BoundaryStream> {
        None
    }

    async fn request_graceful_stop(&mut self) -> Result<(), BoundaryError> {
        self.state.graceful_stops.fetch_add(1, Ordering::SeqCst);
        // The leader dies on SIGTERM; same-group members do NOT (they need force).
        self.graceful_ok.store(true, Ordering::SeqCst);
        self.notify.notify_one();
        Ok(())
    }

    async fn force_stop(&mut self) -> Result<(), BoundaryError> {
        self.state.force_stops.fetch_add(1, Ordering::SeqCst);
        self.force_ok.store(true, Ordering::SeqCst);
        self.notify.notify_one();
        Ok(())
    }

    async fn wait(&mut self) -> Result<BoundaryExit, BoundaryError> {
        match self.behavior {
            Behavior::Exit(exit) => Ok(exit),
            // Re-checkable flags (robust to the supervisor dropping/recreating wait()).
            _ => {
                while !self.graceful_ok.load(Ordering::SeqCst)
                    && !self.force_ok.load(Ordering::SeqCst)
                {
                    self.notify.notified().await;
                }
                Ok(BoundaryExit::Signal(15))
            }
        }
    }

    async fn remaining_members(&self) -> Result<Vec<ProcessIdentity>, BoundaryError> {
        match self.behavior {
            Behavior::LiveWithMembers(n) if !self.force_ok.load(Ordering::SeqCst) => Ok((0..n)
                .map(|i| ProcessIdentity {
                    pid: 500_000 + i as i32,
                    process_group: 424_242,
                    start_time_ticks: 7,
                })
                .collect()),
            _ => Ok(Vec::new()),
        }
    }
}

/// An `AsyncRead` that yields bytes on every read and never reaches EOF.
struct InfiniteReader;

impl AsyncRead for InfiniteReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        const FILLER: [u8; 4096] = [b'x'; 4096];
        let n = buf.remaining().min(FILLER.len());
        if let Some(head) = FILLER.get(..n) {
            buf.put_slice(head);
        }
        Poll::Ready(Ok(()))
    }
}

// ---- builders ----

pub fn repo_id() -> CanonicalRepoId {
    CanonicalRepoId {
        git_common_dir: PathBuf::from("/srv/repo/.git"),
        dev: 66,
        ino: 4242,
    }
}

/// A readable executable file (its content is irrelevant to the fake boundary, but the
/// verifier reads it to bind trust).
pub fn make_exe(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    path
}

pub fn command_for(exe: PathBuf, timeout: Duration, budget: usize) -> TrustedCommand {
    let mut env = BTreeMap::new();
    env.insert(OsString::from("PATH"), OsString::from("/usr/bin:/bin"));
    TrustedCommand {
        executable: exe,
        arguments: vec![OsString::from("--verify")],
        cwd_policy: CwdPolicy::WorktreeRoot,
        environment: env,
        timeout,
        output_limits: OutputLimits {
            max_total_bytes: budget,
        },
        exit_policy: ExitPolicy::exactly_zero(),
    }
}

/// A trust store that trusts exactly `command` in `repo`.
pub fn trust_for(repo: &CanonicalRepoId, command: &TrustedCommand) -> TrustStore {
    let mut store = TrustStore::new();
    let anchor = TrustAnchor::compute(repo, command).unwrap();
    store.trust(&anchor);
    store
}
