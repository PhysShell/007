//! Q-Deck A0 corrective round 1 (`docs/q-deck/a0-candidate-state.md`): the
//! typed, versioned candidate-state receipt an artifact's own bytes must
//! deserialize to, and the SEMANTIC verification layer built on top of
//! [`crate::replay`]'s generic chain/digest machinery.
//!
//! [`crate::replay::verify_prefix`] proves a `CandidateStateCaptured` or
//! `CandidateStateMaterialized` event's own referenced artifact resolves and
//! its digest matches — but that alone proves nothing about what the bytes
//! MEAN: a syntactically valid, digest-consistent JSON blob with a
//! self-consistent internal tree OID is not evidence of anything a
//! materialization decision can trust. The functions here additionally:
//! deserialize the exact typed schema (rejecting unknown fields and
//! unsupported versions), resolve the receipt's own NESTED patch artifact
//! reference and verify ITS digest too (the generic layer only sees the
//! receipt's bytes, never what the receipt itself points at), and cross-bind
//! every load-bearing field against the run's own canonical identity and
//! contract obligation. This is what turns "a file with the right hash
//! exists" into "its content actually proves the claimed lineage."

use serde::{Deserialize, Serialize};

use crate::event::{ArtifactKind, ArtifactRef, CandidatePatchKind, Digest256, RepositoryIdentity};
use crate::ids::RunId;
use crate::replay::{ArtifactError, ArtifactResolver};
use crate::state::RunState;

pub const CANDIDATE_STATE_RECEIPT_SCHEMA_V1: u32 = 1;

/// The typed, versioned content of a `candidate_state_receipt.json` artifact
/// — captured by a run about its own cumulative state, or copied verbatim
/// into a child's own directory as its proof of what it materialized from.
/// `#[serde(deny_unknown_fields)]`: an receipt with an extra field is a
/// foreign or corrupted record, never silently accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateStateReceiptV1 {
    pub schema: u32,
    pub repository_id: RepositoryIdentity,
    pub base_commit: String,
    pub run_id: RunId,
    pub conversation_id: String,
    #[serde(default)]
    pub parent_run_id: Option<RunId>,
    pub candidate_tree_oid: String,
    pub patch_kind: CandidatePatchKind,
    /// The raw cumulative patch, referenced by digest — never inlined as
    /// bytes in the receipt itself, so the (potentially large, binary)
    /// patch stays a separate artifact with its own distinct
    /// `ArtifactKind::CandidatePatch`.
    pub patch: ArtifactRef,
}

/// Everything that can make semantic candidate-state verification fail. A
/// single, descriptive variant — this layer's failures are all "the
/// evidence disagrees with itself or its authority," not a family of
/// distinct mechanical faults worth their own types.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("candidate-state semantic verification failed: {0}")]
pub struct CandidateVerifyError(pub String);

fn fail(reason: impl Into<String>) -> CandidateVerifyError {
    CandidateVerifyError(reason.into())
}

fn resolve_and_verify(
    artifacts: &dyn ArtifactResolver,
    r: &ArtifactRef,
    expected_kind: ArtifactKind,
    what: &str,
) -> Result<Vec<u8>, CandidateVerifyError> {
    if r.kind != expected_kind {
        return Err(fail(format!(
            "{what}'s own artifact reference has kind {:?}, expected {:?}",
            r.kind, expected_kind
        )));
    }
    let bytes = artifacts
        .resolve(r)
        .map_err(|e: ArtifactError| fail(format!("resolving {what}: {e}")))?;
    if Digest256::of_bytes(&bytes) != r.digest {
        return Err(fail(format!(
            "{what}'s resolved bytes do not match its own declared digest"
        )));
    }
    Ok(bytes)
}

fn parse_receipt(
    bytes: &[u8],
    what: &str,
) -> Result<CandidateStateReceiptV1, CandidateVerifyError> {
    let receipt: CandidateStateReceiptV1 =
        serde_json::from_slice(bytes).map_err(|e| fail(format!("parsing {what}: {e}")))?;
    if receipt.schema != CANDIDATE_STATE_RECEIPT_SCHEMA_V1 {
        return Err(fail(format!(
            "{what} has unsupported schema {} (expected {CANDIDATE_STATE_RECEIPT_SCHEMA_V1})",
            receipt.schema
        )));
    }
    Ok(receipt)
}

