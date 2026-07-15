#!/usr/bin/env python3
"""N2-D0: cross-verification of rescued N2-C evidence (section: Verification
chain). Pure, network-free — operates only on already-fetched directories
(see zip_fetch.py for the real download step). Never trusts the committed
candidate-registry.json for real-acquisition hashes (those fields are
intentionally null pre-lock, per N2-C's own documented contract) — the
folded values live only in the accepted n2-source-freeze / source-identity
artifacts, which is what this module actually checks against.
"""
from __future__ import annotations

import json
from pathlib import Path


def load_json(path: Path) -> dict:
    return json.loads(path.read_text())


def verify_per_artifact_file_hashes(fetch_report: dict, artifact_index: list[dict]) -> list[dict]:
    """Every file inside every fetched acquisition-* artifact must match the
    accepted n2c-artifact-index.json's recorded path/sha256 for that exact
    artifact_id — not just artifact_name (an id match rules out any name
    collision across reruns)."""
    index_by_id = {entry["artifact_id"]: entry for entry in artifact_index}
    problems = []
    for fetched in fetch_report["fetched"]:
        if not fetched["artifact_name"].startswith("acquisition-"):
            continue
        aid = fetched["artifact_id"]
        indexed = index_by_id.get(aid)
        if indexed is None:
            problems.append({"artifact_name": fetched["artifact_name"], "artifact_id": aid,
                              "problem": "artifact_id not present in accepted n2c-artifact-index.json"})
            continue
        if indexed["archive_digest"].removeprefix("sha256:") != fetched["api_reported_digest_sha256"]:
            problems.append({"artifact_name": fetched["artifact_name"], "artifact_id": aid,
                              "problem": "archive_digest mismatch vs accepted artifact index",
                              "indexed": indexed["archive_digest"], "fetched": fetched["api_reported_digest_sha256"]})
        indexed_files = {f["path"]: f["sha256"] for f in indexed["contained_files"]}
        fetched_files = {f["path"]: f["sha256"] for f in fetched["contained_files"]}
        missing = sorted(set(indexed_files) - set(fetched_files))
        extra = sorted(set(fetched_files) - set(indexed_files))
        mismatched = sorted(p for p in (set(indexed_files) & set(fetched_files))
                             if indexed_files[p] != fetched_files[p])
        if missing or extra or mismatched:
            problems.append({"artifact_name": fetched["artifact_name"], "artifact_id": aid,
                              "missing_files": missing, "extra_files": extra, "mismatched_files": mismatched})
    return problems


def verify_receipt_field_values(extract_root: Path, n2_source_freeze: dict) -> list[dict]:
    """Re-reads each fetched acquisition-*/acquisition-receipt.json and
    compares its own recorded hash fields against the accepted
    n2-source-freeze.json's folded values for that candidate_id."""
    problems = []
    hash_fields = ("metadata_sha256", "source_content_sha256", "normalized_source_sha256")
    for artifact_dir in sorted(extract_root.glob("acquisition-*")):
        candidate_id = artifact_dir.name.removeprefix("acquisition-")
        receipts = list(artifact_dir.rglob("acquisition-receipt.json"))
        if not receipts:
            problems.append({"candidate_id": candidate_id, "problem": "no acquisition-receipt.json found on disk"})
            continue
        receipt = load_json(receipts[0])
        for field in hash_fields:
            frozen_value = n2_source_freeze.get(field, {}).get(candidate_id)
            receipt_value = receipt.get(field)
            if frozen_value is not None and receipt_value != frozen_value:
                problems.append({"candidate_id": candidate_id, "field": field,
                                  "frozen": frozen_value, "receipt": receipt_value})
    return problems


def verify_candidate_roles(fetch_report: dict, quota_selection_report: dict) -> dict:
    """The set of acquisition-* artifacts actually fetched must be exactly
    the union of the accepted quota-selection-report's primary_case_ids and
    alternate_case_ids — no missing, duplicate, or unexpected candidate."""
    fetched_ids = sorted(f["artifact_name"].removeprefix("acquisition-")
                          for f in fetch_report["fetched"] if f["artifact_name"].startswith("acquisition-"))
    expected_ids = sorted(set(quota_selection_report["primary_case_ids"]) | set(quota_selection_report["alternate_case_ids"]))
    duplicates = sorted({cid for cid in fetched_ids if fetched_ids.count(cid) > 1})
    return {
        "expected_count": len(expected_ids),
        "fetched_count": len(set(fetched_ids)),
        "missing": sorted(set(expected_ids) - set(fetched_ids)),
        "unexpected": sorted(set(fetched_ids) - set(expected_ids)),
        "duplicates": duplicates,
        "roles_match": (set(fetched_ids) == set(expected_ids) and not duplicates),
    }


def verify_all(fetch_report_path: Path, artifact_index_path: Path, n2_source_freeze_path: Path,
                quota_selection_report_path: Path, extract_root: Path) -> dict:
    fetch_report = load_json(fetch_report_path)
    artifact_index = load_json(artifact_index_path)
    n2_source_freeze = load_json(n2_source_freeze_path)
    quota_selection_report = load_json(quota_selection_report_path)

    file_hash_problems = verify_per_artifact_file_hashes(fetch_report, artifact_index)
    receipt_field_problems = verify_receipt_field_values(extract_root, n2_source_freeze)
    role_check = verify_candidate_roles(fetch_report, quota_selection_report)

    return {
        "file_hash_problems": file_hash_problems,
        "receipt_field_problems": receipt_field_problems,
        "role_check": role_check,
        "pass": not file_hash_problems and not receipt_field_problems and role_check["roles_match"],
    }


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("--fetch-report", required=True)
    ap.add_argument("--artifact-index", required=True)
    ap.add_argument("--n2-source-freeze", required=True)
    ap.add_argument("--quota-selection-report", required=True)
    ap.add_argument("--extract-root", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    result = verify_all(Path(args.fetch_report), Path(args.artifact_index), Path(args.n2_source_freeze),
                         Path(args.quota_selection_report), Path(args.extract_root))
    Path(args.out).write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"pass": result["pass"]}, indent=2))
    return 0 if result["pass"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
