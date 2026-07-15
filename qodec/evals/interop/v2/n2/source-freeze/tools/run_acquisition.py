#!/usr/bin/env python3
"""Trusted-source-acquisition CLI (section 15-16, N2-C closure sections 1-4).
Runs ONLY inside the `trusted-source-acquisition` CI job — the ONE job in
this workflow allowed outbound network access. Never executes
repository-controlled code (no restore/build/test/lint/package-manager/
container/binary execution) — every path here is read/download/hash/verify
only.

N2-C closure correction: acquisition must produce REAL SOURCE CONTENT for
every candidate, not a hash of metadata masquerading as content. Every
non-repository source_kind below downloads and retains actual bytes under
`<candidate_id>/source/`, plus a `normalized-source.tar` (deterministic tar
of that directory, mirroring acquisition.build_normalized_archive's
sorted-order/fixed-uid-gid-mtime scheme) and an `acquisition-receipt.json`.
Three distinct hash concepts are always kept separate:
  - metadata_sha256: hash of a pure metadata/provenance document (run+jobs
    API responses, a Zenodo record's own JSON, ...) — NEVER content identity.
  - source_content_sha256: hash of the actual acquired content bytes as
    downloaded (the real log text / dataset file / bot response body).
  - normalized_source_sha256: hash of the deterministic archive wrapping
    that real content.

Source-kind handling:
  - repository-execution: unchanged from the original N2-C acquisition
    (acquisition.acquire_repository_candidate) — already real content (a
    normalized tar of the actual tracked files), never metadata.
  - ci-run-artifact: fetches the run + jobs metadata (as before, now kept
    strictly as metadata_sha256 evidence), applies a deterministic,
    structure-only job-selection rule (see _select_ci_job — never log
    content, QODEC, RTK, or token counts), then fetches and retains the
    ACTUAL log bytes for that one selected job via GitHub's public
    jobs/{id}/logs endpoint. If a candidate's registry entry already
    records a locked-in selected_job_ids[0] (from a prior identity-lock
    commit), that exact job is re-verified rather than re-selected — CI
    must reacquire/revalidate and fail on drift, not silently re-decide.
  - bot-output-artifact: fetches the actual page/API response bytes (as
    before) and now RETAINS them under source/, not just their hash.
  - dataset-artifact / research-corpus-artifact: fetches the publisher's
    own record metadata (Zenodo API JSON) for metadata_sha256, then
    downloads the ONE exact archive member the candidate registry already
    pins (`source_identity.selected_exact_file` /
    `exact_download_url`, chosen deterministically during discovery by
    publisher-declared file size alone — never re-downloading an entire
    multi-GB dataset), verifies it against the publisher's own reported
    checksum, and retains the real file bytes.
"""
from __future__ import annotations

import hashlib
import json
import sys
from datetime import datetime
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen

CORPUS_TOOLS = Path(__file__).resolve().parents[3] / "corpus" / "tools"
sys.path.insert(0, str(CORPUS_TOOLS))
from hashing import sha256_bytes  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parent))
import acquisition  # noqa: E402

USER_AGENT = "qodec-n2c-source-freeze (trusted-acquisition; static-inspection-only)"


def _fetch(url: str, timeout: int = 60) -> bytes:
    # GitHub's API needs its own versioned media type to return JSON in the
    # expected shape; other public APIs used here (Zenodo, syzkaller) return
    # 406 Not Acceptable if sent that GitHub-specific Accept header — a real
    # failure discovered in CI (workflow run 29387543211) and fixed here.
    accept = "application/vnd.github+json" if "api.github.com" in url else "application/json"
    req = Request(url, headers={"User-Agent": USER_AGENT, "Accept": accept})
    with urlopen(req, timeout=timeout) as resp:  # noqa: S310 - public metadata/content fetch only, no execution
        return resp.read()


def _fetch_raw(url: str, timeout: int = 120) -> bytes:
    """Like _fetch but for endpoints that return plain-text/binary content
    (job logs, dataset/response bodies) rather than JSON — no Accept header
    override needed since these aren't api.github.com JSON endpoints, except
    the job-logs endpoint which IS api.github.com but returns a redirect to
    a plain-text blob; urlopen follows the redirect automatically."""
    req = Request(url, headers={"User-Agent": USER_AGENT})
    with urlopen(req, timeout=timeout) as resp:  # noqa: S310 - public content fetch only, no execution
        return resp.read()


def _job_duration_seconds(job: dict) -> float:
    start, end = job.get("started_at"), job.get("completed_at")
    if not start or not end:
        return -1.0
    fmt = "%Y-%m-%dT%H:%M:%SZ"
    try:
        return (datetime.strptime(end, fmt) - datetime.strptime(start, fmt)).total_seconds()
    except ValueError:
        return -1.0


