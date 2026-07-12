#!/usr/bin/env python3
"""score.py — aggregate a Level-1 run into the go/no-go table.

Reads a run directory's metrics.jsonl and reports, per lane and arm, the median
incremental_qodec_gain and the token-level verdict mix. The design doc's
go/no-go for a qodec interop combination is:

    median incremental token saving >= 10%
    quality delta >= -1 percentage point
    no rise in invalid exact IDs / paths
    end-to-end latency not disproportionately worse

Only the first is decidable from Level 1 (tokens). The other three need the
reader (Level 2) and agent (Level 3) rungs — this scorer says so out loud
rather than implying a token win is the whole story. Its verdicts are the
token-level subset:

    win          gain >= 10%, artifact kept
    marginal     0 < gain < 10%
    passthrough  qodec added nothing and honestly fell back (no tax paid)

`harm`, `redundant` and `wrong-layer` are comprehension verdicts reserved for
the model rungs.

Usage:
    python3 score.py runs/<id>
"""

from __future__ import annotations

import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path


def _median(xs: list[float]) -> float:
    return statistics.median(xs) if xs else 0.0


def load(run_dir: Path) -> list[dict]:
    lines = (run_dir / "metrics.jsonl").read_text().splitlines()
    return [json.loads(line) for line in lines if line.strip()]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("run_dir", type=Path, help="a runs/<id> directory")
    ap.add_argument("--go-threshold", type=float, default=0.10, help="median gain go/no-go (default 0.10)")
    args = ap.parse_args()

    records = load(args.run_dir)
    meta = json.loads((args.run_dir / "meta.json").read_text())

    # Group by arm across all cases.
    by_arm: dict[str, list[dict]] = defaultdict(list)
    roundtrip_breaks = 0
    for rec in records:
        for arm, m in rec["arms"].items():
            by_arm[arm].append(m)
            if not m["roundtrip_ok"]:
                roundtrip_breaks += 1

    print(f"# qodec interop bench — Level 1 (artifacts, no model)")
    print(f"run {meta['run_id']}  codec={meta['codec']}  meter={meta['meter']}  "
          f"cases={meta['n_cases']}\n")
    print(f"{'arm':<18}{'n':>3}{'median gain':>13}{'wins':>6}{'marg':>6}{'pass':>6}  go?")
    for arm, ms in sorted(by_arm.items()):
        gains = [m["incremental_qodec_gain"] for m in ms]
        med = _median(gains)
        verdicts = [m["verdict"] for m in ms]
        wins = verdicts.count("win")
        marg = verdicts.count("marginal")
        pas = verdicts.count("passthrough")
        go = "GO" if med >= args.go_threshold else "no"
        print(f"{arm:<18}{len(ms):>3}{med*100:>12.1f}%{wins:>6}{marg:>6}{pas:>6}  {go}")

    print()
    # Per-case detail for the raw home-stadium lane.
    print("raw+qodec by case:")
    for rec in records:
        m = rec["arms"].get("raw+qodec")
        if m:
            print(f"  {rec['id']:<12}{rec['lane']:<15}"
                  f"{m['incremental_qodec_gain']*100:+7.1f}%  {m['codec']:<12}{m['verdict']}")

    skipped = {sk["optimizer"] for rec in records for sk in rec.get("skipped", [])}
    if skipped:
        print(f"\noptimizer lanes skipped (tool absent): {', '.join(sorted(skipped))}")
        print("install them (see doctor.py / tools.lock.toml) to light up those arms.")
    if roundtrip_breaks:
        print(f"\n!! {roundtrip_breaks} roundtrip break(s) — a codec bug, not a scoring event.")
        return 2
    print("\nLevel 1 measures tokens only. Comprehension (harm / redundant / wrong-layer)")
    print("needs the reader (L2) and agent (L3) rungs before any combination ships.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
