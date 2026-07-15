#!/usr/bin/env python3
"""N2-D0: assembles the canonicalized, SHA256-locked durable-input manifest
(Durable manifest section) from the real fetch/verification reports plus
the real GitHub Release asset identities produced after publication. Pure
data assembly — no network, no re-verification (that already happened in
verify_n2c_evidence.py / verify_n2a_evidence.py; this only records the
outcome plus the durable-storage identities)."""
from __future__ import annotations

import hashlib
import json

CANONICAL_INPUT_BY_SOURCE_KIND = {
    "repository-execution": "normalized-source.tar",
    "ci-run-artifact": "normalized-source.tar",
    "dataset-artifact": "normalized-source.tar",
    "research-corpus-artifact": "normalized-source.tar",
    "bot-output-artifact": "normalized-source.tar",
}


def _find_file(entries: list[dict], suffix: str) -> dict | None:
    return next((e for e in entries if e["path"].endswith(suffix)), None)


def build_n2c_entries(fetch_report: dict, quota_selection_report: dict, release_assets_by_name: dict,
                       source_kind_by_candidate: dict, license_by_candidate: dict, verified_at: str) -> list[dict]:
    primary_ids = set(quota_selection_report["primary_case_ids"])
    entries = []
    for fetched in fetch_report["fetched"]:
        name = fetched["artifact_name"]
        if not name.startswith("acquisition-"):
            continue
        candidate_id = name.removeprefix("acquisition-")
        source_kind = source_kind_by_candidate.get(candidate_id, "unknown")
        canonical_suffix = CANONICAL_INPUT_BY_SOURCE_KIND.get(source_kind, "normalized-source.tar")
        canonical_file = _find_file(fetched["contained_files"], canonical_suffix)
        asset = release_assets_by_name.get(name)
        entries.append({
            "logical_id": candidate_id,
            "role": "primary" if candidate_id in primary_ids else "alternate",
            "source_workflow_run_id": fetched["workflow_run_id_of_artifact"],
            "original_artifact_id": fetched["artifact_id"],
            "original_artifact_name": name,
            "original_artifact_digest_sha256": fetched["api_reported_digest_sha256"],
            "byte_size": fetched["api_reported_size_in_bytes"],
            "contained_files": fetched["contained_files"],
            "canonical_benchmark_input_path": canonical_file["path"] if canonical_file else None,
            "canonical_benchmark_input_sha256": canonical_file["sha256"] if canonical_file else None,
            "durable_release_tag": asset["release_tag"] if asset else None,
            "durable_release_asset_name": asset["asset_name"] if asset else None,
            "durable_release_asset_sha256": asset["asset_sha256"] if asset else None,
            "license_reference": license_by_candidate.get(candidate_id),
            "verification_timestamp": verified_at,
            "verifier_tool_identity": "n2d0-verify_n2c_evidence.py",
        })
    return sorted(entries, key=lambda e: e["logical_id"])


def build_n2a_entry(fetch_report: dict, release_assets_by_name: dict, verified_at: str) -> dict:
    capture_a = release_assets_by_name.get("miner-canary-capture-a")
    fetched_by_name = {f["artifact_name"]: f for f in fetch_report["fetched"]}
    return {
        "logical_id": "miner-canary-dotnet-001",
        "role": "n2a-canary",
        "source_workflow_run_id": fetched_by_name.get("miner-canary-source", {}).get("workflow_run_id_of_artifact"),
        "accepted_head_sha": "9c755dba8986323b1f18d49c26b53fec3aff5be4",
        "canonical_capture": "capture-a",
        "reproducibility_evidence_capture": "capture-b",
        "canonical_capture_selection_rule": (
            "capture-a is the prespecified canonical benchmark input; capture-b is retained solely as "
            "reproducibility evidence, never chosen based on later QODEC/RTK results (N2-D0 contract)."
        ),
        "artifacts": {
            name: {
                "original_artifact_id": f["artifact_id"],
                "original_artifact_digest_sha256": f["api_reported_digest_sha256"],
                "byte_size": f["api_reported_size_in_bytes"],
                "contained_files": f["contained_files"],
                "durable_release_tag": release_assets_by_name.get(name, {}).get("release_tag"),
                "durable_release_asset_name": release_assets_by_name.get(name, {}).get("asset_name"),
                "durable_release_asset_sha256": release_assets_by_name.get(name, {}).get("asset_sha256"),
            }
            for name, f in fetched_by_name.items()
        },
        "canonical_benchmark_input_asset": "miner-canary-capture-a" if capture_a else None,
        "license_reference": "MIT (danhpaiva/EncryptAesNet-console-app-csharp, per accepted N2-A source-manifest.json)",
        "verification_timestamp": verified_at,
        "verifier_tool_identity": "n2d0-verify_n2a_evidence.py",
    }


def canonicalize_and_hash(manifest_without_hash: dict) -> tuple[str, str]:
    """Returns (canonical_json_text, sha256) for the manifest body with its
    own self-referential hash field excluded from the hashed text."""
    canonical = json.dumps(manifest_without_hash, indent=2, sort_keys=True) + "\n"
    return canonical, hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def build_manifest(n2c_entries: list[dict], n2a_entry: dict, alternate_fallback_order: list[str],
                    verified_at: str) -> dict:
    body = {
        "manifest_contract_version": "n2d0-durable-input-manifest-v1",
        "verified_at": verified_at,
        "n2c_source_workflow_run_id": "29404265568",
        "n2c_accepted_head_sha": "acb57379e2d0b9ed6fe79fd45e7540d7d00d7490",
        "n2a_source_workflow_run_id": n2a_entry["source_workflow_run_id"],
        "n2a_accepted_head_sha": n2a_entry["accepted_head_sha"],
        "primary_case_count": sum(1 for e in n2c_entries if e["role"] == "primary"),
        "alternate_case_count": sum(1 for e in n2c_entries if e["role"] == "alternate"),
        "alternate_fallback_order": alternate_fallback_order,
        "n2c_entries": n2c_entries,
        "n2a_entry": n2a_entry,
    }
    canonical_text, digest = canonicalize_and_hash(body)
    body["manifest_sha256"] = digest
    return {"body": body, "canonical_text_without_self_hash": canonical_text, "manifest_sha256": digest}
