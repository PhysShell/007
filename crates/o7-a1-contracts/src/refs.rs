//! FD-1.7 media types and FD-1.8 artifact refs.
//!
//! An `ArtifactRef` points into 007-owned CAS **only**. There is no path field
//! and no URL field to populate: "an agent-supplied path or URL appearing
//! anywhere in a payload is inert text and is never dereferenced" (FD-7). The
//! absence is the enforcement — a field that does not exist cannot be followed
//! by a later refactor that means well.

use serde::{Deserialize, Serialize};

use crate::bounds::{MAX_CONTROL_ARTIFACT_BYTES, MAX_EVIDENCE_BLOB_BYTES, MAX_STRING_BYTES};
use crate::kind::ArtifactKindV1;
use crate::scalars::Digest256;

/// A malformed reference.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RefError {
    #[error("media_type must be non-empty")]
    EmptyMediaType,
    #[error("media_type must be <= {MAX_STRING_BYTES} bytes, got {actual}")]
    MediaTypeTooLong { actual: usize },
    #[error("size must be > 0: a zero-sized reference names no bytes")]
    ZeroSize,
    #[error("declared size {actual} exceeds the FD-1.4 maximum {max} for {kind}")]
    SizeAboveBound {
        kind: &'static str,
        actual: u64,
        max: u64,
    },
}

/// FD-1.7 — the media type of a typed A1 artifact.
///
/// `application/vnd.o7.a1.<kind>+json; v=<version>`. Media type is part of every
/// ref and part of the envelope framing, so "the same bytes under a different
/// declared type are a different reference" (FD-2.5).
#[must_use]
pub fn typed_media_type(kind: ArtifactKindV1, version: u32) -> String {
    format!("application/vnd.o7.a1.{}+json; v={}", kind.name(), version)
}

/// FD-1.8 — `ArtifactRef = (kind, media_type, digest, size)` into 007-owned CAS.
///
/// For an envelope-bearing artifact the `digest` is the **envelope** digest
/// (FD-1.2), which commits to `payload_digest`, and the `size` is stored
/// envelope bytes **plus** stored payload bytes. One ref therefore covers the
/// whole artifact, and FD-1.5 charges the true cost of a resolution before
/// reading either half.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRef {
    pub kind: ArtifactKindV1,
    pub media_type: String,
    pub digest: Digest256,
    pub size: u64,
}

impl ArtifactRef {
    /// # Errors
    /// [`RefError`] if the media type is empty or over-long, or the declared
    /// size is zero or above the FD-1.4 maximum for this kind of object.
    pub fn validate(&self) -> Result<(), RefError> {
        if self.media_type.is_empty() {
            return Err(RefError::EmptyMediaType);
        }
        if self.media_type.len() > MAX_STRING_BYTES {
            return Err(RefError::MediaTypeTooLong {
                actual: self.media_type.len(),
            });
        }
        if self.size == 0 {
            return Err(RefError::ZeroSize);
        }
        let max = self.max_size();
        if self.size > max {
            return Err(RefError::SizeAboveBound {
                kind: self.kind.name(),
                actual: self.size,
                max,
            });
        }
        Ok(())
    }

    /// The FD-1.4 per-object maximum that applies to this ref's target.
    ///
    /// FD-1.4 bounds "control artifact payload (any typed A1 payload)" at 1 MiB
    /// and "single evidence blob" at 64 MiB, so which bound applies is decided
    /// by what the referenced object *is*:
    ///
    /// - **envelope-bearing** — the ref's `size` covers envelope **and** payload
    ///   (FD-1.8) and both halves are typed A1 payloads, so the bound is twice
    ///   the control-artifact maximum. Derived from that constant rather than
    ///   written as a literal: an allowance that happens to equal 1 MiB today
    ///   because some unrelated constant does is not a bound, it is a
    ///   coincidence;
    /// - **typed non-envelope A1 payload** — the execution receipt, the scope
    ///   contract and a campaign event payload are typed A1 payloads and get the
    ///   control-artifact maximum. Without this they would inherit 64 MiB and a
    ///   closure resolution could be charged, and then read, sixty-four times
    ///   what the contract permits for a typed object;
    /// - **everything else** — evidence blobs and imported A0 objects.
    ///
    /// `interaction_manifest` is deliberately in the last group: FD-1.4 names
    /// "manifest" explicitly in its evidence-blob line, and its 4096 permitted
    /// `interaction_sequence` entries would sit uncomfortably against a 1 MiB
    /// ceiling. That reading is worth re-checking against the contract if the
    /// manifest schema ever grows a tighter obligation.
    #[must_use]
    fn max_size(&self) -> u64 {
        if self.kind.is_envelope_bearing() {
            MAX_CONTROL_ARTIFACT_BYTES.saturating_mul(2)
        } else if self.kind.is_typed_payload() {
            MAX_CONTROL_ARTIFACT_BYTES
        } else {
            MAX_EVIDENCE_BLOB_BYTES
        }
    }

