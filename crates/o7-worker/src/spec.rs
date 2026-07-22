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

/// Hard upper bound on a single output chunk (defends against an absurd policy
/// allocating enormous read buffers).
pub const MAX_CHUNK_BYTES: usize = 16 * 1024 * 1024;
/// Hard upper bound on the internal output channel capacity.
pub const MAX_CHANNEL_CAPACITY: usize = 65_536;

/// A statically-detectable problem with a [`WorkerSpec`]. All are caught BEFORE
/// spawning, so an invalid policy is a `FailedToStart`, never a supervisor panic.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SpecError {
    #[error("executable must be an absolute path (no PATH search, no shell): {0}")]
    RelativeExecutable(PathBuf),
    #[error("working_directory must be an absolute path: {0}")]
    RelativeWorkingDirectory(PathBuf),
    #[error("output.max_chunk_bytes must be > 0")]
    ZeroChunkSize,
    #[error("output.max_chunk_bytes {0} exceeds the maximum {MAX_CHUNK_BYTES}")]
    ChunkSizeTooLarge(usize),
    #[error("output.channel_capacity must be > 0")]
    ZeroChannelCapacity,
    #[error("output.channel_capacity {0} exceeds the maximum {MAX_CHANNEL_CAPACITY}")]
    ChannelCapacityTooLarge(usize),
    #[error("heartbeat.interval must be > 0 when heartbeat is enabled")]
    ZeroHeartbeatInterval,
}

impl WorkerSpec {
    /// Validate the invariants that can be checked without touching the
    /// filesystem or spawning. This runs BEFORE spawn, so any invalid policy
    /// (zero/oversized channel or chunk, zero heartbeat interval) fails closed as
    /// a `FailedToStart` rather than panicking the supervisor after it owns a
    /// live process. (Existence of the executable / working directory is surfaced
    /// as a spawn failure.)
    ///
    /// # Errors
    /// [`SpecError`] for any violated invariant.
    pub fn validate(&self) -> Result<(), SpecError> {
        if !self.executable.is_absolute() {
            return Err(SpecError::RelativeExecutable(self.executable.clone()));
        }
        if !self.working_directory.is_absolute() {
            return Err(SpecError::RelativeWorkingDirectory(
                self.working_directory.clone(),
            ));
        }
        if self.output.max_chunk_bytes == 0 {
            return Err(SpecError::ZeroChunkSize);
        }
        if self.output.max_chunk_bytes > MAX_CHUNK_BYTES {
            return Err(SpecError::ChunkSizeTooLarge(self.output.max_chunk_bytes));
        }
        if self.output.channel_capacity == 0 {
            return Err(SpecError::ZeroChannelCapacity);
        }
        if self.output.channel_capacity > MAX_CHANNEL_CAPACITY {
            return Err(SpecError::ChannelCapacityTooLarge(
                self.output.channel_capacity,
            ));
        }
        if self.heartbeat.enabled && self.heartbeat.interval.is_zero() {
            return Err(SpecError::ZeroHeartbeatInterval);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn valid_spec() -> WorkerSpec {
        WorkerSpec {
            worker_id: WorkerId::new("t"),
            executable: PathBuf::from("/bin/true"),
            arguments: Vec::new(),
            working_directory: PathBuf::from("/"),
            environment: BTreeMap::new(),
            stdin: StdinMode::Null,
            output: crate::output::OutputPolicy::default(),
            cancellation: crate::cancellation::CancellationPolicy::default(),
            heartbeat: crate::heartbeat::HeartbeatPolicy::default(),
            boundary_requirement: crate::boundary::BoundaryRequirement::AllowUnconfined,
        }
    }

    #[test]
    fn accepts_a_valid_spec() {
        assert!(valid_spec().validate().is_ok());
    }

    #[test]
    fn rejects_relative_paths() {
        let mut spec = valid_spec();
        spec.executable = PathBuf::from("relative/exe");
        assert!(matches!(
            spec.validate(),
            Err(SpecError::RelativeExecutable(_))
        ));

        let mut spec = valid_spec();
        spec.working_directory = PathBuf::from("relative/dir");
        assert!(matches!(
            spec.validate(),
            Err(SpecError::RelativeWorkingDirectory(_))
        ));
    }

    #[test]
    fn rejects_zero_and_oversized_output_bounds() {
        let mut spec = valid_spec();
        spec.output.channel_capacity = 0;
        assert_eq!(spec.validate(), Err(SpecError::ZeroChannelCapacity));

        let mut spec = valid_spec();
        spec.output.channel_capacity = MAX_CHANNEL_CAPACITY + 1;
        assert_eq!(
            spec.validate(),
            Err(SpecError::ChannelCapacityTooLarge(MAX_CHANNEL_CAPACITY + 1))
        );

        let mut spec = valid_spec();
        spec.output.max_chunk_bytes = 0;
        assert_eq!(spec.validate(), Err(SpecError::ZeroChunkSize));

        let mut spec = valid_spec();
        spec.output.max_chunk_bytes = MAX_CHUNK_BYTES + 1;
        assert_eq!(
            spec.validate(),
            Err(SpecError::ChunkSizeTooLarge(MAX_CHUNK_BYTES + 1))
        );
    }

    #[test]
    fn rejects_zero_heartbeat_only_when_enabled() {
        let mut spec = valid_spec();
        spec.heartbeat = crate::heartbeat::HeartbeatPolicy {
            enabled: true,
            interval: Duration::ZERO,
        };
        assert_eq!(spec.validate(), Err(SpecError::ZeroHeartbeatInterval));

        let mut spec = valid_spec();
        spec.heartbeat = crate::heartbeat::HeartbeatPolicy {
            enabled: false,
            interval: Duration::ZERO,
        };
        assert!(spec.validate().is_ok());
    }
}
