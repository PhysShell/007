#!/usr/bin/env python3
"""Capture the pilot cases with real, pinned tools.

Modes:
  --out DIR   capture full bundles (inputs + snapshots + receipts + manifest)
              into DIR/<case_id>/ — used by the Nix bootstrap to produce the
              canonical snapshots as a CI artifact.
  --write     capture in place into the committed bundles and rebuild each
              snapshot-manifest (author bootstrap only).
  (default)   compare-only: capture into a temp dir and diff against the
              committed snapshots; nonzero exit on any drift.

No model calls. No shell. RTK runs over each case's declared primary stream.
"""
from __future__ import annotations

import argparse
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pilot_lib as pl  # noqa: E402

SNAP_FILES = [pl.snap.RAW_STDOUT, pl.snap.RAW_STDERR, pl.snap.RTK_STDOUT, pl.snap.RTK_STDERR]


def _capture_full(case_id: str, dst: Path):
    pl.copy_inputs(case_id, dst)
    pl.capture_case(case_id, dst, dst)
    pl.rebuild_snapshot_manifest(case_id, dst)


def compare(case_id: str) -> list[str]:
    committed = pl.bundle_dir(case_id)
    tmp = pl.capture_into_temp(case_id)
    diffs = []
    for rel in SNAP_FILES:
        a, b = pl.sha256_file(committed / rel), pl.sha256_file(tmp / rel)
        if a != b:
            diffs.append(f"{case_id}: snapshot {rel} differs ({a[:12]} != {b[:12]})")
    for rel in (pl.snap.NATIVE_RECEIPT, pl.snap.RTK_RECEIPT):
        ra = pl.rcpt.semantic_view(pl.load_json(committed / rel))
        rb = pl.rcpt.semantic_view(pl.load_json(tmp / rel))
        if ra != rb:
            keys = [k for k in ra if ra.get(k) != rb.get(k)]
            diffs.append(f"{case_id}: receipt {rel} semantic diff {keys}")
    shutil.rmtree(tmp, ignore_errors=True)
    return diffs


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default=None)
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--case", default=None)
    args = ap.parse_args(argv)
    ids = [args.case] if args.case else pl.case_ids()

    if args.out:
        out = Path(args.out)
        out.mkdir(parents=True, exist_ok=True)
        for cid in ids:
            _capture_full(cid, out / cid)
            print(f"captured {cid} -> {out / cid}")
        return 0
    if args.write:
        for cid in ids:
            b = pl.bundle_dir(cid)
            pl.capture_case(cid, b, b)
            pl.rebuild_snapshot_manifest(cid, b)
            print(f"[--write] captured + manifested {cid}")
        return 0

    diffs = []
    for cid in ids:
        diffs.extend(compare(cid))
    if diffs:
        print("PILOT CAPTURE DRIFT:", file=sys.stderr)
        for d in diffs:
            print("  " + d, file=sys.stderr)
        return 1
    print(f"pilot capture compare-only: {len(ids)} case(s) match committed snapshots")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
