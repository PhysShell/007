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
//! provider role, and the §3 direction that fixes which role may author which
//! kind — plus the collection bounds.
//!
//! `EnvelopeV1` deliberately does **not** implement `Deserialize`. Bytes become
//! an envelope only through [`crate::parse_artifact`], which owns the two rules
//! no value-level type can enforce: the caller-supplied byte bound, and the
//! cross-field rules. A public `Deserialize` was tried and lost both.
//!
//! # One way in, and one way out
//!
//! It does not implement `Serialize` either, for the mirror-image reason. The
//! FD-1.4 ceiling is a property of a *byte string*, so it is only ever true of
//! one encoding — and a public `Serialize` let a producer pick a different one.
//! `EnvelopeV1::new` checked `serde_json::to_vec`; `serde_json::to_vec_pretty`
//! on the very same admitted value produced a document over 1 MiB, which every
//! conforming peer must refuse. The producer would hold it as valid until
//! somebody else said no.
//!
//! ```text
//! EnvelopeFieldsV1 --validate--> --encode--> bytes <= MAX_BYTES --> EnvelopeV1
//!                                    ^                                  |
//!                                    +----------- to_wire_bytes --------+
//! ```
//!
//! The same private encoder runs on both arrows, so "admitted" cannot depend on
//! which door the bytes came out of, any more than on which door they came in.
//!
//! **This introduces no canonical JSON, and must not** (FD-1.2: "No
//! canonical-JSON scheme is introduced"). [`EnvelopeV1::to_wire_bytes`] is one
//! implementation's encoder, not a normative spelling. Inbound is a different
//! question with a different answer: [`crate::parse_artifact`] admits *any*
//! conforming document under the ceiling, whatever its whitespace or key order,
//! and it never re-serializes to compare — that would smuggle in exactly the
//! normalization the contract refuses.

use serde::{Deserialize, Serialize};

