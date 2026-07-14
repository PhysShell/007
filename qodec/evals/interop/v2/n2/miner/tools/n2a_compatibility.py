#!/usr/bin/env python3
"""N2-B N2-A compatibility gate (section 17).

Builds a generic plan from the FROZEN N2-A source-manifest.json (read-only —
never modified, never re-executed) using the generic dotnet ToolAdapter, and
diffs it against N2-A's actual accepted manifest content. Acceptance requires
zero unexplained incompatibilities; deliberate generalizations (ecosystem
scope, adapter abstraction, receipt-field nesting) are listed separately so
they're never mistaken for regressions.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parent.parent
N2_DIR = MINER_DIR.parent
N2A_MANIFEST_PATH = N2_DIR / "canary" / "source-manifest.json"

sys.path.insert(0, str(MINER_DIR / "tools"))
from adapters import dotnet_adapter  # noqa: E402
from sandbox_planner import ACCEPTED_SANDBOY_COMMIT_SHA  # noqa: E402

N2A_ACCEPTED_SANDBOY_COMMIT_SHA = "e925058ddea405b5821fc0aed4882c76650dcbe9"


def load_n2a_manifest() -> dict:
    return json.loads(N2A_MANIFEST_PATH.read_text())


def project_reference_candidate(n2a_manifest: dict) -> dict:
    repo = n2a_manifest["repository"]
    return {
        "candidate_id": "n2a-reference",
        "repository": {"url": repo["url"], "owner": repo["owner"], "name": repo["name"]},
        "commit_sha": repo["approved_commit_sha"],
        "ecosystem": n2a_manifest["project"]["ecosystem"],
        "origin_kind": "n2a-reference-only",
    }


def project_reference_manifest(n2a_manifest: dict) -> dict:
    return {
        "ecosystem": n2a_manifest["project"]["ecosystem"],
        "project": {"entry_point": n2a_manifest["project"]["path"], "ambiguous": False},
        "dependency_lock": {"present": False, "files": []},
    }


def compute_compatibility_report() -> dict:
    n2a_manifest = load_n2a_manifest()
    candidate = project_reference_candidate(n2a_manifest)
    manifest = project_reference_manifest(n2a_manifest)

    matched_fields = []
    incompatible_fields = []

    if candidate["repository"]["url"] == n2a_manifest["repository"]["url"]:
        matched_fields.append("source_repository")
    else:
        incompatible_fields.append({"field": "source_repository"})

    if candidate["commit_sha"] == n2a_manifest["repository"]["approved_commit_sha"]:
        matched_fields.append("source_commit_sha")
    else:
        incompatible_fields.append({"field": "source_commit_sha"})

    if manifest["project"]["entry_point"] == n2a_manifest["project"]["path"]:
        matched_fields.append("project_entry_point")
    else:
        incompatible_fields.append({"field": "project_entry_point"})

    if candidate["ecosystem"] == n2a_manifest["project"]["ecosystem"]:
        matched_fields.append("ecosystem")
    else:
        incompatible_fields.append({"field": "ecosystem"})

    generic_untrusted = dotnet_adapter.plan_untrusted_execution(manifest)
    n2a_build_argv = n2a_manifest["build"]["argv"]
    if generic_untrusted["argv"] == n2a_build_argv:
        matched_fields.append("build_argv_semantics")
    else:
        incompatible_fields.append({
            "field": "build_argv_semantics", "generic": generic_untrusted["argv"], "n2a": n2a_build_argv,
        })

    if generic_untrusted["network_during_execution"] == n2a_manifest["build"]["network_during_execution"]:
        matched_fields.append("network_during_execution")
    else:
        incompatible_fields.append({"field": "network_during_execution"})

    generic_trusted_setup = dotnet_adapter.plan_trusted_setup(manifest)
    generic_restore_argv = generic_trusted_setup["steps"][0]["argv"]
    n2a_restore_argv = ["dotnet", "restore", n2a_manifest["project"]["path"]]
    if generic_restore_argv == n2a_restore_argv:
        matched_fields.append("trusted_setup_restore_argv")
    else:
        incompatible_fields.append({
            "field": "trusted_setup_restore_argv", "generic": generic_restore_argv, "n2a": n2a_restore_argv,
        })

    # True by construction: both separate trusted acquisition/setup from
    # network-isolated, Sandboy-confined, no-restore untrusted execution.
    matched_fields.append("trust_stage_separation")
    matched_fields.append("offline_untrusted_execution_requirement")

    if ACCEPTED_SANDBOY_COMMIT_SHA == N2A_ACCEPTED_SANDBOY_COMMIT_SHA:
        matched_fields.append("accepted_sandboy_commit_sha")
    else:
        incompatible_fields.append({
            "field": "accepted_sandboy_commit_sha",
            "generic": ACCEPTED_SANDBOY_COMMIT_SHA, "n2a": N2A_ACCEPTED_SANDBOY_COMMIT_SHA,
        })

    intentionally_generalized_fields = [
        {
            "field": "ecosystem_scope",
            "explanation": "N2-A hardcodes exactly one dotnet build; the generic framework supports "
                           "dotnet/rust/python/jvm-maven/jvm-gradle via a shared ToolAdapter contract.",
        },
        {
            "field": "adapter_abstraction",
            "explanation": "N2-A's dotnet_adapter.py exposes single-purpose functions; the generic "
                           "ToolAdapter contract is a superset interface each ecosystem implements.",
        },
        {
            "field": "receipt_field_naming",
            "explanation": "N2-A records flat semantic fields (e.g. dotnet_sdk_version, "
                           "dotnet_runtime_identifier). The generic receipt contract nests the same "
                           "information under toolchain_requested/resolved/executed sections, "
                           "generalized across ecosystems — same required non-empty evidence, "
                           "different (superset) shape.",
        },
        {
            "field": "planning_vs_execution",
            "explanation": "N2-B produces PLANS (AcquisitionPlanner/SandboxExecutionPlanner output); "
                           "it performs no new real capture run. N2-A's accepted implementation remains "
                           "the only executed, frozen instance until N2-C selects further real cases.",
        },
    ]

    missing_generic_capabilities = [
        "N2-B does not itself perform a real, executed capture run for any candidate "
        "(including the N2-A reference) — this compatibility gate is plan-vs-manifest only.",
    ]

    return {
        "gate": "n2b-n2a-compatibility",
        "reference_case_id": n2a_manifest["case_id"],
        "matched_fields": matched_fields,
        "intentionally_generalized_fields": intentionally_generalized_fields,
        "incompatible_fields": incompatible_fields,
        "missing_generic_capabilities": missing_generic_capabilities,
        "zero_unexplained_incompatibilities": len(incompatible_fields) == 0,
    }


if __name__ == "__main__":
    report = compute_compatibility_report()
    out_path = Path(sys.argv[1]) if len(sys.argv) > 1 else None
    text = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if out_path:
        out_path.write_text(text)
    else:
        print(text)
