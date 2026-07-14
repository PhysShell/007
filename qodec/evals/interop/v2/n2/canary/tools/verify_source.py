#!/usr/bin/env python3
"""N2-A trusted-acquisition validator (StaticSourceManifest + SourceResolver +
LicenseVerifier, thin-slice).

Runs ONLY in the trusted-source-acquisition job, against an already-checked-out
external repository (via `actions/checkout` at a pinned commit — this script
never clones anything itself). It inspects and hashes; it never executes
repository-controlled build/test/restore logic, never runs `git` hooks, and
never runs any command from the checked-out tree.

On success, writes into --out-dir:
    source-manifest.json     (manifest + resolved identity, echoed/augmented)
    license-record.json
    source-file-manifest.json
    source.tar               (normalized: git-tracked files only, no .git,
                               stable order/uid/gid/mtime)

Exits non-zero (and writes nothing but a diagnostic to stderr) if any
acceptance check fails — see `REQUIRED_CHECKS` below.
"""
from __future__ import annotations

import argparse
import io
import json
import os
import re
import subprocess
import sys
import tarfile
from pathlib import Path

CORPUS_TOOLS = Path(__file__).resolve().parents[3] / "corpus" / "tools"
sys.path.insert(0, str(CORPUS_TOOLS))
from hashing import sha256_bytes, sha256_file, tree_sha256  # noqa: E402

# Files whose mere presence outside the reviewed csproj expectation rejects
# the candidate (custom MSBuild surface / restore-time configuration this
# thin canary explicitly does not support).
DISALLOWED_MSBUILD_FILES = [
    "Directory.Build.props",
    "Directory.Build.targets",
    "Directory.Packages.props",
    "NuGet.config",
    "nuget.config",
]
DISALLOWED_MSBUILD_GLOBS = ["*.targets", "*.props"]

LFS_POINTER_PREFIX = b"version https://git-lfs.github.com/spec"


class RejectedCandidate(Exception):
    """Raised for any failed acceptance check; message is the reject reason."""


def run_git(repo: Path, *args: str) -> str:
    r = subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, text=True, check=False
    )
    if r.returncode != 0:
        raise RejectedCandidate(f"git {' '.join(args)} failed: {r.stderr.strip()}")
    return r.stdout.strip()


def verify_commit_identity(repo: Path, approved_sha: str) -> dict:
    head = run_git(repo, "rev-parse", "HEAD")
    if head != approved_sha:
        raise RejectedCandidate(
            f"checked-out HEAD {head} != approved_commit_sha {approved_sha} "
            "(refusing to substitute a different revision)"
        )
    tree_sha = run_git(repo, "rev-parse", "HEAD^{tree}")
    return {"head_sha": head, "git_tree_sha": tree_sha}


def verify_no_submodules(repo: Path) -> None:
    if (repo / ".gitmodules").exists():
        raise RejectedCandidate(".gitmodules present — submodules are not permitted for N2-A")
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


def verify_no_disallowed_msbuild_surface(repo: Path, files: list[str]) -> None:
    lower_disallowed = {name.lower() for name in DISALLOWED_MSBUILD_FILES}
    for rel in files:
        base = os.path.basename(rel)
        if base.lower() in lower_disallowed:
            raise RejectedCandidate(f"disallowed MSBuild/NuGet configuration file present: {rel}")
        if base.lower().endswith((".targets", ".props")):
            raise RejectedCandidate(f"disallowed MSBuild import file present: {rel}")
    if (repo / "global.json").exists():
        raise RejectedCandidate("global.json present — not reviewed for N2-A (pins a specific SDK roll-forward policy)")


def verify_license(repo: Path, manifest: dict) -> dict:
    license_name = manifest["license"]["file"]
    license_path = repo / license_name
    if not license_path.is_file():
        raise RejectedCandidate(f"license file {license_name!r} not found")
    text = license_path.read_text(errors="replace")
    if manifest["license"]["spdx"] == "MIT":
        # Minimal, explicit heuristic — not a general SPDX classifier: MIT's
        # canonical text contains this permission grant sentence verbatim.
        if "Permission is hereby granted, free of charge" not in text or "MIT" not in text.upper():
            raise RejectedCandidate(f"{license_name} does not look like the MIT license text")
    else:
        raise RejectedCandidate(f"unsupported license spdx {manifest['license']['spdx']!r} for N2-A")
    return {
        "spdx": manifest["license"]["spdx"],
        "file": license_name,
        "sha256": sha256_file(license_path),
    }