use crate::bounds::MAX_ARTIFACT_REFS;
use crate::framing::Preimage;
use crate::json::{FromDocument, WireArtifact};
use crate::kind::{MessageKindV1, ProducerRole};
use crate::refs::{ArtifactRef, ArtifactRefWire, RefError};
use crate::scalars::{
    AdapterVersion, BoundedVec, CommitId, FrozenVersion, Id, ModelIdentity, Optional, Timestamp,
    WireDigest,
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
    #[error("message_kind {kind} is authored by producer_role {expected}, not {actual}")]
    WrongProducerRole {
        kind: &'static str,
        expected: &'static str,
        actual: &'static str,
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
    #[error("the serialized envelope is {actual} bytes, exceeding the FD-1.4 maximum {max}")]
    AboveByteCeiling { actual: usize, max: u64 },
    #[error("the envelope could not be serialized to check its size")]
    Unserializable,
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

/// The wire form converts to **fields**, never straight to an envelope: fields
/// carry no authority, so an infallible conversion into them is honest. The
/// step from fields to `EnvelopeV1` is [`EnvelopeV1::new`], and it checks.
impl TryFrom<EnvelopeWireV1> for EnvelopeFieldsV1 {
    type Error = EnvelopeError;

    fn try_from(w: EnvelopeWireV1) -> Result<Self, EnvelopeError> {
        Ok(Self {
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
            artifact_refs: w
                .artifact_refs
                .try_map_into()
                .map_err(|(index, source)| EnvelopeError::BadRef { index, source })?,
            provider_execution_receipt_ref: w
                .provider_execution_receipt_ref
                .try_map_into()
                .map_err(|source| EnvelopeError::BadReceiptRef { source })?,
            created_at: w.created_at,
        })
    }
}

/// §3.0 — the common envelope, v1.
///
/// `first_observed_at` is deliberately **not** here: it is controller-owned and
/// recorded on the acceptance record (FD-5.4). Neither is anything the payload
/// owns — "payloads never restate an envelope-owned field" (FD-5.3), and the
/// converse holds too.
///
/// No `Serialize`: emission goes through [`Self::to_wire_bytes`], which is the
/// checked encoder — see the module note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeV1 {
    envelope_version: EnvelopeVersion,
    message_kind: MessageKindV1,
    message_kind_version: MessageKindVersion,
    message_id: Id,
    root_goal_id: Id,
    task_id: Id,
    campaign_id: Id,
    round_id: Optional<Id>,
    causation_id: Optional<Id>,
    correlation_id: Id,
    producer_role: ProducerRole,
    producer_execution_id: Id,
    producer_adapter_version: AdapterVersion,
    model_identity: Optional<ModelIdentity>,
    prompt_digest: Optional<WireDigest>,
    tool_policy_digest: Optional<WireDigest>,
    contract_digest: WireDigest,
    expected_input_head: Optional<CommitId>,
    payload_digest: WireDigest,
    artifact_refs: ArtifactRefs,
    provider_execution_receipt_ref: Optional<ArtifactRef>,
    created_at: Timestamp,
}

/// The producer-side encoding of an envelope: **private**, and the only thing
/// serde ever serializes.
///
/// It borrows rather than clones, so encoding an envelope costs one document
/// and no copy of its fields. Its shape is the field order and the
/// omit-when-absent policy of §3.0 — an absent optional is omitted, never
/// written as null (FD-1.3), which is why the crate can read back what it
/// writes.
///
/// A third mirror of the same field list is the price of having exactly one
/// emission route, and it is not left to inspection: [`EnvelopeV1::to_wire_bytes`]
/// feeding [`crate::parse_artifact`] must reproduce the same `EnvelopeV1`, so a
/// field missing here fails as a missing field and a field spelled differently
/// fails as an unknown one, both against `EnvelopeWireV1`'s
/// `deny_unknown_fields`.
#[derive(Serialize)]
struct EnvelopeEncodeV1<'a> {
    envelope_version: &'a EnvelopeVersion,
    message_kind: &'a MessageKindV1,
    message_kind_version: &'a MessageKindVersion,
    message_id: &'a Id,
    root_goal_id: &'a Id,
    task_id: &'a Id,
    campaign_id: &'a Id,
    #[serde(skip_serializing_if = "optional_is_absent")]
    round_id: &'a Optional<Id>,
    #[serde(skip_serializing_if = "optional_is_absent")]
    causation_id: &'a Optional<Id>,
    correlation_id: &'a Id,
    producer_role: &'a ProducerRole,
    producer_execution_id: &'a Id,
    producer_adapter_version: &'a AdapterVersion,
    #[serde(skip_serializing_if = "optional_is_absent")]
    model_identity: &'a Optional<ModelIdentity>,
    #[serde(skip_serializing_if = "optional_is_absent")]
    prompt_digest: &'a Optional<WireDigest>,
    #[serde(skip_serializing_if = "optional_is_absent")]
    tool_policy_digest: &'a Optional<WireDigest>,
    contract_digest: &'a WireDigest,
    #[serde(skip_serializing_if = "optional_is_absent")]
    expected_input_head: &'a Optional<CommitId>,
    payload_digest: &'a WireDigest,
    artifact_refs: &'a ArtifactRefs,
    #[serde(skip_serializing_if = "optional_is_absent")]
    provider_execution_receipt_ref: &'a Optional<ArtifactRef>,
    created_at: &'a Timestamp,
}

/// `skip_serializing_if` is handed a reference to the field, and every field of
/// the encoder is already one — so this is `Optional::is_absent` with the extra
/// layer peeled off.
fn optional_is_absent<T>(v: &&Optional<T>) -> bool {
    v.is_absent()
}

impl<'a> From<&'a EnvelopeV1> for EnvelopeEncodeV1<'a> {
    fn from(e: &'a EnvelopeV1) -> Self {
        Self {
            envelope_version: &e.envelope_version,
            message_kind: &e.message_kind,
            message_kind_version: &e.message_kind_version,
            message_id: &e.message_id,
            root_goal_id: &e.root_goal_id,
            task_id: &e.task_id,
            campaign_id: &e.campaign_id,
            round_id: &e.round_id,
            causation_id: &e.causation_id,
            correlation_id: &e.correlation_id,
            producer_role: &e.producer_role,
            producer_execution_id: &e.producer_execution_id,
            producer_adapter_version: &e.producer_adapter_version,
            model_identity: &e.model_identity,
            prompt_digest: &e.prompt_digest,
            tool_policy_digest: &e.tool_policy_digest,
            contract_digest: &e.contract_digest,
            expected_input_head: &e.expected_input_head,
            payload_digest: &e.payload_digest,
            artifact_refs: &e.artifact_refs,
            provider_execution_receipt_ref: &e.provider_execution_receipt_ref,
            created_at: &e.created_at,
        }
    }
}

