//! Q-Deck A0 corrective round 1 (`docs/q-deck/a0-candidate-state.md`):
//! structural ordering (the pure reducer) and semantic verification (the
//! `candidate` module, built on `replay`) for candidate-state evidence.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use common::*;
use o7_run::candidate::{
    verify_candidate_state_captured, verify_candidate_state_materialized, CandidateVerifyError,
};
use o7_run::event::{
    ArtifactKind, ArtifactRef, CandidatePatchKind, CandidateStateContractV1, Digest256,
    RepositoryIdentity, RunContract, RunEventKind,
};
use o7_run::ids::RunId;
use o7_run::reduce::{reduce_all, ReduceError};
use o7_run::replay::verify_prefix;
use o7_run::{CandidateStateReceiptV1, Verdict};

fn repo_id() -> RepositoryIdentity {
    RepositoryIdentity {
        git_common_dir: "/fixture/.git".to_owned(),
        dev: 1,
        ino: 2,
    }
}

fn candidate_obligation() -> CandidateStateContractV1 {
    CandidateStateContractV1 {
        schema: 1,
        conversation_id: "conv-1".to_owned(),
        repository_id: repo_id(),
        base_commit: "a".repeat(40),
        patch_kind: CandidatePatchKind::GitBinaryCumulativePatchV1,
    }
}

/// A contract requiring the candidate-state obligation, gate/policy/sandbox-free, with the
/// given agent obligation.
fn contract_with_candidate(agent: o7_run::event::AgentObligation) -> RunContract {
    let mut c = contract_full(vec![], vec![], vec![], agent);
    c.candidate_state = Some(candidate_obligation());
    c
}

fn command_binding_artifact() -> ArtifactRef {
    artifact(ArtifactKind::CommandBinding, "command_binding.json", b"{}")
}

fn patch_artifact(bytes: &[u8]) -> ArtifactRef {
    artifact(ArtifactKind::CandidatePatch, "candidate.patch", bytes)
}

fn receipt_artifact(bytes: &[u8]) -> ArtifactRef {
    artifact(
        ArtifactKind::CandidateState,
        "candidate_state_receipt.json",
        bytes,
    )
}

fn structural_err(events: &[o7_run::RunEvent]) -> ReduceError {
    match reduce_all(events) {
        Ok(_) => panic!("expected a structural error, got Ok"),
        Err(e) => e,
    }
}

// ============================ structural ordering (Part 5) ============================

#[test]
fn candidate_materialized_without_command_binding_fails_structurally() {
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: receipt_artifact(b"{}"),
            source_patch: patch_artifact(b""),
            materialized_tree_oid: "a".repeat(40),
        },
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::CandidateMaterializationWithoutCommandBinding { .. }
    ));
}

#[test]
fn candidate_materialized_after_agent_started_fails_structurally() {
    // No candidate obligation on the contract here, deliberately: the point of THIS test is
    // the materialization-after-AgentStarted ordering rule in isolation, not the (separate)
    // AgentStarted-requires-materialization rule a candidate obligation would also trigger.
    let events = chained(vec![
        run_started(contract_full(
            vec![],
            vec![],
            vec![],
            o7_run::event::AgentObligation::Required,
        )),
        RunEventKind::CommandBindingCaptured {
            binding: command_binding_artifact(),
        },
        RunEventKind::AgentStarted,
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: receipt_artifact(b"{}"),
            source_patch: patch_artifact(b""),
            materialized_tree_oid: "a".repeat(40),
        },
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::CandidateMaterializationAfterAgentStarted { .. }
    ));
}

#[test]
fn agent_started_without_prior_materialization_fails_when_contract_requires_it() {
    let events = chained(vec![
        run_started(contract_with_candidate(
            o7_run::event::AgentObligation::Required,
        )),
        RunEventKind::CommandBindingCaptured {
            binding: command_binding_artifact(),
        },
        RunEventKind::AgentStarted,
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::AgentStartedWithoutCandidateMaterialization { .. }
    ));
}

#[test]
fn agent_started_after_materialization_is_fine_when_contract_requires_it() {
    let events = chained(vec![
        run_started(contract_with_candidate(
            o7_run::event::AgentObligation::Required,
        )),
        RunEventKind::CommandBindingCaptured {
            binding: command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: receipt_artifact(b"{}"),
            source_patch: patch_artifact(b""),
            materialized_tree_oid: "a".repeat(40),
        },
        RunEventKind::AgentStarted,
    ]);
    assert!(reduce_all(&events).is_ok());
}

