"""Execution-receipt construction and reproducibility identity.

capture_timestamp is bound to SOURCE_DATE_EPOCH (not wall-clock) so committed
receipts are reproducible; it is metadata and never enters snapshot bytes.
Identity fields that require Nix are read from the environment (set by the flake
wrapper) with a git fallback for local use; they may be null but are always
present.
"""
from __future__ import annotations

import datetime
import json
import os
import subprocess
from pathlib import Path

from hashing import sha256_bytes, sha256_file

RECEIPT_VERSION = "corpus-receipt-v1"

# Semantic receipt fields compared for reproducibility. capture_timestamp and
# wall_time_s are the only allowed-to-vary metadata and are excluded.
SEMANTIC_RECEIPT_FIELDS = [
    "receipt_version", "case_id", "phase", "argv", "cwd", "environment_allowlist",
    "stdin_sha256", "stdout_sha256", "stderr_sha256", "exit_code", "timeout_status",
    "locale", "timezone", "tool_identity", "tool_binary_sha256",
    "rtk_source_sha", "rtk_argv", "rtk_classification", "payload_changed",
    "never_worse_returned_raw",
]
IGNORED_RECEIPT_FIELDS = ["capture_timestamp", "wall_time_s"]


def _git(root: Path, *args: str):
    try:
        r = subprocess.run(["git", "-C", str(root), *args], capture_output=True, text=True)
        return r.stdout.strip() if r.returncode == 0 and r.stdout.strip() else None
    except Exception:
        return None


def assemble_identity(repo_root: Path) -> dict:
    lock = repo_root / "flake.lock"
    nixpkgs_rev = os.environ.get("NIXPKGS_REV")
    if not nixpkgs_rev and lock.exists():
        try:
            nixpkgs_rev = json.loads(lock.read_text())["nodes"]["nixpkgs_2"]["locked"]["rev"]
        except Exception:
            nixpkgs_rev = None
    return {
        "nix_system": os.environ.get("NIX_SYSTEM"),
        "nix_version": os.environ.get("NIX_VERSION"),
        "nixpkgs_revision": nixpkgs_rev,
        "flake_lock_sha256": os.environ.get("FLAKE_LOCK_SHA256")
        or (sha256_file(lock) if lock.exists() else None),
        "repository_head_sha": os.environ.get("REPO_COMMIT_SHA") or _git(repo_root, "rev-parse", "HEAD"),
        "checked_out_commit_sha": os.environ.get("CHECKED_OUT_COMMIT_SHA")
        or os.environ.get("REPO_COMMIT_SHA") or _git(repo_root, "rev-parse", "HEAD"),
        "qodec_tree_sha": os.environ.get("QODEC_TREE_SHA") or _git(repo_root, "rev-parse", "HEAD:qodec"),
    }


def _iso_from_epoch(recipe: dict) -> str:
    epoch = int(recipe["source_date_epoch"])
    return datetime.datetime.fromtimestamp(epoch, tz=datetime.timezone.utc).isoformat()


def build_receipt(case_id: str, phase: str, step: dict, recipe: dict, identity: dict,
                  tool_identity: str, tool_binary_sha256: str | None,
                  rtk_extra: dict | None = None) -> dict:
    receipt = {
        "receipt_version": RECEIPT_VERSION,
        "case_id": case_id,
        "phase": phase,
        "argv": step["argv"],
        "cwd": step["cwd"],
        "environment_allowlist": list(recipe["environment_allowlist"]),
        "stdin_sha256": sha256_bytes(step["stdin_bytes"]) if step.get("stdin_bytes") else None,
        "stdout_sha256": sha256_bytes(step["stdout"]),
        "stderr_sha256": sha256_bytes(step["stderr"]),
        "exit_code": step["exit_code"],
        "wall_time_s": step["wall_time_s"],
        "timeout_status": "timed-out" if step["timed_out"] else "ok",
        "locale": recipe["locale"],
        "timezone": recipe["timezone"],
        "nix_system": identity["nix_system"],
        "nix_version": identity["nix_version"],
        "nixpkgs_revision": identity["nixpkgs_revision"],
        "flake_lock_sha256": identity["flake_lock_sha256"],
        "repository_head_sha": identity["repository_head_sha"],
        "checked_out_commit_sha": identity["checked_out_commit_sha"],
        "qodec_tree_sha": identity["qodec_tree_sha"],
        "tool_identity": tool_identity,
        "tool_binary_sha256": tool_binary_sha256,
        "capture_timestamp": _iso_from_epoch(recipe),
    }
    if rtk_extra:
        receipt.update(rtk_extra)
    return receipt


def semantic_view(receipt: dict) -> dict:
    return {k: receipt.get(k) for k in SEMANTIC_RECEIPT_FIELDS}


def write_json(path: Path, obj: dict) -> None:
    path.write_text(json.dumps(obj, indent=2, sort_keys=True) + "\n")
