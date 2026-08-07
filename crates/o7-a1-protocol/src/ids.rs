//! Opaque identities and `BlobDigest` (contract §2, §4.1).
//!
//! Every ID is a distinct newtype — never a bare `String` passed
//! positionally. Validation is uniform: non-empty, ASCII printable, no
//! control characters, bounded length. `BlobDigest` is this crate's own
//! small mirror of a 64-hex SHA-256 — the same deliberate
//! dependency-light mirroring precedent as `o7_run::RepositoryIdentity`
//! vs `o7_worktree::CanonicalRepoId` (A0 corrective round 1): the A1
//! protocol layer stays recomputable with serde + a hash, and the
//! sandbox protocol's `Digest256` remains a different boundary's type.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// Why an identity string was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// Empty input.
    #[error("{kind}: empty identifier")]
    Empty {
        /// The newtype being constructed.
        kind: &'static str,
    },
    /// Longer than [`MAX_ID_BYTES`].
    #[error("{kind}: identifier exceeds {MAX_ID_BYTES} bytes")]
    TooLong {
        /// The newtype being constructed.
        kind: &'static str,
    },
    /// A byte outside printable ASCII (control chars, spaces, non-ASCII).
    #[error("{kind}: identifier contains a non-printable or non-ASCII byte")]
    BadByte {
        /// The newtype being constructed.
        kind: &'static str,
    },
    /// A digest string that is not exactly 64 lowercase hex characters.
    #[error("blob digest must be exactly 64 lowercase hex characters")]
    BadDigest,
}

/// Hard ceiling on any single identifier.
pub const MAX_ID_BYTES: usize = 128;

fn validate(kind: &'static str, s: &str) -> Result<(), IdError> {
    if s.is_empty() {
        return Err(IdError::Empty { kind });
    }
    if s.len() > MAX_ID_BYTES {
        return Err(IdError::TooLong { kind });
    }
    if !s
        .bytes()
        .all(|b| b.is_ascii_graphic() || b == b'-' || b == b'_')
    {
        return Err(IdError::BadByte { kind });
    }
    Ok(())
}

macro_rules! opaque_id {
    ($(#[$doc:meta])* $name:ident, $kind:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Validate and wrap a raw identifier.
            ///
            /// # Errors
            /// [`IdError`] on empty/oversize/non-printable input.
            pub fn new(raw: impl Into<String>) -> Result<Self, IdError> {
                let raw = raw.into();
                validate($kind, &raw)?;
                Ok(Self(raw))
            }

            /// The validated identifier string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdError;
            fn try_from(raw: String) -> Result<Self, Self::Error> {
                Self::new(raw)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }
    };
}

opaque_id!(
    /// Root goal identity (logical lineage root, contract §2.1).
    RootGoalId,
    "root_goal_id"
);
opaque_id!(
    /// Task identity under a root goal.
    TaskId,
    "task_id"
);
opaque_id!(
    /// Campaign identity — a distinct logical authority, never an R1
    /// conversation id (contract §2.1).
    CampaignId,
    "campaign_id"
);
opaque_id!(
    /// Opaque controller-minted round identity (contract §2.3).
    RoundId,
    "round_id"
);
opaque_id!(
    /// Message identity: the LOGICAL/idempotency identity of a canonical
    /// artifact (contract §4.3). Content identity is `BlobDigest`.
    MessageId,
    "message_id"
);
opaque_id!(
    /// One bounded role execution, spanning its whole tool loop
    /// (contract §8.6).
    ProviderExecutionId,
    "provider_execution_id"
);
opaque_id!(
    /// One external provider request (contract §8.6).
    ProviderDispatchId,
    "provider_dispatch_id"
);
opaque_id!(
    /// The single configured human principal (contract §8.8).
    PrincipalId,
    "principal_id"
);
opaque_id!(
    /// Attention request identity (contract §8.7).
    AttentionId,
    "attention_id"
);
opaque_id!(
    /// Gate identity from the controller-owned registry (contract §10).
    GateId,
    "gate_id"
);

/// Monotonic round ordinal within a campaign, beginning at 0. Travels
/// with `RoundId` as ONE inline identity pair in every round-scoped
/// canonical object and inside `message_binding_digest` (contract §2.3,
/// review P1-16) — so historical replay re-proves every ordinal
/// comparison from canonical bytes alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RoundOrdinal(pub u64);

/// The §2.3 pair. Kept as one struct so a round-scoped object cannot
/// carry one half of the identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoundRef {
    /// Opaque round identity.
    pub round_id: RoundId,
    /// Its ordinal within the campaign.
    pub round_ordinal: RoundOrdinal,
}

/// Content identity: SHA-256 over exact stored bytes, 64 lowercase hex
/// characters (contract §4.1). Deliberately NOT domain-separated — it is
/// a content address; type protection lives in the typed ref around it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BlobDigest(String);

impl BlobDigest {
    /// Compute the digest of `bytes`.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut h = Sha256::new();
        h.update(bytes);
        let out = h.finalize();
        let mut s = String::with_capacity(64);
        for b in out {
            use std::fmt::Write as _;
            // Writing hex into a pre-sized String cannot fail.
            let _ = write!(s, "{b:02x}");
        }
        Self(s)
    }

    /// Validate a 64-lowercase-hex digest string.
    ///
    /// # Errors
    /// [`IdError::BadDigest`] unless exactly 64 lowercase hex chars.
    pub fn parse(raw: impl Into<String>) -> Result<Self, IdError> {
        let raw = raw.into();
        if raw.len() == 64
            && raw
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(raw))
        } else {
            Err(IdError::BadDigest)
        }
    }

    /// The hex form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for BlobDigest {
    type Error = IdError;
    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::parse(raw)
    }
}

impl From<BlobDigest> for String {
    fn from(d: BlobDigest) -> Self {
        d.0
    }
}

#[cfg(test)]
mod tests {
    // Test-only unwrap/expect/panics operate exclusively on this module's
    // own constructed fixtures; a panic here is the test failing loudly,
    // never a production fault path (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn ids_reject_empty_control_and_oversize() {
        assert!(CampaignId::new("").is_err());
        assert!(CampaignId::new("has space").is_err());
        assert!(CampaignId::new("ctl\x07char").is_err());
        assert!(CampaignId::new("a".repeat(MAX_ID_BYTES + 1)).is_err());
        assert!(CampaignId::new("camp-2026_08").is_ok());
    }

    #[test]
    fn serde_round_trip_revalidates() {
        let bad: Result<MessageId, _> = serde_json::from_str("\"has space\"");
        assert!(bad.is_err());
        let ok: MessageId = serde_json::from_str("\"m-1\"").unwrap();
        assert_eq!(ok.as_str(), "m-1");
    }

    #[test]
    fn blob_digest_known_answer_and_validation() {
        // SHA-256("") — the standard known answer.
        assert_eq!(
            BlobDigest::of_bytes(b"").as_str(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(
            BlobDigest::parse("E3B0".repeat(16)).is_err(),
            "uppercase rejected"
        );
        assert!(BlobDigest::parse("abc").is_err(), "short rejected");
    }
}
