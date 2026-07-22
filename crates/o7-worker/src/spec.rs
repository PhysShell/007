//! The immutable description of a worker to run. Everything is explicit: the
//! executable and argv are separate (never a shell string), the working
//! directory is required, and the environment is exactly what is listed here —
//! the child inherits nothing from the host.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::boundary::BoundaryRequirement;
use crate::cancellation::CancellationPolicy;
use crate::heartbeat::HeartbeatPolicy;
use crate::output::OutputPolicy;

/// Opaque worker identifier assigned by the caller.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkerId(String);

impl WorkerId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// How the child's stdin is wired. PR 2 supports only a null stdin — the child is
/// never handed the parent's terminal. (Piped stdin can be added later.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdinMode {
    /// stdin is `/dev/null`.
    Null,
}

/// The (only) supported environment policy: the child's environment is CLEARED
/// and then populated with exactly [`WorkerSpec::environment`]. Nothing — no API
/// keys, SSH agent, cloud creds, HOME, PATH, proxy vars, RUST_LOG, shell hooks —
/// is inherited. Enforced by the boundary at spawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EnvironmentPolicy;

/// Everything needed to launch and own one worker process.
#[derive(Debug, Clone)]
pub struct WorkerSpec {
    pub worker_id: WorkerId,
    /// Absolute path to the executable (relative paths are rejected — no PATH
    /// search, no shell).
    pub executable: PathBuf,
    pub arguments: Vec<OsString>,
    /// Absolute working directory.
    pub working_directory: PathBuf,
    /// The COMPLETE environment for the child (the host env is cleared first).
    pub environment: BTreeMap<OsString, OsString>,
    pub stdin: StdinMode,
    pub output: OutputPolicy,
    pub cancellation: CancellationPolicy,
    pub heartbeat: HeartbeatPolicy,
    pub boundary_requirement: BoundaryRequirement,
}

/// A statically-detectable problem with a [`WorkerSpec`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SpecError {
    #[error("executable must be an absolute path (no PATH search, no shell): {0}")]
    RelativeExecutable(PathBuf),
    #[error("working_directory must be an absolute path: {0}")]
    RelativeWorkingDirectory(PathBuf),
}

impl WorkerSpec {
    /// Validate the invariants that can be checked without touching the
    /// filesystem or spawning. (Existence of the executable / working directory is
    /// surfaced as a spawn failure.)
    ///
    /// # Errors
    /// [`SpecError`] for a relative executable or working directory.
    pub fn validate(&self) -> Result<(), SpecError> {
        if !self.executable.is_absolute() {
            return Err(SpecError::RelativeExecutable(self.executable.clone()));
        }
        if !self.working_directory.is_absolute() {
            return Err(SpecError::RelativeWorkingDirectory(
                self.working_directory.clone(),
            ));
        }
        Ok(())
    }
}
