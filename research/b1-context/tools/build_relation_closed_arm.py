#!/usr/bin/env python3
"""Build the deterministic relation-closure control arm (Q1 cell C1).

Reads the frozen gold state, the frozen evaluation contract's explicit relation
requirements, and the baseline (B0) task contexts; writes a relation-closed arm:
per-task context.json + context.md, plus a truthful report.json and
projection-comparison.json carrying the ARM's own selector identity and recomputed
projection digests (never the baseline's).

Generic: no case literal. Run:

  build_relation_closed_arm.py \
    --contract CONTRACT.yaml --gold GOLD.json --registry tasks-v1.yaml \
    --registry-path PATH --b0-report REPORT.json --b0-comparison CMP.json \
    --b0-context TASK_ID=PATH ... --out OUT_DIR
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import yaml  # noqa: E402

from o7b1.canonical import canonical_bytes, canonical_file_bytes, sha256_hex  # noqa: E402
from o7b1 import relation_closed_arm as rca  # noqa: E402


def _sha(b):
    return "sha256:" + sha256_hex(b)


def _canon(o):
    return "sha256:" + sha256_hex(canonical_bytes(o))


def _write(path, data: bytes):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)


def _pair_compare(sel_a, sel_b):
    a, b = set(sel_a), set(sel_b)
    return {"selected_only_by_a": sorted(a - b), "selected_only_by_b": sorted(b - a),
            "selected_by_both": sorted(a & b), "symmetric_difference_count": len(a ^ b),
            "neither_is_a_superset": not (a <= b or b <= a), "projections_differ": a != b}


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--contract", required=True)
    ap.add_argument("--gold", required=True)
    ap.add_argument("--registry", required=True)         # logical filename
    ap.add_argument("--b0-report", required=True)
    ap.add_argument("--b0-comparison", required=True)
    ap.add_argument("--b0-context", action="append", default=[], dest="b0_contexts")  # TASK=PATH
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)

    contract = yaml.safe_load(open(args.contract, "rb").read())
    gold = json.load(open(args.gold, "rb"))
    gold_idx = rca._gold_index(gold)
    b0_report = json.load(open(args.b0_report, "rb"))
    b0_comparison = json.load(open(args.b0_comparison, "rb"))
    b0_ctx_paths = {}
    for spec in args.b0_contexts:
        tid, path = spec.split("=", 1)
        b0_ctx_paths[tid] = path

    arm_selector = {"selector_id": rca.PRODUCER_ID, "selector_version": rca.PRODUCER_VERSION,
                    "selector_impl_digest": rca.producer_impl_digest()}

    task_ids = sorted(contract["tasks"].keys())
    closed = {}
    arm_ctx_digests = {}
    for tid in task_ids:
        reqs = contract["tasks"][tid].get("relation_requirements", [])
        b0_ctx = json.load(open(b0_ctx_paths[tid], "rb"))
        res = rca.close_task(tid, reqs, gold_idx, b0_ctx)
        ctx = res["context"]
        ctx_bytes = canonical_file_bytes(ctx)
        md_bytes = rca.render_md(ctx).encode("utf-8")
        _write(os.path.join(args.out, "tasks", tid, "context.json"), ctx_bytes)
        _write(os.path.join(args.out, "tasks", tid, "context.md"), md_bytes)
        arm_ctx_digests[tid] = {"context.json": _sha(ctx_bytes), "context.md": _sha(md_bytes)}
        closed[tid] = res

    # ---- arm report.json (truthful; arm identity; recomputed projection digests) ----
    report = dict(b0_report)
    report["selector_contract"] = arm_selector
    report["produced_by"] = rca.PRODUCER_ID
    new_tasks = []
    for t in b0_report.get("tasks", []):
        tid = t["task_id"]
        nt = dict(t)
        nt["selector"] = arm_selector
        nt["projection"] = {"context_artifact_digests": arm_ctx_digests[tid]}
        new_tasks.append(nt)
    report["tasks"] = new_tasks
    # recompute the report's pairwise task_dependence diagnostic from arm selections
    if len(task_ids) >= 2:
        a, b = task_ids[0], task_ids[1]
        sa = [o["observation_id"] for o in closed[a]["context"]["selected"]]
        sb = [o["observation_id"] for o in closed[b]["context"]["selected"]]
        td = dict(report.get("task_dependence", {}))
        td.update({"task_a": a, "task_b": b, **_pair_compare(sa, sb)})
        report["task_dependence"] = td
    report_bytes = canonical_file_bytes(report)
    _write(os.path.join(args.out, "report.json"), report_bytes)

    # ---- arm projection-comparison.json ----
    comparison = dict(b0_comparison)
    comparison["selector"] = arm_selector
    comparison["produced_by"] = rca.PRODUCER_ID
    ctasks = []
    for t in b0_comparison.get("tasks", []):
        tid = t["task_id"]
        res = closed[tid]
        ct = dict(t)
        ct["selector"] = arm_selector
        ct["artifact_digests"] = arm_ctx_digests[tid]
        ct["selected_ids"] = sorted(o["observation_id"] for o in res["context"]["selected"])
        ct["selected_count"] = res["selected_count"]
        ct["omitted"] = [o.get("observation_id") for o in res["context"]["omitted"]]
        ct["omitted_count"] = len(res["context"]["omitted"])
        ct["used_bytes"] = res["used_bytes"]
        ct["context_bytes"] = len(canonical_file_bytes(res["context"]))
        ctasks.append(ct)
    comparison["tasks"] = ctasks
    comparison_bytes = canonical_file_bytes(comparison)
    _write(os.path.join(args.out, "projection-comparison.json"), comparison_bytes)

    summary = {"producer": rca.PRODUCER_ID, "producer_impl_digest": rca.producer_impl_digest(),
               "report_sha256": _sha(report_bytes), "report_canonical": _canon(report),
               "comparison_sha256": _sha(comparison_bytes), "comparison_canonical": _canon(comparison),
               "tasks": {tid: {"context_sha256": arm_ctx_digests[tid]["context.json"],
                               "context_canonical": _canon(closed[tid]["context"]),
                               "materialized": closed[tid]["materialized"],
                               "added_edges": ["/".join(e) for e in closed[tid]["added_edges"]],
                               "selected_count": closed[tid]["selected_count"],
                               "used_bytes": closed[tid]["used_bytes"]} for tid in task_ids}}
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