#[test]
fn duplicate_candidate_materialized_fails_structurally() {
    let events = chained(vec![
        run_started(contract_full(
            vec![],
            vec![],
            vec![],
            o7_run::event::AgentObligation::Required,
        )),
        RunEventKind::CommandBindingCaptured {
            binding: command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: receipt_artifact(b"{}"),
            source_patch: patch_artifact(b""),
            materialized_tree_oid: "a".repeat(40),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: receipt_artifact(b"{}"),
            source_patch: patch_artifact(b""),
            materialized_tree_oid: "a".repeat(40),
        },
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::DuplicateCandidateMaterialized { .. }
    ));
}

#[test]
fn candidate_captured_before_patch_fails_structurally() {
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CandidateStateCaptured {
            receipt: receipt_artifact(b"{}"),
        },
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::CandidateCaptureBeforePatch { .. }
    ));
}

#[test]
fn candidate_captured_before_agent_terminal_fails_structurally() {
    let events = chained(vec![
        run_started(contract_with_candidate(
            o7_run::event::AgentObligation::Required,
        )),
        RunEventKind::PatchCaptured {
            patch: artifact(ArtifactKind::Diff, "diff.patch", b""),
        },
        RunEventKind::CandidateStateCaptured {
            receipt: receipt_artifact(b"{}"),
        },
    ]);
    assert!(matches!(
        structural_err(&events),
        ReduceError::CandidateCaptureBeforeAgentTerminal { .. }
    ));
}

#[test]
fn run_sealed_without_candidate_capture_is_blocked_when_contract_requires_it() {
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).unwrap();
    assert_eq!(state.verdict, Some(Verdict::Blocked));
}

#[test]
fn run_sealed_with_candidate_capture_present_can_pass() {
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::PatchCaptured {
            patch: artifact(ArtifactKind::Diff, "diff.patch", b""),
        },
        RunEventKind::CandidateStateCaptured {
            receipt: receipt_artifact(b"{}"),
        },
        RunEventKind::RunSealed,
    ]);
    let state = reduce_all(&events).unwrap();
    assert_eq!(state.verdict, Some(Verdict::Pass));
}

#[test]
fn a_pre_a0_contract_with_no_candidate_obligation_stays_replay_compatible() {
    // Legacy/non-A0 contract: no candidate_state obligation declared at all — sealing
    // without ever capturing candidate state must NOT be Blocked.
    let events = chained(vec![run_started(contract_full(
        vec![],
        vec![],
        vec![],
        not_used(),
    ))]);
    let events = {
        let mut v = events;
        v.push(make_chained_next(&v, RunEventKind::RunSealed));
        v
    };
    let state = reduce_all(&events).unwrap();
    assert_eq!(state.verdict, Some(Verdict::Pass));
}

/// Append one more event onto an already-chained stream, computing its own
/// correct sequence/link.
fn make_chained_next(existing: &[o7_run::RunEvent], kind: RunEventKind) -> o7_run::RunEvent {
    let run = existing[0].run_id.clone();
    let last = existing.last().unwrap();
    let mut event = o7_run::RunEvent {
        event_id: o7_run::ids::RunEventId::new(format!("ev-{}", last.sequence + 1)).unwrap(),
        schema_version: o7_run::event::RUN_EVENT_SCHEMA_VERSION,
        run_id: run,
        sequence: last.sequence + 1,
        previous_event_digest: last.event_digest.clone(),
        event_digest: Digest256::genesis(),
        timestamp_millis: (last.sequence + 1) as i64,
        kind,
    };
    event.event_digest = event.compute_digest();
    event
}

// ============================ semantic verification (Part 4 / the P8 fix) ============================