/// The producer-side input to [`EnvelopeV1::new`] — **not** an admitted
/// artifact.
///
/// Its fields are public and freely mutable on purpose: a caller may turn a
/// reviewer into a controller and back three times, because nothing here
/// carries authority. Authority appears only after checked construction, and
/// `EnvelopeFieldsV1` deliberately does not implement `Deserialize`: a public
/// `bytes -> partially-validated aggregate` route is exactly what the admission
/// path exists to not have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeFieldsV1 {
    pub envelope_version: EnvelopeVersion,
    pub message_kind: MessageKindV1,
    pub message_kind_version: MessageKindVersion,
    pub message_id: Id,
    pub root_goal_id: Id,
    pub task_id: Id,
    pub campaign_id: Id,
    pub round_id: Optional<Id>,
    pub causation_id: Optional<Id>,
    pub correlation_id: Id,
    pub producer_role: ProducerRole,
    pub producer_execution_id: Id,
    pub producer_adapter_version: AdapterVersion,
    pub model_identity: Optional<ModelIdentity>,
    pub prompt_digest: Optional<WireDigest>,
    pub tool_policy_digest: Optional<WireDigest>,
    pub contract_digest: WireDigest,
    pub expected_input_head: Optional<CommitId>,
    pub payload_digest: WireDigest,
    pub artifact_refs: ArtifactRefs,
    pub provider_execution_receipt_ref: Optional<ArtifactRef>,
    pub created_at: Timestamp,
}

impl EnvelopeV1 {
    /// The only public way to build an envelope.
    ///
    /// # Errors
    /// [`EnvelopeError`] if the fields are individually admissible but jointly
    /// are not — provider evidence against the producer role, a kind authored
    /// by a role §3 does not permit, an inadmissible reference, or fields whose
    /// own encoding would not fit under the FD-1.4 ceiling.
    pub fn new(f: EnvelopeFieldsV1) -> Result<Self, EnvelopeError> {
        let candidate = Self {
            envelope_version: f.envelope_version,
            message_kind: f.message_kind,
            message_kind_version: f.message_kind_version,
            message_id: f.message_id,
            root_goal_id: f.root_goal_id,
            task_id: f.task_id,
            campaign_id: f.campaign_id,
            round_id: f.round_id,
            causation_id: f.causation_id,
            correlation_id: f.correlation_id,
            producer_role: f.producer_role,
            producer_execution_id: f.producer_execution_id,
            producer_adapter_version: f.producer_adapter_version,
            model_identity: f.model_identity,
            prompt_digest: f.prompt_digest,
            tool_policy_digest: f.tool_policy_digest,
            contract_digest: f.contract_digest,
            expected_input_head: f.expected_input_head,
            payload_digest: f.payload_digest,
            artifact_refs: f.artifact_refs,
            provider_execution_receipt_ref: f.provider_execution_receipt_ref,
            created_at: f.created_at,
        };
        candidate.validate()?;
        // FD-1.4 bounds the *stored object*, and `parse_artifact` enforces it
        // on the way in. A producer-side constructor that skipped it would mean
        // "admitted" depended on which door you came through: `new` would mint
        // an envelope that a conforming peer must reject, and the producer
        // would hold it as valid until someone else refused it.
        //
        // What is checked is the encoding this crate will actually emit, from
        // the same private encoder `to_wire_bytes` uses — not a representation
        // that merely happens to be small. The earlier version checked
        // `serde_json::to_vec(&candidate)` while `EnvelopeV1` was publicly
        // `Serialize`, so `to_vec_pretty` on the admitted value produced a
        // document over the ceiling: the check proved a property of a
        // representation nobody was obliged to use.
        //
        // The cost is one encoding per construction. An envelope is bounded at
        // 1 MiB, and the alternative is a type whose invariant is weaker than
        // the one its own admission path enforces.
        candidate.encode()?;
        Ok(candidate)
    }

    /// FD-1.4 — this envelope's wire bytes, from the encoder that admitted it.
    ///
    /// The single producer emission route. Every admitted `EnvelopeV1` has
    /// conforming bytes by construction — [`Self::new`] does not return until
    /// this encoder has produced them under the ceiling — so in practice this
    /// cannot fail. It still returns a `Result`, and re-runs the bound rather
    /// than trusting that it ran earlier: the guarantee then belongs to the
    /// emission path itself instead of resting on a fact about some other
    /// function, which is the property that was missing when the ceiling was
    /// checked in one place and the bytes produced in another.
    ///
    /// This is **not** a canonicalization. FD-1.2 introduces no canonical-JSON
    /// scheme, and identity here is the framed digest ([`Self::framed_digest`]),
    /// which is computed from fields and is indifferent to whitespace and key
    /// order. A peer that encodes the same envelope differently is conforming,
    /// and [`crate::parse_artifact`] admits its document.
    ///
    /// # Errors
    /// [`EnvelopeError::AboveByteCeiling`] if the encoding exceeds the FD-1.4
    /// maximum, [`EnvelopeError::Unserializable`] if it cannot be produced.
    pub fn to_wire_bytes(&self) -> Result<Vec<u8>, EnvelopeError> {
        self.encode()
    }

