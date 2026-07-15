#!/usr/bin/env python3
"""N2-D1b: assembles the canonicalized, self-hash-locked N2-D1-derived
raw-input manifest from derive_raw_input.py's per-case results. Does not
modify durable-input-manifest.json or the n2d0-durable-evidence-v1 release
-- this is a NEW, N2-D1-owned manifest."""
from __future__ import annotations

import hashlib
import json


def canonicalize_and_hash(manifest_without_hash: dict) -> tuple[str, str]:
    canonical = json.dumps(manifest_without_hash, indent=2, sort_keys=True) + "\n"
    return canonical, hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def build_manifest(cases: list[dict], derived_at: str) -> dict:
    body = {
        "manifest_contract_version": "n2d1-raw-input-manifest-v1",
        "derived_at": derived_at,
        "derives_from_durable_input_manifest_sha256": "d48d1b1fe3ce6d0cc46af31c73155802d6eb11aa83e8b6550be37c3cfcde0a53",
        "n2d0_closure_head": "4e40b6f393cbdaf1bfcc36b8c422f7e17ae41dee",
        "case_count": len(cases),
        "cases_with_problems": sum(1 for c in cases if c["problems"]),
        "cases": sorted(cases, key=lambda c: c["case_id"]),
    }
    _, digest = canonicalize_and_hash(body)
    body["manifest_sha256"] = digest
    return body


def main() -> int:
    import argparse
    from pathlib import Path

    ap = argparse.ArgumentParser()
    ap.add_argument("--derivation-result", required=True, help="output of run_derivation-style script: {'cases': [...]}")
    ap.add_argument("--derived-at", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    derivation = json.loads(Path(args.derivation_result).read_text())
    manifest = build_manifest(derivation["cases"], args.derived_at)
    Path(args.out).write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"manifest_sha256": manifest["manifest_sha256"], "case_count": manifest["case_count"]}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
