//! Validated newtypes that bind a report to a specific policy, launch, and target. Each
//! one validates on the untrusted deserialize path, so a backend (or a compromised target)
//! cannot smuggle a malformed digest, nonce, or anonymous backend name past the parser.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest as _, Sha256};

/// A lowercase-hex SHA-256 digest (exactly 64 hex chars). Used for the policy digest and
/// the target-executable digest that bind a report to what was actually confined.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Digest256(String);

impl Digest256 {
    /// The SHA-256 of `bytes` as a canonical digest.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        Digest256(to_lower_hex(&hasher.finalize()))
    }

    /// Parse and validate an untrusted digest string (64 lowercase hex chars).
    ///
    /// # Errors
    /// [`IdError::Digest`] if the string is not exactly 64 lowercase hex characters.
    pub fn parse(value: &str) -> Result<Self, IdError> {
        if value.len() == 64 && is_lower_hex(value) {
            Ok(Digest256(value.to_owned()))
        } else {
            Err(IdError::Digest(value.to_owned()))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Digest256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Digest256 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Digest256::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// A 128-bit launch nonce (32 lowercase hex chars) minted by the parent per spawn and
/// echoed back in the report, so a stale or replayed report cannot be bound to a fresh
/// launch. The protocol crate only validates the FORM — minting (which needs randomness)
/// is the parent's job.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct LaunchNonce(String);

impl LaunchNonce {
    /// Parse and validate an untrusted nonce (32 lowercase hex chars).
    ///
    /// # Errors
    /// [`IdError::Nonce`] if the string is not exactly 32 lowercase hex characters.
    pub fn parse(value: &str) -> Result<Self, IdError> {
        if value.len() == 32 && is_lower_hex(value) {
            Ok(LaunchNonce(value.to_owned()))
        } else {
            Err(IdError::Nonce(value.to_owned()))
        }
    }

    /// Build a nonce from 16 raw bytes (the parent's RNG output), rendered as hex.
    #[must_use]
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        LaunchNonce(to_lower_hex(&bytes))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for LaunchNonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for LaunchNonce {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        LaunchNonce::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// The backend mechanism that produced a report — name plus version, both non-empty. An
/// anonymous `"trust-me"` string is not evidence; a typed, non-empty identity at least
/// records which mechanism made the claim (and lets the parent pin an expected backend).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackendIdentity {
    name: String,
    version: String,
}

impl BackendIdentity {
    /// Build and validate a backend identity.
    ///
    /// # Errors
    /// [`IdError::Backend`] if either the name or the version is blank.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Result<Self, IdError> {
        let name = name.into();
        let version = version.into();
        if name.trim().is_empty() || version.trim().is_empty() {
            return Err(IdError::Backend);
        }
        Ok(BackendIdentity { name, version })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

impl<'de> Deserialize<'de> for BackendIdentity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            name: String,
            version: String,
        }
        let raw = Raw::deserialize(deserializer)?;
        BackendIdentity::new(raw.name, raw.version).map_err(serde::de::Error::custom)
    }
}

/// Whether every byte is a lowercase hex digit (`0-9`, `a-f`) — the canonical form for our
/// digests and nonces (uppercase is rejected so a value has ONE representation).
fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Render bytes as lowercase hex without table indexing (the tree denies `indexing_slicing`).
fn to_lower_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        // Each nibble is 0..=15, so `from_digit(_, 16)` is always `Some`; the total fallback
        // keeps `unwrap`/`expect` out of the tree.
        hex.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
        hex.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'));
    }
    hex
}

/// A malformed identity value on the untrusted path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("digest must be 64 lowercase hex chars: {0:?}")]
    Digest(String),
    #[error("launch nonce must be 32 lowercase hex chars: {0:?}")]
    Nonce(String),
    #[error("backend identity must name a non-empty mechanism and version")]
    Backend,
}