    /// The one encoder. Private, so there is no second spelling of it and no
    /// route to the bytes that skips the bound.
    fn encode(&self) -> Result<Vec<u8>, EnvelopeError> {
        let encoded = serde_json::to_vec(&EnvelopeEncodeV1::from(self))
            .map_err(|_| EnvelopeError::Unserializable)?;
        if encoded.len() as u64 > <Self as WireArtifact>::MAX_BYTES {
            return Err(EnvelopeError::AboveByteCeiling {
                actual: encoded.len(),
                max: <Self as WireArtifact>::MAX_BYTES,
            });
        }
        Ok(encoded)
    }

    /// Copy this envelope's values out as editable, **unauthorised** fields.
    ///
    /// This is the whole of "rebuild": edit the fields, call [`Self::new`].
    /// There is deliberately no in-place `rebuild(&mut self)` — an admitted
    /// object is never mutated and then checked, because between those two
    /// statements it is an admitted-typed value that violates its own
    /// invariant, and anything reading it there (a panic handler, a log, a
    /// concurrent borrow) sees a state the type claims cannot exist.
    #[must_use]
    pub fn to_fields(&self) -> EnvelopeFieldsV1 {
        EnvelopeFieldsV1 {
            envelope_version: self.envelope_version,
            message_kind: self.message_kind,
            message_kind_version: self.message_kind_version,
            message_id: self.message_id.clone(),
            root_goal_id: self.root_goal_id.clone(),
            task_id: self.task_id.clone(),
            campaign_id: self.campaign_id.clone(),
            round_id: self.round_id.clone(),
            causation_id: self.causation_id.clone(),
            correlation_id: self.correlation_id.clone(),
            producer_role: self.producer_role,
            producer_execution_id: self.producer_execution_id.clone(),
            producer_adapter_version: self.producer_adapter_version.clone(),
            model_identity: self.model_identity.clone(),
            prompt_digest: self.prompt_digest.clone(),
            tool_policy_digest: self.tool_policy_digest.clone(),
            contract_digest: self.contract_digest.clone(),
            expected_input_head: self.expected_input_head.clone(),
            payload_digest: self.payload_digest.clone(),
            artifact_refs: self.artifact_refs.clone(),
            provider_execution_receipt_ref: self.provider_execution_receipt_ref.clone(),
            created_at: self.created_at.clone(),
        }
    }

    /// §3.0 — `envelope_version`, frozen by FD-1.6.
    #[must_use]
    pub fn envelope_version(&self) -> u32 {
        self.envelope_version.get()
    }
    /// §3.0 — `message_kind`.
    #[must_use]
    pub fn message_kind(&self) -> MessageKindV1 {
        self.message_kind
    }
    /// §3.0 — `message_kind_version`, frozen by FD-1.6.
    #[must_use]
    pub fn message_kind_version(&self) -> u32 {
        self.message_kind_version.get()
    }
    /// §3.0 — `message_id`.
    #[must_use]
    pub fn message_id(&self) -> &Id {
        &self.message_id
    }
    /// §3.0 — `root_goal_id`.
    #[must_use]
    pub fn root_goal_id(&self) -> &Id {
        &self.root_goal_id
    }
    /// §3.0 — `task_id`.
    #[must_use]
    pub fn task_id(&self) -> &Id {
        &self.task_id
    }
    /// §3.0 — `campaign_id`.
    #[must_use]
    pub fn campaign_id(&self) -> &Id {
        &self.campaign_id
    }
    /// §3.0 — `round_id`.
    #[must_use]
    pub fn round_id(&self) -> &Optional<Id> {
        &self.round_id
    }
    /// §3.0 — `causation_id`.
    #[must_use]
    pub fn causation_id(&self) -> &Optional<Id> {
        &self.causation_id
    }
    /// §3.0 — `correlation_id`.
    #[must_use]
    pub fn correlation_id(&self) -> &Id {
        &self.correlation_id
    }
    /// §3.0 — `producer_role`.
    #[must_use]
    pub fn producer_role(&self) -> ProducerRole {
        self.producer_role
    }
    /// §3.0 — `producer_execution_id`.
    #[must_use]
    pub fn producer_execution_id(&self) -> &Id {
        &self.producer_execution_id
    }
    /// §3.0 — `producer_adapter_version`.
    #[must_use]
    pub fn producer_adapter_version(&self) -> &AdapterVersion {
        &self.producer_adapter_version
    }
    /// §3.0 — `model_identity`.
    #[must_use]
    pub fn model_identity(&self) -> &Optional<ModelIdentity> {
        &self.model_identity
    }
    /// §3.0 — `prompt_digest`.
    #[must_use]
    pub fn prompt_digest(&self) -> &Optional<WireDigest> {
        &self.prompt_digest
    }
    /// §3.0 — `tool_policy_digest`.
    #[must_use]
    pub fn tool_policy_digest(&self) -> &Optional<WireDigest> {
        &self.tool_policy_digest
    }
    /// §3.0 — `contract_digest`.
    #[must_use]
    pub fn contract_digest(&self) -> &WireDigest {
        &self.contract_digest
    }
    /// §3.0 — `expected_input_head`.
    #[must_use]
    pub fn expected_input_head(&self) -> &Optional<CommitId> {
        &self.expected_input_head
    }
    /// §3.0 — `payload_digest`.
    #[must_use]
    pub fn payload_digest(&self) -> &WireDigest {
        &self.payload_digest
    }
    /// §3.0 — `artifact_refs`.
    #[must_use]
    pub fn artifact_refs(&self) -> &ArtifactRefs {
        &self.artifact_refs
    }
    /// §3.0 — `provider_execution_receipt_ref`.
    #[must_use]
    pub fn provider_execution_receipt_ref(&self) -> &Optional<ArtifactRef> {
        &self.provider_execution_receipt_ref
    }
    /// §3.0 — `created_at`.
    #[must_use]
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }
}

