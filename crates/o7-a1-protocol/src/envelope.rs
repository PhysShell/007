//! The envelope core (contract §3): small, with everything
//! role-conditional in the tagged producer binding — no nullable soup.

use serde::{Deserialize, Serialize};

use crate::canonical::RecordedMetadataV1;
use crate::cas::CasObjectRefV1;
use crate::edges::MessageKind;
use crate::ids::{BlobDigest, CampaignId, MessageId, RootGoalId, RoundRef, TaskId};

/// Causation (contract §3, review P1-24): either a digest edge to an
/// already-committed envelope-bearing artifact — cross-checked so the
/// carried kind/id cannot diverge from the resolved blob — or the
/// campaign-genesis marker, valid only for the campaign's first
/// WorkOrder (round_ordinal 0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum CausationV1 {
    /// A digest edge to the causing artifact.
    Artifact {
        /// Claimed kind of the causing artifact (must equal the
        /// resolved envelope's kind — constructor-checked).
        message_kind: MessageKind,
        /// Claimed logical identity (must equal the resolved
        /// envelope's — constructor-checked).
        message_id: MessageId,
        /// The causing artifact's canonical bytes.
        blob_ref: CasObjectRefV1,
    },
    /// The campaign lineage binding itself is the cause (first
    /// WorkOrder only).
    CampaignGenesis,
}

/// Tagged producer binding (contract §3, reviews E10/P1-20): a provider
/// artifact without a Provider binding, a controller artifact carrying
/// one, or a human artifact without an authenticated principal are
/// unrepresentable — there is no field to misuse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProducerBindingV1 {
    /// Controller-derived artifact.
    Controller {
        /// Controller component version.
        component_version: String,
        /// Active policy digest — a PROTOCOL digest, never content
        /// identity (review T2).
        policy_digest: crate::digest::PolicyDigest,
    },
    /// Provider-produced raw report. The ONLY field (P1-20): role,
    /// execution, run binding, model route, adapter, prompt/tool-policy
    /// digests all resolve THROUGH the receipt — no denormalized copy
    /// to splice.
    Provider {
        /// The controller-produced invocation receipt for this
        /// execution.
        invocation_receipt_ref: CasObjectRefV1,
    },
    /// Human-produced raw command, recorded after transport authn.
    Human {
        /// The derived `AuthenticatedPrincipalV1` record (§8.8) — no
        /// session field; sessions are delivery observations.
        authenticated_principal_ref: CasObjectRefV1,
    },
}

/// One entry of the controller-derived reference manifest (contract §3):
/// the mechanical collection of ALL direct digest refs — typed payload +
/// producer binding + causation — deduplicated and sorted by
/// `(edge kind tag, target digest bytes)`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefManifestEntry {
    /// Stable edge tag from the frozen §11.3 registry.
    pub edge_tag: String,
    /// Target content identity.
    pub target: BlobDigest,
}

/// The derived manifest. Construction sorts and deduplicates; a manifest
/// that differs from the mechanical collection is not constructible
/// through this API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<RefManifestEntry>", into = "Vec<RefManifestEntry>")]
pub struct RefManifest(Vec<RefManifestEntry>);

/// Manifest construction failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// More refs than the frozen §5 ceiling.
    #[error("manifest exceeds {max} refs", max = crate::limits::MAX_ARTIFACT_REFS)]
    TooMany,
    /// Serde carried an unsorted or duplicated manifest.
    #[error("manifest is not the canonical sorted, deduplicated form")]
    NotCanonical,
}

impl RefManifest {
    /// Build the canonical manifest from a mechanical collection.
    ///
    /// # Errors
    /// [`ManifestError::TooMany`] beyond the refs ceiling.
    pub fn derive(mut entries: Vec<RefManifestEntry>) -> Result<Self, ManifestError> {
        entries.sort();
        entries.dedup();
        if entries.len() > crate::limits::MAX_ARTIFACT_REFS {
            return Err(ManifestError::TooMany);
        }
        Ok(Self(entries))
    }

