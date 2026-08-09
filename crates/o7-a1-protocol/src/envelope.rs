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
/// A CLOSED edge tag — only strings present in the frozen §11.3
/// registry (or the global causation tag) are constructible (review
/// T4: an arbitrary `String` let `"completely_made_up_edge"` become a
/// valid manifest row).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EdgeTag(String);

/// Rejected edge tag.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("edge tag is not in the frozen §11.3 registry")]
pub struct UnknownEdgeTag;

impl EdgeTag {
    /// Admit a tag only if the registry knows it.
    ///
    /// # Errors
    /// [`UnknownEdgeTag`] otherwise.
    pub fn new(raw: impl Into<String>) -> Result<Self, UnknownEdgeTag> {
        let raw = raw.into();
        if raw == crate::edges::CAUSATION_TAG || crate::edges::EDGES.iter().any(|e| e.tag == raw) {
            Ok(Self(raw))
        } else {
            Err(UnknownEdgeTag)
        }
    }

    /// The tag string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for EdgeTag {
    type Error = UnknownEdgeTag;
    fn try_from(raw: String) -> Result<Self, Self::Error> {
        Self::new(raw)
    }
}

impl From<EdgeTag> for String {
    fn from(t: EdgeTag) -> Self {
        t.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefManifestEntry {
    /// Closed edge tag from the frozen §11.3 registry.
    pub edge_tag: EdgeTag,
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

/// The RAW wire envelope core (contract §3). Review T4/T4-R1: this
/// type is what serde admits from bytes — it deliberately CANNOT
/// promise the per-kind invariants. The boundary is three-stage:
///
/// ```text
/// WireEnvelopeCoreV1
///     ↓ syntactic/shape checks (versions, producer mapping, round
///       scope, genesis rule)          — THIS slice
/// ShapeCheckedEnvelopeCoreV1
///     ↓ typed payload + campaign binding + resolver PROOFS
///       (mechanical ref-manifest equality, §2.4 lineage derivation,
///       committed-causation evidence) — the resolver slice
/// AcceptedEnvelopeCoreV1
/// ```
///
/// The name `Accepted…` is deliberately NOT handed out yet (re-review
/// T4-R1): shape-valid is not authority. `contract_digest`, candidate
/// preconditions, and action-specific bindings live in typed payloads,
/// not here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireEnvelopeCoreV1 {
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

/// The one supported envelope version (re-review T4-R2: unknown
/// versions are rejected fail closed, never best-effort parsed).
pub const ENVELOPE_VERSION_V1: u32 = 1;

/// The supported kind version per message kind (all v1 today).
#[must_use]
pub fn supported_kind_version(_kind: MessageKind) -> u32 {
    1
}

/// Why a wire envelope failed shape acceptance.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvelopeShapeError {
    /// Unknown envelope version (T4-R2).
    #[error("unsupported envelope_version")]
    UnsupportedEnvelopeVersion,
    /// Unknown kind version (T4-R2).
    #[error("unsupported message_kind_version")]
    UnsupportedMessageKindVersion,
    /// Producer variant does not match the frozen §11.2 mapping.
    #[error("producer binding variant is not the one §11.2 freezes for this kind")]
    WrongProducer,
    /// Round pair presence does not match the kind's round scope.
    #[error("round pair presence does not match the kind's round scope")]
    WrongRoundScope,
    /// `CampaignGenesis` on anything but a round-0 WorkOrder.
    #[error("campaign_genesis causation outside the first WorkOrder")]
    GenesisOutsideFirstWorkOrder,
    /// Causation claims mismatch the resolved artifact (P1-24 check —
    /// requires the resolver context the caller supplies).
    #[error("causation claims do not match the resolved artifact")]
    CausationMismatch,
}

/// The SHAPE-CHECKED envelope core: private inner, constructible only via
/// [`Self::try_shape_check`] — the §15.2 construction boundary. Checks that
/// need no external resolution run here (producer mapping, round scope,
/// genesis rule); causation resolution takes the resolved facts as
/// PROOF INPUTS so "two adjacent valid claims" cannot be built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ShapeCheckedEnvelopeCoreV1(WireEnvelopeCoreV1);

/// INTERIM shape-level causation input (re-review T4-R1): freely
/// constructible, so it proves shape agreement only — NOT that the
/// facts came from resolving `wire.causation.blob_ref` against a
/// COMMITTED artifact. The resolver slice replaces this with a
/// non-freely-constructible `VerifiedCommittedCausationTarget` bound to
/// both the blob ref and the kind/id, and only that evidence type will
/// mint `AcceptedEnvelopeCoreV1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedCausation {
    /// The resolved target's kind and logical identity.
    Artifact {
        /// Kind read from the RESOLVED committed artifact.
        message_kind: MessageKind,
        /// Logical id read from the RESOLVED committed artifact.
        message_id: MessageId,
    },
    /// No artifact — genesis.
    Genesis,
}

impl ShapeCheckedEnvelopeCoreV1 {
    /// Shape-check a wire envelope: versions, §11.2 producer mapping,
    /// round scope,
    /// genesis rule, and the P1-24 causation cross-check against the
    /// RESOLVED facts.
    ///
    /// # Errors
    /// [`EnvelopeShapeError`] on any violated frozen invariant.
    pub fn try_shape_check(
        wire: WireEnvelopeCoreV1,
        resolved_causation: &ResolvedCausation,
    ) -> Result<Self, EnvelopeShapeError> {
        use crate::edges::{is_round_scoped, required_producer, ProducerClass};
        if wire.envelope_version != ENVELOPE_VERSION_V1 {
            return Err(EnvelopeShapeError::UnsupportedEnvelopeVersion);
        }
        if wire.message_kind_version != supported_kind_version(wire.message_kind) {
            return Err(EnvelopeShapeError::UnsupportedMessageKindVersion);
        }
        let ok = matches!(
            (required_producer(wire.message_kind), &wire.producer),
            (
                ProducerClass::Controller,
                ProducerBindingV1::Controller { .. }
            ) | (
                ProducerClass::Provider(_),
                ProducerBindingV1::Provider { .. }
            ) | (ProducerClass::Human, ProducerBindingV1::Human { .. })
        );
        if !ok {
            return Err(EnvelopeShapeError::WrongProducer);
        }
        if is_round_scoped(wire.message_kind) != wire.round.is_some() {
            return Err(EnvelopeShapeError::WrongRoundScope);
        }
        match (&wire.causation, resolved_causation) {
            (CausationV1::CampaignGenesis, ResolvedCausation::Genesis) => {
                let first_work_order = wire.message_kind == MessageKind::WorkOrder
                    && wire.round.as_ref().is_some_and(|r| r.round_ordinal.0 == 0);
                if !first_work_order {
                    return Err(EnvelopeShapeError::GenesisOutsideFirstWorkOrder);
                }
            }
            (
                CausationV1::Artifact {
                    message_kind,
                    message_id,
                    ..
                },
                ResolvedCausation::Artifact {
                    message_kind: got_kind,
                    message_id: got_id,
                },
            ) if message_kind == got_kind && message_id == got_id => {}
            _ => return Err(EnvelopeShapeError::CausationMismatch),
        }
        Ok(Self(wire))
    }

