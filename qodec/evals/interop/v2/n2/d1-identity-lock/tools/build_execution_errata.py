#!/usr/bin/env python3
"""N2-D1b: builds the immutable, canonicalized, self-hash-locked execution-plan
errata record. An erratum corrects a frozen N2-C execution_expectation.argv
that is provably unexecutable against the real, frozen source tree -- never a
silent substitution, never a change to N2-C's own files.
"""
from __future__ import annotations

import hashlib
import json


def canonicalize_and_hash(body_without_hash: dict) -> tuple[str, str]:
    canonical = json.dumps(body_without_hash, indent=2, sort_keys=True) + "\n"
    return canonical, hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def build_errata(entries: list[dict]) -> dict:
    body = {
        "errata_contract_version": "n2d1b-execution-plan-errata-v1",
        "approving_scope": "N2-D1b",
        "rule": (
            "An erratum is authorized ONLY under one of two narrow classes, "
            "never a silent substitution, and never a change to N2-C's own "
            "files: "
            "(A) path correction -- the frozen argv is provably unexecutable "
            "against the exact frozen, pinned-commit source tree (a real 'no "
            "such file or directory' / equivalent failure, not a toolchain or "
            "dependency issue), and the correction is the smallest possible "
            "fix (replacing only the nonexistent assumed path with the real "
            "one); or "
            "(B) confinement-implicit-network-suppression correction -- the "
            "frozen argv is otherwise correct against the real source tree, "
            "but performs an implicit, redundant network-dependent action "
            "(e.g. its own re-restore) under the sandbox's legitimate, "
            "permanent network denial, EVEN AFTER a documented, real trusted "
            "restore/setup succeeded on the exact same fresh source tree used "
            "for confinement, and the correction is the smallest possible fix "
            "that suppresses ONLY that redundant action (e.g. appending a "
            "single --no-restore-style flag), never retargeting the project, "
            "test selection, framework, or build configuration. "
            "Either class must change nothing else about the case (source, "
            "tool, command class, family, primary/alternate role, quotas)."
        ),
        "entries": sorted(entries, key=lambda e: e["case_id"]),
    }
    _, digest = canonicalize_and_hash(body)
    body["errata_sha256"] = digest
    return body


def main() -> int:
    import argparse
    from pathlib import Path

    ap = argparse.ArgumentParser()
    ap.add_argument("--entries-json", required=True, help="path to a JSON list of entry dicts")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    entries = json.loads(Path(args.entries_json).read_text())
    body = build_errata(entries)
    Path(args.out).write_text(json.dumps(body, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"errata_sha256": body["errata_sha256"], "entry_count": len(entries)}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
