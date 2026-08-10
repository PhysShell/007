//! §3.0 the common envelope, and FD-1.2 its framed identity.
//!
//! The envelope is the record that identifies, binds, and references a payload
//! (FD-1.1). The payload itself is a separate immutable byte string: **for a
//! provider-produced artifact the payload bytes ARE the adapter-normalized
//! provider output bytes**, and the controller "never edits, re-orders,
//! re-encodes, or enriches the body". This crate therefore never constructs a
//! payload — it attaches, validates, and hashes.
//!
//! # What the type refuses, and what validation refuses
//!
//! Everything a single field can establish is refused by that field's own type:
//! the frozen versions, the closed enums, the scalar bounds, and explicit
//! `null`. What remains for [`EnvelopeV1::validate`] is only what relates two
//! fields to each other — provider evidence required iff the producer is a
//! provider role — plus the collection bounds.
//!
//! `EnvelopeV1` deliberately does **not** implement `Deserialize`. Bytes become
//! an envelope only through [`crate::parse_artifact`], which owns the two rules
//! no value-level type can enforce: the caller-supplied byte bound, and the
//! cross-field rules. A public `Deserialize` was tried and lost both.

use serde::{Deserialize, Serialize};

use crate::bounds::MAX_ARTIFACT_REFS;
use crate::framing::Preimage;
use crate::json::{FromDocument, WireArtifact};
use crate::kind::{MessageKindV1, ProducerRole};
use crate::refs::{ArtifactRef, ArtifactRefWire, RefError};
use crate::scalars::{
    AdapterVersion, BoundedVec, CommitId, Digest256, FrozenVersion, Id, ModelIdentity, Optional,
    Timestamp, WireDigest,
};

/// `artifact_refs`, bounded by FD-1.4 in the field's own type rather than by a
/// validator a caller has to remember.
pub type ArtifactRefs = BoundedVec<ArtifactRef, MAX_ARTIFACT_REFS>;

/// FD-1.6 — the frozen envelope framing and field set.
pub const ENVELOPE_VERSION_V1: u32 = 1;
/// FD-1.6 — every message kind frozen in §3 is at version 1.
pub const MESSAGE_KIND_VERSION_V1: u32 = 1;
/// FD-1.6 — the reducer semantics of FD-14.
pub const CAMPAIGN_PROTOCOL_VERSION_V1: u32 = 1;

/// The envelope version as a type: no value of it represents an unsupported
/// version, so a deserialized envelope cannot carry one.
pub type EnvelopeVersion = FrozenVersion<ENVELOPE_VERSION_V1>;
/// The message-kind version as a type, for the same reason.
pub type MessageKindVersion = FrozenVersion<MESSAGE_KIND_VERSION_V1>;

/// An envelope whose fields are individually admissible but jointly are not.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvelopeError {
    #[error("{field} is required for producer_role {role}")]
    MissingProviderField {
        field: &'static str,
        role: &'static str,
    },
    #[error("{field} is forbidden for producer_role {role}")]
    ForbiddenProviderField {
        field: &'static str,
        role: &'static str,
    },
    #[error("artifact_refs[{index}] is invalid: {source}")]
    BadRef {
        index: usize,
        #[source]
        source: RefError,
    },
    #[error("provider_execution_receipt_ref is invalid: {source}")]
    BadReceiptRef {
        #[source]
        source: RefError,
    },
}

/// The wire form of [`EnvelopeV1`]: identical fields, no cross-field rules.
///
/// Private, and the only thing serde ever deserializes. `EnvelopeV1` itself has
/// no `Deserialize`, so this is not merely the preferred route from bytes to an
/// envelope — it is the only one that exists, and it runs inside
/// [`crate::parse_artifact`] where the byte bound and
/// [`EnvelopeV1::validate`] both apply.
///
/// The duplication is the price of that guarantee. `From` below is written
/// field by field, so a field added to one struct and not the other fails to
/// compile, and a round-trip test catches a field that exists only on the
/// public type.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvelopeWireV1 {
    envelope_version: EnvelopeVersion,
    message_kind: MessageKindV1,
    message_kind_version: MessageKindVersion,
    message_id: Id,
    root_goal_id: Id,
    task_id: Id,
    campaign_id: Id,
    #[serde(default)]
    round_id: Optional<Id>,
    #[serde(default)]
    causation_id: Optional<Id>,
    correlation_id: Id,
    producer_role: ProducerRole,
    producer_execution_id: Id,
    producer_adapter_version: AdapterVersion,
    #[serde(default)]
    model_identity: Optional<ModelIdentity>,
    #[serde(default)]
    prompt_digest: Optional<WireDigest>,
    #[serde(default)]
    tool_policy_digest: Optional<WireDigest>,
    contract_digest: WireDigest,
    #[serde(default)]
    expected_input_head: Optional<CommitId>,
    payload_digest: WireDigest,
    artifact_refs: BoundedVec<ArtifactRefWire, MAX_ARTIFACT_REFS>,
    #[serde(default)]
    provider_execution_receipt_ref: Optional<ArtifactRefWire>,
    created_at: Timestamp,
}