def verify_project(repo: Path, manifest: dict) -> dict:
    proj = manifest["project"]
    proj_path = repo / proj["path"]
    if not proj_path.is_file():
        raise RejectedCandidate(f"project file not found at {proj['path']}")
    text = proj_path.read_text(errors="replace")

    tfm_match = re.search(r"<TargetFramework>([^<]+)</TargetFramework>", text)
    tfm = tfm_match.group(1).strip() if tfm_match else None
    if tfm != proj["expected_target_framework"]:
        raise RejectedCandidate(f"TargetFramework {tfm!r} != expected {proj['expected_target_framework']!r}")

    pkg_refs = len(re.findall(r"<PackageReference\b", text))
    if pkg_refs != proj["expected_package_reference_count"]:
        raise RejectedCandidate(f"found {pkg_refs} PackageReference entries, expected {proj['expected_package_reference_count']}")

    proj_refs = len(re.findall(r"<ProjectReference\b", text))
    if proj_refs != proj["expected_project_reference_count"]:
        raise RejectedCandidate(f"found {proj_refs} ProjectReference entries, expected {proj['expected_project_reference_count']}")

    # An explicit <Import> element is a custom MSBuild import; the Sdk="..."
    # attribute on <Project> is the implicit SDK import and is expected/fine.
    if re.search(r"<Import\b", text):
        raise RejectedCandidate("project file contains an explicit <Import> element (custom MSBuild import)")

    return {
        "path": proj["path"],
        "sha256": sha256_bytes(text.encode("utf-8")),
        "target_framework": tfm,
        "package_reference_count": pkg_refs,
        "project_reference_count": proj_refs,
    }


def build_normalized_archive(repo: Path, files: list[str], out_path: Path) -> str:
    """Deterministic tar of tracked files only: sorted path order, fixed
    uid/gid/uname/gname/mtime. No .git directory, no checkout credentials
    (git-tracked content only — a credential helper or token is never a
    tracked file)."""
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


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--source-dir", required=True, help="checked-out external-source path")
    ap.add_argument("--manifest", required=True, help="source-manifest.json (the reviewed static manifest)")
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--workflow-run-id", default=os.environ.get("GITHUB_RUN_ID", ""))
    ap.add_argument("--runner-identity", default=os.environ.get("RUNNER_NAME", ""))
    args = ap.parse_args()

    repo = Path(args.source_dir)
    manifest = json.loads(Path(args.manifest).read_text())
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    try:
        identity = verify_commit_identity(repo, manifest["repository"]["approved_commit_sha"])
        verify_no_submodules(repo)
        files = tracked_files(repo)
        verify_no_git_lfs(repo, files)
        verify_no_executable_git_hooks(repo)
        verify_no_disallowed_msbuild_surface(repo, files)
        license_record = verify_license(repo, manifest)
        project_record = verify_project(repo, manifest)
        archive_sha256 = build_normalized_archive(repo, files, out_dir / "source.tar")
        file_manifest = sorted(
            (
                {"path": rel, "sha256": sha256_file(repo / rel)}
                for rel in files
                if (repo / rel).is_file()
            ),
            key=lambda e: e["path"],
        )
        tree_hash = tree_sha256(repo)  # informational cross-check against git's own tree sha
    except RejectedCandidate as e:
        print(f"verify_source: REJECTED: {e}", file=sys.stderr)
        return 1

    resolved = {
        **manifest,
        "resolved": {
            "actual_head_sha": identity["head_sha"],
            "git_tree_sha": identity["git_tree_sha"],
            "corpus_style_tree_sha256": tree_hash,
            "archive_sha256": archive_sha256,
            "tracked_file_count": len(files),
            "acquisition_runner_identity": args.runner_identity,
            "acquisition_workflow_run_id": args.workflow_run_id,
        },
        "project_verification": project_record,
    }
    (out_dir / "source-manifest.json").write_text(json.dumps(resolved, indent=2, sort_keys=True) + "\n")
    (out_dir / "license-record.json").write_text(json.dumps(license_record, indent=2, sort_keys=True) + "\n")
    (out_dir / "source-file-manifest.json").write_text(json.dumps(file_manifest, indent=2, sort_keys=True) + "\n")

    print(f"verify_source: ACCEPTED head={identity['head_sha']} archive_sha256={archive_sha256}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
