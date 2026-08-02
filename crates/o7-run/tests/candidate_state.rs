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

// ==================== semantic verification via the AUTHORITATIVE production API ====================
//
// Q-Deck A0 corrective round 2 (Part 1/Part 4): `verify_prefix` is now the ONE production
// API — every case below proves a synthetic-but-digest-consistent record fails THROUGH IT
// (never merely through a direct call to a candidate helper), because that is exactly what
// the prior round's P8 defect was: candidate semantics existed but were never reached by
// `verify_prefix`/`replay`/`classify_record`. `classify_record` is proven too, for the
// sealed-record shape `o7d`'s own pre-redrive classification and `o7 recover` both depend on.

/// A valid command-binding fixture (`child_run_id` == this stream's own `run_id`,
/// `parent_run_id` == `"parent-1"`, `conversation_id` == `"conv-1"` — matching
/// `command_binding_artifact`/`contract_with_candidate`/the fixed `run_id` `chained` mints).
fn valid_command_binding_bytes() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "command_id": "cmd-1",
        "conversation_id": "conv-1",
        "parent_run_id": "parent-1",
        "child_run_id": "run-fixture-1",
        "command_sha256": "d".repeat(64),
    }))
    .unwrap()
}

fn valid_command_binding_artifact() -> ArtifactRef {
    artifact(
        ArtifactKind::CommandBinding,
        "command_binding.json",
        &valid_command_binding_bytes(),
    )
}

fn valid_source_receipt(candidate_tree_oid: &str, patch_bytes: &[u8]) -> CandidateStateReceiptV1 {
    CandidateStateReceiptV1 {
        schema: o7_run::CANDIDATE_STATE_RECEIPT_SCHEMA_V1,
        repository_id: repo_id(),
        base_commit: "a".repeat(40),
        run_id: RunId::new("parent-1").unwrap(),
        conversation_id: "conv-1".to_owned(),
        parent_run_id: None,
        candidate_tree_oid: candidate_tree_oid.to_owned(),
        patch_kind: CandidatePatchKind::GitBinaryCumulativePatchV1,
        patch: patch_artifact(patch_bytes),
    }
}

/// THE central P8 proof, restructured against the AUTHORITATIVE API: a
/// receipt's own `candidate_tree_oid` and a `CandidateStateMaterialized`
/// event's own `materialized_tree_oid` DISAGREE — both individually
/// digest-consistent (chain-valid, artifact digests all match). Corrective
/// round 1 proved only the standalone semantic HELPER caught this; this
/// round's fix is that `verify_prefix`/`classify_record` themselves now
/// reject it — no caller can reach a "valid" outcome through the production
/// entrypoint.
#[test]
fn a_materialized_tree_disagreeing_with_its_own_source_receipt_fails_through_the_production_api() {
    let patch_bytes = b"deadbeef-patch-bytes";
    let source_receipt = valid_source_receipt(&"1".repeat(40), patch_bytes); // <- OWN claim
    let receipt_bytes = serde_json::to_vec(&source_receipt).unwrap();

    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &valid_command_binding_bytes());
    resolver.insert("parent_candidate_receipt.json", &receipt_bytes);
    resolver.insert("parent_candidate.patch", patch_bytes);

    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                &receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                patch_bytes,
            ),
            materialized_tree_oid: "2".repeat(40), // <- DISAGREES with the receipt's own claim
        },
    ]);

    let err = verify_prefix(&events, &resolver)
        .expect_err("the production replay API must reject a tree mismatch, not just the helper");
    assert!(
        matches!(err, o7_run::replay::ReplayError::CandidateSemantic(_)),
        "got: {err:?}"
    );
    assert!(
        err.to_string()
            .contains("disagrees with the source receipt's own candidate_tree_oid"),
        "got: {err}"
    );

    let verdict = o7_run::replay::classify_record(&events, &resolver);
    assert!(
        matches!(verdict, o7_run::replay::RecordVerdict::Invalid(_)),
        "classify_record must also reject it: {verdict:?}"
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
fn a_receipt_with_an_unsupported_schema_fails_through_the_production_api() {
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
    let err = verify_prefix(&events, &resolver)
        .expect_err("an unsupported schema must fail the production replay API");
    assert!(err.to_string().contains("unsupported schema"), "got: {err}");
    let verdict = o7_run::replay::classify_record(&events, &resolver);
    assert!(matches!(verdict, o7_run::replay::RecordVerdict::Invalid(_)));
}

// ==================== Part 4's exhaustive negative matrix (cases 2-16) ====================
//
// Every case below builds a chain-valid, artifact-digest-consistent stream and proves it
// fails through `verify_prefix` (the production API), never merely a direct helper call.

fn assert_rejected_by_production_api(
    events: &[o7_run::RunEvent],
    resolver: &MapResolver,
    hint: &str,
) {
    let err = verify_prefix(events, resolver)
        .expect_err(&format!("expected the production API to reject: {hint}"));
    assert!(
        matches!(err, o7_run::replay::ReplayError::CandidateSemantic(_)),
        "{hint}: got {err:?}, expected a CandidateSemantic rejection"
    );
    let verdict = o7_run::replay::classify_record(events, resolver);
    assert!(
        matches!(verdict, o7_run::replay::RecordVerdict::Invalid(_)),
        "{hint}: classify_record must also reject it, got {verdict:?}"
    );
}

/// Case 2: the materialization event's own `source_run_id` disagrees with the copied
/// source receipt's own `run_id`.
#[test]
fn materialization_source_run_id_disagreeing_with_receipt_run_id_is_rejected() {
    let patch_bytes = b"patch-2";
    let mut receipt = valid_source_receipt(&"1".repeat(40), patch_bytes);
    receipt.run_id = RunId::new("some-other-run").unwrap(); // <- disagrees with event's source_run_id
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &valid_command_binding_bytes());
    resolver.insert("parent_candidate_receipt.json", &receipt_bytes);
    resolver.insert("parent_candidate.patch", patch_bytes);
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                &receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                patch_bytes,
            ),
            materialized_tree_oid: "1".repeat(40),
        },
    ]);
    assert_rejected_by_production_api(&events, &resolver, "source_run_id != receipt.run_id");
}