    /// The typed-object identity used to deduplicate a closure (FD-1.5:
    /// "typed object identity: `(ref.kind, ref.digest)`").
    ///
    /// Note what is *not* in it: `media_type` and `size`. Two refs that agree on
    /// kind and digest are the same object, so a peer cannot inflate a closure's
    /// accounting by re-declaring one object under a second media type.
    #[must_use]
    pub fn identity(&self) -> (ArtifactKindV1, &str) {
        (self.kind, self.digest.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest() -> Digest256 {
        Digest256::of_bytes(b"payload")
    }

    fn evidence_ref(size: u64) -> ArtifactRef {
        ArtifactRef {
            kind: ArtifactKindV1::Diff,
            media_type: "text/x-diff".to_owned(),
            digest: digest(),
            size,
        }
    }

    #[test]
    fn a_reference_has_no_path_and_no_url() {
        // FD-1.8 / FD-7: the guard is structural. If a `path` or `url` field is
        // ever added, this fails rather than a reviewer having to notice.
        let json = serde_json::to_string(&evidence_ref(1)).unwrap_or_default();
        assert!(!json.contains("path"), "{json}");
        assert!(!json.contains("url"), "{json}");
        assert!(serde_json::from_str::<ArtifactRef>(
            r#"{"kind":"diff","media_type":"text/x-diff","digest":"0000000000000000000000000000000000000000000000000000000000000000","size":1,"path":"/etc/passwd"}"#
        )
        .is_err());
    }

    #[test]
    fn a_zero_sized_reference_is_refused() {
        assert_eq!(evidence_ref(0).validate(), Err(RefError::ZeroSize));
    }

    #[test]
    fn an_evidence_blob_above_its_bound_is_refused() {
        assert!(evidence_ref(MAX_EVIDENCE_BLOB_BYTES).validate().is_ok());
        assert!(matches!(
            evidence_ref(MAX_EVIDENCE_BLOB_BYTES + 1).validate(),
            Err(RefError::SizeAboveBound { .. })
        ));
    }

    fn ref_of(kind: ArtifactKindV1, size: u64) -> ArtifactRef {
        ArtifactRef {
            kind,
            media_type: typed_media_type(kind, 1),
            digest: digest(),
            size,
        }
    }

    #[test]
    fn an_envelope_bearing_ref_is_bounded_by_both_halves() {
        // FD-1.8: size covers envelope + payload, each a typed A1 payload.
        let max = MAX_CONTROL_ARTIFACT_BYTES * 2;
        assert!(ref_of(ArtifactKindV1::WorkOrder, max).validate().is_ok());
        assert!(matches!(
            ref_of(ArtifactKindV1::WorkOrder, max + 1).validate(),
            Err(RefError::SizeAboveBound { .. })
        ));
    }

    #[test]
    fn a_typed_non_envelope_payload_gets_the_control_bound_not_the_blob_bound() {
        // Without this, a ref could declare a 64 MiB execution receipt and the
        // resolver would charge and then read sixty-four times what FD-1.4
        // permits for a typed object.
        for kind in [
            ArtifactKindV1::ProviderExecutionReceipt,
            ArtifactKindV1::ScopeContract,
            ArtifactKindV1::CampaignEventPayload,
        ] {
            assert!(
                ref_of(kind, MAX_CONTROL_ARTIFACT_BYTES).validate().is_ok(),
                "{kind:?} should be admissible at the control bound"
            );
            assert!(
                matches!(
                    ref_of(kind, MAX_CONTROL_ARTIFACT_BYTES + 1).validate(),
                    Err(RefError::SizeAboveBound { .. })
                ),
                "{kind:?} must be refused above the control bound"
            );
        }
    }

    #[test]
    fn an_interaction_manifest_keeps_the_evidence_bound() {
        // FD-1.4 names "manifest" in its evidence-blob line; recorded as a test
        // so the reading is visible rather than implied by an absent branch.
        assert!(
            ref_of(ArtifactKindV1::InteractionManifest, MAX_EVIDENCE_BLOB_BYTES)
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn identity_ignores_media_type_and_size() {
        let a = ArtifactRef {
            kind: ArtifactKindV1::Diff,
            media_type: "text/x-diff".to_owned(),
            digest: digest(),
            size: 10,
        };
        let b = ArtifactRef {
            kind: ArtifactKindV1::Diff,
            media_type: "application/octet-stream".to_owned(),
            digest: digest(),
            size: 999,
        };
        assert_eq!(a.identity(), b.identity());
    }

    #[test]
    fn a_typed_media_type_carries_kind_and_version() {
        assert_eq!(
            typed_media_type(ArtifactKindV1::WorkOrder, 1),
            "application/vnd.o7.a1.work_order+json; v=1"
        );
    }

    #[test]
    fn an_unknown_ref_field_fails_closed() {
        assert!(serde_json::from_str::<ArtifactRef>(
            r#"{"kind":"diff","media_type":"text/x-diff","digest":"0000000000000000000000000000000000000000000000000000000000000000","size":1,"extra":true}"#
        )
        .is_err());
    }
}
