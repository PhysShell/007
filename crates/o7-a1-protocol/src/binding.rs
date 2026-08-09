//! `CampaignRunBindingV1` — the canonical bridge between logical
//! lineage and physical execution (contract §2.1, C5) — its typed ref,
//! and the frozen cause vocabulary (§8.6, T5-R1). W1: the checked
//! ref/cause types do NOT implement `Deserialize` — their guarantees
//! come from cross-object proofs, so the wire carries `Wire…` forms and
//! only the construction API mints checked values. W4: a ref to a
//! committed canonical message carries `canonical-message-blob`.

use serde::{Deserialize, Serialize};

use o7_run::ids::RunId;

use crate::cas::{CasObjectRefV1, ContentKind};
use crate::ids::{
    AttemptId, CampaignId, CommandId, ConversationId, ProviderDispatchId, ProviderExecutionId,
    RoundRef,
};
use crate::wrappers::{EstablishedNonDispatchEvidenceRefV1, InputStateBindingV1};

/// Execution role (closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Coder lane.
    Coder,
    /// Reviewer lane (fresh session, own conversation in v1).
    Reviewer,
}

/// The canonical binding receipt (contract §2.1): `campaign_id` is
/// never a conversation id; one campaign may bind several
/// conversations; `command_id` present only where the execution really
/// was an R1 continuation; the `input_state_binding` states what THIS
/// execution actually materialized. Its own cross-object invariants are
/// acceptance-classifier obligations; the payload shape itself is wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignRunBindingV1 {
    /// Logical campaign.
    pub campaign_id: CampaignId,
    /// The §2.3 round pair, inline (P1-16).
    pub round: RoundRef,
    /// Role of this execution.
    pub role: Role,
    /// The bounded role execution.
    pub provider_execution_id: ProviderExecutionId,
    /// The R1 conversation carrying the physical execution.
    pub conversation_id: ConversationId,
    /// Present only for a genuine R1 continuation.
    pub command_id: Option<CommandId>,
    /// The canonical run.
    pub run_id: RunId,
    /// The attempt.
    pub attempt_id: AttemptId,
    /// What this execution materialized (§7.1a).
    pub input_state_binding: InputStateBindingV1,
}

/// Ref/cause construction failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BindingError {
    /// W4: the blob must be the canonical-message storage kind.
    #[error("blob_ref must be a canonical-message-blob")]
    NotACanonicalMessageBlob,
    /// The carried tuple does not equal the resolved binding's.
    #[error("carried identity tuple does not match the resolved binding")]
    TupleMismatch,
    /// SafeRedrive: the binding's execution id is not the prior one.
    #[error("prior binding's provider_execution_id != prior_execution_id")]
    ExecutionIdMismatch,
    /// SafeRedrive: the binding's run is not the evidence's run.
    #[error("prior binding's run_id != non-dispatch evidence run_id")]
    RunIdMismatch,
    /// W3: prior binding is not the SAME role chain (campaign, round,
    /// role) as the current execution.
    #[error("prior binding is not the same role chain as the current execution")]
    NotSameChain,
    /// W3: a redrive must mint a FRESH execution — the current
    /// execution cannot be its own prior.
    #[error("current execution id equals the prior execution id")]
    SelfRedrive,
    /// W1/follow-up: a tool result must be the tool-result-blob kind.
    #[error("tool_result_ref must be a tool-result-blob")]
    NotAToolResultBlob,
}

/// A typed ref to a COMMITTED binding (contract §2.1). Checked: no
/// `Deserialize` — the wire carries [`WireCampaignRunBindingRefV1`],
/// and only [`Self::new`] (with the resolved binding as proof input,
/// interim per T4-R1) mints this type, copying the identity tuple FROM
/// the resolved binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CampaignRunBindingRefV1 {
    /// Campaign (copied from the resolved binding).
    pub campaign_id: CampaignId,
    /// Round pair (copied).
    pub round: RoundRef,
    /// Role (copied).
    pub role: Role,
    /// Execution (copied).
    pub provider_execution_id: ProviderExecutionId,
    blob_ref: CasObjectRefV1,
}