def select_ci_job(jobs: list[dict]) -> dict:
    """Deterministic, structure-only job selection (N2-C closure section 2):
    the job with the longest wall-clock duration, tie-broken by the lowest
    numeric job id. Computed purely from job start/end timestamps and id —
    never from log content, QODEC, RTK, or token counts — so the same rule
    applied to the same immutable run always picks the same job."""
    if not jobs:
        raise acquisition.RejectedCandidate("no jobs found for this run")
    return min(jobs, key=lambda j: (-_job_duration_seconds(j), j["id"]))


def acquire_ci_run(candidate: dict, out_dir: Path) -> dict:
    ident = candidate["source_identity"]
    repo_url = ident["repository_url"]
    run_id = ident["run_id"]
    owner_repo = repo_url.removeprefix("https://github.com/")

    run_meta_bytes = _fetch(f"https://api.github.com/repos/{owner_repo}/actions/runs/{run_id}")
    jobs_meta_bytes = _fetch(f"https://api.github.com/repos/{owner_repo}/actions/runs/{run_id}/jobs?per_page=100")
    run_meta = json.loads(run_meta_bytes)
    jobs_meta = json.loads(jobs_meta_bytes)
    jobs = jobs_meta.get("jobs", [])

    locked_in = ident.get("selected_job_ids") or []
    if locked_in:
        # Identity-lock commit already froze the exact job — re-verify it
        # still exists in the (immutable, completed) run rather than
        # re-running the selection rule, per section 5's "CI must
        # reacquire or revalidate ... and fail if observed identities
        # differ from the committed values."
        wanted_id = int(locked_in[0])
        matches = [j for j in jobs if j["id"] == wanted_id]
        if not matches:
            raise acquisition.RejectedCandidate(
                f"locked-in job id {wanted_id} no longer present in run {run_id}'s job list "
                "(upstream history changed) — candidate must be re-evaluated, not silently re-selected"
            )
        selected = matches[0]
    else:
        selected = select_ci_job(jobs)

    job_id = str(selected["id"])
    job_name = selected.get("name", "")
    log_url = f"https://api.github.com/repos/{owner_repo}/actions/jobs/{job_id}/logs"
    try:
        log_bytes = _fetch_raw(log_url)
    except (HTTPError, URLError) as e:
        raise acquisition.RejectedCandidate(
            f"cannot fetch real job-log bytes for job {job_id} ({job_name!r}) at {log_url}: {e} "
            "— per section 2, an upstream log that cannot be technically acquired makes this "
            "candidate ineligible; it must not be silently replaced by metadata"
        ) from e

    case_out = out_dir / candidate["candidate_id"]
    source_dir = case_out / "source"
    source_dir.mkdir(parents=True, exist_ok=True)
    log_filename = f"job-{job_id}.log"
    (source_dir / log_filename).write_bytes(log_bytes)

    metadata_sha256 = sha256_bytes(run_meta_bytes + b"\n" + jobs_meta_bytes)
    source_content_sha256 = sha256_bytes(log_bytes)
    normalized_source_sha256 = acquisition.build_normalized_archive(
        case_out, [f"source/{log_filename}"], case_out / "normalized-source.tar"
    )
    file_manifest = [{"path": f"source/{log_filename}", "sha256": source_content_sha256}]
    (case_out / "source-file-manifest.json").write_text(
        json.dumps(file_manifest, indent=2, sort_keys=True) + "\n"
    )

    result = {
        "candidate_id": candidate["candidate_id"],
        "metadata_sha256": metadata_sha256,
        "source_content_sha256": source_content_sha256,
        "normalized_source_sha256": normalized_source_sha256,
        "source_commit_sha": run_meta.get("head_sha"),
        "selected_job_ids": [job_id],
        "selected_job_names": [job_name],
        "log_acquisition_endpoint": log_url,
        "acquisition_size_bytes": len(log_bytes),
        "media_type": "text/plain",
        "acquisition_method": (
            "github-actions-real-job-log-bytes "
            f"(job {job_id!r} {'re-verified against locked-in identity' if locked_in else 'selected via job_selection_rule'})"
        ),
    }
    (case_out / "acquisition-receipt.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return result


def acquire_bot_output(candidate: dict, out_dir: Path) -> dict:
    ident = candidate["source_identity"]
    obj_id = ident["object_id_or_doi"]
    media_type = ident.get("media_type") or "application/octet-stream"
    if obj_id.startswith("extid:"):
        extid = obj_id.removeprefix("extid:")
        url = f"https://syzkaller.appspot.com/bug?extid={extid}"
        ext = "html"
    else:
        owner_repo, _, pr_num = obj_id.rpartition("#")
        url = f"https://api.github.com/repos/{owner_repo}/pulls/{pr_num}"
        ext = "json"
    data = _fetch(url)

    case_out = out_dir / candidate["candidate_id"]
    source_dir = case_out / "source"
    source_dir.mkdir(parents=True, exist_ok=True)
    filename = f"response.{ext}"
    (source_dir / filename).write_bytes(data)

    source_content_sha256 = sha256_bytes(data)
    normalized_source_sha256 = acquisition.build_normalized_archive(
        case_out, [f"source/{filename}"], case_out / "normalized-source.tar"
    )
    file_manifest = [{"path": f"source/{filename}", "sha256": source_content_sha256}]
    (case_out / "source-file-manifest.json").write_text(
        json.dumps(file_manifest, indent=2, sort_keys=True) + "\n"
    )

    result = {
        "candidate_id": candidate["candidate_id"],
        # There is no separate "metadata about the page" distinct from the
        # page/API response itself for this source kind — the fetched bytes
        # ARE the content, so no separate metadata artifact exists.
        "metadata_sha256": None,
        "source_content_sha256": source_content_sha256,
        "normalized_source_sha256": normalized_source_sha256,
        "acquisition_size_bytes": len(data),
        "media_type": media_type,
        "acquisition_method": "direct-public-page-or-api-fetch (actual response bytes retained under source/)",
    }
    (case_out / "acquisition-receipt.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return result


def acquire_dataset_or_research_corpus(candidate: dict, out_dir: Path) -> dict:
    ident = candidate["source_identity"]
    doi = ident.get("object_id_or_doi") or ""
    exact_file = ident.get("selected_exact_file")
    exact_url = ident.get("exact_download_url")
    if not exact_file or not exact_url:
        raise acquisition.RejectedCandidate(
            f"{candidate['candidate_id']}: no selected_exact_file/exact_download_url recorded in "
            "the registry — a dataset/research-corpus candidate cannot be acquired from a bare "
            "publisher-record identity alone (section 3 requires an exact, pre-selected file/member)"
        )

    if doi.startswith("10."):
        record_id = doi.rsplit(".", 1)[-1]
        metadata_bytes = _fetch(f"https://zenodo.org/api/records/{record_id}")
        method_prefix = "zenodo-api-record-metadata-plus-exact-file-download"
    else:
        metadata_bytes = _fetch(candidate["public_canonical_url"])
        method_prefix = "direct-fetch-of-publisher-canonical-page-plus-exact-file-download"
    metadata_sha256 = sha256_bytes(metadata_bytes)

    content = _fetch_raw(exact_url)
    expected_checksum = ident.get("publisher_reported_checksum")
    algo = (ident.get("publisher_checksum_algorithm") or "").lower()
    if expected_checksum and algo == "md5":
        actual = hashlib.md5(content).hexdigest()  # publisher's own declared algorithm, integrity check only
        if actual != expected_checksum:
            raise acquisition.RejectedCandidate(
                f"{candidate['candidate_id']}: downloaded {exact_file} md5 {actual} != "
                f"publisher-reported {expected_checksum} — refusing to accept mismatched content"
            )

    case_out = out_dir / candidate["candidate_id"]
    source_dir = case_out / "source"
    source_dir.mkdir(parents=True, exist_ok=True)
    (source_dir / exact_file).write_bytes(content)

    source_content_sha256 = sha256_bytes(content)
    normalized_source_sha256 = acquisition.build_normalized_archive(
        case_out, [f"source/{exact_file}"], case_out / "normalized-source.tar"
    )
    file_manifest = [{"path": f"source/{exact_file}", "sha256": source_content_sha256}]
    (case_out / "source-file-manifest.json").write_text(
        json.dumps(file_manifest, indent=2, sort_keys=True) + "\n"
    )

    result = {
        "candidate_id": candidate["candidate_id"],
        "metadata_sha256": metadata_sha256,
        "source_content_sha256": source_content_sha256,
        "normalized_source_sha256": normalized_source_sha256,
        "acquisition_size_bytes": len(content),
        "media_type": ident.get("media_type"),
        "acquisition_method": (
            f"{method_prefix} ({exact_file}, "
            f"{'checksum-verified against publisher ' + algo if expected_checksum and algo == 'md5' else 'no publisher checksum available'})"
        ),
    }
    (case_out / "acquisition-receipt.json").write_text(json.dumps(result, indent=2, sort_keys=True) + "\n")
    return result


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
        "metadata_sha256": result["metadata_sha256"],
        "source_content_sha256": result["source_content_sha256"],
        "normalized_source_sha256": result["normalized_source_sha256"],
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
            result = acquire_ci_run(candidate, out_dir)
        elif candidate["source_kind"] == "bot-output-artifact":
            result = acquire_bot_output(candidate, out_dir)
        elif candidate["source_kind"] in ("dataset-artifact", "research-corpus-artifact"):
            result = acquire_dataset_or_research_corpus(candidate, out_dir)
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
