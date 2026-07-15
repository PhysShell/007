#!/usr/bin/env python3
"""Generates per-case source manifests (section 17) and license records
(section 11) for every frozen primary/alternate case, from the final
selection.run_selection() result plus the candidate registry. Manifests are
schema-validated before being written; run this only after acquisition.py
has recorded real normalized_archive_sha256/original_sha256 values into the
candidate registry entries for repository/artifact candidates that were
actually, trustedly acquired in CI (see the qodec-n2-source-freeze.yml
workflow's trusted-source-acquisition job).
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

SOURCE_FREEZE_DIR = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SOURCE_FREEZE_DIR.parent.parent / "corpus" / "tools"))
import jsonschema_mini  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
import license_review  # noqa: E402

MANIFEST_SCHEMA = json.loads((SOURCE_FREEZE_DIR / "schemas" / "source-manifest.schema.json").read_text())


def build_source_manifest(candidate: dict, role: str, fallback_priority: int | None,
                           fallback_quota_groups: list) -> dict:
    ident = candidate["source_identity"]
    return {
        "case_id": candidate["candidate_id"],
        "selection_role": role,
        "fallback_priority": fallback_priority,
        "fallback_quota_groups": fallback_quota_groups,
        "source_kind": candidate["source_kind"],
        "origin_kind": candidate["origin_kind"],
        "ecosystem": candidate["ecosystem"],
        "primary_family": candidate["primary_family"],
        "secondary_tags": candidate.get("secondary_tags", []),
        "source_identity": {
            "canonical_url": candidate["public_canonical_url"],
            "commit_sha": ident.get("commit_sha"),
            "tree_sha": ident.get("tree_sha"),
            "object_id_or_doi": ident.get("object_id_or_doi"),
            "original_sha256": ident.get("original_content_sha256"),
            "normalized_archive_sha256": ident.get("normalized_archive_sha256"),
        },
        "license_identity": {
            "spdx": candidate["license"]["spdx"],
            "license_sha256": ident.get("license_sha256"),
            "redistribution_allowed": candidate["license"]["redistribution_allowed"],
        },
        "project": candidate.get("project", {"entry_point": None}),
        "execution_expectation": {
            "command_class": candidate.get("expected_capture_command_class", ""),
            "trusted_dependency_realization": (
                "dependency-lock-present" if candidate.get("dependency_lock", {}).get("present")
                else "not-applicable-non-repository" if candidate["source_kind"] != "repository-execution"
                else "no-lockfile-restore-required"
            ),
            "network_during_untrusted_execution": "denied",
            "expected_size_bucket": candidate["expected_size_bucket"],
            "size_estimation_basis": candidate["expected_size_estimation_basis"],
            "argv": [],
        },
    }


def validate_manifest(manifest: dict) -> list[str]:
    return jsonschema_mini.validate(manifest, MANIFEST_SCHEMA)


_DEFAULT_REDISTRIBUTION_BASIS = {
    "MIT": "MIT license explicitly permits use, copying, modification, and redistribution, with attribution.",
    "Apache-2.0": "Apache License 2.0 explicitly permits redistribution and modification, with attribution and a NOTICE-preservation requirement.",
    "BSD-2-Clause": "BSD 2-Clause license explicitly permits redistribution and modification in source form, with attribution.",
    "BSD-3-Clause": "BSD 3-Clause license explicitly permits redistribution and modification, with attribution and a no-endorsement clause.",
    "CC-BY-4.0": "Creative Commons Attribution 4.0 explicitly permits redistribution and adaptation, with attribution.",
    "CC0-1.0": "CC0 is a public-domain waiver; no redistribution restriction applies.",
}


def build_license_record_for_candidate(candidate: dict) -> dict:
    license_ = candidate["license"]
    spdx = license_.get("spdx")
    evidence = list(candidate.get("evidence_references", []))
    return license_review.build_license_record(
        {**candidate, "license": {
            **license_,
            "license_source_url": candidate["public_canonical_url"],
            "redistribution_basis": license_.get("redistribution_basis")
            or _DEFAULT_REDISTRIBUTION_BASIS.get(spdx, ""),
            "attribution_requirements": "Retain upstream copyright/license notice.",
            "modification_requirements": "None beyond attribution, per the stated SPDX license.",
        }},
        evidence,
    )


def generate_all(primary_ids: list, alternate_ids: list, by_id: dict, quota_contract: dict) -> dict:
    written = {"primary": [], "alternate": [], "license_records": [], "errors": []}
    primary_dir = SOURCE_FREEZE_DIR / "source-manifests" / "primary"
    alternate_dir = SOURCE_FREEZE_DIR / "source-manifests" / "alternate"
    license_dir = SOURCE_FREEZE_DIR / "license-records"
    primary_dir.mkdir(parents=True, exist_ok=True)
    alternate_dir.mkdir(parents=True, exist_ok=True)
    license_dir.mkdir(parents=True, exist_ok=True)

    for cid in primary_ids:
        candidate = by_id[cid]
        manifest = build_source_manifest(candidate, "primary", None, [])
        errors = validate_manifest(manifest)
        if errors:
            written["errors"].append({"case_id": cid, "errors": errors})
            continue
        (primary_dir / f"{cid}.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        written["primary"].append(cid)

        record = build_license_record_for_candidate(candidate)
        rec_errors = license_review.validate_license_record(record)
        if rec_errors:
            written["errors"].append({"case_id": cid, "license_errors": rec_errors})
            continue
        (license_dir / f"{cid}.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
        written["license_records"].append(cid)

    for priority, cid in enumerate(alternate_ids, start=1):
        candidate = by_id[cid]
        quota_groups = [candidate["origin_family_group"], candidate["ecosystem_quota_group"]]
        manifest = build_source_manifest(candidate, "alternate", priority, quota_groups)
        errors = validate_manifest(manifest)
        if errors:
            written["errors"].append({"case_id": cid, "errors": errors})
            continue
        (alternate_dir / f"{cid}.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        written["alternate"].append(cid)

        record = build_license_record_for_candidate(candidate)
        rec_errors = license_review.validate_license_record(record)
        if rec_errors:
            written["errors"].append({"case_id": cid, "license_errors": rec_errors})
            continue
        (license_dir / f"{cid}.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
        written["license_records"].append(cid)

    return written


if __name__ == "__main__":
    import registry as registry_mod  # noqa: E402
    import eligibility as eligibility_mod  # noqa: E402
    import selection as selection_mod  # noqa: E402

    reg = registry_mod.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
    reports = eligibility_mod.evaluate_registry(reg)
    eligible_ids = {r["candidate_id"] for r in reports if r["eligible"]}
    eligible = [c for c in reg["candidates"] if c["candidate_id"] in eligible_ids]
    result = selection_mod.run_selection(eligible)
    by_id = {c["candidate_id"]: c for c in eligible}
    outcome = generate_all(result["primary_case_ids"], result["alternate_case_ids"], by_id,
                            selection_mod.load_quota_contract())
    print(json.dumps({k: (v if k == "errors" else len(v)) for k, v in outcome.items()}, indent=2))
