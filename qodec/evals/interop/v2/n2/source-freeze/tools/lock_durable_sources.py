#!/usr/bin/env python3
"""N2-C closure section 9 (durability rule): a 90-day GitHub Actions
artifact is not a permanent source freeze. For sources that may disappear
upstream — especially GitHub Actions job logs, and secondarily small
bot-output responses that aren't bound to a permanent DOI — this copies the
REAL acquired content bytes (already downloaded by a real
trusted-source-acquisition CI run) into a durable, content-addressed path
inside this repository, and records that durable identity + its SHA256 in
the candidate registry.

DOI-bound dataset/research-corpus files are exempt (section 9 explicitly
treats "exact file identity plus publisher checksum and independently
verified SHA256" as sufficient for those — Zenodo's own versioned record is
already a durable, third-party-hosted, checksummed object).

This script is run manually by whoever is building the identity-lock commit
(see section 5's two-phase process) against a directory containing the
downloaded, merged acquisition-<candidate_id> artifacts from a real CI run
— it does not run inside the CI workflow itself, since committing files
back into a PR branch from within a job would require elevated write
permissions this scope deliberately avoids (see the workflow's read-only
trust-boundary comment).
"""
from __future__ import annotations

import json
import shutil
import sys
from pathlib import Path

CORPUS_TOOLS = Path(__file__).resolve().parents[3] / "corpus" / "tools"
sys.path.insert(0, str(CORPUS_TOOLS))
from hashing import sha256_file  # noqa: E402

SOURCE_FREEZE_DIR = Path(__file__).resolve().parent.parent
FROZEN_SOURCES_DIR = SOURCE_FREEZE_DIR / "frozen-sources"

# Origin kinds whose acquired content is durability-exempt per section 9
# (a DOI-bound publisher record is already durable + independently
# checksummed; nothing needs to be re-hosted inside this repository).
_DURABILITY_EXEMPT_ORIGIN_KINDS = {"public-runtime-dataset", "reproducible-research-corpus"}


def stage_durable_sources(acquisition_root: Path, registry: dict, dest_root: Path | None = None) -> dict:
    """Mutates `registry`'s candidates in place, setting
    source_identity.durable_object_identity / durable_sha256 for every
    non-exempt, non-repository candidate whose real acquired content is
    present under acquisition_root/<candidate_id>/source/. Returns a report:
    {"staged": [case_id, ...], "skipped_exempt": [...], "skipped_no_content": [...]}.
    `dest_root` defaults to this repository's real frozen-sources directory
    (FROZEN_SOURCES_DIR) — tests MUST override it with a temp directory so
    running the test suite never writes into the actual working tree."""
    dest_root = dest_root if dest_root is not None else FROZEN_SOURCES_DIR
    staged, skipped_exempt, skipped_no_content = [], [], []
    for candidate in registry["candidates"]:
        cid = candidate["candidate_id"]
        if candidate["source_kind"] == "repository-execution":
            continue
        if candidate["origin_kind"] in _DURABILITY_EXEMPT_ORIGIN_KINDS:
            skipped_exempt.append(cid)
            continue
        source_dir = acquisition_root / cid / "source"
        if not source_dir.is_dir():
            skipped_no_content.append(cid)
            continue
        files = sorted(f for f in source_dir.rglob("*") if f.is_file())
        if not files:
            skipped_no_content.append(cid)
            continue
        dest_dir = dest_root / cid
        dest_dir.mkdir(parents=True, exist_ok=True)
        for f in files:
            rel = f.relative_to(source_dir)
            dest = dest_dir / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(f, dest)
        # Durable identity is a stable, deterministic hash of the staged
        # file set (path -> sha256), not just the first file, so it stays
        # correct even if a future candidate ever stages more than one file.
        file_hashes = sorted(
            (str(f.relative_to(source_dir)), sha256_file(f)) for f in files
        )
        import hashlib
        durable_sha256 = hashlib.sha256(
            json.dumps(file_hashes, sort_keys=True).encode("utf-8")
        ).hexdigest()
        # Repo-relative path built by direct string join (not relative_to()
        # walking a fixed number of .parent hops) so it can't silently drift
        # if this module ever moves within the tree.
        rel_repo_path = f"qodec/evals/interop/v2/n2/source-freeze/frozen-sources/{cid}"
        candidate["source_identity"]["durable_object_identity"] = rel_repo_path
        candidate["source_identity"]["durable_sha256"] = durable_sha256
        staged.append(cid)
    return {"staged": staged, "skipped_exempt": skipped_exempt, "skipped_no_content": skipped_no_content}


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("--acquisition-root", required=True,
                     help="directory containing downloaded/merged acquisition-<candidate_id> artifacts")
    ap.add_argument("--registry", default=str(SOURCE_FREEZE_DIR / "candidate-registry.json"))
    args = ap.parse_args()

    reg_path = Path(args.registry)
    registry = json.loads(reg_path.read_text())
    report = stage_durable_sources(Path(args.acquisition_root), registry)
    reg_path.write_text(json.dumps(registry, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