/// The wire form of a binding ref — freely deserializable CLAIMS; a
/// checked ref is minted by [`CampaignRunBindingRefV1::from_wire`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireCampaignRunBindingRefV1 {
    /// Claimed campaign.
    pub campaign_id: CampaignId,
    /// Claimed round pair.
    pub round: RoundRef,
    /// Claimed role.
    pub role: Role,
    /// Claimed execution.
    pub provider_execution_id: ProviderExecutionId,
    /// The canonical binding bytes (canonical-message-blob).
    pub blob_ref: CasObjectRefV1,
}

impl CampaignRunBindingRefV1 {
    /// Mint a ref from the RESOLVED binding: the tuple is copied, never
    /// claimed.
    ///
    /// # Errors
    /// [`BindingError::NotACanonicalMessageBlob`] on a wrong blob kind.
    pub fn new(
        resolved: &CampaignRunBindingV1,
        blob_ref: CasObjectRefV1,
    ) -> Result<Self, BindingError> {
        if blob_ref.content_kind != ContentKind::CanonicalMessageBlob {
            return Err(BindingError::NotACanonicalMessageBlob);
        }
        Ok(Self {
            campaign_id: resolved.campaign_id.clone(),
            round: resolved.round.clone(),
            role: resolved.role,
            provider_execution_id: resolved.provider_execution_id.clone(),
            blob_ref,
        })
    }

    /// Admit a wire claim by proving EVERY carried field equals the
    /// resolved binding's own (W3's anti-splice discipline).
    ///
    /// # Errors
    /// [`BindingError::TupleMismatch`] /
    /// [`BindingError::NotACanonicalMessageBlob`].
    pub fn from_wire(
        wire: WireCampaignRunBindingRefV1,
        resolved: &CampaignRunBindingV1,
    ) -> Result<Self, BindingError> {
        if wire.campaign_id != resolved.campaign_id
            || wire.round != resolved.round
            || wire.role != resolved.role
            || wire.provider_execution_id != resolved.provider_execution_id
        {
            return Err(BindingError::TupleMismatch);
        }
        Self::new(resolved, wire.blob_ref)
    }

    /// The canonical binding bytes.
    #[must_use]
    pub fn blob_ref(&self) -> &CasObjectRefV1 {
        &self.blob_ref
    }
}

/// §8.6 execution causes (closed; T5-R1). Checked: no `Deserialize` —
/// the wire carries `WireExecutionCauseV1`; SafeRedrive is mintable
/// only through [`ExecutionCauseV1::safe_redrive`] with its
/// cross-checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionCauseV1 {
    /// First execution of its chain.
    Initial,
    /// Opens a corrective round.
    CorrectiveRound {
        /// The accepted `ReviewVerdict` (canonical bytes).
        prior_verdict_ref: CasObjectRefV1,
    },
    /// Proven pre-dispatch retry of a prior execution of the SAME role
    /// chain (W3). The prior RUN BINDING — durable before dispatch — is
    /// the antecedent authority; a receipt cannot be (unconstructible
    /// before a terminal/ambiguous outcome; the intersection with
    /// redrive-legal states is empty).
    SafeRedrive {
        /// The prior execution being redriven.
        prior_execution_id: ProviderExecutionId,
        /// Its binding (boxed for variant size).
        prior_run_binding_ref: Box<CampaignRunBindingRefV1>,
        /// Established non-dispatch.
        evidence: EstablishedNonDispatchEvidenceRefV1,
    },
}