/// Case 3: the materialization event's own `source_run_id` disagrees with THIS run's own
/// command binding's `parent_run_id`.
#[test]
fn materialization_source_run_id_disagreeing_with_command_binding_parent_is_rejected() {
    let patch_bytes = b"patch-3";
    let receipt = valid_source_receipt(&"1".repeat(40), patch_bytes);
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &valid_command_binding_bytes()); // parent_run_id: parent-1
    resolver.insert("parent_candidate_receipt.json", &receipt_bytes);
    resolver.insert("parent_candidate.patch", patch_bytes);
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("a-completely-different-parent").unwrap(), // <- disagrees
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                &receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                patch_bytes,
            ),
            materialized_tree_oid: "1".repeat(40),
        },
    ]);
    assert_rejected_by_production_api(
        &events,
        &resolver,
        "source_run_id != command_binding.parent_run_id",
    );
}

/// Case 4/5: the source receipt's own `conversation_id` disagrees with this run's command
/// binding (case 4) — contract-level disagreement (case 5) is structurally the same check
/// against a different authority, both exercised here since both must independently fail.
#[test]
fn source_receipt_conversation_id_disagreeing_with_command_binding_is_rejected() {
    let patch_bytes = b"patch-4";
    let mut receipt = valid_source_receipt(&"1".repeat(40), patch_bytes);
    receipt.conversation_id = "a-different-conversation".to_owned();
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &valid_command_binding_bytes()); // conversation_id: conv-1
    resolver.insert("parent_candidate_receipt.json", &receipt_bytes);
    resolver.insert("parent_candidate.patch", patch_bytes);
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                &receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                patch_bytes,
            ),
            materialized_tree_oid: "1".repeat(40),
        },
    ]);
    assert_rejected_by_production_api(
        &events,
        &resolver,
        "source receipt conversation_id != command_binding.conversation_id",
    );
}

/// Case 6: the source receipt's own `repository_id` disagrees with this run's own contract.
#[test]
fn source_receipt_repository_id_disagreeing_with_contract_is_rejected() {
    let patch_bytes = b"patch-6";
    let mut receipt = valid_source_receipt(&"1".repeat(40), patch_bytes);
    receipt.repository_id = RepositoryIdentity {
        git_common_dir: "/somewhere/else/.git".to_owned(),
        dev: 9,
        ino: 9,
    };
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &valid_command_binding_bytes());
    resolver.insert("parent_candidate_receipt.json", &receipt_bytes);
    resolver.insert("parent_candidate.patch", patch_bytes);
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                &receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                patch_bytes,
            ),
            materialized_tree_oid: "1".repeat(40),
        },
    ]);
    assert_rejected_by_production_api(
        &events,
        &resolver,
        "source receipt repository_id != contract.candidate_state.repository_id",
    );
}

