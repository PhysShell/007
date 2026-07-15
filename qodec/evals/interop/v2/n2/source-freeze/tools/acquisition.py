#!/usr/bin/env python3
"""N2-C trusted acquisition (section 15) — static inspection only, never
executes repository-controlled code.

Generalizes N2-A's frozen single-repo `verify_source.py`
(qodec/evals/interop/v2/n2/canary/tools/verify_source.py, NOT modified —
that script is hardcoded to exactly one dotnet .csproj expectation) into a
multi-candidate, multi-license, multi-ecosystem inspector. Runs against an
already-checked-out repository (via actions/checkout at a pinned commit —
this module never clones/fetches anything itself) for repository-miner
candidates, or validates an already-downloaded artifact for the four
non-repository origin kinds.
"""
from __future__ import annotations

import io
import json
import os
import subprocess
import sys
import tarfile
from pathlib import Path

CORPUS_TOOLS = Path(__file__).resolve().parents[3] / "corpus" / "tools"
sys.path.insert(0, str(CORPUS_TOOLS))
from hashing import sha256_bytes, sha256_file  # noqa: E402

LFS_POINTER_PREFIX = b"version https://git-lfs.github.com/spec"

_LICENSE_TEXT_MARKERS = {
    "MIT": ("Permission is hereby granted, free of charge",),
    "Apache-2.0": ("Apache License", "Version 2.0"),
    "BSD-2-Clause": ("Redistributions of source code must retain",),
    "BSD-3-Clause": ("Redistributions of source code must retain", "endorse or promote"),
    "CC-BY-4.0": ("Creative Commons Attribution 4.0",),
    "CC0-1.0": ("CC0",),
}


class RejectedCandidate(Exception):
    """Raised for any failed acceptance check; message is the reject reason."""


def run_git(repo: Path, *args: str) -> str:
    r = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, check=False)
    if r.returncode != 0:
        raise RejectedCandidate(f"git {' '.join(args)} failed: {r.stderr.strip()}")
    return r.stdout.strip()


def verify_commit_identity(repo: Path, approved_sha: str) -> dict:
    head = run_git(repo, "rev-parse", "HEAD")
    if head != approved_sha:
        raise RejectedCandidate(
            f"checked-out HEAD {head} != approved commit_sha {approved_sha} "
            "(refusing to substitute a different revision)"
        )
    tree_sha = run_git(repo, "rev-parse", "HEAD^{tree}")
    return {"head_sha": head, "git_tree_sha": tree_sha}


def verify_no_submodules(repo: Path) -> None:
    if (repo / ".gitmodules").exists():
        raise RejectedCandidate(".gitmodules present — submodules are not permitted")
    status = run_git(repo, "submodule", "status")
    if status.strip():
        raise RejectedCandidate(f"git submodule status non-empty: {status!r}")


def tracked_files(repo: Path) -> list[str]:
    out = run_git(repo, "ls-files", "-z")
    return [p for p in out.split("\0") if p]


def verify_no_git_lfs(repo: Path, files: list[str]) -> None:
    attrs = repo / ".gitattributes"
    if attrs.exists() and "filter=lfs" in attrs.read_text(errors="replace"):
        raise RejectedCandidate(".gitattributes references a Git LFS filter")
    for rel in files:
        p = repo / rel
        try:
            with open(p, "rb") as f:
                head = f.read(len(LFS_POINTER_PREFIX))
        except OSError:
            continue
        if head == LFS_POINTER_PREFIX:
            raise RejectedCandidate(f"Git LFS pointer file detected: {rel}")


def verify_no_executable_git_hooks(repo: Path) -> None:
    hooks_dir = repo / ".git" / "hooks"
    if not hooks_dir.is_dir():
        return
    for entry in hooks_dir.iterdir():
        if entry.name.endswith(".sample"):
            continue
        if entry.is_file() and os.access(entry, os.X_OK):
            raise RejectedCandidate(f"executable Git hook present: {entry.name}")


