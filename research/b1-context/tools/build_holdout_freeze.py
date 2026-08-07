#!/usr/bin/env python3
"""Freeze the H1/H2/H3 holdout batch and record each projector-v0 baseline verdict.

No Qodec is invoked here. Selector/projector v0, the evaluator, and the contract
semantics are unchanged. Output under research/b1-context/holdout/<case>/.
"""
import json
import os
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import yaml  # noqa: E402
from jsonschema import Draft202012Validator  # noqa: E402
from o7b1.canonical import canonical_bytes, sha256_hex  # noqa: E402
from o7b1 import evaluate_v1 as ev  # noqa: E402
import build_holdout_batch as hb  # noqa: E402

RB = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCHEMA = json.load(open(os.path.join(RB, "schema", "evaluation-contract-v1-r3.schema.json")))


def sha(b):
    return "sha256:" + sha256_hex(b)


def evaluate_baseline(case_dir, contract_path, mat):
    """Run the frozen evaluator on the projector-v0 baseline (dev mode)."""
    priv = tempfile.mkdtemp()
    body_path = os.path.join(priv, "holdout-session.jsonl")
    with open(body_path, "wb") as fh:
        fh.write(mat["body_bytes"])
    args = ["--contract", contract_path, "--out", os.path.join(priv, "eval.json"), "--mode", "dev"]
    d = case_dir

    def inp(role, logical, path):
        args.extend(["--input", "%s:%s=%s" % (role, logical, path)])
    inp("gold_state", "gold-state.json", f"{d}/gold-state.json")
    inp("fixture_manifest", "manifest.yaml", f"{d}/manifest.yaml")
    inp("source_selectors", "source-selectors.yaml", f"{d}/source-selectors.yaml")
    inp("task_registry", "tasks.yaml", f"{d}/tasks.yaml")
    inp("budget", "budget-v0.yaml", f"{d}/budget-v0.yaml")
    inp("report", "report.json", f"{d}/report.json")
    inp("projection_comparison", "projection-comparison.json", f"{d}/projection-comparison.json")
    inp("derived_manifest", "derived-manifest.json", f"{d}/derived-manifest.json")
    for tid in mat["task_ids"]:
        inp("task_file", mat["task_file_names"][tid], f"{d}/{mat['task_file_names'][tid]}")
        inp("questions_file", mat["q_file_names"][tid], f"{d}/{mat['q_file_names'][tid]}")
        inp("task_context", tid, f"{d}/baseline/tasks/{tid}/context.json")
    inp("derived_body", mat["session"], body_path)
    rc = ev.main(args)
    out = json.load(open(os.path.join(priv, "eval.json"))) if rc == 0 else None
    return rc, out


def main():
    out_root = os.path.join(RB, "holdout")
    os.makedirs(out_root, exist_ok=True)
    cases = {"h1": hb.case_h1, "h2": hb.case_h2, "h3": hb.case_h3}
    freeze = {"record": "o7.b1.holdout-batch-freeze/v0",
              "discipline": "inputs authored + frozen BEFORE any Qodec run; producer QA 6a5d4030 + adapter f8fa87e3 unchanged; selector/projector v0 + evaluator + contract semantics unchanged",
              "cases": {}}
    for key, fn in cases.items():
        fid, gold, tasks = fn()
        case_dir = os.path.join(out_root, key)
        mat = hb.materialize(fid, gold, tasks, case_dir)
        contract = hb.build_contract(fid, gold, tasks, mat)
        errs = sorted(Draft202012Validator(SCHEMA).iter_errors(contract), key=lambda e: list(e.path))
        if errs:
            raise SystemExit("%s contract fails rev3 schema: %s at %s" % (key, errs[0].message, errs[0].json_path))
        cpath = os.path.join(case_dir, "evaluation-contract-v1-r3.yaml")
        with open(cpath, "wb") as fh:
            fh.write(yaml.safe_dump(contract, sort_keys=False, allow_unicode=True, width=120).encode())
        rc, out = evaluate_baseline(case_dir, cpath, mat)
        verdict = out["overall"] if out else "REFUSAL(exit %d)" % rc
        axes = out["outcome_axes"] if out else {}
        # freeze digests of the frozen inputs
        frozen_digests = {name: sha(b) for name, (b, _o) in mat["files"].items()}
        frozen_digests["evaluation-contract-v1-r3.yaml"] = sha(open(cpath, "rb").read())
        for tid in mat["task_ids"]:
            frozen_digests["baseline/tasks/%s/context.json" % tid] = mat["baseline_ctx"][tid]["sha"]
        freeze["cases"][key] = {
            "fixture_id": fid, "shape": {"h1": "relation-light", "h2": "relation-heavy", "h3": "budget-pressure"}[key],
            "task_ids": mat["task_ids"], "budget": mat["budget"],
            "baseline_producer": "o7.b1.selector/projector v0",
            "baseline_overall": verdict, "baseline_axes": axes,
            "baseline_failed_axes": sorted(k for k, v in axes.items() if v == "FAIL"),
            "contract_canonical_object_sha256": "sha256:" + sha256_hex(canonical_bytes(contract)),
            "frozen_input_digests": frozen_digests}
        print("%s (%s): baseline overall=%s failed=%s" % (
            key, freeze["cases"][key]["shape"], verdict,
            freeze["cases"][key]["baseline_failed_axes"]))
    with open(os.path.join(out_root, "holdout-freeze-manifest.json"), "wb") as fh:
        fh.write((json.dumps(freeze, indent=2, sort_keys=True) + "\n").encode())
    print("freeze manifest written")


if __name__ == "__main__":
    raise SystemExit(main())
