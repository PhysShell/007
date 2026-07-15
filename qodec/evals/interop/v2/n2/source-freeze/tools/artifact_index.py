#!/usr/bin/env python3
"""N2-C closure section 8: n2c-artifact-index construction + validation.

Per artifact: artifact_name, artifact_id, archive_digest, contained_files
(with per-file SHA256), source_workflow_run, logical_head_sha, execution_sha.

logical_head_sha is the PR's actual accepted branch head
(`pull_request.head.sha`); execution_sha is whatever commit the runner
actually checked out and ran (`github.sha` — for a `pull_request`-triggered
run this is GitHub's synthetic merge commit, NOT the branch head). Section 8
requires these to be recorded as two distinct fields — a run must never
label the synthetic merge commit as if it were the accepted branch head.

Real artifact_id/archive_digest values come from the GitHub Actions
Artifacts REST API (`GET /repos/{owner}/{repo}/actions/runs/{run_id}/
artifacts`), queried by the workflow with `actions: read` permission — never
fabricated or left as a placeholder.
"""
from __future__ import annotations

import hashlib
from pathlib import Path


def build_index(all_artifacts_root: Path, api_artifacts: list[dict], source_workflow_run: str,
                 logical_head_sha: str, execution_sha: str, self_artifact_name: str) -> list[dict]:
    """`api_artifacts` is the (possibly paginated, already-flattened) list of
    artifact objects as returned by the real GitHub Artifacts API for this
    run — each expected to carry at least `name`, `id`, and `digest`.
    `self_artifact_name` (this index's own artifact name) is excluded by
    construction: it cannot describe its own digest before it exists."""
    by_name = {a["name"]: a for a in api_artifacts}
    index = []
    for artifact_dir in sorted(all_artifacts_root.iterdir()):
        if not artifact_dir.is_dir():
            continue
        name = artifact_dir.name
        if name == self_artifact_name:
            continue
        files = []
        for f in sorted(artifact_dir.rglob("*")):
            if f.is_file():
                files.append({
                    "path": str(f.relative_to(artifact_dir)),
                    "sha256": hashlib.sha256(f.read_bytes()).hexdigest(),
                })
        api_entry = by_name.get(name, {})
        index.append({
            "artifact_name": name,
            "artifact_id": api_entry.get("id"),
            "archive_digest": api_entry.get("digest"),
            "contained_files": files,
            "source_workflow_run": source_workflow_run,
            "logical_head_sha": logical_head_sha,
            "execution_sha": execution_sha,
            "file_count": len(files),
        })
    return index


_REQUIRED_FIELDS = (
    "artifact_name", "artifact_id", "archive_digest", "contained_files",
    "source_workflow_run", "logical_head_sha", "execution_sha",
)


def validate_artifact_index(index: list[dict]) -> list[str]:
    """Section 8 negative tests: every entry must carry a non-null
    artifact_id, archive_digest, and logical_head_sha (kept as a field
    distinct from execution_sha — both must be present as separate keys,
    even if their values happen to coincide for a non-PR trigger), and
    every contained file must carry a sha256."""
    errors = []
    for entry in index:
        name = entry.get("artifact_name", "<unnamed>")
        for field in _REQUIRED_FIELDS:
            if field not in entry:
                errors.append(f"{name}: missing required field {field!r}")
            elif field in ("artifact_id", "archive_digest", "logical_head_sha") and entry[field] is None:
                errors.append(f"{name}: {field!r} must not be null")
        if "logical_head_sha" not in entry or "execution_sha" not in entry:
            errors.append(f"{name}: must carry both logical_head_sha and execution_sha as distinct fields")
        for f in entry.get("contained_files", []):
            if not f.get("sha256"):
                errors.append(f"{name}: contained file {f.get('path')!r} is missing a sha256")
    return errors