def verify_license_text(license_path: Path, spdx: str) -> str:
    if not license_path.is_file():
        raise RejectedCandidate(f"license file {license_path} not found")
    text = license_path.read_text(errors="replace")
    markers = _LICENSE_TEXT_MARKERS.get(spdx)
    if markers is None:
        raise RejectedCandidate(f"no license-text heuristic configured for spdx {spdx!r}")
    if not all(marker in text for marker in markers):
        raise RejectedCandidate(f"{license_path} does not look like {spdx} license text")
    return sha256_file(license_path)


def build_normalized_archive(repo: Path, files: list[str], out_path: Path) -> str:
    """Deterministic tar of tracked files only: sorted path order, fixed
    uid/gid/uname/gname/mtime. No .git directory, no checkout credentials."""
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.USTAR_FORMAT) as tar:
        for rel in sorted(files):
            full = repo / rel
            if not full.is_file() and not full.is_symlink():
                continue
            info = tar.gettarinfo(str(full), arcname=rel)
            info.uid = 0
            info.gid = 0
            info.uname = ""
            info.gname = ""
            info.mtime = 0
            if info.isreg():
                with open(full, "rb") as f:
                    tar.addfile(info, f)
            else:
                tar.addfile(info)
    data = buf.getvalue()
    out_path.write_bytes(data)
    return sha256_bytes(data)


def acquire_repository_candidate(repo_dir: Path, candidate: dict, out_dir: Path,
                                  workflow_run_id: str = "", runner_identity: str = "") -> dict:
    """Full repository-miner acquisition pipeline for one candidate. `repo_dir`
    must already be an actions/checkout'd working tree at the candidate's
    pinned commit. Writes source.tar + source-file-manifest.json under
    out_dir/{candidate_id}/. Returns the identity record to fold into that
    candidate's source manifest."""
    ident = candidate["source_identity"]
    case_out = out_dir / candidate["candidate_id"]
    case_out.mkdir(parents=True, exist_ok=True)

    identity = verify_commit_identity(repo_dir, ident["commit_sha"])
    verify_no_submodules(repo_dir)
    files = tracked_files(repo_dir)
    verify_no_git_lfs(repo_dir, files)
    verify_no_executable_git_hooks(repo_dir)

    license_path = repo_dir / (candidate["license"].get("license_file") or "LICENSE")
    license_sha256 = verify_license_text(license_path, candidate["license"]["spdx"])

    archive_sha256 = build_normalized_archive(repo_dir, files, case_out / "source.tar")
    file_manifest = sorted(
        ({"path": rel, "sha256": sha256_file(repo_dir / rel)} for rel in files if (repo_dir / rel).is_file()),
        key=lambda e: e["path"],
    )
    (case_out / "source-file-manifest.json").write_text(
        json.dumps(file_manifest, indent=2, sort_keys=True) + "\n"
    )

    return {
        "candidate_id": candidate["candidate_id"],
        "actual_head_sha": identity["head_sha"],
        "git_tree_sha": identity["git_tree_sha"],
        "normalized_archive_sha256": archive_sha256,
        "license_sha256": license_sha256,
        "tracked_file_count": len(files),
        "acquisition_runner_identity": runner_identity,
        "acquisition_workflow_run_id": workflow_run_id,
    }


def verify_downloaded_artifact(artifact_path: Path, expected_sha256: str | None = None) -> dict:
    """For the four non-repository origin kinds: verify integrity of an
    already-downloaded, already-extracted-nowhere artifact file. Does not
    fetch anything — the trusted acquisition job's own HTTP client did that;
    this only hashes and checks. Extraction/archive-structure inspection for
    such artifacts is handled by archive_security.py before this is called
    if the artifact is itself an archive."""
    if not artifact_path.is_file():
        raise RejectedCandidate(f"artifact {artifact_path} not found")
    original_sha256 = sha256_file(artifact_path)
    if expected_sha256 and original_sha256 != expected_sha256:
        raise RejectedCandidate(
            f"artifact {artifact_path} sha256 {original_sha256} != expected {expected_sha256}"
        )
    return {"original_sha256": original_sha256, "size_bytes": artifact_path.stat().st_size}