impl ExecutionCauseV1 {
    /// Build a SafeRedrive with the frozen cross-checks (§8.6 + W3):
    ///
    /// ```text
    /// prior_ref tuple            == resolved prior tuple   (from_wire)
    /// current.campaign/round/role == prior's               (SAME chain)
    /// prior.provider_execution_id == prior_execution_id
    /// prior.run_id                == evidence.run_id
    /// current.provider_execution_id != prior_execution_id  (fresh exec)
    /// ```
    ///
    /// Resolution of the prior binding to committed bytes stays the
    /// resolver slice's proof (interim per T4-R1).
    ///
    /// # Errors
    /// [`BindingError`] naming the violated rule.
    pub fn safe_redrive(
        current_binding: &CampaignRunBindingV1,
        prior_execution_id: ProviderExecutionId,
        prior_run_binding_ref: CampaignRunBindingRefV1,
        resolved_prior_binding: &CampaignRunBindingV1,
        evidence: EstablishedNonDispatchEvidenceRefV1,
    ) -> Result<Self, BindingError> {
        if prior_run_binding_ref.campaign_id != resolved_prior_binding.campaign_id
            || prior_run_binding_ref.round != resolved_prior_binding.round
            || prior_run_binding_ref.role != resolved_prior_binding.role
            || prior_run_binding_ref.provider_execution_id
                != resolved_prior_binding.provider_execution_id
        {
            return Err(BindingError::TupleMismatch);
        }
        if current_binding.campaign_id != resolved_prior_binding.campaign_id
            || current_binding.round != resolved_prior_binding.round
            || current_binding.role != resolved_prior_binding.role
        {
            return Err(BindingError::NotSameChain);
        }
        if resolved_prior_binding.provider_execution_id != prior_execution_id {
            return Err(BindingError::ExecutionIdMismatch);
        }
        if resolved_prior_binding.run_id != evidence.run_id {
            return Err(BindingError::RunIdMismatch);
        }
        if current_binding.provider_execution_id == prior_execution_id {
            return Err(BindingError::SelfRedrive);
        }
        Ok(Self::SafeRedrive {
            prior_execution_id,
            prior_run_binding_ref: Box::new(prior_run_binding_ref),
            evidence,
        })
    }
}

/// §8.6 dispatch causes — `Initial | ToolContinuation` ONLY (T5-R1).
/// Checked: no `Deserialize`; `ToolContinuation` is mintable only via
/// [`DispatchCauseV1::tool_continuation`], which enforces the
/// tool-result blob kind (§11.3 names it a specific CAS kind).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchCauseV1 {
    /// The execution's first dispatch.
    Initial,
    /// Continuation after a controller-authorized tool result.
    ToolContinuation {
        /// The prior dispatch.
        prior_dispatch_id: ProviderDispatchId,
        /// The observed tool result blob (`tool-result-blob`).
        tool_result_ref: CasObjectRefV1,
    },
}

