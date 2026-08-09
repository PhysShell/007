//! Class-3 external wrappers (contract §7.1/§6/§11.1) and
//! `InputStateBindingV1` (§7.1a). These are the cross-universe refs into
//! A0/R1 canonical records — they consume `o7_run` types EXACTLY
//! (never re-declare, never typedef; E9), and their checked
//! constructors enforce every cross-field rule that needs no external
//! resolution. Rules that DO need resolution (full `verify_prefix`,
//! event-kind/ordering checks, correspondence verdicts) take interim
//! proof inputs documented like `ResolvedCausation` (T4-R1): freely
//! constructible for now, replaced by resolver-minted evidence in the
//! resolver slice — which alone will mint `Accepted…` forms.

use serde::{Deserialize, Serialize};

use o7_run::event::{ArtifactKind, ArtifactRef};
use o7_run::ids::{RunEventId, RunId};

use crate::cas::{CasObjectRefV1, ContentKind};

/// Wrapper construction failures — each names the violated frozen rule.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WrapperError {
    /// §7.1: the wrapped run artifact must be `ArtifactKind::CandidateState`.
    #[error("run_artifact_ref must be ArtifactKind::CandidateState")]
    NotACandidateStateArtifact,
    /// §7.1a: the correspondence blob must be the worktree-correspondence kind.
    #[error("correspondence_ref must be a worktree-correspondence-evidence-blob")]
    NotACorrespondenceBlob,
    /// §11.1: the classification record must be the non-dispatch kind.
    #[error("classification_record_ref must be a non-dispatch-classification-blob")]
    NotAClassificationBlob,
    /// §7.1a InitialMaterialization: both refs must name ONE run.
    #[error("initial-materialization refs name two different runs")]
    TwoDifferentRuns,
}

/// §7.1: a candidate-state receipt in an A0 canonical record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateStateReceiptRefV1 {
    /// The A0 run that captured the receipt.
    pub source_run_id: RunId,
    /// The receipt artifact — `ArtifactKind::CandidateState`, enforced.
    run_artifact_ref: ArtifactRef,
}

impl CandidateStateReceiptRefV1 {
    /// Wrap a candidate-state artifact ref.
    ///
    /// # Errors
    /// [`WrapperError::NotACandidateStateArtifact`] for any other kind.
    pub fn new(source_run_id: RunId, run_artifact_ref: ArtifactRef) -> Result<Self, WrapperError> {
        if run_artifact_ref.kind != ArtifactKind::CandidateState {
            return Err(WrapperError::NotACandidateStateArtifact);
        }
        Ok(Self {
            source_run_id,
            run_artifact_ref,
        })
    }

    /// The wrapped artifact ref.
    #[must_use]
    pub fn run_artifact_ref(&self) -> &ArtifactRef {
        &self.run_artifact_ref
    }
}

/// §7.1: the `CandidateStateMaterialized` event in a child A0 record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateMaterializationRefV1 {
    /// The continuation child run.
    pub child_run_id: RunId,
    /// The materialization event.
    pub materialization_event_id: RunEventId,
    /// Its recorded digest.
    pub materialization_event_digest: crate::ids::BlobDigest,
}

/// §7.1 (P1-8): the obligation lives INSIDE
/// `RunStarted.contract.candidate_state` in accepted A0 — this ref
/// names that exact event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunContractCandidateStateRefV1 {
    /// The run whose contract carries the obligation.
    pub run_id: RunId,
    /// The `run_started` event.
    pub run_started_event_id: RunEventId,
    /// Its recorded digest.
    pub run_started_event_digest: crate::ids::BlobDigest,
}

/// §7.1 (P1-8): materialization evidence enters A0 via
/// `WorktreeCreated { worktree: ArtifactRef }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorktreeMaterializationRefV1 {
    /// The run that created the worktree.
    pub run_id: RunId,
    /// The `worktree_created` event.
    pub worktree_created_event_id: RunEventId,
    /// Its recorded digest.
    pub worktree_created_event_digest: crate::ids::BlobDigest,
}