impl From<EnvelopeWireV1> for EnvelopeV1 {
    fn from(w: EnvelopeWireV1) -> Self {
        Self {
            envelope_version: w.envelope_version,
            message_kind: w.message_kind,
            message_kind_version: w.message_kind_version,
            message_id: w.message_id,
            root_goal_id: w.root_goal_id,
            task_id: w.task_id,
            campaign_id: w.campaign_id,
            round_id: w.round_id,
            causation_id: w.causation_id,
            correlation_id: w.correlation_id,
            producer_role: w.producer_role,
            producer_execution_id: w.producer_execution_id,
            producer_adapter_version: w.producer_adapter_version,
            model_identity: w.model_identity,
            prompt_digest: w.prompt_digest,
            tool_policy_digest: w.tool_policy_digest,
            contract_digest: w.contract_digest,
            expected_input_head: w.expected_input_head,
            payload_digest: w.payload_digest,
            artifact_refs: w.artifact_refs.map_into(),
            provider_execution_receipt_ref: w.provider_execution_receipt_ref.map_into(),
            created_at: w.created_at,
        }
    }
}

/// §3.0 — the common envelope, v1.
///
/// `first_observed_at` is deliberately **not** here: it is controller-owned and
/// recorded on the acceptance record (FD-5.4). Neither is anything the payload
/// owns — "payloads never restate an envelope-owned field" (FD-5.3), and the
/// converse holds too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeV1 {
    pub envelope_version: EnvelopeVersion,
    pub message_kind: MessageKindV1,
    pub message_kind_version: MessageKindVersion,
    pub message_id: Id,
    pub root_goal_id: Id,
    pub task_id: Id,
    pub campaign_id: Id,
    #[serde(default, skip_serializing_if = "Optional::is_absent")]
    pub round_id: Optional<Id>,
    #[serde(default, skip_serializing_if = "Optional::is_absent")]
    pub causation_id: Optional<Id>,
    pub correlation_id: Id,
    pub producer_role: ProducerRole,
    pub producer_execution_id: Id,
    pub producer_adapter_version: AdapterVersion,
    #[serde(default, skip_serializing_if = "Optional::is_absent")]
    pub model_identity: Optional<ModelIdentity>,
    #[serde(default, skip_serializing_if = "Optional::is_absent")]
    pub prompt_digest: Optional<WireDigest>,
    #[serde(default, skip_serializing_if = "Optional::is_absent")]
    pub tool_policy_digest: Optional<WireDigest>,
    pub contract_digest: WireDigest,
    #[serde(default, skip_serializing_if = "Optional::is_absent")]
    pub expected_input_head: Optional<CommitId>,
    pub payload_digest: WireDigest,
    pub artifact_refs: ArtifactRefs,
    #[serde(default, skip_serializing_if = "Optional::is_absent")]
    pub provider_execution_receipt_ref: Optional<ArtifactRef>,
    pub created_at: Timestamp,
}

