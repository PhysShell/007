//! `CampaignRunBindingV1` — the canonical bridge between logical
//! lineage and physical execution (contract §2.1, C5) — its typed ref,
//! and the frozen execution/dispatch cause vocabulary (§8.6 as
//! re-adjudicated by T5-R1). The binding ref is not decoration here: it
//! is the ANTECEDENT AUTHORITY for execution SafeRedrive, existing
//! durably before dispatch in exactly the states where redrive is
//! legal.

use serde::{Deserialize, Serialize};

use o7_run::ids::RunId;

use crate::cas::{CasObjectRefV1, ContentKind};
use crate::ids::{AttemptId, CampaignId, CommandId, ConversationId, ProviderExecutionId, RoundRef};
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
/// execution actually materialized (P1-8 execution-input equality is
/// the report classifiers' obligation).
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

/// Ref construction failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BindingError {
    /// The blob ref must carry a canonical message payload kind.
    #[error("binding blob_ref must be a message-payload-blob")]
    NotAPayloadBlob,
    /// The resolved binding does not carry exactly the claimed tuple.
    #[error("resolved binding does not match the carried identity tuple")]
    TupleMismatch,
    /// SafeRedrive: the binding's execution id is not the prior one.
    #[error("prior binding's provider_execution_id != prior_execution_id")]
    ExecutionIdMismatch,
    /// SafeRedrive: the binding's run is not the evidence's run.
    #[error("prior binding's run_id != non-dispatch evidence run_id")]
    RunIdMismatch,
}

/// A typed ref to a committed binding (contract §2.1). The checked
/// constructor takes the RESOLVED binding as its proof input — interim,
/// freely constructible like `ResolvedCausation` (T4-R1), replaced by
/// resolver-minted evidence in the resolver slice — and requires the
/// carried tuple to EQUAL the resolved binding's own fields, so a ref
/// claiming one identity over a blob holding another is not
/// constructible even at this layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignRunBindingRefV1 {
    /// Claimed campaign.
    pub campaign_id: CampaignId,
    /// Claimed round pair.
    pub round: RoundRef,
    /// Claimed role.
    pub role: Role,
    /// Claimed execution.
    pub provider_execution_id: ProviderExecutionId,
    /// The canonical binding bytes.
    blob_ref: CasObjectRefV1,
}

impl CampaignRunBindingRefV1 {
    /// Build a ref proven against the resolved binding.
    ///
    /// # Errors
    /// [`BindingError`] on a kind violation or any tuple mismatch.
    pub fn new(
        resolved: &CampaignRunBindingV1,
        blob_ref: CasObjectRefV1,
    ) -> Result<Self, BindingError> {
        if blob_ref.content_kind != ContentKind::MessagePayloadBlob {
            return Err(BindingError::NotAPayloadBlob);
        }
        Ok(Self {
            campaign_id: resolved.campaign_id.clone(),
            round: resolved.round.clone(),
            role: resolved.role,
            provider_execution_id: resolved.provider_execution_id.clone(),
            blob_ref,
        })
    }

    /// The canonical binding bytes.
    #[must_use]
    pub fn blob_ref(&self) -> &CasObjectRefV1 {
        &self.blob_ref
    }
}

/// §8.6 execution causes (closed; re-adjudicated T5-R1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutionCauseV1 {
    /// First execution of its chain.
    Initial,
    /// Opens a corrective round; the verdict is the prior round's.
    CorrectiveRound {
        /// The accepted `ReviewVerdict` (canonical bytes).
        prior_verdict_ref: CasObjectRefV1,
    },
    /// Proven pre-dispatch retry of a prior execution of the SAME role
    /// chain. The prior RUN BINDING — durable before dispatch — is the
    /// antecedent authority; a receipt cannot be (it is unconstructible
    /// before a terminal/ambiguous outcome, and redrive is only legal
    /// pre-dispatch: the intersection is empty).
    SafeRedrive {
        /// The prior execution being redriven.
        prior_execution_id: ProviderExecutionId,
        /// Its binding — durable pre-dispatch (boxed: the variant is
        /// large and clippy's variant-size lint is right about it).
        prior_run_binding_ref: Box<CampaignRunBindingRefV1>,
        /// Established non-dispatch.
        evidence: EstablishedNonDispatchEvidenceRefV1,
    },
}