impl DispatchCauseV1 {
    /// Build a tool continuation.
    ///
    /// # Errors
    /// [`BindingError::NotAToolResultBlob`] on a wrong blob kind.
    pub fn tool_continuation(
        prior_dispatch_id: ProviderDispatchId,
        tool_result_ref: CasObjectRefV1,
    ) -> Result<Self, BindingError> {
        if tool_result_ref.content_kind != ContentKind::ToolResultBlob {
            return Err(BindingError::NotAToolResultBlob);
        }
        Ok(Self::ToolContinuation {
            prior_dispatch_id,
            tool_result_ref,
        })
    }
}

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::ids::{BlobDigest, RoundId, RoundOrdinal};
    use crate::wrappers::NonDispatchClassification;
    use o7_run::event::{ArtifactKind, ArtifactRef, Digest256};
    use o7_run::ids::RunEventId;

    fn cas(kind: ContentKind) -> CasObjectRefV1 {
        CasObjectRefV1 {
            digest: BlobDigest::of_bytes(b"x"),
            size: 1,
            media_type: "application/json".into(),
            content_kind: kind,
        }
    }

    fn binding(run: &str, exec: &str, role: Role, round: u64) -> CampaignRunBindingV1 {
        CampaignRunBindingV1 {
            campaign_id: CampaignId::new("c1").unwrap(),
            round: RoundRef {
                round_id: RoundId::new(format!("r{round}")).unwrap(),
                round_ordinal: RoundOrdinal(round),
            },
            role,
            provider_execution_id: ProviderExecutionId::new(exec).unwrap(),
            conversation_id: ConversationId::new("conv1").unwrap(),
            command_id: None,
            run_id: RunId::new(run).unwrap(),
            attempt_id: AttemptId::new("a1").unwrap(),
            input_state_binding: InputStateBindingV1::continued(
                crate::wrappers::CandidateStateReceiptRefV1::new(
                    RunId::new("parent").unwrap(),
                    ArtifactRef {
                        kind: ArtifactKind::CandidateState,
                        locator: "candidate_state_receipt.json".into(),
                        digest: Digest256::of_bytes(b"y"),
                    },
                )
                .unwrap(),
                crate::wrappers::CandidateMaterializationRefV1 {
                    child_run_id: RunId::new(run).unwrap(),
                    materialization_event_id: RunEventId::new("e9").unwrap(),
                    materialization_event_digest: Digest256::of_bytes(b"z"),
                },
            ),
        }
    }

    fn evidence(run: &str) -> EstablishedNonDispatchEvidenceRefV1 {
        EstablishedNonDispatchEvidenceRefV1::new(
            RunId::new(run).unwrap(),
            NonDispatchClassification::Absent,
            "clf-1".into(),
            cas(ContentKind::NonDispatchClassificationBlob),
        )
        .unwrap()
    }

    #[test]
    fn ref_requires_canonical_message_blob_and_copies_tuple() {
        let b = binding("run1", "exec1", Role::Coder, 0);
        assert_eq!(
            CampaignRunBindingRefV1::new(&b, cas(ContentKind::MessagePayloadBlob)),
            Err(BindingError::NotACanonicalMessageBlob),
            "W4: payload bytes are not the complete canonical artifact"
        );
        let r = CampaignRunBindingRefV1::new(&b, cas(ContentKind::CanonicalMessageBlob)).unwrap();
        assert_eq!(r.provider_execution_id, b.provider_execution_id);
    }

    #[test]
    fn wire_ref_claims_must_match_the_resolved_binding() {
        let b = binding("run1", "exec1", Role::Coder, 0);
        let mut wire = WireCampaignRunBindingRefV1 {
            campaign_id: b.campaign_id.clone(),
            round: b.round.clone(),
            role: b.role,
            provider_execution_id: b.provider_execution_id.clone(),
            blob_ref: cas(ContentKind::CanonicalMessageBlob),
        };
        assert!(CampaignRunBindingRefV1::from_wire(wire.clone(), &b).is_ok());
        wire.provider_execution_id = ProviderExecutionId::new("other").unwrap();
        assert_eq!(
            CampaignRunBindingRefV1::from_wire(wire, &b),
            Err(BindingError::TupleMismatch)
        );
    }

    #[test]
    fn safe_redrive_proves_the_same_chain_and_freshness() {
        let prior = binding("run1", "exec1", Role::Coder, 0);
        let current = binding("run2", "exec2", Role::Coder, 0);
        let r =
            CampaignRunBindingRefV1::new(&prior, cas(ContentKind::CanonicalMessageBlob)).unwrap();

        assert!(ExecutionCauseV1::safe_redrive(
            &current,
            ProviderExecutionId::new("exec1").unwrap(),
            r.clone(),
            &prior,
            evidence("run1"),
        )
        .is_ok());

        // W3: a reviewer execution cannot consume a coder-chain prior.
        let reviewer = binding("run3", "exec3", Role::Reviewer, 0);
        assert_eq!(
            ExecutionCauseV1::safe_redrive(
                &reviewer,
                ProviderExecutionId::new("exec1").unwrap(),
                r.clone(),
                &prior,
                evidence("run1"),
            ),
            Err(BindingError::NotSameChain)
        );

        // W3: ref/resolved splice — tuple copied from binding A, resolved B.
        let other_chain = binding("run9", "exec1", Role::Coder, 3);
        assert_eq!(
            ExecutionCauseV1::safe_redrive(
                &current,
                ProviderExecutionId::new("exec1").unwrap(),
                r.clone(),
                &other_chain,
                evidence("run9"),
            ),
            Err(BindingError::TupleMismatch)
        );

        // W3: redrive mints a FRESH execution.
        let same_exec = binding("run2", "exec1", Role::Coder, 0);
        assert_eq!(
            ExecutionCauseV1::safe_redrive(
                &same_exec,
                ProviderExecutionId::new("exec1").unwrap(),
                r.clone(),
                &prior,
                evidence("run1"),
            ),
            Err(BindingError::SelfRedrive)
        );

        // Evidence about a different run.
        assert_eq!(
            ExecutionCauseV1::safe_redrive(
                &current,
                ProviderExecutionId::new("exec1").unwrap(),
                r,
                &prior,
                evidence("run-other"),
            ),
            Err(BindingError::RunIdMismatch)
        );
    }

    #[test]
    fn tool_continuation_requires_tool_result_kind() {
        assert_eq!(
            DispatchCauseV1::tool_continuation(
                ProviderDispatchId::new("d1").unwrap(),
                cas(ContentKind::GateEvidenceBlob),
            ),
            Err(BindingError::NotAToolResultBlob)
        );
        assert!(DispatchCauseV1::tool_continuation(
            ProviderDispatchId::new("d1").unwrap(),
            cas(ContentKind::ToolResultBlob),
        )
        .is_ok());
    }
}