    /// Read access to the shape-checked core.
    #[must_use]
    pub fn as_wire(&self) -> &WireEnvelopeCoreV1 {
        &self.0
    }
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
            edge_tag: EdgeTag::new("verdict_ref").unwrap(),
            target: digest(1),
        };
        let b = RefManifestEntry {
            edge_tag: EdgeTag::new("attention_ref").unwrap(),
            target: digest(2),
        };
        let m = RefManifest::derive(vec![a.clone(), b.clone(), a.clone()]).unwrap();
        assert_eq!(m.entries(), &[b, a]);

        // A writer-supplied unsorted manifest is rejected at deserialization.
        let unsorted = serde_json::json!([
            {"edge_tag": "verdict_ref", "target": digest(1).as_str()},
            {"edge_tag": "attention_ref", "target": digest(2).as_str()},
        ]);
        let bad: Result<RefManifest, _> = serde_json::from_value(unsorted);
        assert!(bad.is_err());
    }

    fn valid_work_order_wire() -> WireEnvelopeCoreV1 {
        WireEnvelopeCoreV1 {
            envelope_version: ENVELOPE_VERSION_V1,
            message_kind: MessageKind::WorkOrder,
            message_kind_version: supported_kind_version(MessageKind::WorkOrder),
            message_id: MessageId::new("m-1").unwrap(),
            root_goal_id: crate::ids::RootGoalId::new("g-1").unwrap(),
            task_id: crate::ids::TaskId::new("t-1").unwrap(),
            campaign_id: crate::ids::CampaignId::new("c-1").unwrap(),
            round: Some(crate::ids::RoundRef {
                round_id: crate::ids::RoundId::new("r-0").unwrap(),
                round_ordinal: crate::ids::RoundOrdinal(0),
            }),
            causation: CausationV1::CampaignGenesis,
            producer: ProducerBindingV1::Controller {
                component_version: "0.1.0".into(),
                policy_digest: crate::digest::PolicyDigest::compute(|_| {}),
            },
            payload_digest: digest(9),
            ref_manifest: RefManifest::derive(vec![]).unwrap(),
            recorded: RecordedMetadataV1 {
                accepted_at_unix_ns: 1,
                writer_version: crate::canonical::WRITER_VERSION,
            },
        }
    }

    /// Re-review T4-R2 + diff-review: REAL negative oracles through the
    /// shape checker itself — an otherwise-valid WorkOrder with an
    /// unknown version must be rejected by the checker, not by a
    /// constant comparison.
    #[test]
    fn unknown_versions_fail_closed_through_the_shape_checker() {
        // Positive control: the fixture passes as-is.
        assert!(ShapeCheckedEnvelopeCoreV1::try_shape_check(
            valid_work_order_wire(),
            &ResolvedCausation::Genesis
        )
        .is_ok());

        let mut bad_env = valid_work_order_wire();
        bad_env.envelope_version = 999;
        assert_eq!(
            ShapeCheckedEnvelopeCoreV1::try_shape_check(bad_env, &ResolvedCausation::Genesis),
            Err(EnvelopeShapeError::UnsupportedEnvelopeVersion)
        );

        let mut bad_kind = valid_work_order_wire();
        bad_kind.message_kind_version = 999;
        assert_eq!(
            ShapeCheckedEnvelopeCoreV1::try_shape_check(bad_kind, &ResolvedCausation::Genesis),
            Err(EnvelopeShapeError::UnsupportedMessageKindVersion)
        );
    }

    /// The other shape invariants keep their own oracles: wrong
    /// producer class and missing round pair are rejected.
    #[test]
    fn producer_and_round_shape_violations_fail_closed() {
        let mut wrong_producer = valid_work_order_wire();
        wrong_producer.producer = ProducerBindingV1::Human {
            authenticated_principal_ref: CasObjectRefV1 {
                digest: digest(3),
                size: 1,
                media_type: "application/json".into(),
                content_kind: crate::cas::ContentKind::AuthenticatedPrincipalRecord,
            },
        };
        assert_eq!(
            ShapeCheckedEnvelopeCoreV1::try_shape_check(
                wrong_producer,
                &ResolvedCausation::Genesis
            ),
            Err(EnvelopeShapeError::WrongProducer)
        );

        let mut no_round = valid_work_order_wire();
        no_round.round = None;
        assert_eq!(
            ShapeCheckedEnvelopeCoreV1::try_shape_check(no_round, &ResolvedCausation::Genesis),
            Err(EnvelopeShapeError::WrongRoundScope)
        );
    }

    /// Review T4: a fabricated edge tag is unrepresentable.
    #[test]
    fn made_up_edge_tags_are_unrepresentable() {
        assert!(EdgeTag::new("completely_made_up_edge").is_err());
        assert!(EdgeTag::new(crate::edges::CAUSATION_TAG).is_ok());
        let bad: Result<RefManifestEntry, _> = serde_json::from_value(serde_json::json!(
            {"edge_tag": "completely_made_up_edge", "target": digest(1).as_str()}
        ));
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
