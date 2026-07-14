#!/usr/bin/env python3
"""N2-B AcquisitionPlanner (section 11).

Generates a PLAN — trusted acquisition / trusted dependency realization /
untrusted execution / artifact collection, each stage's allowed operations
made explicit — for a candidate + resolved ecosystem adapter. It never
performs a new acquisition run itself; N2-B's only real checkout stays the
frozen N2-A one.
"""
from __future__ import annotations

CHECKOUT_ACTION_IDENTITY = "actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 #v4.2.2"


def plan_acquisition(candidate: dict, manifest: dict, adapter) -> dict:
    trusted_acquisition = {
        "repository": candidate["repository"],
        "immutable_commit": candidate["commit_sha"],
        "checkout_action_identity": CHECKOUT_ACTION_IDENTITY,
        "persist_credentials": False,
        "allowed_operations": [
            "checkout", "source_validation", "license_validation",
            "archive_normalization", "archive_hashing", "git_dir_removal", "credential_removal",
        ],
        "forbidden_operations": [
            "restore", "build", "test", "execute_repository_script", "package_manager_invocation",
        ],
    }
    trusted_dependency_realization = adapter.plan_trusted_setup(manifest)
    trusted_dependency_realization["allowed_operations"] = [
        "dependency_realization_via_declared_toolchain_command",
    ]
    trusted_dependency_realization["forbidden_operations"] = [
        "execute_arbitrary_repository_script", "network_access_after_this_stage_closes",
    ]

    untrusted_execution = adapter.plan_untrusted_execution(manifest)
    untrusted_execution["allowed_operations"] = ["run_declared_build_command_under_sandbox"]
    untrusted_execution["forbidden_operations"] = ["network_access", "credential_access"]

    artifact_collection = {
        "captures": ["stdout", "stderr", "exit_code", "receipt.json", "sanitization-report.json"],
        "allowed_operations": ["read_capture_outputs", "hash_capture_outputs", "sanitize_capture_outputs"],
        "forbidden_operations": ["re_execute_repository_code", "re_fetch_source"],
    }

    return {
        "candidate_id": candidate.get("candidate_id"),
        "plan_only": True,
        "note": "PLAN ONLY — does not execute any new acquisition, build, or test run in Scope N2-B",
        "stages": {
            "trusted_acquisition": trusted_acquisition,
            "trusted_dependency_realization": trusted_dependency_realization,
            "untrusted_execution": untrusted_execution,
            "artifact_collection": artifact_collection,
        },
    }
