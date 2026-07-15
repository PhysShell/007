#!/usr/bin/env python3
"""CLI entry points rendering N2-C CI artifacts (mirrors N2-B's
generate_ci_artifacts.py structure). Kept as one script (subcommands) since
every subcommand is a thin wrapper around functions already exercised
directly by tests/.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import registry  # noqa: E402
import eligibility  # noqa: E402
import scoring  # noqa: E402
import selection  # noqa: E402
import license_review  # noqa: E402
import frozen_base_check  # noqa: E402
import generate_manifests  # noqa: E402
import generate_freeze_receipt  # noqa: E402

SOURCE_FREEZE_DIR = TOOLS_DIR.parent


def _load_eligible():
    reg = registry.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
    reports = eligibility.evaluate_registry(reg)
    eligible_ids = {r["candidate_id"] for r in reports if r["eligible"]}
    eligible = [c for c in reg["candidates"] if c["candidate_id"] in eligible_ids]
    return reg, reports, eligible


def cmd_frozen_base(args):
    report = frozen_base_check.check(Path(args.repo_root))
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if not report["pass"]:
        print(f"::error::N2-C frozen-base check failed: {report}", file=sys.stderr)
        return 1
    return 0


def cmd_registry_validation(args):
    reg = registry.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
    errors = registry.validate_registry(reg)
    report = {"errors": errors, "valid": not errors, "candidate_count": len(reg["candidates"])}
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if errors:
        print(f"::error::candidate-registry.json failed validation: {errors}", file=sys.stderr)
        return 1
    return 0


def cmd_candidate_inspection(args):
    reg, reports, eligible = _load_eligible()
    report = {
        "candidates_discovered": len(reg["candidates"]),
        "candidates_inspected": len(reg["candidates"]),
        "candidates_eligible": len(eligible),
        "candidates_rejected": len(reg["candidates"]) - len(eligible),
        "per_candidate": reports,
    }
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0


def cmd_license_review(args):
    reg, _reports, eligible = _load_eligible()
    results = []
    for c in eligible:
        record = generate_manifests.build_license_record_for_candidate(c)
        errors = license_review.validate_license_record(record)
        hard_reject = license_review.hard_reject_reasons(record)
        results.append({"candidate_id": c["candidate_id"], "schema_errors": errors,
                         "hard_reject_reasons": hard_reject, "passes": not errors and not hard_reject})
    report = {"results": results, "all_pass": all(r["passes"] for r in results)}
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if not report["all_pass"]:
        print(f"::error::license review failures: {[r for r in results if not r['passes']]}", file=sys.stderr)
        return 1
    return 0


def cmd_eligibility_and_ranking(args):
    reg, reports, eligible = _load_eligible()
    ranked = scoring.rank_all(eligible)
    report = {"eligibility_reports": reports, "ranking": ranked}
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0


def cmd_quota_selection(args):
    _reg, _reports, eligible = _load_eligible()
    result = selection.run_selection(eligible)
    Path(args.out).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    if result["status"] != "FINAL":
        print(f"::error::quota selection did not reach FINAL status: {result['status']}", file=sys.stderr)
        return 1
    return 0


def cmd_selection_determinism(args):
    _reg, _reports, eligible = _load_eligible()
    result_a = selection.run_selection(eligible)
    result_b = selection.run_selection(eligible)
    identical = json.dumps(result_a, sort_keys=True) == json.dumps(result_b, sort_keys=True)
    report = {
        "byte_identical_across_two_runs": identical,
        "primary_case_ids_a": sorted(result_a["primary_case_ids"]),
        "primary_case_ids_b": sorted(result_b["primary_case_ids"]),
    }
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if not identical:
        print("::error::selection is not byte-identical across two independent runs", file=sys.stderr)
        return 1
    return 0


def cmd_generate_manifests(args):
    _reg, _reports, eligible = _load_eligible()
    result = selection.run_selection(eligible)
    by_id = {c["candidate_id"]: c for c in eligible}
    outcome = generate_manifests.generate_all(
        result["primary_case_ids"], result["alternate_case_ids"], by_id, selection.load_quota_contract()
    )
    Path(args.out).write_text(json.dumps(outcome, indent=2, sort_keys=True) + "\n")
    if outcome["errors"]:
        print(f"::error::manifest generation errors: {outcome['errors']}", file=sys.stderr)
        return 1
    return 0


def cmd_acquisition_matrix(args):
    """Emits the JSON matrix `include` list for the trusted-source-acquisition
    job: one entry per frozen primary+alternate candidate, with enough
    fields for the workflow to conditionally checkout (repository-execution)
    or skip straight to the network-fetch script (everything else)."""
    _reg, _reports, eligible = _load_eligible()
    result = selection.run_selection(eligible)
    by_id = {c["candidate_id"]: c for c in eligible}
    matrix = []
    for role, ids in (("primary", result["primary_case_ids"]), ("alternate", result["alternate_case_ids"])):
        for cid in ids:
            c = by_id[cid]
            ident = c["source_identity"]
            matrix.append({
                "candidate_id": cid,
                "role": role,
                "source_kind": c["source_kind"],
                "repository": (ident.get("owner", "") + "/" + ident.get("name", "")) if c["source_kind"] == "repository-execution" else "",
                "ref": ident.get("commit_sha", "") if c["source_kind"] == "repository-execution" else "",
            })
    Path(args.out).write_text(json.dumps({"include": matrix}, sort_keys=True) + "\n")
    print(json.dumps({"include": matrix}))
    return 0


def cmd_freeze_receipt(args):
    receipt = generate_freeze_receipt.build_freeze_receipt(args.main_sha, args.workflow_run_ids or [], args.freeze_timestamp)
    Path(args.out).write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n")
    Path(args.out_md).write_text(generate_freeze_receipt.build_summary_md(receipt))
    return 0


def cmd_seal_check(args):
    """Section 19 sealing checks: candidate registry / selection report /
    source manifests / workflow contain no QODEC/RTK/token-metric/benchmark-
    output markers."""
    reg = registry.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
    problems = registry.validate_no_forbidden_fields(reg)

    workflow_text = (Path(args.repo_root) / ".github" / "workflows" / "qodec-n2-source-freeze.yml").read_text()
    for marker in ("qodec_runner", "rtk_runner", "run-qodec", "run-rtk", "model_call", "anthropic", "openai"):
        if marker in workflow_text.lower():
            problems.append(f"workflow file contains forbidden marker {marker!r}")

    for manifest_path in (SOURCE_FREEZE_DIR / "source-manifests").rglob("*.json"):
        text = manifest_path.read_text().lower()
        for marker in ("compression_ratio", "token_savings", "winner", "preferred_arm"):
            if marker in text:
                problems.append(f"{manifest_path.name} contains forbidden marker {marker!r}")

    report = {"problems": problems, "sealed": not problems}
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    if problems:
        print(f"::error::seal check failed: {problems}", file=sys.stderr)
        return 1
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="command", required=True)

    p = sub.add_parser("frozen-base")
    p.add_argument("--repo-root", required=True)
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_frozen_base)

    p = sub.add_parser("registry-validation")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_registry_validation)

    p = sub.add_parser("candidate-inspection")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_candidate_inspection)

    p = sub.add_parser("license-review")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_license_review)

    p = sub.add_parser("eligibility-and-ranking")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_eligibility_and_ranking)

    p = sub.add_parser("quota-selection")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_quota_selection)

    p = sub.add_parser("selection-determinism")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_selection_determinism)

    p = sub.add_parser("generate-manifests")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_generate_manifests)

    p = sub.add_parser("acquisition-matrix")
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_acquisition_matrix)

    p = sub.add_parser("freeze-receipt")
    p.add_argument("--main-sha", required=True)
    p.add_argument("--workflow-run-ids", nargs="*")
    p.add_argument("--freeze-timestamp", required=True)
    p.add_argument("--out", required=True)
    p.add_argument("--out-md", required=True)
    p.set_defaults(func=cmd_freeze_receipt)

    p = sub.add_parser("seal-check")
    p.add_argument("--repo-root", required=True)
    p.add_argument("--out", required=True)
    p.set_defaults(func=cmd_seal_check)

    args = ap.parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
