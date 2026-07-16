#!/usr/bin/env python3
"""Independently re-verifies docs/qodec-cleanup-record.json's self-hash lock.

Recomputes record_sha256 from the committed file's own content (never from
a separate builder's constants) over the compact canonical JSON form
(sort_keys, no whitespace) with record_sha256 excluded, and fails closed on
any mismatch. Mirrors PhysShell/qodec's tools/verify_migration_provenance.py
convention.
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

RECORD_PATH = Path(__file__).resolve().parents[1] / "docs" / "qodec-cleanup-record.json"

REQUIRED_FIELDS = [
    "schema_version",
    "record_type",
    "old_repository",
    "old_main_before_sha",
    "standalone_repository",
    "standalone_provenance_commit_sha",
    "standalone_migration_record_sha256",
    "standalone_migration_tag",
    "standalone_migration_tag_published",
    "removed_root_path",
    "removed_workflows",
    "removed_flake_outputs",
    "retained_historical_prs",
    "retained_release_tag",
    "history_rewritten",
    "benchmark_work_performed",
    "record_sha256",
]


def _compact_canonical_bytes(body: dict) -> bytes:
    return json.dumps(body, sort_keys=True, separators=(",", ":")).encode("utf-8")


def main() -> int:
    record = json.loads(RECORD_PATH.read_text())

    missing = [f for f in REQUIRED_FIELDS if f not in record]
    if missing:
        print(f"FAIL: missing required fields: {missing}")
        return 1

    if record["record_type"] != "embedded-qodec-removal-v1":
        print(f"FAIL: unexpected record_type {record['record_type']!r}")
        return 1

    stated_hash = record["record_sha256"]
    without_hash = dict(record)
    without_hash["record_sha256"] = None
    recomputed = f"sha256:{hashlib.sha256(_compact_canonical_bytes(without_hash)).hexdigest()}"

    if recomputed != stated_hash:
        print(f"FAIL: record_sha256 mismatch: stated={stated_hash} recomputed={recomputed}")
        return 1

    if record["history_rewritten"] is not False:
        print("FAIL: history_rewritten must be false")
        return 1

    if record["benchmark_work_performed"] is not False:
        print("FAIL: benchmark_work_performed must be false")
        return 1

    if record["standalone_migration_tag_published"] is not False:
        print(
            "FAIL: standalone_migration_tag_published must be false until the "
            "tag is actually confirmed on GitHub (do not fabricate a remote tag)"
        )
        return 1

    print(f"qodec cleanup record verification passed: OK (record_sha256={stated_hash})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
