//! The out-of-band LAUNCH REQUEST: the exact execution the backend must set up for the target,
//! carried on a dedicated descriptor (`--request-fd`) rather than the backend's argv (visible via
//! `/proc`) or the backend's environment (which is the trusted control plane). One versioned,
//! length-bounded frame, same codec discipline as the report.
//!
//! It carries the WHOLE execution — target object digest, argv, cwd, exact (allowlisted)
//! environment, stdin mode, and the launch nonce — and hashes to a canonical
//! [`LaunchRequest::spec_digest`], so a report can bind not just "which binary" but "which exact
//! invocation of it".

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::ids::{Digest256, LaunchNonce};

/// How the confined target's stdin is wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StdinKind {
    /// `/dev/null`.
    Null,
}

/// One allowlisted environment entry (name → value). Stored as bytes so a non-UTF-8 OS value
/// survives byte-exact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvEntry {
    pub name: Vec<u8>,
    pub value: Vec<u8>,
}

/// The exact execution to confine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRequest {
    /// Wire-format version.
    pub schema_version: u32,
    /// The digest of the exact (sealed) target object to execute.
    pub target_digest: Digest256,
    /// The target's argv (each argument byte-exact).
    pub argv: Vec<Vec<u8>>,
    /// The target's working directory (byte-exact).
    pub cwd: Vec<u8>,
    /// The COMPLETE allowlisted environment the target may keep (byte-exact).
    pub env: Vec<EnvEntry>,
    /// The target's stdin wiring.
    pub stdin: StdinKind,
    /// The per-launch nonce (also echoed in the report), binding request↔report↔launch.
    pub launch_nonce: LaunchNonce,
}

impl LaunchRequest {
    /// The canonical digest of this exact execution — the value a report's `launch_spec_digest`
    /// must equal. Domain-separated, order-preserving over argv, and order-independent over the
    /// env SET (sorted), so equal executions hash identically.
    #[must_use]
    pub fn spec_digest(&self) -> Digest256 {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend_from_slice(b"o7-launch-spec\0v1\0");
        buf.extend_from_slice(&(self.schema_version).to_be_bytes());
        push_bytes(&mut buf, self.target_digest.as_str().as_bytes());
        push_len(&mut buf, self.argv.len() as u64);
        for a in &self.argv {
            push_bytes(&mut buf, a);
        }
        push_bytes(&mut buf, &self.cwd);
        // env is a SET: sort by (name, value) so declaration order does not change the digest.
        let mut env: Vec<(&[u8], &[u8])> = self
            .env
            .iter()
            .map(|e| (e.name.as_slice(), e.value.as_slice()))
            .collect();
        env.sort();
        push_len(&mut buf, env.len() as u64);
        for (n, v) in env {
            push_bytes(&mut buf, n);
            push_bytes(&mut buf, v);
        }
        buf.push(match self.stdin {
            StdinKind::Null => 0x00,
        });
        push_bytes(&mut buf, self.launch_nonce.as_str().as_bytes());
        Digest256::of_bytes(&buf)
    }
}

/// The hard ceiling on a launch-request frame body.
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;

const LEN_PREFIX: usize = 4;

/// A launch-request frame failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RequestFrameError {
    #[error("request frame is truncated (have {have} bytes, need {need})")]
    Truncated { have: usize, need: usize },
    #[error("request frame body length {len} exceeds the {MAX_REQUEST_BYTES}-byte maximum")]
    TooLarge { len: usize },
    #[error("request frame has {extra} trailing bytes after the message")]
    Trailing { extra: usize },
    #[error("request frame body invalid: {0}")]
    Body(String),
    /// Two env entries share a name. The launch env is a SET keyed by name — `spec_digest`
    /// sorts it and [`crate::SandboxPolicy::validate`] rejects duplicate env names — so a frame
    /// carrying the same name twice is ambiguous (which value the target keeps) and would let a
    /// multiset masquerade as a set (`[(A,1),(A,1)]` digests differently from `[(A,1)]`). It is
    /// refused at the wire boundary so the request the backend decodes matches the set the parent
    /// authorized.
    #[error("request frame env has a duplicate name: {name:?}")]
    DuplicateEnvName { name: Vec<u8> },
    #[error(
        "request frame schema version {found} != supported {}",
        crate::SCHEMA_VERSION
    )]
    UnsupportedVersion { found: u32 },
}

/// Encode exactly one launch request as a frame.
///
/// # Errors
/// [`RequestFrameError::TooLarge`] if the body exceeds [`MAX_REQUEST_BYTES`];
/// [`RequestFrameError::Body`] if it cannot be serialized.
pub fn encode(request: &LaunchRequest) -> Result<Vec<u8>, RequestFrameError> {
    let body = serde_json::to_vec(request).map_err(|e| RequestFrameError::Body(e.to_string()))?;
    if body.len() > MAX_REQUEST_BYTES {
        return Err(RequestFrameError::TooLarge { len: body.len() });
    }
    let mut out = Vec::with_capacity(LEN_PREFIX + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Decode EXACTLY one launch-request frame.
///
/// # Errors
/// [`RequestFrameError`] on truncation, oversize, trailing data, a bad body, or a version
/// mismatch.
pub fn decode(bytes: &[u8]) -> Result<LaunchRequest, RequestFrameError> {
    let Some(prefix) = bytes.get(..LEN_PREFIX) else {
        return Err(RequestFrameError::Truncated {
            have: bytes.len(),
            need: LEN_PREFIX,
        });
    };
    let len = match <[u8; LEN_PREFIX]>::try_from(prefix) {
        Ok(arr) => u32::from_be_bytes(arr) as usize,
        Err(_) => {
            return Err(RequestFrameError::Truncated {
                have: bytes.len(),
                need: LEN_PREFIX,
            })
        }
    };
    if len > MAX_REQUEST_BYTES {
        return Err(RequestFrameError::TooLarge { len });
    }
    let need = LEN_PREFIX + len;
    let Some(body) = bytes.get(LEN_PREFIX..need) else {
        return Err(RequestFrameError::Truncated {
            have: bytes.len(),
            need,
        });
    };
    if bytes.len() > need {
        return Err(RequestFrameError::Trailing {
            extra: bytes.len() - need,
        });
    }
    let request: LaunchRequest =
        serde_json::from_slice(body).map_err(|e| RequestFrameError::Body(e.to_string()))?;
    if request.schema_version != crate::SCHEMA_VERSION {
        return Err(RequestFrameError::UnsupportedVersion {
            found: request.schema_version,
        });
    }
    // The launch env is a SET keyed by name: `spec_digest` sorts it and the policy rejects
    // duplicate env names, so decode must too. A repeated name is ambiguous (which value the
    // target keeps) and lets a multiset pose as a set; reject it at the wire boundary rather than
    // silently accept an env the parent never authorized as a set.
    let mut seen_env: BTreeSet<&[u8]> = BTreeSet::new();
    for entry in &request.env {
        if !seen_env.insert(entry.name.as_slice()) {
            return Err(RequestFrameError::DuplicateEnvName {
                name: entry.name.clone(),
            });
        }
    }
    Ok(request)
}

fn push_len(buf: &mut Vec<u8>, len: u64) {
    buf.extend_from_slice(&len.to_be_bytes());
}

fn push_bytes(buf: &mut Vec<u8>, bytes: &[u8]) {
    push_len(buf, bytes.len() as u64);
    buf.extend_from_slice(bytes);
}
