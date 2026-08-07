#!/usr/bin/env python3
"""Q1R producer: drive `qodec project` through the B1 adapter (Q2).

For each task: build a generic qodec-project request from B0 + gold + contract,
invoke the supplied Qodec binary, reconstruct a B1 context.json from the result,
and emit a truthful report.json / projection-comparison.json plus a complete
mapping receipt. Never imports Qodec code; the binary path is explicit.
"""
import argparse
import json
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import yaml  # noqa: E402
from o7b1.canonical import canonical_bytes, canonical_file_bytes, sha256_hex  # noqa: E402
from o7b1 import qodec_project_adapter as ad  # noqa: E402


def sha(b): return "sha256:" + sha256_hex(b)
def canon(o): return "sha256:" + sha256_hex(canonical_bytes(o))
def write(path, b):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(b)
    return b


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--contract", required=True)
    ap.add_argument("--gold", required=True)
    ap.add_argument("--b0-report", required=True)
    ap.add_argument("--b0-comparison", required=True)
    ap.add_argument("--b0-context", action="append", default=[], dest="b0_contexts")
    ap.add_argument("--qodec-bin", required=True)
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)

    contract = yaml.safe_load(open(args.contract, "rb").read())
    gold = json.load(open(args.gold, "rb"))
    b0_report = json.load(open(args.b0_report, "rb"))
    b0_comparison = json.load(open(args.b0_comparison, "rb"))
    b0_paths = dict(s.split("=", 1) for s in args.b0_contexts)
    byte_budget = contract["budget"]["byte_budget"]
    record_budget = contract["budget"]["record_budget"]

    task_ids = sorted(contract["tasks"].keys())
    mapping = {"record": "o7.b1.q2-qodec-project-mapping/v0", "adapter_id": ad.ADAPTER_ID,
               "adapter_impl_digest": ad.adapter_impl_digest(), "qodec_bin_sha256": sha(open(args.qodec_bin, "rb").read()),
               "tasks": {}}
    ctx_digests = {}
    closed = {}
    qodec_producer = None
    for tid in task_ids:
        tspec = contract["tasks"][tid]
        b0 = json.load(open(b0_paths[tid], "rb"))
        req = ad.build_request(tid, gold, tspec, b0, byte_budget, record_budget, request_id="q2-" + tid)
        req_bytes = canonical_file_bytes(req)
        req_path = os.path.join(args.out, "requests", tid + ".request.json")
        write(req_path, req_bytes)
        res_path = os.path.join(args.out, "results", tid + ".result.json")
        os.makedirs(os.path.dirname(res_path), exist_ok=True)
        # invoke qodec (execution identity via env, per protocol)
        env = dict(os.environ)
        r = subprocess.run([args.qodec_bin, "project", "--request", req_path, "--out", res_path],
                           capture_output=True, env=env)
        if r.returncode != 0:
            raise SystemExit("qodec project failed for %s: %s" % (tid, r.stderr.decode(errors="replace")))
        result = json.load(open(res_path, "rb"))
        qodec_producer = result["producer"]
        ctx = ad.reconstruct_context(tid, b0, result, qodec_producer)
        ctx_bytes = ad.canonical_context_bytes(ctx)
        md = ("# qodec-project context: %s\nproducer: %s\n\n" % (tid, ad.ADAPTER_ID)).encode()
        write(os.path.join(args.out, "tasks", tid, "context.json"), ctx_bytes)
        write(os.path.join(args.out, "tasks", tid, "context.md"), md)
        ctx_digests[tid] = {"context.json": sha(ctx_bytes), "context.md": sha(md)}
        closed[tid] = {"context": ctx, "result": result,
                       "selected": sorted(o["observation_id"] for o in ctx["selected"]),
                       "used_bytes": sum(len(canonical_bytes(o)) + 1 for o in ctx["selected"])}
        mapping["tasks"][tid] = {
            "request_sha256": sha(req_bytes), "request_canonical": canon(req),
            "result_request_canonical_sha256": result["request_canonical_sha256"],
            "result_canonical_sha256": result["result_canonical_sha256"],
            "selected_ids": closed[tid]["selected"],
            "requirements": [{"requirement_id": q["requirement_id"], "satisfied": q["satisfied"],
                              "materialized": q["materialized_targets"], "relation_only": q["relation_only_targets"]}
                             for q in result["requirements"]]}

    arm_selector = {"selector_id": ad.ADAPTER_ID, "qodec_project_version": qodec_producer.get("version"),
                    "qodec_implementation_sha256": qodec_producer.get("implementation_sha256"),
                    "adapter_impl_digest": ad.adapter_impl_digest()}

    report = dict(b0_report)
    report["selector_contract"] = arm_selector
    report["produced_by"] = ad.ADAPTER_ID
    report["tasks"] = [dict(t, selector=arm_selector,
                            projection={"context_artifact_digests": ctx_digests[t["task_id"]]})
                       for t in b0_report.get("tasks", [])]
    write(os.path.join(args.out, "report.json"), canonical_file_bytes(report))

    comparison = dict(b0_comparison)
    comparison["selector"] = arm_selector
    comparison["produced_by"] = ad.ADAPTER_ID
    ctasks = []
    for t in b0_comparison.get("tasks", []):
        tid = t["task_id"]
        ct = dict(t, selector=arm_selector, artifact_digests=ctx_digests[tid],
                  selected_ids=closed[tid]["selected"], selected_count=len(closed[tid]["selected"]),
                  used_bytes=closed[tid]["used_bytes"])
        ctasks.append(ct)
    comparison["tasks"] = ctasks
    write(os.path.join(args.out, "projection-comparison.json"), canonical_file_bytes(comparison))
    write(os.path.join(args.out, "q2-qodec-project-mapping.json"), canonical_file_bytes(mapping))

    print(json.dumps({tid: {"selected": closed[tid]["selected"], "used_bytes": closed[tid]["used_bytes"],
                            "requirements": mapping["tasks"][tid]["requirements"]} for tid in task_ids},
                     indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