/// Semantically verify a run's OWN captured candidate-state receipt against
/// its contract obligation and canonical run id. Requires the caller to have
/// already run [`crate::replay::verify_prefix`] (so `state`/`contract`/the
/// receipt's own event-level digest are already proven); this function
/// proves the receipt's CONTENT, and its nested patch reference, actually
/// matches what the run declared up front.
///
/// # Errors
/// [`CandidateVerifyError`] if: no receipt was captured; it fails to parse
/// or has an unsupported schema; its `conversation_id`/`repository_id`/
/// `base_commit`/`patch_kind` disagree with `contract.candidate_state`; its
/// `run_id` disagrees with this run's own canonical id; its nested patch
/// artifact has the wrong kind, cannot be resolved, or its digest disagrees.
pub fn verify_candidate_state_captured(
    state: &RunState,
    contract: &crate::event::RunContract,
    artifacts: &dyn ArtifactResolver,
) -> Result<CandidateStateReceiptV1, CandidateVerifyError> {
    let receipt_ref = state
        .candidate_state
        .as_ref()
        .ok_or_else(|| fail("no CandidateStateCaptured receipt in this run's state"))?;
    let bytes = resolve_and_verify(
        artifacts,
        receipt_ref,
        ArtifactKind::CandidateState,
        "this run's own candidate-state receipt",
    )?;
    let receipt = parse_receipt(&bytes, "this run's own candidate-state receipt")?;

    let obligation = contract
        .candidate_state
        .as_ref()
        .ok_or_else(|| fail("this run's own contract declares no candidate_state obligation"))?;
    if receipt.conversation_id != obligation.conversation_id {
        return Err(fail(
            "receipt conversation_id disagrees with the run's own contract",
        ));
    }
    if receipt.repository_id != obligation.repository_id {
        return Err(fail(
            "receipt repository_id disagrees with the run's own contract",
        ));
    }
    if receipt.base_commit != obligation.base_commit {
        return Err(fail(
            "receipt base_commit disagrees with the run's own contract",
        ));
    }
    if receipt.patch_kind != obligation.patch_kind {
        return Err(fail(
            "receipt patch_kind disagrees with the run's own contract",
        ));
    }
    let Some(expected_run_id) = &state.run_id else {
        return Err(fail("this run has no bound canonical run_id yet"));
    };
    if &receipt.run_id != expected_run_id {
        return Err(fail(
            "receipt run_id disagrees with this run's own canonical run_id",
        ));
    }
    if state.command_binding.is_none() && receipt.parent_run_id.is_some() {
        return Err(fail(
            "an initial (non-continuation) run's receipt must not declare a parent_run_id",
        ));
    }
    resolve_and_verify(
        artifacts,
        &receipt.patch,
        ArtifactKind::CandidatePatch,
        "this receipt's own nested patch artifact",
    )?;
    Ok(receipt)
}

/// The result of semantically verifying a command-continuation child's
/// `CandidateStateMaterialized` evidence.
#[derive(Debug, Clone)]
pub struct CandidateMaterializationCheck {
    pub source_run_id: RunId,
    pub source_receipt: CandidateStateReceiptV1,
    pub patch_bytes: Vec<u8>,
}

/// Semantically verify a command-continuation child's own materialization
/// evidence — the fix for the "vacuous expected/actual pair" defect: THIS
/// function is what actually proves `materialized_tree_oid` against an
/// INDEPENDENTLY resolved and digest-verified source receipt, not a second
/// field on the same event asserting the same thing about itself.
///
/// # Errors
/// [`CandidateVerifyError`] if: no materialization evidence exists; the
/// copied source receipt fails to parse/has an unsupported schema/disagrees
/// with `source_run_id`; the copied patch fails to resolve or disagrees with
/// the source receipt's own patch digest; or `materialized_tree_oid`
/// disagrees with the source receipt's own `candidate_tree_oid`.
pub fn verify_candidate_state_materialized(
    state: &RunState,
    artifacts: &dyn ArtifactResolver,
) -> Result<CandidateMaterializationCheck, CandidateVerifyError> {
    let mat = state
        .candidate_materialized
        .as_ref()
        .ok_or_else(|| fail("no CandidateStateMaterialized evidence in this run's state"))?;
    let receipt_bytes = resolve_and_verify(
        artifacts,
        &mat.source_receipt,
        ArtifactKind::CandidateState,
        "the child's own copy of the parent's candidate-state receipt",
    )?;
    let source_receipt = parse_receipt(
        &receipt_bytes,
        "the child's own copy of the parent's candidate-state receipt",
    )?;
    if source_receipt.run_id != mat.source_run_id {
        return Err(fail(
            "the copied source receipt's own run_id disagrees with this event's source_run_id",
        ));
    }
    let patch_bytes = resolve_and_verify(
        artifacts,
        &mat.source_patch,
        ArtifactKind::CandidatePatch,
        "the child's own copy of the parent's patch",
    )?;
    if Digest256::of_bytes(&patch_bytes) != source_receipt.patch.digest {
        return Err(fail(
            "the child's copied patch does not match the source receipt's own patch digest",
        ));
    }
    // THE central fix: compare the materialized tree OID against the
    // INDEPENDENTLY resolved, digest-verified source receipt's own claim —
    // never a value the same event alone asserts about itself.
    if mat.materialized_tree_oid != source_receipt.candidate_tree_oid {
        return Err(fail(format!(
            "materialized tree {} disagrees with the source receipt's own candidate_tree_oid {}",
            mat.materialized_tree_oid, source_receipt.candidate_tree_oid
        )));
    }
    Ok(CandidateMaterializationCheck {
        source_run_id: mat.source_run_id.clone(),
        source_receipt,
        patch_bytes,
    })
}