/// §6 (P1-18): the run-relative source of a CAS import — the ONLY other
/// legal home of a bare `o7_run::ArtifactRef` besides the A0 wrappers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunArtifactSourceRefV1 {
    /// The source run.
    pub source_run_id: RunId,
    /// The exact run artifact being imported.
    pub run_artifact_ref: ArtifactRef,
}

/// §11.1: the ONLY two R1 classifications that establish non-dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NonDispatchClassification {
    /// No canonical stream at all.
    Absent,
    /// A valid prefix that never reached the dispatch boundary.
    ValidUnsealedPreDispatch,
}

/// §11.1: "which artifact proves established non-dispatch" — a typed
/// answer, not a convincing field name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstablishedNonDispatchEvidenceRefV1 {
    /// The prior attempt's run.
    pub run_id: RunId,
    /// One of the two pre-dispatch classes (any other classification is
    /// unconstructible here by the enum).
    pub classification: NonDispatchClassification,
    /// The classifier that produced the recorded output.
    pub classifier_version: String,
    /// The recorded classifier output blob.
    classification_record_ref: CasObjectRefV1,
}

impl EstablishedNonDispatchEvidenceRefV1 {
    /// Wrap recorded non-dispatch evidence.
    ///
    /// # Errors
    /// [`WrapperError::NotAClassificationBlob`] unless the blob is the
    /// `non-dispatch-classification-blob` kind.
    pub fn new(
        run_id: RunId,
        classification: NonDispatchClassification,
        classifier_version: String,
        classification_record_ref: CasObjectRefV1,
    ) -> Result<Self, WrapperError> {
        if classification_record_ref.content_kind != ContentKind::NonDispatchClassificationBlob {
            return Err(WrapperError::NotAClassificationBlob);
        }
        Ok(Self {
            run_id,
            classification,
            classifier_version,
            classification_record_ref,
        })
    }

    /// The recorded classification blob.
    #[must_use]
    pub fn classification_record_ref(&self) -> &CasObjectRefV1 {
        &self.classification_record_ref
    }
}

/// §7.1a: the exact input state of an execution — a CLOSED type, never
/// an `Option` (P1-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum InputStateBindingV1 {
    /// First round of a fresh task: obligation + worktree + the P1-21
    /// correspondence evidence.
    InitialMaterialization {
        /// The A0 obligation ref.
        run_contract_ref: RunContractCandidateStateRefV1,
        /// The worktree evidence ref.
        worktree_ref: WorktreeMaterializationRefV1,
        /// `worktree-correspondence-evidence-blob` (kind enforced).
        correspondence_ref: CasObjectRefV1,
    },
    /// Continuation: a specific accepted candidate, materialized.
    ContinuedCandidate {
        /// The parent's candidate-state receipt.
        candidate_state_ref: CandidateStateReceiptRefV1,
        /// The child's materialization of it.
        materialization_ref: CandidateMaterializationRefV1,
    },
}

impl InputStateBindingV1 {
    /// Build the initial-materialization variant, enforcing the
    /// field-level rules that need no resolution: one run across both
    /// refs (P1-8's "ONE verified run" — the verified half is the
    /// resolver slice's proof) and the correspondence blob kind.
    ///
    /// # Errors
    /// [`WrapperError`] on a kind or same-run violation.
    pub fn initial(
        run_contract_ref: RunContractCandidateStateRefV1,
        worktree_ref: WorktreeMaterializationRefV1,
        correspondence_ref: CasObjectRefV1,
    ) -> Result<Self, WrapperError> {
        if run_contract_ref.run_id != worktree_ref.run_id {
            return Err(WrapperError::TwoDifferentRuns);
        }
        if correspondence_ref.content_kind != ContentKind::WorktreeCorrespondenceEvidenceBlob {
            return Err(WrapperError::NotACorrespondenceBlob);
        }
        Ok(Self::InitialMaterialization {
            run_contract_ref,
            worktree_ref,
            correspondence_ref,
        })
    }

