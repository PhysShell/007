#!/usr/bin/env python3
"""Capture a case twice in independent temp dirs and assert byte-identical raw +
RTK snapshots and identical semantic receipt fields (capture_timestamp and
wall_time_s are ignored). Exit non-zero on any nondeterminism.

Used by checks.qodec-v2-demo-reproducible.
"""
import shutil
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import corpus_tool as ct  # noqa: E402


def main(argv) -> int:
    case = argv[1] if len(argv) > 1 else "deterministic-log-demo"
    a = ct.capture_into_temp(case)
    b = ct.capture_into_temp(case)
    try:
        diffs = ct.compare_captures(a, b)
    finally:
        shutil.rmtree(a, ignore_errors=True)
        shutil.rmtree(b, ignore_errors=True)
    for d in diffs:
        print("NONDETERMINISTIC:", d, file=sys.stderr)
    if diffs:
        print(f"reproducibility FAILED for {case}", file=sys.stderr)
        return 1
    print(f"reproducible: two independent captures of {case} are byte-identical")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