/// THE central P8 proof: a receipt's own `candidate_tree_oid` and a
/// `CandidateStateMaterialized` event's own `materialized_tree_oid` DISAGREE
/// — both individually digest-consistent (so the GENERIC `verify_prefix`
/// happily accepts the stream: no chain break, no artifact digest
/// mismatch), yet the SEMANTIC layer (`verify_candidate_state_materialized`)
/// independently resolves the source receipt and refuses the disagreement.
/// This is exactly the "vacuous expected/actual pair" defect this round
/// fixes: the event no longer carries its own comparison value at all.
#[test]
fn a_materialized_tree_disagreeing_with_its_own_source_receipt_fails_semantic_verification_but_not_generic_replay(
) {
    let patch_bytes = b"deadbeef-patch-bytes";
    let source_receipt = CandidateStateReceiptV1 {
        schema: o7_run::CANDIDATE_STATE_RECEIPT_SCHEMA_V1,
        repository_id: repo_id(),
        base_commit: "a".repeat(40),
        run_id: RunId::new("parent-1").unwrap(),
        conversation_id: "conv-1".to_owned(),
        parent_run_id: None,
        candidate_tree_oid: "1".repeat(40), // <- the receipt's OWN claim
        patch_kind: CandidatePatchKind::GitBinaryCumulativePatchV1,
        patch: patch_artifact(patch_bytes),
    };
    let receipt_bytes = serde_json::to_vec(&source_receipt).unwrap();

    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", b"{}");
    resolver.insert("candidate_state_receipt.json", &receipt_bytes);
    resolver.insert("candidate.patch", patch_bytes);

    let events = chained(vec![
        run_started(contract_full(
            vec![],
            vec![],
            vec![],
            o7_run::event::AgentObligation::Required,
        )),
        RunEventKind::CommandBindingCaptured {
            binding: command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: receipt_artifact(&receipt_bytes),
            source_patch: patch_artifact(patch_bytes),
            materialized_tree_oid: "2".repeat(40), // <- DISAGREES with the receipt's own claim
        },
        RunEventKind::AgentStarted,
    ]);

    // The GENERIC layer sees only digest-consistent, correctly-chained
    // events and two artifacts whose bytes match their own declared
    // digests — it has no opinion on what the receipt's CONTENT means, so
    // it succeeds.
    let (state, _artifacts_verified) =
        verify_prefix(&events, &resolver).expect("generic replay has no candidate-state opinion");

    // The SEMANTIC layer resolves the source receipt's own content and
    // catches the disagreement.
    let err = verify_candidate_state_materialized(&state, &resolver).expect_err(
        "semantic verification must catch a materialized tree OID that disagrees with its own \
         source receipt",
    );
    let CandidateVerifyError(reason) = err;
    assert!(
        reason.contains("disagrees with the source receipt's own candidate_tree_oid"),
        "got: {reason}"
    );
}

#[test]
fn a_receipt_with_an_unknown_field_is_rejected() {
    let mut value = serde_json::json!({
        "schema": 1,
        "repository_id": {"git_common_dir": "/x/.git", "dev": 1, "ino": 2},
        "base_commit": "a".repeat(40),
        "run_id": "run-1",
        "conversation_id": "conv-1",
        "candidate_tree_oid": "b".repeat(40),
        "patch_kind": "git_binary_cumulative_patch_v1",
        "patch": {"kind": "candidate_patch", "locator": "candidate.patch", "digest": "c".repeat(64)},
    });
    value["totally_unexpected_field"] = serde_json::json!("should not parse");
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(serde_json::from_slice::<CandidateStateReceiptV1>(&bytes).is_err());
}

#[test]
fn a_receipt_with_an_unsupported_schema_fails_semantic_verification() {
    let patch_bytes = b"patch";
    let bad_receipt = serde_json::json!({
        "schema": 999,
        "repository_id": {"git_common_dir": "/x/.git", "dev": 1, "ino": 2},
        "base_commit": "a".repeat(40),
        "run_id": "run-1",
        "conversation_id": "conv-1",
        "candidate_tree_oid": "b".repeat(40),
        "patch_kind": "git_binary_cumulative_patch_v1",
        "patch": {
            "kind": "candidate_patch",
            "locator": "candidate.patch",
            "digest": Digest256::of_bytes(patch_bytes).as_str(),
        },
    });
    let receipt_bytes = serde_json::to_vec(&bad_receipt).unwrap();
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("diff.patch", b"");
    resolver.insert("candidate_state_receipt.json", &receipt_bytes);
    resolver.insert("candidate.patch", patch_bytes);

    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::PatchCaptured {
            patch: artifact(ArtifactKind::Diff, "diff.patch", b""),
        },
        RunEventKind::CandidateStateCaptured {
            receipt: receipt_artifact(&receipt_bytes),
        },
    ]);
    let (state, _) = verify_prefix(&events, &resolver).unwrap();
    let contract = state.contract.clone().unwrap();
    let err = verify_candidate_state_captured(&state, &contract, &resolver)
        .expect_err("an unsupported schema must fail semantic verification");
    assert!(err.0.contains("unsupported schema"), "got: {}", err.0);
}