impl EnvelopeV1 {
    /// FD-1.2 — the envelope's own identity, computed by field framing.
    ///
    /// The field order is fixed forever and every optional field occupies its
    /// position framed as the empty string. `created_at` is deliberately
    /// excluded (FD-5.4): an artifact must not change identity because a clock
    /// disagreed.
    #[must_use]
    pub fn framed_digest(&self) -> WireDigest {
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
                    .map(|r| r.digest().as_str()),
            )
            .frame_u64(self.artifact_refs.len() as u64);
        for r in &self.artifact_refs {
            p.frame_str(r.kind().name())
                .frame_str(r.media_type())
                .frame_str(r.digest().as_str())
                .frame_u64(r.size());
        }
        p.digest()
    }

    /// §3.0 cross-field validation: the rules no single field can see.
    ///
    /// The one semantic invariant, and both routes run it. [`Self::new`] calls
    /// it before an envelope exists; [`crate::parse_artifact`] reaches it twice
    /// over — once through `new` inside `from_document`, once through
    /// [`WireArtifact::validate_wire`] — because there is a single
    /// implementation to reach rather than a producer-side copy and an
    /// inbound-side copy that can disagree.
    ///
    /// # Errors
    /// [`EnvelopeError`] for provider-only evidence on a non-provider role (or
    /// missing on a provider role), a kind authored by a role §3 does not
    /// permit, too many refs, or a bad ref.
    fn validate(&self) -> Result<(), EnvelopeError> {
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

        // §3.1–§3.11 fix the direction of every kind in its section heading, so
        // the author of a kind is contract text: a `WorkOrder` whose
        // `producer_role` is `coder` is a malformed envelope, not a doubtful
        // combination for a later layer to weigh.
        //
        // Checked after the provider-evidence rules deliberately. The two are
        // independent — an envelope can break either alone — and this order
        // keeps the more local complaint first: "this role must not carry that
        // field" is about the role's own consistency, while this one is about
        // the role against the kind.
        let expected = self.message_kind.authorized_producer_role();
        if role != expected {
            return Err(EnvelopeError::WrongProducerRole {
                kind: self.message_kind.name(),
                expected: expected.name(),
                actual: role.name(),
            });
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
    fn from_bytes(bytes: &[u8]) -> Result<Self, crate::json::ParseError> {
        // Straight from the slice: the structural scan already ran over these
        // same bytes and produced no value, so there is no intermediate
        // representation for this step to disagree with.
        let wire: EnvelopeWireV1 =
            serde_json::from_slice(bytes).map_err(|e| crate::json::ParseError::SchemaMismatch {
                category: crate::json::classify(&e),
            })?;
        // Wire -> fields -> checked construction. There is no route, on any
        // side of the boundary, that yields an `EnvelopeV1` without validation.
        let fields =
            EnvelopeFieldsV1::try_from(wire).map_err(|e| crate::json::ParseError::Invalid {
                reason: e.to_string(),
            })?;
        Self::new(fields).map_err(|e| crate::json::ParseError::Invalid {
            reason: e.to_string(),
        })
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
