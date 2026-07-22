//! Worker output. stdout and stderr are read independently as raw BYTES (never
//! assumed to be UTF-8), each with its own monotonic per-stream sequence.

use std::time::Duration;

use bytes::Bytes;

/// Which stream a chunk came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// A contiguous slice of a worker's output.
///
/// Per-stream ordering (by `sequence`) is guaranteed; the global interleaving of
/// stdout vs stderr is NOT. Bytes are preserved verbatim (non-UTF-8 included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputChunk {
    pub stream: OutputStream,
    pub sequence: u64,
    pub bytes: Bytes,
}

/// Bounds on output handling. Nothing is buffered without limit, and a sink that
/// cannot keep up within `sink_backpressure_timeout` is a fatal error (the worker
/// is cancelled) rather than a silent truncation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputPolicy {
    /// Maximum bytes per emitted chunk.
    pub max_chunk_bytes: usize,
    /// Bounded capacity of the internal reader→supervisor channel (per stream).
    pub channel_capacity: usize,
    /// If publishing to the sink blocks longer than this, the worker fails closed.
    pub sink_backpressure_timeout: Duration,
}

impl Default for OutputPolicy {
    fn default() -> Self {
        Self {
            max_chunk_bytes: 64 * 1024,
            channel_capacity: 256,
            sink_backpressure_timeout: Duration::from_secs(5),
        }
    }
}

/// Headroom added to the trailing-output BYTE budget above the configured in-flight
/// channel buffer, covering output the OS pipe (and the reader's own buffer) still holds
/// when the leader exits. Legitimate trailing output cannot exceed the channel buffer
/// plus the pipe capacity (the process has already exited, so no NEW data arrives); a
/// Linux pipe is 64 KiB by default and at most ~1 MiB, so 2 MiB is comfortably above any
/// real trailing burst while still bounding an ESCAPED descendant that keeps writing.
pub const TRAILING_DRAIN_PIPE_ALLOWANCE_BYTES: usize = 2 * 1024 * 1024;

impl OutputPolicy {
    /// The trailing-output BYTE budget for the post-exit drain: the configured in-flight
    /// channel buffer plus [`TRAILING_DRAIN_PIPE_ALLOWANCE_BYTES`]. The drain fails
    /// closed only once trailing output EXCEEDS this (an escaped descendant writing
    /// without end); output that merely REACHES it and is then followed by EOF is a
    /// clean drain. A byte budget (not a message count) is used so a tiny
    /// `max_chunk_bytes` cannot inflate legitimate output into a false overflow.
    #[must_use]
    pub fn trailing_drain_byte_budget(&self) -> usize {
        self.channel_capacity
            .saturating_mul(self.max_chunk_bytes)
            .saturating_add(TRAILING_DRAIN_PIPE_ALLOWANCE_BYTES)
    }
}