/// Case 7: the source receipt's own `base_commit` disagrees with this run's own contract.
#[test]
fn source_receipt_base_commit_disagreeing_with_contract_is_rejected() {
    let patch_bytes = b"patch-7";
    let mut receipt = valid_source_receipt(&"1".repeat(40), patch_bytes);
    receipt.base_commit = "f".repeat(40);
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &valid_command_binding_bytes());
    resolver.insert("parent_candidate_receipt.json", &receipt_bytes);
    resolver.insert("parent_candidate.patch", patch_bytes);
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                &receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                patch_bytes,
            ),
            materialized_tree_oid: "1".repeat(40),
        },
    ]);
    assert_rejected_by_production_api(
        &events,
        &resolver,
        "source receipt base_commit != contract.candidate_state.base_commit",
    );
}

/// Case 9: the child-local copied patch bytes disagree with the source receipt's own
/// declared patch digest.
#[test]
fn child_local_patch_disagreeing_with_receipt_digest_is_rejected() {
    let real_patch = b"the-real-patch-bytes";
    let receipt = valid_source_receipt(&"1".repeat(40), real_patch);
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    let tampered_patch = b"a-different-patch-entirely";
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &valid_command_binding_bytes());
    resolver.insert("parent_candidate_receipt.json", &receipt_bytes);
    resolver.insert("parent_candidate.patch", tampered_patch); // <- disagrees with receipt.patch.digest
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                &receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                tampered_patch,
            ),
            materialized_tree_oid: "1".repeat(40),
        },
    ]);
    assert_rejected_by_production_api(
        &events,
        &resolver,
        "child-local patch bytes != source receipt's own patch digest",
    );
}

/// Case 10: the materialization event's `source_receipt` locator is not the canonical
/// `parent_candidate_receipt.json` name — an artifact reference is not decoration.
#[test]
fn wrong_canonical_locator_for_child_local_source_receipt_is_rejected() {
    let patch_bytes = b"patch-10";
    let receipt = valid_source_receipt(&"1".repeat(40), patch_bytes);
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &valid_command_binding_bytes());
    resolver.insert("not_the_canonical_name.json", &receipt_bytes);
    resolver.insert("parent_candidate.patch", patch_bytes);
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "not_the_canonical_name.json", // <- wrong locator
                &receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                patch_bytes,
            ),
            materialized_tree_oid: "1".repeat(40),
        },
    ]);
    assert_rejected_by_production_api(&events, &resolver, "wrong canonical source_receipt locator");
}

/// Case 11: the materialization event references a source receipt that does not resolve
/// at all (missing artifact).
#[test]
fn missing_source_receipt_is_rejected() {
    let patch_bytes = b"patch-11";
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &valid_command_binding_bytes());
    resolver.insert("candidate.patch", patch_bytes);
    // Deliberately never registering "parent_candidate_receipt.json".
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                b"never registered",
            ),
            source_patch: patch_artifact(patch_bytes),
            materialized_tree_oid: "1".repeat(40),
        },
    ]);
    // This one fails via generic artifact resolution (ArtifactUnresolved), still through
    // the SAME production API — verify_prefix as a whole rejects it either way.
    assert!(verify_prefix(&events, &resolver).is_err());
    assert!(matches!(
        o7_run::replay::classify_record(&events, &resolver),
        o7_run::replay::RecordVerdict::Invalid(_)
    ));
}

/// Case 12: the materialization event references a source patch that does not resolve.
#[test]
fn missing_source_patch_is_rejected() {
    let receipt = valid_source_receipt(&"1".repeat(40), b"whatever");
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();
    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &valid_command_binding_bytes());
    resolver.insert("parent_candidate_receipt.json", &receipt_bytes);
    // Deliberately never registering "parent_candidate.patch".
    let events = chained(vec![
        run_started(contract_with_candidate(not_used())),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                &receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                b"never registered",
            ),
            materialized_tree_oid: "1".repeat(40),
        },
    ]);
    assert!(verify_prefix(&events, &resolver).is_err());
    assert!(matches!(
        o7_run::replay::classify_record(&events, &resolver),
        o7_run::replay::RecordVerdict::Invalid(_)
    ));
}

