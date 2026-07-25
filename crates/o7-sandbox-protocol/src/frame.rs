//! The report CHANNEL frame: exactly one length-prefixed JSON report, size-bounded, with a
//! defined end-of-message. The backend writes one frame to the report descriptor and closes
//! it; the parent reads exactly one frame. A malformed, oversized, truncated, or
//! trailing-garbage frame fails closed — there is no "read as much as arrived and hope".
//!
//! Wire shape: a 4-byte big-endian length `N` (the JSON body length), then `N` bytes of
//! report JSON. `N` must be `<= MAX_REPORT_BYTES`.

use crate::report::{ReportError, SandboxReport};

/// The hard ceiling on a report body. A report is a handful of small fields; anything larger
/// is a bug or an attack and is rejected rather than buffered.
pub const MAX_REPORT_BYTES: usize = 64 * 1024;

/// The fixed length-prefix width.
const LEN_PREFIX: usize = 4;

/// A frame that could not be accepted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    /// Fewer than the 4 length-prefix bytes, or fewer body bytes than the prefix promised.
    #[error("report frame is truncated (have {have} bytes, need {need})")]
    Truncated { have: usize, need: usize },
    /// The declared body length exceeds [`MAX_REPORT_BYTES`].
    #[error("report frame body length {len} exceeds the {MAX_REPORT_BYTES}-byte maximum")]
    TooLarge { len: usize },
    /// Bytes remain after the single declared frame — a second frame or garbage.
    #[error("report frame has {extra} trailing bytes after the message")]
    Trailing { extra: usize },
    /// The body was not a valid report.
    #[error("report frame body invalid: {0}")]
    Body(#[from] ReportError),
}

/// Encode exactly one report as a frame.
///
/// # Errors
/// [`FrameError::TooLarge`] if the serialized body exceeds [`MAX_REPORT_BYTES`], or
/// [`FrameError::Body`] if the report cannot be serialized.
pub fn encode(report: &SandboxReport) -> Result<Vec<u8>, FrameError> {
    let body = serde_json::to_vec(report)
        .map_err(|e| FrameError::Body(ReportError::Malformed(e.to_string())))?;
    if body.len() > MAX_REPORT_BYTES {
        return Err(FrameError::TooLarge { len: body.len() });
    }
    let mut out = Vec::with_capacity(LEN_PREFIX + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Decode EXACTLY one frame from `bytes`, rejecting truncation, oversize, and trailing data.
///
/// # Errors
/// [`FrameError`] on any of those, or an invalid report body.
pub fn decode(bytes: &[u8]) -> Result<SandboxReport, FrameError> {
    let Some(prefix) = bytes.get(..LEN_PREFIX) else {
        return Err(FrameError::Truncated {
            have: bytes.len(),
            need: LEN_PREFIX,
        });
    };
    // `prefix` is exactly 4 bytes, so this conversion cannot fail.
    let len = match <[u8; LEN_PREFIX]>::try_from(prefix) {
        Ok(arr) => u32::from_be_bytes(arr) as usize,
        Err(_) => {
            return Err(FrameError::Truncated {
                have: bytes.len(),
                need: LEN_PREFIX,
            })
        }
    };
    if len > MAX_REPORT_BYTES {
        return Err(FrameError::TooLarge { len });
    }
    let need = LEN_PREFIX + len;
    let Some(body) = bytes.get(LEN_PREFIX..need) else {
        return Err(FrameError::Truncated {
            have: bytes.len(),
            need,
        });
    };
    if bytes.len() > need {
        return Err(FrameError::Trailing {
            extra: bytes.len() - need,
        });
    }
    Ok(SandboxReport::parse(body)?)
}
