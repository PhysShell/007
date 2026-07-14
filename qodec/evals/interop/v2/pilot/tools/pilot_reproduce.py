#!/usr/bin/env python3
"""Reproducibility gate for the pilot corpus.

For every case: capture twice in independent temporary directories, compare the
canonical raw snapshots byte-for-byte, run RTK independently over each, compare
the RTK snapshots byte-for-byte, and verify semantic receipt fields are identical
(only capture_timestamp and wall_time_s are allowed to vary). When committed
snapshots exist, the pair is also compared against them. Any case that cannot
meet reproducibility fails the gate.
"""
from __future__ import annotations

import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pilot_lib as pl  # noqa: E402

SNAP_FILES = [pl.snap.RAW_STDOUT, pl.snap.RAW_STDERR, pl.snap.RTK_STDOUT, pl.snap.RTK_STDERR]


def _compare_dirs(tag: str, a: Path, b: Path) -> list[str]:
    diffs = []
    for rel in SNAP_FILES:
        ha, hb = pl.sha256_file(a / rel), pl.sha256_file(b / rel)
        if ha != hb:
            diffs.append(f"{tag}: {rel} differs ({ha[:12]} != {hb[:12]})")
    for rel in (pl.snap.NATIVE_RECEIPT, pl.snap.RTK_RECEIPT):
        ra = pl.rcpt.semantic_view(pl.load_json(a / rel))
        rb = pl.rcpt.semantic_view(pl.load_json(b / rel))
        if ra != rb:
            diffs.append(f"{tag}: receipt {rel} semantic diff {[k for k in ra if ra.get(k) != rb.get(k)]}")
    return diffs


def main(argv=None) -> int:
    ids = pl.case_ids()
    all_diffs = []
    for cid in ids:
        cap1 = pl.capture_into_temp(cid)
        cap2 = pl.capture_into_temp(cid)
        diffs = _compare_dirs(f"{cid} [twice]", cap1, cap2)
        committed = pl.bundle_dir(cid)
        if (committed / pl.snap.RAW_STDOUT).exists():
            diffs += _compare_dirs(f"{cid} [vs committed]", committed, cap1)
        shutil.rmtree(cap1, ignore_errors=True)
        shutil.rmtree(cap2, ignore_errors=True)
        status = "OK" if not diffs else "DRIFT"
        print(f"  {cid:32s} {status}")
        all_diffs += diffs
    if all_diffs:
        print(f"PILOT REPRODUCIBILITY FAILED — {len(all_diffs)} difference(s):", file=sys.stderr)
        for d in all_diffs:
            print("  " + d, file=sys.stderr)
        return 1
    print(f"PILOT REPRODUCIBLE — {len(ids)} case(s) captured twice, byte-identical")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
