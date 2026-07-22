//! The PR-2 boundary: a bare host process GROUP. It owns the leader and every
//! descendant that stays in the group, so cancellation/cleanup act on the whole
//! set — but it enforces NO isolation. It is attested as
//! [`EnforcementLevel::None`] and is deliberately named `Unconfined` so nobody
//! mistakes lifecycle control for a sandbox.

use std::os::unix::process::ExitStatusExt as _;
use std::process::Stdio;

use async_trait::async_trait;
use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;
use tokio::process::{Child, Command};

use crate::boundary::{
    BoundaryAttestation, BoundaryError, BoundaryExit, BoundaryKind, BoundaryProcess,
    BoundarySpawnSpec, BoundaryStream, EnforcementLevel, ProcessBoundary,
};
use crate::process_identity::ProcessIdentity;
use crate::spec::StdinMode;

/// A boundary that runs the process in its own POSIX process group on the host,
/// with **no** confinement.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnconfinedHostBoundary;

#[async_trait]
impl ProcessBoundary for UnconfinedHostBoundary {
    async fn spawn(
        &self,
        spec: BoundarySpawnSpec,
    ) -> Result<Box<dyn BoundaryProcess>, BoundaryError> {
        // Membership/cleanup attestation relies on Linux `/proc`. Refuse anything
        // else rather than pretend to enforce a boundary we cannot verify.
        if !cfg!(target_os = "linux") {
            return Err(BoundaryError::UnsupportedPlatform);
        }
        let mut cmd = Command::new(&spec.executable);
        cmd.args(&spec.arguments);
        cmd.current_dir(&spec.working_directory);
        // Nothing is inherited from the host: clear, then set exactly the spec env.
        cmd.env_clear();
        cmd.envs(&spec.environment);
        match spec.stdin {
            StdinMode::Null => {
                cmd.stdin(Stdio::null());
            }
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        // A NEW process group led by the child (pgid == child pid) so the whole
        // tree can be signalled together.
        cmd.process_group(0);
        // Backstop only — the supervisor performs the real, verified cleanup.
        cmd.kill_on_drop(true);

        let child = cmd.spawn().map_err(BoundaryError::Spawn)?;
        let pid = i32::try_from(child.id().unwrap_or(0)).unwrap_or(0);
        // The child leads its own group, so pgid == pid. Read the live identity
        // (start-time included) where possible.
        let identity = ProcessIdentity::read(pid).unwrap_or(ProcessIdentity {
            pid,
            process_group: pid,
            start_time_ticks: 0,
        });
        let pgid = identity.process_group;

        Ok(Box::new(HostBoundaryProcess {
            child,
            identity,
            pgid,
        }))
    }

    fn attestation(&self) -> BoundaryAttestation {
        BoundaryAttestation {
            implementation: BoundaryKind::UnconfinedHost,
            enforcement: EnforcementLevel::None,
        }
    }
}

struct HostBoundaryProcess {
    child: Child,
    identity: ProcessIdentity,
    pgid: i32,
}

impl HostBoundaryProcess {
    /// Signal the whole group; a missing group (`ESRCH`) means it is already gone,
    /// which is success for our purposes.
    fn signal_group(&self, signal: Signal) -> Result<(), BoundaryError> {
        match killpg(Pid::from_raw(self.pgid), signal) {
            Ok(()) | Err(nix::errno::Errno::ESRCH) => Ok(()),
            Err(err) => Err(BoundaryError::Signal(err.to_string())),
        }
    }
}

#[async_trait]
impl BoundaryProcess for HostBoundaryProcess {
    fn identity(&self) -> ProcessIdentity {
        self.identity.clone()
    }

    fn take_stdout(&mut self) -> Option<BoundaryStream> {
        self.child
            .stdout
            .take()
            .map(|s| Box::pin(s) as BoundaryStream)
    }

    fn take_stderr(&mut self) -> Option<BoundaryStream> {
        self.child
            .stderr
            .take()
            .map(|s| Box::pin(s) as BoundaryStream)
    }

    async fn request_graceful_stop(&mut self) -> Result<(), BoundaryError> {
        self.signal_group(Signal::SIGTERM)
    }

    async fn force_stop(&mut self) -> Result<(), BoundaryError> {
        self.signal_group(Signal::SIGKILL)
    }

    async fn wait(&mut self) -> Result<BoundaryExit, BoundaryError> {
        let status = self.child.wait().await.map_err(BoundaryError::Wait)?;
        if let Some(code) = status.code() {
            Ok(BoundaryExit::Code(code))
        } else if let Some(signal) = status.signal() {
            Ok(BoundaryExit::Signal(signal))
        } else {
            // Neither a code nor a signal should be impossible on Unix; report it
            // as a wait failure rather than inventing a success.
            Err(BoundaryError::Signal(
                "process ended with neither exit code nor signal".to_owned(),
            ))
        }
    }

    async fn remaining_members(&self) -> Result<Vec<ProcessIdentity>, BoundaryError> {
        ProcessIdentity::enumerate_group(self.pgid)
            .map_err(|e| BoundaryError::Membership(e.to_string()))
    }
}
