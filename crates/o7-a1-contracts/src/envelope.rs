//! §3.0 the common envelope, and FD-1.2 its framed identity.
//!
//! The envelope is the record that identifies, binds, and references a payload
//! (FD-1.1). The payload itself is a separate immutable byte string: **for a
//! provider-produced artifact the payload bytes ARE the adapter-normalized
//! provider output bytes**, and the controller "never edits, re-orders,
//! re-encodes, or enriches the body". This crate therefore never constructs a
//! payload — it attaches, validates, and hashes.

use serde::{Deserialize, Serialize};

use crate::bounds::MAX_ARTIFACT_REFS;
use crate::framing::Preimage;
use crate::kind::{MessageKindV1, ProducerRole};
use crate::refs::{ArtifactRef, RefError};
use crate::scalars::{CommitId, Digest256, Id, Text, Timestamp};

/// FD-1.6 — the frozen envelope framing and field set.
pub const ENVELOPE_VERSION_V1: u32 = 1;
/// FD-1.6 — every message kind frozen in §3 is at version 1.
pub const MESSAGE_KIND_VERSION_V1: u32 = 1;
/// FD-1.6 — the reducer semantics of FD-14.
pub const CAMPAIGN_PROTOCOL_VERSION_V1: u32 = 1;

/// An envelope that violates §3.0 or FD-1.6.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EnvelopeError {
    #[error("unsupported envelope_version {actual}: a reader must refuse it, never parse it as well as it can")]
    UnsupportedEnvelopeVersion { actual: u32 },
    #[error("unsupported message_kind_version {actual} for {kind}")]
    UnsupportedMessageKindVersion { kind: &'static str, actual: u32 },
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
    #[error("artifact_refs has {actual} entries, exceeding {MAX_ARTIFACT_REFS}")]
    TooManyRefs { actual: usize },
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

/// §3.0 — the common envelope, v1.
///
/// `first_observed_at` is deliberately **not** here: it is controller-owned and
/// recorded on the acceptance record (FD-5.4). Neither is anything the payload
/// owns — "payloads never restate an envelope-owned field" (FD-5.3), and the
/// converse holds too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeV1 {
    pub envelope_version: u32,
    pub message_kind: MessageKindV1,
    pub message_kind_version: u32,
    pub message_id: Id,
    pub root_goal_id: Id,
    pub task_id: Id,
    pub campaign_id: Id,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub round_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub causation_id: Option<Id>,
    pub correlation_id: Id,
    pub producer_role: ProducerRole,
    pub producer_execution_id: Id,
    pub producer_adapter_version: Text,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model_identity: Option<Text>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub prompt_digest: Option<Digest256>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tool_policy_digest: Option<Digest256>,
    pub contract_digest: Digest256,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub expected_input_head: Option<CommitId>,
    pub payload_digest: Digest256,
    pub artifact_refs: Vec<ArtifactRef>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider_execution_receipt_ref: Option<ArtifactRef>,
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
        p.frame_u32(self.envelope_version)
            .frame_str(self.message_kind.name())
            .frame_u32(self.message_kind_version)
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
            .frame_optional_str(self.model_identity.as_ref().map(Text::as_str))
            .frame_optional_str(self.prompt_digest.as_ref().map(Digest256::as_str))
            .frame_optional_str(self.tool_policy_digest.as_ref().map(Digest256::as_str))
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

    /// §3.0 and FD-1.6 structural validation.
    ///
    /// # Errors
    /// [`EnvelopeError`] for an unsupported version, a provider-only field on a
    /// non-provider role (or missing on a provider role), or a bad ref.
    pub fn validate(&self) -> Result<(), EnvelopeError> {
        if self.envelope_version != ENVELOPE_VERSION_V1 {
            return Err(EnvelopeError::UnsupportedEnvelopeVersion {
                actual: self.envelope_version,
            });
        }
        if self.message_kind_version != MESSAGE_KIND_VERSION_V1 {
            return Err(EnvelopeError::UnsupportedMessageKindVersion {
                kind: self.message_kind.name(),
                actual: self.message_kind_version,
            });
        }

        let role = self.producer_role;
        let provider = role.is_provider_role();
        for (field, present) in [
            ("model_identity", self.model_identity.is_some()),
            ("prompt_digest", self.prompt_digest.is_some()),
            ("tool_policy_digest", self.tool_policy_digest.is_some()),
            (
                "provider_execution_receipt_ref",
                self.provider_execution_receipt_ref.is_some(),
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

        if self.artifact_refs.len() > MAX_ARTIFACT_REFS {
            return Err(EnvelopeError::TooManyRefs {
                actual: self.artifact_refs.len(),
            });
        }
        for (index, r) in self.artifact_refs.iter().enumerate() {
            r.validate()
                .map_err(|source| EnvelopeError::BadRef { index, source })?;
        }
        if let Some(r) = &self.provider_execution_receipt_ref {
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
        Digest256::of_bytes(payload_bytes) == self.payload_digest
    }

    /// FD-1.8 — the `size` a ref to this artifact must declare: stored envelope
    /// bytes plus stored payload bytes, together.
    #[must_use]
    pub fn ref_size(envelope_bytes: u64, payload_bytes: u64) -> u64 {
        envelope_bytes.saturating_add(payload_bytes)
    }
}
