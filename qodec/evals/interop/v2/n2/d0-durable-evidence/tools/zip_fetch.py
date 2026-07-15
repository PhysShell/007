#!/usr/bin/env python3
"""N2-D0: real, credential-bearing retrieval of GitHub Actions workflow-run
artifacts for the SAME repository (physshell/007) — never a foreign repo,
so the standard authenticated Actions API is used directly (no anonymous-
access investigation needed here; that was N2-C's concern, not N2-D0's).

Runs only inside the N2-D0 rescue workflow, which has a real GITHUB_TOKEN
with `actions: read` (list/download artifacts) and `contents: write`
(create the release) for this repository. Never executes any downloaded
content — download, hash-verify, and archive-safety-check only.
"""
from __future__ import annotations

import hashlib
import json
import os
import sys
import zipfile
from pathlib import Path
from urllib.parse import urlsplit
from urllib.request import HTTPRedirectHandler, Request, build_opener

USER_AGENT = "qodec-n2d0-durable-evidence-rescue (read-only artifact retrieval)"


class _StripAuthOnCrossHostRedirect(HTTPRedirectHandler):
    """The artifacts/{id}/zip endpoint 302s to a signed Azure Blob Storage
    URL. Azure rejects the request (401) if GitHub's own Authorization
    header is still attached — the same class of bug run_acquisition.py
    (N2-C) already had to guard against for other hosts. Only strip the
    header when the redirect target's host differs from the request that
    produced it; same-host redirects (if any) keep it."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        new_req = super().redirect_request(req, fp, code, msg, headers, newurl)
        if new_req is not None and urlsplit(new_req.full_url).netloc != urlsplit(req.full_url).netloc:
            new_req.remove_header("Authorization")
        return new_req


_OPENER = build_opener(_StripAuthOnCrossHostRedirect)


def _urlopen(req, timeout):
    return _OPENER.open(req, timeout=timeout)


def _api(url: str, token: str) -> dict:
    req = Request(url, headers={
        "User-Agent": USER_AGENT,
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "Authorization": f"Bearer {token}",
    })
    with _urlopen(req, timeout=60) as resp:  # noqa: S310 - authenticated same-repo metadata fetch only
        return json.loads(resp.read())


def list_run_artifacts(owner: str, repo: str, run_id: str, token: str) -> list[dict]:
    """Real, paginated listing of every artifact recorded against a real,
    already-completed workflow run — the authoritative source for artifact
    ID + name + archive_digest, never trusted from memory/hardcoded values."""
    artifacts, page = [], 1
    while True:
        url = f"https://api.github.com/repos/{owner}/{repo}/actions/runs/{run_id}/artifacts?per_page=100&page={page}"
        data = _api(url, token)
        batch = data.get("artifacts", [])
        artifacts.extend(batch)
        if len(batch) < 100:
            break
        page += 1
    return artifacts


def download_artifact(owner: str, repo: str, artifact_id: int, token: str, dest: Path) -> str:
    """Downloads one artifact's real ZIP bytes via the authenticated
    same-repo download-artifact endpoint. Returns the sha256 of the
    downloaded bytes (computed locally, independent of any API-reported
    digest, for a second, independent check)."""
    url = f"https://api.github.com/repos/{owner}/{repo}/actions/artifacts/{artifact_id}/zip"
    req = Request(url, headers={
        "User-Agent": USER_AGENT,
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "Authorization": f"Bearer {token}",
    })
    with _urlopen(req, timeout=300) as resp:  # noqa: S310 - authenticated same-repo artifact fetch only
        data = resp.read()
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_bytes(data)
    return hashlib.sha256(data).hexdigest()


def extract_zip_safely(zip_path: Path, dest_dir: Path, archive_security_module) -> list[dict]:
    """Runs archive_security.assert_safe (N2-C's own, reused read-only —
    not modified) before ever extracting, then extracts and returns a
    sorted [{path, sha256, size}] list of every contained file."""
    archive_security_module.assert_safe(zip_path)
    dest_dir.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(zip_path) as zf:
        zf.extractall(dest_dir)
    entries = []
    for f in sorted(dest_dir.rglob("*")):
        if f.is_file():
            data = f.read_bytes()
            entries.append({
                "path": str(f.relative_to(dest_dir)),
                "sha256": hashlib.sha256(data).hexdigest(),
                "size": len(data),
            })
    return sorted(entries, key=lambda e: e["path"])


def main() -> int:
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("--owner", required=True)
    ap.add_argument("--repo", required=True)
    ap.add_argument("--run-id", required=True)
    ap.add_argument("--artifact-names", default="", help="comma-separated exact artifact names to fetch")
    ap.add_argument("--include-prefix", default="",
                     help="also fetch every artifact on this run whose name starts with this prefix "
                          "(discovered from the real, freshly-listed run artifacts — never hardcoded)")
    ap.add_argument("--out-dir", required=True)
    ap.add_argument("--archive-security-path", required=True,
                     help="path to N2-C's archive_security.py, imported read-only")
    args = ap.parse_args()

    token = os.environ.get("GITHUB_TOKEN", "")
    if not token:
        raise SystemExit("GITHUB_TOKEN must be set (same-repo authenticated retrieval only)")

    sys.path.insert(0, str(Path(args.archive_security_path).parent))
    import archive_security  # noqa: E402

    all_artifacts = list_run_artifacts(args.owner, args.repo, args.run_id, token)
    by_name = {a["name"]: a for a in all_artifacts}

    wanted = {n for n in args.artifact_names.split(",") if n}
    if args.include_prefix:
        wanted |= {name for name in by_name if name.startswith(args.include_prefix)}

    missing = sorted(wanted - set(by_name))
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    results = {"run_id": args.run_id, "missing_artifacts": missing, "fetched": []}

    for name in sorted(wanted & set(by_name)):
        meta = by_name[name]
        zip_path = out_dir / f"{name}.zip"
        local_sha256 = download_artifact(args.owner, args.repo, meta["id"], token, zip_path)
        api_digest = (meta.get("digest") or "").removeprefix("sha256:")
        digest_match = bool(api_digest) and api_digest == local_sha256
        extract_dir = out_dir / name
        contained_files = extract_zip_safely(zip_path, extract_dir, archive_security)
        results["fetched"].append({
            "artifact_name": name,
            "artifact_id": meta["id"],
            "api_reported_size_in_bytes": meta.get("size_in_bytes"),
            "local_downloaded_size_bytes": zip_path.stat().st_size,
            "api_reported_digest_sha256": api_digest,
            "locally_computed_zip_sha256": local_sha256,
            "digest_match": digest_match,
            "workflow_run_id_of_artifact": meta.get("workflow_run", {}).get("id"),
            "head_sha_of_artifact_run": meta.get("workflow_run", {}).get("head_sha"),
            "expired": meta.get("expired"),
            "contained_files": contained_files,
        })

    (out_dir / "fetch-report.json").write_text(json.dumps(results, indent=2, sort_keys=True) + "\n")
    print(json.dumps({
        "requested": sorted(wanted), "fetched_count": len(results["fetched"]), "missing": missing,
    }, indent=2))
    return 1 if missing or any(not r["digest_match"] for r in results["fetched"]) else 0


if __name__ == "__main__":
    raise SystemExit(main())
