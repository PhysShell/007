#!/usr/bin/env python3
"""N2-C closure section 5: fold-or-verify acquisition identity into the
candidate registry.

Before the identity-lock commit, a candidate's real-content identity fields
(source_identity.normalized_archive_sha256 / metadata_sha256 /
source_content_sha256 / normalized_source_sha256 / license_sha256 /
tree_sha / commit_sha / source_commit_sha / selected_job_ids / ...) start
null in the committed registry — real acquisition folds them IN for the
first time.

After the identity-lock commit, those fields are already final/committed.
Any later CI run's fresh re-acquisition of the SAME immutable sources must
then VERIFY, not silently overwrite: a freshly-acquired value that differs
from an already-committed non-null value is drift and must fail the build
(section 5: "CI must reacquire or revalidate sources and fail if observed
identities differ from the committed values"). One function implements
both behaviors, keyed only on whether the committed field is currently
null — no separate "pre-lock" / "post-lock" job graph is needed.
"""
from __future__ import annotations

# Registry source_identity field name -> key in the acquisition result dict
# (the *.acquisition.json written by run_acquisition.py).
_REPO_FIELD_MAP = {
    "commit_sha": "actual_head_sha",
    "tree_sha": "git_tree_sha",
    "normalized_archive_sha256": "normalized_archive_sha256",
    "license_sha256": "license_sha256",
}
_NON_REPO_FIELD_MAP = {
    "metadata_sha256": "metadata_sha256",
    "source_content_sha256": "source_content_sha256",
    "normalized_source_sha256": "normalized_source_sha256",
}
_CI_LOG_EXTRA_FIELD_MAP = {
    "source_commit_sha": "source_commit_sha",
    "selected_job_ids": "selected_job_ids",
    "selected_job_names": "selected_job_names",
    "log_acquisition_endpoint": "log_acquisition_endpoint",
}


def _field_map_for(source_kind: str) -> dict:
    if source_kind == "repository-execution":
        return _REPO_FIELD_MAP
    if source_kind == "ci-run-artifact":
        return {**_NON_REPO_FIELD_MAP, **_CI_LOG_EXTRA_FIELD_MAP}
    return _NON_REPO_FIELD_MAP


def fold_or_verify(registry: dict, acquisition_results: list[dict]) -> dict:
    """Mutates `registry`'s candidates in place (folding currently-null
    fields with freshly-acquired values) and returns a report:
    {"updated": [case_id, ...], "drift": [{"candidate_id", "field",
    "committed", "fresh"}, ...], "unmatched_results": [case_id, ...]}.
    A non-empty "drift" list means the build must fail — the caller is
    responsible for that (this function only detects and reports)."""
    by_id = {c["candidate_id"]: c for c in registry["candidates"]}
    updated = []
    drift = []
    unmatched_results = []
    for result in acquisition_results:
        cid = result["candidate_id"]
        candidate = by_id.get(cid)
        if candidate is None:
            unmatched_results.append(cid)
            continue
        ident = candidate["source_identity"]
        field_map = _field_map_for(candidate["source_kind"])
        touched = False
        for registry_field, result_key in field_map.items():
            if result_key not in result:
                continue
            fresh_value = result[result_key]
            if fresh_value is None or fresh_value == []:
                continue
            committed_value = ident.get(registry_field)
            if committed_value is None or committed_value == []:
                ident[registry_field] = fresh_value
                touched = True
            elif committed_value != fresh_value:
                drift.append({
                    "candidate_id": cid, "field": registry_field,
                    "committed": committed_value, "fresh": fresh_value,
                })
        if touched:
            updated.append(cid)
    return {"updated": sorted(updated), "drift": drift, "unmatched_results": sorted(unmatched_results)}
