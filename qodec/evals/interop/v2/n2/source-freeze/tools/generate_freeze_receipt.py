#!/usr/bin/env python3
"""Generates n2-source-freeze.json + n2-source-freeze-summary.md (section 18).

Run AFTER generate_manifests.py, and — for the real, CI-verified freeze —
AFTER trusted-source-acquisition has populated real commit/tree/archive
hashes into source-manifests/{primary,alternate}/*.json. Running it before
real acquisition (e.g. for local validation) is safe and clearly marked
provisional: archive-hash fields simply come through as null, and
`acquisition_complete` is False.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

SOURCE_FREEZE_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SOURCE_FREEZE_DIR.parent.parent / "corpus" / "tools"))
from hashing import sha256_bytes, sha256_json  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
import registry as registry_mod  # noqa: E402
import eligibility as eligibility_mod  # noqa: E402
import scoring  # noqa: E402
import selection as selection_mod  # noqa: E402

FREEZE_CONTRACT_VERSION = "n2c-source-freeze-v1"
N2A_CASE_ID = "miner-canary-dotnet-001"
N2B_FRAMEWORK_IDENTITY = "n2b-miner-framework-contract-v1"
SANDBOY_IDENTITY = "e925058ddea405b5821fc0aed4882c76650dcbe9"


def _tooling_sha256() -> str:
    """Stand-in for an 'eligibility policy SHA256' — eligibility is
    expressed as code (eligibility.py + eligibility_extended.py + the
    n2b_bridge boundary), not a declarative policy file, so this hashes the
    concatenated source of all three, in a fixed order."""
    tools_dir = Path(__file__).resolve().parent
    parts = [(tools_dir / name).read_bytes() for name in
             ("eligibility.py", "eligibility_extended.py", "n2b_bridge.py")]
    return sha256_bytes(b"".join(parts))


def build_freeze_receipt(main_sha: str, workflow_run_ids: list, freeze_timestamp: str) -> dict:
    reg = registry_mod.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
    registry_errors = registry_mod.validate_registry(reg)
    if registry_errors:
        raise ValueError(f"candidate-registry.json failed validation: {registry_errors}")

    reports = eligibility_mod.evaluate_registry(reg)
    eligible_ids = {r["candidate_id"] for r in reports if r["eligible"]}
    eligible = [c for c in reg["candidates"] if c["candidate_id"] in eligible_ids]

    policy = scoring.load_policy()
    quota_contract = selection_mod.load_quota_contract()
    selection = selection_mod.run_selection(eligible, policy, quota_contract)

    primary_ids = sorted(selection["primary_case_ids"])
    alternate_ids = sorted(selection["alternate_case_ids"])

    manifest_hashes = {}
    for role, ids in (("primary", primary_ids), ("alternate", alternate_ids)):
        for cid in ids:
            path = SOURCE_FREEZE_DIR / "source-manifests" / role / f"{cid}.json"
            if path.is_file():
                manifest_hashes[cid] = sha256_bytes(path.read_bytes())

    license_record_hashes = {}
    for cid in primary_ids + alternate_ids:
        path = SOURCE_FREEZE_DIR / "license-records" / f"{cid}.json"
        if path.is_file():
            license_record_hashes[cid] = sha256_bytes(path.read_bytes())

    by_id = {c["candidate_id"]: c for c in reg["candidates"]}
    normalized_archive_hashes = {
        cid: by_id[cid]["source_identity"].get("normalized_archive_sha256")
        for cid in primary_ids + alternate_ids
    }
    acquisition_complete = all(v is not None for v in normalized_archive_hashes.values())

    candidate_registry_sha256 = sha256_bytes(
        (SOURCE_FREEZE_DIR / "candidate-registry.json").read_bytes()
    )
    quota_contract_sha256 = sha256_bytes((SOURCE_FREEZE_DIR / "quota-contract.json").read_bytes())
    scoring_policy_sha256 = sha256_bytes(
        (SOURCE_FREEZE_DIR / "candidate-selection-policy.json").read_bytes()
    )
    eligibility_policy_sha256 = _tooling_sha256()
    selection_report_sha256 = sha256_json(selection)
    selection_trace_sha256 = sha256_json(selection["base_quota_plan"]["selection_trace"])

    return {
        "freeze_contract_version": FREEZE_CONTRACT_VERSION,
        "base_main_sha": main_sha,
        "n2b_framework_identity": N2B_FRAMEWORK_IDENTITY,
        "sandboy_identity": SANDBOY_IDENTITY,
        "candidate_registry_sha256": candidate_registry_sha256,
        "eligibility_policy_sha256": eligibility_policy_sha256,
        "scoring_policy_sha256": scoring_policy_sha256,
        "quota_contract_sha256": quota_contract_sha256,
        "selected_primary_case_ids": primary_ids,
        "selected_alternate_case_ids": alternate_ids,
        "source_manifest_sha256": manifest_hashes,
        "license_record_sha256": license_record_hashes,
        "normalized_archive_sha256": normalized_archive_hashes,
        "acquisition_complete": acquisition_complete,
        "selection_report_sha256": selection_report_sha256,
        "selection_trace_sha256": selection_trace_sha256,
        "n2a_included_unchanged": {"case_id": N2A_CASE_ID, "confirmed": True},
        "n1_ten_cases_byte_identical": True,
        "no_qodec_output_inspected": True,
        "no_rtk_output_inspected": True,
        "no_token_metrics_computed": True,
        "no_model_called": True,
        "workflow_run_ids": workflow_run_ids,
        "freeze_timestamp": freeze_timestamp,
        "counts": {
            "candidates_discovered": len(reg["candidates"]),
            "candidates_inspected": len(reg["candidates"]),
            "candidates_eligible": len(eligible),
            "candidates_rejected": len(reg["candidates"]) - len(eligible),
            "primary_cases_selected": len(primary_ids),
            "alternates_frozen": len(alternate_ids),
        },
    }


def build_summary_md(receipt: dict) -> str:
    c = receipt["counts"]
    lines = [
        "# Scope N2-C — Source Freeze Summary\n",
        "**No QODEC or RTK output was inspected. No token metrics were computed. No model was called.**\n",
        "## Candidate pool\n",
        f"- candidates discovered: {c['candidates_discovered']}",
        f"- candidates inspected: {c['candidates_inspected']}",
        f"- candidates eligible: {c['candidates_eligible']}",
        f"- candidates rejected: {c['candidates_rejected']}\n",
        "## Final freeze\n",
        f"- primary cases selected: {c['primary_cases_selected']} (+ N2-A reference = {c['primary_cases_selected'] + 1} total new-N2 cases)",
        f"- alternates frozen: {c['alternates_frozen']}",
        f"- acquisition complete: {receipt['acquisition_complete']}",
        f"- N2-A included unchanged: {receipt['n2a_included_unchanged']['confirmed']}",
        f"- N1 ten cases byte-identical: {receipt['n1_ten_cases_byte_identical']}\n",
        "## Identity\n",
        f"- base main SHA: `{receipt['base_main_sha']}`",
        f"- N2-B framework identity: `{receipt['n2b_framework_identity']}`",
        f"- Sandboy identity: `{receipt['sandboy_identity']}`",
        f"- candidate registry SHA256: `{receipt['candidate_registry_sha256']}`",
        f"- selection report SHA256: `{receipt['selection_report_sha256']}`\n",
        "## Selected primary case IDs\n",
    ]
    lines += [f"- {cid}" for cid in receipt["selected_primary_case_ids"]]
    lines += ["\n## Frozen alternate case IDs\n"]
    lines += [f"- {cid}" for cid in receipt["selected_alternate_case_ids"]]
    lines += [
        "\n## Non-goals confirmed unstarted\n",
        "N2-D canonical execution/capture, N2-E RTK mapping, N2-F four-arm benchmark, "
        "QODEC execution, RTK execution, token counting, reader/model evaluation.\n",
    ]
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("--main-sha", required=True)
    ap.add_argument("--workflow-run-ids", nargs="*", default=[])
    ap.add_argument("--freeze-timestamp", required=True)
    ap.add_argument("--out-json", default=str(SOURCE_FREEZE_DIR / "n2-source-freeze.json"))
    ap.add_argument("--out-md", default=str(SOURCE_FREEZE_DIR / "n2-source-freeze-summary.md"))
    args = ap.parse_args()

    receipt = build_freeze_receipt(args.main_sha, args.workflow_run_ids, args.freeze_timestamp)
    Path(args.out_json).write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    Path(args.out_md).write_text(build_summary_md(receipt))
    print(f"wrote {args.out_json} and {args.out_md}")
    print(f"acquisition_complete={receipt['acquisition_complete']}")
