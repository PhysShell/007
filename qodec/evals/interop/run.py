#!/usr/bin/env python3
"""run.py — Level 1: the artifact benchmark (no model), over real tools.

produce -> transform -> qodec for each case in a manifest, saving every hashed
artifact and reporting cold/warm incremental_qodec_gain. Before running, it
strict-checks the tools the manifest actually needs (via doctor), so a run that
claims reproducibility cannot proceed on a drifted tool or an unpinned repo.

    python3 run.py --manifest manifests/rtk.json --name rtk
    python3 run.py --manifest manifests/codegraph.json --name cg
    python3 run.py --manifest manifests/corpus.json --name corpus  # fixtures
"""

from __future__ import annotations

import argparse
import datetime as _dt
import json
from pathlib import Path

from bench import doctor, lockfiles, manifest, runner

HERE = Path(__file__).resolve().parent


def _required_tools(cases: list[manifest.Case]) -> list[str]:
    req: set[str] = set()
    for c in cases:
        if c.producer.type in ("rtk-command",):
            req.add("rtk")
        if c.producer.type == "codegraph":
            req.add("codegraph")
        for t in c.transforms:
            if t.type == "rtk":
                req.add("rtk")
    return sorted(req)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--manifest", type=Path, required=True)
    ap.add_argument("--codec", default="squeeze")
    ap.add_argument("--meter", default="o200k")
    ap.add_argument("--name")
    ap.add_argument("--out", type=Path, default=HERE / "runs")
    ap.add_argument("--allow-unhealthy", action="store_true",
                    help="run even if required tools fail strict checks (for debugging)")
    args = ap.parse_args()

    tools = lockfiles.tools()
    repos = lockfiles.repos()
    stdin_filters = set(tools["rtk"].stdin_filters) if "rtk" in tools else set()
    cases = manifest.load(args.manifest, stdin_filters=stdin_filters)

    required = _required_tools(cases)
    receipt = doctor.build_receipt(required)
    if not receipt["healthy"]:
        print("qodec unhealthy — run `python3 doctor.py`.")
        return 1
    if required and not receipt["strict_ok"] and not args.allow_unhealthy:
        print(f"required tools not ready for {args.manifest.name}: {required}")
        for f in receipt["strict_failures"]:
            print(f"  - {f}")
        print("fix with `python3 manage.py sync` / install pinned versions, or --allow-unhealthy.")
        return 1

    run_id = args.name or _dt.datetime.now(_dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = args.out / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    records = [runner.run_case(c, tools, repos, run_dir, codec=args.codec, meter=args.meter)
               for c in cases]

    meta = {
        "run_id": run_id, "level": 1, "manifest": str(args.manifest),
        "codec": args.codec, "meter": args.meter, "n_cases": len(records),
        "required_tools": required, "setup": receipt,
    }
    (run_dir / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
    with (run_dir / "metrics.jsonl").open("w") as fh:
        for rec in records:
            fh.write(json.dumps(rec) + "\n")

    # Human table.
    print(f"run {run_id}  manifest={args.manifest.name}  codec={args.codec} meter={args.meter}")
    hdr = f"{'case':<22}{'arm':<11}{'tool_tok':>9}{'cold':>8}{'warm':>8}{'up_ms':>8}{'q_ms':>7}  codec"
    print(hdr)
    broke = []
    for rec in records:
        if rec["status"] != "ok":
            print(f"{rec['id']:<22}{rec['arm']:<11}{rec['status'].upper():>9}  {rec.get('reason','')[:60]}")
            continue
        cold = f"{rec['cold_gain']*100:+.1f}%"
        warm = f"{rec['warm_gain']*100:+.1f}%"
        qms = rec["encode_ms"] + rec["decode_ms"]
        print(f"{rec['id']:<22}{rec['arm']:<11}{rec['tool_only_tokens']:>9}{cold:>8}{warm:>8}"
              f"{rec['upstream_tool_ms']:>8.0f}{qms:>7.0f}  {rec['codec']}")
        if not rec["roundtrip_ok"]:
            broke.append(rec["id"])
    print(f"\nwrote {run_dir}  (artifacts under cases/, metrics.jsonl, meta.json)")
    if broke:
        print(f"ROUNDTRIP FAILURES (codec bug): {broke}")
        return 2
    print("score with: python3 score.py " + str(run_dir))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