    /// The sorted, deduplicated entries.
    #[must_use]
    pub fn entries(&self) -> &[RefManifestEntry] {
        &self.0
    }
}

impl TryFrom<Vec<RefManifestEntry>> for RefManifest {
    type Error = ManifestError;
    fn try_from(raw: Vec<RefManifestEntry>) -> Result<Self, Self::Error> {
        let canonical = Self::derive(raw.clone())?;
        if canonical.0 != raw {
            return Err(ManifestError::NotCanonical);
        }
        Ok(canonical)
    }
}

impl From<RefManifest> for Vec<RefManifestEntry> {
    fn from(m: RefManifest) -> Self {
        m.0
    }
}

/// The envelope core (contract §3). `contract_digest`, candidate
/// preconditions, and action-specific bindings live in typed payloads,
/// not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeCoreV1 {
    /// Envelope schema version.
    pub envelope_version: u32,
    /// Message kind.
    pub message_kind: MessageKind,
    /// Kind schema version.
    pub message_kind_version: u32,
    /// Logical/idempotency identity (contract §4.3 — NOT content
    /// identity; that is `blob_digest`).
    pub message_id: MessageId,
    /// Logical lineage.
    pub root_goal_id: RootGoalId,
    /// Task under the root goal.
    pub task_id: TaskId,
    /// Campaign (a distinct logical authority — never a conversation).
    pub campaign_id: CampaignId,
    /// The §2.3 round pair, where the kind is round-scoped.
    pub round: Option<RoundRef>,
    /// Causation (§3).
    pub causation: CausationV1,
    /// Producer binding (§3).
    pub producer: ProducerBindingV1,
    /// Content digest of the exact stored payload bytes (envelope
    /// excluded).
    pub payload_digest: BlobDigest,
    /// Controller-derived exact reference manifest (§3).
    pub ref_manifest: RefManifest,
    /// Controller-assigned acceptance metadata (§4.4/§4.5) — outside
    /// `message_binding_digest`.
    pub recorded: RecordedMetadataV1,
}

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn digest(tag: u8) -> BlobDigest {
        BlobDigest::of_bytes(&[tag])
    }

    #[test]
    fn manifest_derive_sorts_and_dedups_and_serde_rejects_noncanonical() {
        let a = RefManifestEntry {
            edge_tag: "b.tag".into(),
            target: digest(1),
        };
        let b = RefManifestEntry {
            edge_tag: "a.tag".into(),
            target: digest(2),
        };
        let m = RefManifest::derive(vec![a.clone(), b.clone(), a.clone()]).unwrap();
        assert_eq!(m.entries(), &[b, a]);

        // A writer-supplied unsorted manifest is rejected at deserialization.
        let unsorted = serde_json::json!([
            {"edge_tag": "z.tag", "target": digest(1).as_str()},
            {"edge_tag": "a.tag", "target": digest(2).as_str()},
        ]);
        let bad: Result<RefManifest, _> = serde_json::from_value(unsorted);
        assert!(bad.is_err());
    }

    #[test]
    fn producer_variants_are_closed_and_tagged() {
        let bad: Result<ProducerBindingV1, _> =
            serde_json::from_str(r#"{"provider":{"invocation_receipt_ref":null,"role":"coder"}}"#);
        assert!(bad.is_err(), "extra role field must be unrepresentable");
        let bad2: Result<ProducerBindingV1, _> = serde_json::from_str(r#"{"mystery":{}}"#);
        assert!(bad2.is_err());
    }

    #[test]
    fn causation_is_closed() {
        let genesis: CausationV1 = serde_json::from_str(r#""campaign_genesis""#).unwrap();
        assert_eq!(genesis, CausationV1::CampaignGenesis);
        let bad: Result<CausationV1, _> = serde_json::from_str(r#""spontaneous""#);
        assert!(bad.is_err());
    }
}