impl EnvelopeV1 {
    /// FD-1.2 — the envelope's own identity, computed by field framing.
    ///
    /// The field order is fixed forever and every optional field occupies its
    /// position framed as the empty string. `created_at` is deliberately
    /// excluded (FD-5.4): an artifact must not change identity because a clock
    /// disagreed.
    #[must_use]
    pub fn framed_digest(&self) -> Digest256 {
        let mut p = Preimage::new(b"o7-a1-envelope\0v1\0");
        p.frame_u32(self.envelope_version.get())
            .frame_str(self.message_kind.name())
            .frame_u32(self.message_kind_version.get())
            .frame_str(self.message_id.as_str())
            .frame_str(self.root_goal_id.as_str())
            .frame_str(self.task_id.as_str())
            .frame_str(self.campaign_id.as_str())
            .frame_optional_str(self.round_id.as_ref().map(Id::as_str))
            .frame_optional_str(self.causation_id.as_ref().map(Id::as_str))
            .frame_str(self.correlation_id.as_str())
            .frame_str(self.producer_role.name())
            .frame_str(self.producer_execution_id.as_str())
            .frame_str(self.producer_adapter_version.as_str())
            .frame_optional_str(self.model_identity.as_ref().map(ModelIdentity::as_str))
            .frame_optional_str(self.prompt_digest.as_ref().map(WireDigest::as_str))
            .frame_optional_str(self.tool_policy_digest.as_ref().map(WireDigest::as_str))
            .frame_str(self.contract_digest.as_str())
            .frame_optional_str(self.expected_input_head.as_ref().map(CommitId::as_str))
            .frame_str(self.payload_digest.as_str())
            .frame_optional_str(
                self.provider_execution_receipt_ref
                    .as_ref()
                    .map(|r| r.digest.as_str()),
            )
            .frame_u64(self.artifact_refs.len() as u64);
        for r in &self.artifact_refs {
            p.frame_str(r.kind.name())
                .frame_str(&r.media_type)
                .frame_str(r.digest.as_str())
                .frame_u64(r.size);
        }
        p.digest()
    }

    /// §3.0 cross-field validation: the rules no single field can see.
    ///
    /// # Errors
    /// [`EnvelopeError`] for provider-only evidence on a non-provider role (or
    /// missing on a provider role), too many refs, or a bad ref.
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        let role = self.producer_role;
        let provider = role.is_provider_role();
        for (field, present) in [
            ("model_identity", self.model_identity.is_present()),
            ("prompt_digest", self.prompt_digest.is_present()),
            ("tool_policy_digest", self.tool_policy_digest.is_present()),
            (
                "provider_execution_receipt_ref",
                self.provider_execution_receipt_ref.is_present(),
            ),
        ] {
            match (provider, present) {
                (true, false) => {
                    return Err(EnvelopeError::MissingProviderField {
                        field,
                        role: role.name(),
                    })
                }
                (false, true) => {
                    return Err(EnvelopeError::ForbiddenProviderField {
                        field,
                        role: role.name(),
                    })
                }
                _ => {}
            }
        }

        // The count itself is bounded by `ArtifactRefs` at deserialization
        // (FD-1.4), so what remains here is each entry's own admissibility.
        for (index, r) in self.artifact_refs.iter().enumerate() {
            r.validate()
                .map_err(|source| EnvelopeError::BadRef { index, source })?;
        }
        if let Some(r) = self.provider_execution_receipt_ref.as_ref() {
            r.validate()
                .map_err(|source| EnvelopeError::BadReceiptRef { source })?;
        }
        Ok(())
    }

    /// FD-1.1 — the payload bytes must hash to this envelope's `payload_digest`.
    ///
    /// "No reader ever re-serializes anything to verify an identity": the check
    /// is over the exact stored bytes.
    #[must_use]
    pub fn binds_payload(&self, payload_bytes: &[u8]) -> bool {
        WireDigest::of_bytes(payload_bytes) == self.payload_digest
    }

    /// FD-1.8 — the `size` a ref to this artifact must declare: stored envelope
    /// bytes plus stored payload bytes, together.
    #[must_use]
    pub fn ref_size(envelope_bytes: u64, payload_bytes: u64) -> u64 {
        envelope_bytes.saturating_add(payload_bytes)
    }
}

impl FromDocument for EnvelopeV1 {
    fn from_document(value: serde_json::Value) -> Result<Self, crate::json::ParseError> {
        let wire: EnvelopeWireV1 =
            serde_json::from_value(value).map_err(|e| crate::json::ParseError::SchemaMismatch {
                category: crate::json::classify(&e),
            })?;
        Ok(Self::from(wire))
    }
}

impl WireArtifact for EnvelopeV1 {
    /// FD-1.4 — an envelope is a typed A1 JSON object, and the payload it binds
    /// is a *separate* byte string (FD-1.1), so this ceiling covers the
    /// envelope document alone. The doubled allowance in
    /// [`crate::ArtifactRef`] answers a different question: how many bytes a
    /// ref charges when it names both halves at once.
    const MAX_BYTES: u64 = crate::bounds::MAX_CONTROL_ARTIFACT_BYTES;

    fn validate_wire(&self) -> Result<(), String> {
        // `EnvelopeError` names field names and roles, both of which are frozen
        // protocol vocabulary rather than payload content.
        self.validate().map_err(|e| e.to_string())
    }
}