impl ExecutionCauseV1 {
    /// Build a SafeRedrive with the frozen cross-checks: the binding's
    /// execution id equals `prior_execution_id` (field-level, now), and
    /// the RESOLVED prior binding's run equals the evidence's run
    /// (interim proof input, resolver-minted later).
    ///
    /// # Errors
    /// [`BindingError::ExecutionIdMismatch`] / [`BindingError::RunIdMismatch`].
    pub fn safe_redrive(
        prior_execution_id: ProviderExecutionId,
        prior_run_binding_ref: CampaignRunBindingRefV1,
        resolved_prior_binding: &CampaignRunBindingV1,
        evidence: EstablishedNonDispatchEvidenceRefV1,
    ) -> Result<Self, BindingError> {
        if prior_run_binding_ref.provider_execution_id != prior_execution_id
            || resolved_prior_binding.provider_execution_id != prior_execution_id
        {
            return Err(BindingError::ExecutionIdMismatch);
        }
        if resolved_prior_binding.run_id != evidence.run_id {
            return Err(BindingError::RunIdMismatch);
        }
        Ok(Self::SafeRedrive {
            prior_execution_id,
            prior_run_binding_ref: Box::new(prior_run_binding_ref),
            evidence,
        })
    }
}

/// §8.6 dispatch causes — `Initial | ToolContinuation` ONLY: v1 has no
/// dispatch-level SafeRedrive (T5-R1); a proven pre-dispatch retry
/// mints a fresh execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum DispatchCauseV1 {
    /// The execution's first dispatch.
    Initial,
    /// Continuation after a controller-authorized tool result.
    ToolContinuation {
        /// The prior dispatch.
        prior_dispatch_id: crate::ids::ProviderDispatchId,
        /// The observed tool result blob.
        tool_result_ref: CasObjectRefV1,
    },
}

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::ids::{BlobDigest, RoundId, RoundOrdinal};
    use crate::wrappers::NonDispatchClassification;
    use o7_run::event::{ArtifactKind, ArtifactRef};
    use o7_run::ids::RunEventId;

    fn cas(kind: ContentKind) -> CasObjectRefV1 {
        CasObjectRefV1 {
            digest: BlobDigest::of_bytes(b"x"),
            size: 1,
            media_type: "application/json".into(),
            content_kind: kind,
        }
    }

    fn binding(run: &str, exec: &str) -> CampaignRunBindingV1 {
        CampaignRunBindingV1 {
            campaign_id: CampaignId::new("c1").unwrap(),
            round: RoundRef {
                round_id: RoundId::new("r0").unwrap(),
                round_ordinal: RoundOrdinal(0),
            },
            role: Role::Coder,
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
                        digest: o7_run::event::Digest256::of_bytes(b"y"),
                    },
                )
                .unwrap(),
                crate::wrappers::CandidateMaterializationRefV1 {
                    child_run_id: RunId::new(run).unwrap(),
                    materialization_event_id: RunEventId::new("e9").unwrap(),
                    materialization_event_digest: BlobDigest::of_bytes(b"z"),
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
    fn binding_ref_copies_the_resolved_tuple() {
        let b = binding("run1", "exec1");
        let r = CampaignRunBindingRefV1::new(&b, cas(ContentKind::MessagePayloadBlob)).unwrap();
        assert_eq!(r.provider_execution_id, b.provider_execution_id);
        assert!(
            CampaignRunBindingRefV1::new(&b, cas(ContentKind::GateEvidenceBlob)).is_err(),
            "wrong blob kind rejected"
        );
    }

    #[test]
    fn safe_redrive_cross_checks_execution_and_run() {
        let prior = binding("run1", "exec1");
        let r = CampaignRunBindingRefV1::new(&prior, cas(ContentKind::MessagePayloadBlob)).unwrap();

        // Happy path.
        assert!(ExecutionCauseV1::safe_redrive(
            ProviderExecutionId::new("exec1").unwrap(),
            r.clone(),
            &prior,
            evidence("run1"),
        )
        .is_ok());

        // Wrong execution id.
        assert_eq!(
            ExecutionCauseV1::safe_redrive(
                ProviderExecutionId::new("exec2").unwrap(),
                r.clone(),
                &prior,
                evidence("run1"),
            ),
            Err(BindingError::ExecutionIdMismatch)
        );

        // Evidence about a DIFFERENT run than the binding's.
        assert_eq!(
            ExecutionCauseV1::safe_redrive(
                ProviderExecutionId::new("exec1").unwrap(),
                r,
                &prior,
                evidence("run-other"),
            ),
            Err(BindingError::RunIdMismatch)
        );
    }

    #[test]
    fn dispatch_safe_redrive_is_unrepresentable() {
        let parsed: Result<DispatchCauseV1, _> =
            serde_json::from_str(r#"{"safe_redrive":{"prior_dispatch_id":"d1"}}"#);
        assert!(parsed.is_err(), "v1 has no dispatch-level SafeRedrive");
    }
}
