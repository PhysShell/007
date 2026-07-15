#!/usr/bin/env python3
"""Trusted-source-acquisition CLI (section 15-16). Runs ONLY inside the
`trusted-source-acquisition` CI job — the ONE job in this workflow allowed
outbound network access. Never executes repository-controlled code (no
restore/build/test/lint/package-manager/container/binary execution) — every
path here is read/download/hash/verify only.

Source-kind handling:
  - repository-execution: the workflow has already `actions/checkout`'d this
    candidate's repo at its pinned commit into a per-candidate directory
    (see the workflow's matrix step) — this script verifies HEAD, rejects
    submodules/LFS/hooks, verifies the license text, and builds a normalized
    archive (acquisition.acquire_repository_candidate).
  - ci-run-artifact / bot-output-artifact: small (single page/run-metadata),
    fetched here directly via the public GitHub REST API (no auth required
    for public repos) and hashed as-is.
  - dataset-artifact / research-corpus-artifact: identity for these is the
    immutable, versioned Zenodo/publisher DOI record plus that publisher's
    OWN published per-file checksums (already recorded in the candidate's
    `evidence_references`/discovery notes during research) — for multi-
    gigabyte datasets (Loghub is 6GB across 19 files), re-downloading and
    re-hashing the entire archive set in every CI run is neither necessary
    nor a responsible use of runner disk/network/time. This script instead
    re-fetches and hashes the Zenodo record's OWN metadata JSON (a small,
    immutable, versioned document identified by the DOI) as the acquired
    evidence artifact, and records the DOI as the content identity.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path
from urllib.request import Request, urlopen

CORPUS_TOOLS = Path(__file__).resolve().parents[3] / "corpus" / "tools"
sys.path.insert(0, str(CORPUS_TOOLS))
from hashing import sha256_bytes  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
import acquisition  # noqa: E402

USER_AGENT = "qodec-n2c-source-freeze (trusted-acquisition; static-inspection-only)"


def _fetch(url: str, timeout: int = 60) -> bytes:
    req = Request(url, headers={"User-Agent": USER_AGENT, "Accept": "application/vnd.github+json"})
    with urlopen(req, timeout=timeout) as resp:  # noqa: S310 - public metadata only, no execution
        return resp.read()


def acquire_ci_run(candidate: dict) -> dict:
    ident = candidate["source_identity"]
    repo_url = ident["repository_url"]
    run_id = ident["run_id"]
    owner_repo = repo_url.removeprefix("https://github.com/")
    run_meta = _fetch(f"https://api.github.com/repos/{owner_repo}/actions/runs/{run_id}")
    jobs_meta = _fetch(f"https://api.github.com/repos/{owner_repo}/actions/runs/{run_id}/jobs")
    combined = run_meta + b"\n" + jobs_meta
    return {
        "candidate_id": candidate["candidate_id"],
        "original_sha256": sha256_bytes(combined),
        "normalized_archive_sha256": sha256_bytes(combined),  # metadata IS the artifact for this source kind
        "size_bytes": len(combined),
        "acquisition_method": "github-actions-public-api-run-and-jobs-metadata",
    }


def acquire_bot_output(candidate: dict) -> dict:
    ident = candidate["source_identity"]
    obj_id = ident["object_id_or_doi"]
    if obj_id.startswith("extid:"):
        extid = obj_id.removeprefix("extid:")
        url = f"https://syzkaller.appspot.com/bug?extid={extid}"
    else:
        owner_repo, _, pr_num = obj_id.rpartition("#")
        url = f"https://api.github.com/repos/{owner_repo}/pulls/{pr_num}"
    data = _fetch(url)
    return {
        "candidate_id": candidate["candidate_id"],
        "original_sha256": sha256_bytes(data),
        "normalized_archive_sha256": sha256_bytes(data),
        "size_bytes": len(data),
        "acquisition_method": "direct-public-page-or-api-fetch",
    }


def acquire_dataset_or_research_corpus(candidate: dict) -> dict:
    ident = candidate["source_identity"]
    doi = ident["object_id_or_doi"]
    record_id = doi.rsplit(".", 1)[-1]
    data = _fetch(f"https://zenodo.org/api/records/{record_id}")
    return {
        "candidate_id": candidate["candidate_id"],
        "original_sha256": sha256_bytes(data),
        "normalized_archive_sha256": sha256_bytes(data),  # Zenodo's own versioned record metadata is the acquired identity artifact
        "size_bytes": len(data),
        "acquisition_method": "zenodo-api-record-metadata (see candidate discovery notes for the publisher's own per-file MD5 checksums of the underlying multi-GB dataset — not re-downloaded here)",
    }


def acquire_repository(candidate: dict, checkout_dir: Path, out_dir: Path,
                        workflow_run_id: str, runner_identity: str) -> dict:
    result = acquisition.acquire_repository_candidate(
        checkout_dir, candidate, out_dir, workflow_run_id, runner_identity
    )
    return {
        "candidate_id": candidate["candidate_id"],
        "actual_head_sha": result["actual_head_sha"],
        "git_tree_sha": result["git_tree_sha"],
        "normalized_archive_sha256": result["normalized_archive_sha256"],
        "license_sha256": result["license_sha256"],
        "tracked_file_count": result["tracked_file_count"],
        "acquisition_method": "actions-checkout-pinned-commit-plus-normalized-tar",
    }


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("--registry", required=True)
    ap.add_argument("--candidate-id", required=True)
    ap.add_argument("--checkout-dir", default=None, help="required for repository-execution candidates")
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--workflow-run-id", default="")
    ap.add_argument("--runner-identity", default="")
    args = ap.parse_args()

    registry = json.loads(Path(args.registry).read_text())
    candidate = next(c for c in registry["candidates"] if c["candidate_id"] == args.candidate_id)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    try:
        if candidate["source_kind"] == "repository-execution":
            if not args.checkout_dir:
                raise SystemExit("--checkout-dir is required for repository-execution candidates")
            result = acquire_repository(candidate, Path(args.checkout_dir), out_dir,
                                         args.workflow_run_id, args.runner_identity)
        elif candidate["source_kind"] == "ci-run-artifact":
            result = acquire_ci_run(candidate)
        elif candidate["source_kind"] == "bot-output-artifact":
            result = acquire_bot_output(candidate)
        elif candidate["source_kind"] in ("dataset-artifact", "research-corpus-artifact"):
            result = acquire_dataset_or_research_corpus(candidate)
        else:
            raise SystemExit(f"unknown source_kind {candidate['source_kind']!r}")
    except acquisition.RejectedCandidate as e:
        print(f"run_acquisition: REJECTED {args.candidate_id}: {e}", file=sys.stderr)
        return 1

    (out_dir / f"{args.candidate_id}.acquisition.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n"
    )
    print(f"run_acquisition: ACCEPTED {args.candidate_id}: {result}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
