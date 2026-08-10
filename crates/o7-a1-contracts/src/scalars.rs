//! §3 shared scalar types.
//!
//! `Digest256` is re-exported from `o7-run` rather than redefined: FD-1.1 names
//! `o7_run::event::Digest256::of_bytes` as *the* form for an A1 payload digest,
//! and a parallel type would be a second spelling of one truth — the failure
//! FD-2.4 rejects for imported kinds, applied to a scalar.

use serde::{Deserialize, Serialize};

use crate::bounds::{MAX_ID_BYTES, MAX_STRING_BYTES};

pub use o7_run::event::{Digest256, DigestFormatError};

/// A scalar that violates its §3 constraint.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScalarError {
    #[error("an Id must be a non-empty UTF-8 string")]
    EmptyId,
    #[error("an Id must be <= {MAX_ID_BYTES} bytes, got {actual}")]
    IdTooLong { actual: usize },
    #[error("a text field must be <= {max} bytes, got {actual}")]
    TextTooLong { actual: usize, max: usize },
    #[error("a CommitId must be a non-empty full object id, never abbreviated")]
    EmptyCommitId,
    #[error("a CommitId must not be abbreviated: {value:?} is {actual} chars, expected 40 or 64")]
    AbbreviatedCommitId { value: String, actual: usize },
    #[error("a CommitId must be lowercase hex: {value:?}")]
    NonHexCommitId { value: String },
}

/// §3 — "opaque non-empty UTF-8 string, never parsed for meaning", ≤ 256 bytes.
///
/// The "never parsed for meaning" half is enforced by having no accessor that
/// invites it: an `Id` compares and frames, and that is all.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    /// # Errors
    /// [`ScalarError`] if empty or over the FD-1.4 opaque-id bound.
    pub fn parse(s: &str) -> Result<Self, ScalarError> {
        if s.is_empty() {
            return Err(ScalarError::EmptyId);
        }
        if s.len() > MAX_ID_BYTES {
            return Err(ScalarError::IdTooLong { actual: s.len() });
        }
        Ok(Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// §3 — UTF-8 string, ≤ 65536 bytes unless a schema states a tighter bound.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct Text(String);

impl Text {
    /// # Errors
    /// [`ScalarError::TextTooLong`] above the FD-1.4 string bound.
    pub fn parse(s: &str) -> Result<Self, ScalarError> {
        Self::parse_bounded(s, MAX_STRING_BYTES)
    }

    /// Parse with a schema-specific tighter bound, e.g. `producer_adapter_version`
    /// at 128 bytes or `model_identity` at 256 (§3.0).
    ///
    /// # Errors
    /// [`ScalarError::TextTooLong`] above `max`.
    pub fn parse_bounded(s: &str, max: usize) -> Result<Self, ScalarError> {
        let max = max.min(MAX_STRING_BYTES);
        if s.len() > max {
            return Err(ScalarError::TextTooLong {
                actual: s.len(),
                max,
            });
        }
        Ok(Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Text {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// §3 — "full object id, the repository's object-format width", never
/// abbreviated (`docs/decision-and-admission-protocol.md` §4).
///
/// Both widths are accepted because the repository's object format is the
/// repository's to choose; what is refused is a *prefix*, which is the actual
/// hazard — a seven-character id resolves differently in two clones.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct CommitId(String);

impl CommitId {
    /// # Errors
    /// [`ScalarError`] if empty, abbreviated, or not lowercase hex.
    pub fn parse(s: &str) -> Result<Self, ScalarError> {
        if s.is_empty() {
            return Err(ScalarError::EmptyCommitId);
        }
        if s.len() != 40 && s.len() != 64 {
            return Err(ScalarError::AbbreviatedCommitId {
                value: s.to_owned(),
                actual: s.chars().count(),
            });
        }
        if !s
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ScalarError::NonHexCommitId {
                value: s.to_owned(),
            });
        }
        Ok(Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for CommitId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

/// §3 — RFC 3339 UTC, metadata only (FD-5.4), excluded from every framing.
///
/// Deliberately not parsed into a calendar type. It authorizes nothing, it is
/// never compared for ordering by anything in A1, and giving it a rich type
/// would invite exactly the reasoning FD-5.4 forbids.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(String);

impl Timestamp {
    /// # Errors
    /// [`ScalarError::TextTooLong`] above a generous fixed bound; the value is
    /// otherwise opaque.
    pub fn parse(s: &str) -> Result<Self, ScalarError> {
        if s.len() > 64 {
            return Err(ScalarError::TextTooLong {
                actual: s.len(),
                max: 64,
            });
        }
        Ok(Self(s.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_bounded_and_non_empty() {
        assert_eq!(Id::parse(""), Err(ScalarError::EmptyId));
        assert!(Id::parse("m-1").is_ok());
        let long = "x".repeat(MAX_ID_BYTES + 1);
        assert!(matches!(
            Id::parse(&long),
            Err(ScalarError::IdTooLong { .. })
        ));
        assert!(Id::parse(&"x".repeat(MAX_ID_BYTES)).is_ok());
    }

    #[test]
    fn a_commit_id_may_not_be_abbreviated() {
        let full = "a".repeat(40);
        assert!(CommitId::parse(&full).is_ok());
        assert!(CommitId::parse(&"a".repeat(64)).is_ok());
        assert!(matches!(
            CommitId::parse("a4a9f97"),
            Err(ScalarError::AbbreviatedCommitId { .. })
        ));
    }

    #[test]
    fn a_commit_id_must_be_lowercase_hex() {
        assert!(matches!(
            CommitId::parse(&"A".repeat(40)),
            Err(ScalarError::NonHexCommitId { .. })
        ));
        assert!(matches!(
            CommitId::parse(&"z".repeat(40)),
            Err(ScalarError::NonHexCommitId { .. })
        ));
    }

    #[test]
    fn text_honours_a_schema_specific_tighter_bound() {
        assert!(Text::parse_bounded(&"x".repeat(128), 128).is_ok());
        assert!(matches!(
            Text::parse_bounded(&"x".repeat(129), 128),
            Err(ScalarError::TextTooLong { max: 128, .. })
        ));
    }

    #[test]
    fn a_digest_is_the_o7_run_form() {
        let d = Digest256::of_bytes(b"");
        assert_eq!(d.as_str().len(), 64);
        assert!(Digest256::parse(d.as_str()).is_ok());
        assert!(Digest256::parse("nope").is_err());
    }
}
