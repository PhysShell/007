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
