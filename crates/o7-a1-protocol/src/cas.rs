//! Content-addressed storage: the closed `ContentKind` registry
//! (contract §11.1 class 2 + class 5), `CasObjectRefV1` (contract §6),
//! and the store INTERFACE. A1 owns the interface and a test-only
//! in-memory implementation; production durability is A2's (§14, C6).

use serde::{Deserialize, Serialize};

use crate::ids::BlobDigest;
use crate::limits::MAX_EVIDENCE_BLOB_BYTES;

/// The closed CAS content-kind registry. `#[serde(other)]` is
/// deliberately absent: an unknown kind fails at deserialization
/// (wire-unrepresentable, contract §15.1). Extension is a contract
/// change via the supersede path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContentKind {
    /// Class 2 — typed support objects.
    ContractBlob,
    /// Exact provider-facing request after adapter construction.
    CanonicalRequestBlob,
    /// Pre-envelope adapter-normalized provider output (§8.6).
    NormalizedOutputBlob,
    /// Logical model route (§9).
    ModelRouteBlob,
    /// Gate registry snapshot (§10).
    GateRegistrySnapshotBlob,
    /// Gate evidence.
    GateEvidenceBlob,
    /// Diff evidence.
    DiffEvidenceBlob,
    /// Authenticated principal record (§8.8).
    AuthenticatedPrincipalRecord,
    /// Worktree↔contract correspondence evidence (§7.1a).
    WorktreeCorrespondenceEvidenceBlob,
    /// Recorded R1 non-dispatch classification (§11.1).
    NonDispatchClassificationBlob,
    /// Exact payload bytes of a canonical message (§4.6 seed target).
    MessagePayloadBlob,
    /// Class 5 — terminal opaque blobs.
    RawProviderEventBlob,
    /// Tool-call arguments as observed.
    ToolArgumentBlob,
    /// Tool-call result as observed.
    ToolResultBlob,
}

/// A global content-addressed object reference (contract §6) — a
/// DIFFERENT address model from run-relative `o7_run::ArtifactRef`;
/// no typedef of one into the other, ever.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CasObjectRefV1 {
    /// Content identity of the exact stored bytes.
    pub digest: BlobDigest,
    /// Exact byte size.
    pub size: u64,
    /// MIME-ish media type (informational; identity is the digest).
    pub media_type: String,
    /// Closed registry kind.
    pub content_kind: ContentKind,
}

/// Store failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CasError {
    /// The object exceeds the evidence-blob ceiling.
    #[error("object of {size} bytes exceeds the {MAX_EVIDENCE_BLOB_BYTES}-byte blob ceiling")]
    TooLarge {
        /// Observed size.
        size: usize,
    },
    /// The requested digest is not present.
    #[error("object not found")]
    NotFound,
    /// Stored bytes no longer match the requested digest (corruption).
    #[error("stored bytes do not match the requested digest")]
    DigestMismatch,
}

/// The A1 blob-store interface. Put is IDEMPOTENT by construction —
/// content addressing makes a re-put of identical bytes a no-op — which
/// is exactly what the §4.6 recovery path requires.
pub trait CasStore {
    /// Store `bytes`, returning their content identity.
    ///
    /// # Errors
    /// [`CasError::TooLarge`] beyond the blob ceiling.
    fn put(&mut self, bytes: &[u8]) -> Result<BlobDigest, CasError>;

    /// Fetch the exact bytes of `digest`, re-verifying content identity
    /// on the way out (a store never returns bytes it cannot prove).
    ///
    /// # Errors
    /// [`CasError::NotFound`] / [`CasError::DigestMismatch`].
    fn get(&self, digest: &BlobDigest) -> Result<Vec<u8>, CasError>;
}

/// Test-only in-memory store (contract §14: "test-only append sink").
/// Deliberately NOT durable and NOT the production authority.
#[derive(Debug, Default)]
pub struct MemCasStore {
    objects: std::collections::BTreeMap<BlobDigest, Vec<u8>>,
}

impl CasStore for MemCasStore {
    fn put(&mut self, bytes: &[u8]) -> Result<BlobDigest, CasError> {
        if bytes.len() > MAX_EVIDENCE_BLOB_BYTES {
            return Err(CasError::TooLarge { size: bytes.len() });
        }
        let digest = BlobDigest::of_bytes(bytes);
        self.objects
            .entry(digest.clone())
            .or_insert_with(|| bytes.to_vec());
        Ok(digest)
    }

    fn get(&self, digest: &BlobDigest) -> Result<Vec<u8>, CasError> {
        let bytes = self.objects.get(digest).ok_or(CasError::NotFound)?;
        if &BlobDigest::of_bytes(bytes) != digest {
            return Err(CasError::DigestMismatch);
        }
        Ok(bytes.clone())
    }
}

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn unknown_content_kind_is_wire_unrepresentable() {
        let bad: Result<ContentKind, _> = serde_json::from_str("\"surprise-blob\"");
        assert!(bad.is_err());
        let ok: ContentKind = serde_json::from_str("\"message-payload-blob\"").unwrap();
        assert_eq!(ok, ContentKind::MessagePayloadBlob);
    }

    #[test]
    fn put_is_idempotent_and_get_reverifies() {
        let mut store = MemCasStore::default();
        let d1 = store.put(b"bytes").unwrap();
        let d2 = store.put(b"bytes").unwrap();
        assert_eq!(d1, d2);
        assert_eq!(store.get(&d1).unwrap(), b"bytes");
        assert_eq!(
            store.get(&BlobDigest::of_bytes(b"other")),
            Err(CasError::NotFound)
        );
    }

    #[test]
    fn cas_ref_rejects_unknown_fields() {
        let json = r#"{"digest":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","size":0,"media_type":"application/json","content_kind":"contract-blob","extra":1}"#;
        let bad: Result<CasObjectRefV1, _> = serde_json::from_str(json);
        assert!(bad.is_err());
    }
}