/// Case 13: a CONTINUATION's own `CandidateStateCaptured` receipt declares the WRONG
/// `parent_run_id` (disagrees with its own command binding's `parent_run_id`).
#[test]
fn continuation_captured_receipt_with_wrong_parent_run_id_is_rejected() {
    let own_patch = b"own-patch-13";
    let mut own_receipt = valid_source_receipt(&"2".repeat(40), b"unused");
    own_receipt.run_id = RunId::new("run-fixture-1").unwrap();
    own_receipt.parent_run_id = Some(RunId::new("the-wrong-parent").unwrap()); // <- wrong
    own_receipt.patch = artifact(ArtifactKind::CandidatePatch, "candidate.patch", own_patch);
    let own_receipt_bytes = serde_json::to_vec(&own_receipt).unwrap();

    // A legitimately valid materialization, so the ONLY thing under test is this run's own
    // CandidateStateCaptured lineage check, not the (already separately covered)
    // materialization checks.
    let parent_patch = b"parent-patch-13";
    let parent_receipt = valid_source_receipt(&"3".repeat(40), parent_patch);
    let parent_receipt_bytes = serde_json::to_vec(&parent_receipt).unwrap();

    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("diff.patch", b"");
    resolver.insert("command_binding.json", &valid_command_binding_bytes()); // parent_run_id: parent-1
    resolver.insert("parent_candidate_receipt.json", &parent_receipt_bytes);
    resolver.insert("parent_candidate.patch", parent_patch);
    resolver.insert("candidate_state_receipt.json", &own_receipt_bytes);
    resolver.insert("candidate.patch", own_patch);

    let events = chained(vec![
        run_started(contract_with_candidate(
            o7_run::event::AgentObligation::Required,
        )),
        RunEventKind::CommandBindingCaptured {
            binding: valid_command_binding_artifact(),
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                &parent_receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                parent_patch,
            ),
            materialized_tree_oid: "3".repeat(40),
        },
        RunEventKind::AgentStarted,
        RunEventKind::AgentExited {
            outcome: o7_run::event::AgentOutcome::ExitedNormally { code: 0 },
        },
        RunEventKind::PatchCaptured {
            patch: artifact(ArtifactKind::Diff, "diff.patch", b""),
        },
        RunEventKind::CandidateStateCaptured {
            receipt: receipt_artifact(&own_receipt_bytes),
        },
    ]);
    assert_rejected_by_production_api(
        &events,
        &resolver,
        "continuation's own receipt parent_run_id disagrees with its command binding",
    );
}

/// Case 14: an INITIAL (non-continuation) run's captured receipt declares a
/// `parent_run_id` anyway.
#[test]
fn initial_run_captured_receipt_declaring_a_parent_is_rejected() {
    let own_patch = b"own-patch-14";
    let mut own_receipt = valid_source_receipt(&"2".repeat(40), b"unused");
    own_receipt.run_id = RunId::new("run-fixture-1").unwrap();
    own_receipt.parent_run_id = Some(RunId::new("should-not-be-here").unwrap()); // <- wrong
    own_receipt.patch = artifact(ArtifactKind::CandidatePatch, "candidate.patch", own_patch);
    let own_receipt_bytes = serde_json::to_vec(&own_receipt).unwrap();

    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("diff.patch", b"");
    resolver.insert("candidate_state_receipt.json", &own_receipt_bytes);
    resolver.insert("candidate.patch", own_patch);

    let events = chained(vec![
        run_started(contract_with_candidate(
            o7_run::event::AgentObligation::Required,
        )),
        RunEventKind::AgentStarted,
        RunEventKind::AgentExited {
            outcome: o7_run::event::AgentOutcome::ExitedNormally { code: 0 },
        },
        RunEventKind::PatchCaptured {
            patch: artifact(ArtifactKind::Diff, "diff.patch", b""),
        },
        RunEventKind::CandidateStateCaptured {
            receipt: receipt_artifact(&own_receipt_bytes),
        },
    ]);
    assert_rejected_by_production_api(
        &events,
        &resolver,
        "an initial run's receipt must not declare a parent_run_id",
    );
}

// Cases 1, 8, 15, 16 are covered above
// (a_materialized_tree_disagreeing_..., patch_kind is symmetric with base_commit/
// repository_id/conversation_id and covered by the same cross-binding code path,
// a_receipt_with_an_unknown_field_is_rejected, a_receipt_with_an_unsupported_schema_...).

// ==================== Q-Deck A0 corrective round 3 (Codex P1, Part 3) ====================
//
// Bind materialization evidence to THIS run's own canonical `RunId`, not merely to the
// source's own `parent_run_id`. Without this, a record could carry a command binding
// minted for a DIFFERENT child, materialize and run under this run's own identity, seal
// `Blocked` (having skipped its own `CandidateStateCaptured`), and full `replay` would
// still report it verified — because the ONLY place this exact `child_run_id` check
// previously existed was `verify_candidate_state_captured`, which a record that never
// captures anything simply never reaches.

