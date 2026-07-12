#!/usr/bin/env python3
"""score.py — aggregate a run into the cold/warm go/no-go table.

The design doc's go/no-go needs median incremental_qodec_gain >= 10%. This
scorer reports it for BOTH cold (notation brief + artifact, what a one-shot
message pays) and warm (artifact only, brief amortized in a cached prefix), so a
combination that wins only warm — i.e. only after ignoring the mandatory decoder
instruction cold — is visible as exactly that, never sold as a flat win.

harm / redundant / wrong-layer stay comprehension verdicts for the L2/L3 rungs;
Level 1 decides win / marginal / loss / passthrough on tokens alone.

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


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("run_dir", type=Path)
    ap.add_argument("--go-threshold", type=float, default=0.10)
    args = ap.parse_args()

    meta = json.loads((args.run_dir / "meta.json").read_text())
    records = [json.loads(l) for l in (args.run_dir / "metrics.jsonl").read_text().splitlines() if l.strip()]
    ok = [r for r in records if r["status"] == "ok"]
    other = [r for r in records if r["status"] != "ok"]

    by_arm: dict[str, list[dict]] = defaultdict(list)
    for r in ok:
        by_arm[r["arm"]].append(r)

    print("# qodec interop bench — Level 1 (artifacts, no model)")
    print(f"run {meta['run_id']}  manifest={Path(meta['manifest']).name}  "
          f"codec={meta['codec']}  meter={meta['meter']}  cases={meta['n_cases']}\n")

    print(f"{'arm':<11}{'n':>3}{'med cold':>10}{'med warm':>10}  cold_go  warm_go")
    for arm, rs in sorted(by_arm.items()):
        cold = _median([r["cold_gain"] for r in rs])
        warm = _median([r["warm_gain"] for r in rs])
        cg = "GO" if cold >= args.go_threshold else "no"
        wg = "GO" if warm >= args.go_threshold else "no"
        print(f"{arm:<11}{len(rs):>3}{cold*100:>9.1f}%{warm*100:>9.1f}%   {cg:<7}  {wg}")

    print("\nby case:")
    print(f"{'case':<22}{'arm':<11}{'cold':>8}{'warm':>8}  {'codec':<10}{'up_ms':>7}{'q_ms':>7}  roundtrip")
    for r in ok:
        qms = r["encode_ms"] + r["decode_ms"]
        print(f"{r['id']:<22}{r['arm']:<11}{r['cold_gain']*100:+7.1f}%{r['warm_gain']*100:+7.1f}%  "
              f"{r['codec']:<10}{r['upstream_tool_ms']:>7.0f}{qms:>7.0f}  {'ok' if r['roundtrip_ok'] else 'FAIL'}")
        if r.get("baseline"):
            b = r["baseline"]
            red = 1 - r["tool_only_tokens"] / b["tokens"] if b["tokens"] else 0.0
            print(f"{'  ↳ raw baseline':<33}{b['tokens']:>7} tok  ->  tool {r['tool_only_tokens']} "
                  f"({red*100:+.1f}% by {b['tool']})")

    for r in other:
        print(f"{r['id']:<22}{r['arm']:<11}{r['status'].upper():>8}  {r.get('reason','')[:55]}")

    broke = [r["id"] for r in ok if not r["roundtrip_ok"]]
    if broke:
        print(f"\n!! roundtrip break(s): {broke} — a codec bug, not a scoring event.")
        return 2
    print("\ncold = brief + artifact (one-shot). warm = artifact only (brief cached).")
    print("Level 1 is tokens only; comprehension (harm/redundant/wrong-layer) is L2/L3.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