    /// Build the continued-candidate variant. The §7.1a cross-object
    /// rule (the materialization event's copied source receipt is
    /// byte/digest-identical to the referenced receipt, same
    /// `source_run_id`) requires A0 record resolution — the resolver
    /// slice's proof obligation, recorded here so the seam is explicit.
    #[must_use]
    pub fn continued(
        candidate_state_ref: CandidateStateReceiptRefV1,
        materialization_ref: CandidateMaterializationRefV1,
    ) -> Self {
        Self::ContinuedCandidate {
            candidate_state_ref,
            materialization_ref,
        }
    }
}

#[cfg(test)]
mod tests {
    // Test-only panics operate on this module's own fixtures; a panic
    // here is the test failing loudly (workspace test convention).
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn cas(kind: ContentKind) -> CasObjectRefV1 {
        CasObjectRefV1 {
            digest: crate::ids::BlobDigest::of_bytes(b"x"),
            size: 1,
            media_type: "application/json".into(),
            content_kind: kind,
        }
    }

    fn artifact(kind: ArtifactKind) -> ArtifactRef {
        ArtifactRef {
            kind,
            locator: "candidate_state_receipt.json".into(),
            digest: o7_run::event::Digest256::of_bytes(b"y"),
        }
    }

    #[test]
    fn candidate_state_ref_enforces_artifact_kind() {
        let run = RunId::new("r1").unwrap();
        assert!(
            CandidateStateReceiptRefV1::new(run.clone(), artifact(ArtifactKind::Diff)).is_err()
        );
        assert!(
            CandidateStateReceiptRefV1::new(run, artifact(ArtifactKind::CandidateState)).is_ok()
        );
    }

    #[test]
    fn initial_binding_enforces_one_run_and_blob_kind() {
        let contract = RunContractCandidateStateRefV1 {
            run_id: RunId::new("r1").unwrap(),
            run_started_event_id: RunEventId::new("e1").unwrap(),
            run_started_event_digest: crate::ids::BlobDigest::of_bytes(b"a"),
        };
        let worktree_same = WorktreeMaterializationRefV1 {
            run_id: RunId::new("r1").unwrap(),
            worktree_created_event_id: RunEventId::new("e2").unwrap(),
            worktree_created_event_digest: crate::ids::BlobDigest::of_bytes(b"b"),
        };
        let worktree_other = WorktreeMaterializationRefV1 {
            run_id: RunId::new("r2").unwrap(),
            ..worktree_same.clone()
        };
        assert_eq!(
            InputStateBindingV1::initial(
                contract.clone(),
                worktree_other,
                cas(ContentKind::WorktreeCorrespondenceEvidenceBlob)
            ),
            Err(WrapperError::TwoDifferentRuns)
        );
        assert_eq!(
            InputStateBindingV1::initial(
                contract.clone(),
                worktree_same.clone(),
                cas(ContentKind::ContractBlob)
            ),
            Err(WrapperError::NotACorrespondenceBlob)
        );
        assert!(InputStateBindingV1::initial(
            contract,
            worktree_same,
            cas(ContentKind::WorktreeCorrespondenceEvidenceBlob)
        )
        .is_ok());
    }

    #[test]
    fn non_dispatch_evidence_enforces_blob_kind_and_closed_classes() {
        let bad = EstablishedNonDispatchEvidenceRefV1::new(
            RunId::new("r1").unwrap(),
            NonDispatchClassification::Absent,
            "clf-1".into(),
            cas(ContentKind::GateEvidenceBlob),
        );
        assert_eq!(bad, Err(WrapperError::NotAClassificationBlob));
        // dispatch_ambiguous is not even spellable here — the enum has
        // two variants, wire-unrepresentable beyond them:
        let parsed: Result<NonDispatchClassification, _> =
            serde_json::from_str("\"dispatch_ambiguous\"");
        assert!(parsed.is_err());
    }
}