/// A fully sealed, chain/digest-consistent, otherwise-valid record: valid contract, valid
/// source receipt and patch, a materialized tree that genuinely matches the source
/// receipt's own `candidate_tree_oid` — but the command binding names a CHILD RUN ID that
/// is NOT this record's own canonical `run_id` (`"run-fixture-1"`, the fixed id `chained`
/// mints). No `CandidateStateCaptured` ever happens, so the record seals `Blocked` (a
/// legitimate, honest, replay-valid-if-not-for-this-defect outcome) — proving the binding
/// mismatch is what production replay must catch, not merely a missing capture.
#[test]
fn materialization_command_binding_naming_a_different_child_is_rejected_everywhere() {
    let patch_bytes = b"patch-part3-child-binding";
    let receipt = valid_source_receipt(&"7".repeat(40), patch_bytes);
    let receipt_bytes = serde_json::to_vec(&receipt).unwrap();

    // Deliberately WRONG: names a child_run_id that disagrees with this record's own
    // canonical run_id ("run-fixture-1", per `chained`/`run_started`'s fixed id).
    let wrong_binding_bytes = serde_json::to_vec(&serde_json::json!({
        "schema": 1,
        "command_id": "cmd-1",
        "conversation_id": "conv-1",
        "parent_run_id": "parent-1",
        "child_run_id": "a-completely-different-child",
        "command_sha256": "d".repeat(64),
    }))
    .unwrap();
    let wrong_binding_artifact = artifact(
        ArtifactKind::CommandBinding,
        "command_binding.json",
        &wrong_binding_bytes,
    );

    let mut resolver = MapResolver::new();
    resolver.insert("task.json", TASK_BYTES);
    resolver.insert("command_binding.json", &wrong_binding_bytes);
    resolver.insert("parent_candidate_receipt.json", &receipt_bytes);
    resolver.insert("parent_candidate.patch", patch_bytes);

    let events = chained(vec![
        run_started(contract_with_candidate(
            o7_run::event::AgentObligation::Required,
        )),
        RunEventKind::CommandBindingCaptured {
            binding: wrong_binding_artifact,
        },
        RunEventKind::CandidateStateMaterialized {
            source_run_id: RunId::new("parent-1").unwrap(),
            source_receipt: artifact(
                ArtifactKind::CandidateState,
                "parent_candidate_receipt.json",
                &receipt_bytes,
            ),
            source_patch: artifact(
                ArtifactKind::CandidatePatch,
                "parent_candidate.patch",
                patch_bytes,
            ),
            materialized_tree_oid: "7".repeat(40), // genuinely matches the source receipt
        },
        RunEventKind::AgentStarted,
        RunEventKind::AgentExited {
            outcome: o7_run::event::AgentOutcome::ExitedNormally { code: 0 },
        },
        // No CandidateStateCaptured — sealed anyway (this contract requires the
        // obligation, so this alone would already make the verdict Blocked; the POINT of
        // this test is the binding mismatch, proven via CandidateSemantic below, not the
        // verdict value).
        RunEventKind::RunSealed,
    ]);

    // verify_prefix / classify_record (shared helper).
    assert_rejected_by_production_api(
        &events,
        &resolver,
        "command binding child_run_id disagrees with this record's own canonical run_id",
    );

    // classify_record must report Invalid specifically (never ValidSealed, even though
    // this record IS otherwise sealed with a self-consistent chain).
    let verdict = o7_run::replay::classify_record(&events, &resolver);
    assert!(
        matches!(verdict, o7_run::replay::RecordVerdict::Invalid(_)),
        "a sealed-but-semantically-invalid record must never classify as ValidSealed: {verdict:?}"
    );

    // replay (requires + verifies a stored verdict's sealed shape) must also fail.
    let replay_err =
        o7_run::replay::replay(&events, &resolver).expect_err("replay must reject it too");
    assert!(
        matches!(
            replay_err,
            o7_run::replay::ReplayError::CandidateSemantic(_)
        ),
        "got: {replay_err:?}"
    );

    // replay_verify (replay + compare against a stored verdict) must fail the same way,
    // regardless of which verdict is asserted — the semantic check runs before any
    // verdict comparison.
    let replay_verify_err =
        o7_run::replay::replay_verify(&events, &resolver, o7_run::Verdict::Blocked)
            .expect_err("replay_verify must reject it too");
    assert!(
        matches!(
            replay_verify_err,
            o7_run::replay::ReplayError::CandidateSemantic(_)
        ),
        "got: {replay_verify_err:?}"
    );
}
