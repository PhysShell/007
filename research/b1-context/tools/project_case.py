#!/usr/bin/env python3
"""project_case.py — task-dependent projection only, for every task in a fixture.

    python3 research/b1-context/tools/project_case.py \
      --fixture case-0001 --out research/b1-context/results/case-0001-v0

Why this exists separately from `run_case.py`
---------------------------------------------
Projection is a pure function of

    gold state + task + selector contract/version + budget

and needs NO private data: the gold state is committed, and nothing here reads a
RAW blob. The 3-arm evaluation is different — it needs the derived transcripts
and the sealed negative control out of the owner's local CAS, so `run_case.py`
cannot run anywhere else.

Splitting them means the task-dependence result — the whole point of the
corrective round — is reproducible by anyone who checks out this repository,
offline, with no secrets and no CAS:

    same gold + different tasks -> different byte-stable projections

The comparison artifact this writes (`projection-comparison.json`) is the proof:
it records each task's selected set, the symmetric difference between them, and
each projection's canonical digests. Two runs produce byte-identical output.

This tool is READ-ONLY over the fixture and writes only under --out.
"""
from __future__ import annotations

import argparse
import json
import os
import sys

import yaml

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _HERE)

from o7b1 import projector as pj
from o7b1 import selector as sl
from o7b1.canonical import canonical_bytes, canonical_file_bytes, sha256_hex, sha256_of_file
from o7b1.schema import default_schema_path, validate_gold_state

BYTE_BUDGET = 20000
RECORD_BUDGET = 32

#: (task file, questions file) pairs for each fixture, in deterministic order.
TASKS = [
    ("task-v0.yaml", "questions-v0.yaml"),
    ("task-b-v0.yaml", "questions-b-v0.yaml"),
]


class ProjectionError(Exception):
    pass


def _load_yaml(path: str):
    with open(path, encoding="utf-8") as fh:
        return yaml.safe_load(fh)


def _fixture_dir(fixture: str) -> str:
    return os.path.normpath(os.path.join(_HERE, "..", "fixtures", fixture))


def _write_canonical(path: str, obj) -> str:
    data = canonical_file_bytes(obj)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)
    return "sha256:" + sha256_hex(data)


def _write_text(path: str, text: str) -> str:
    data = text.encode("utf-8")
    if not data.endswith(b"\n"):
        data += b"\n"
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)
    return "sha256:" + sha256_hex(data)


def project_one(gold: dict, task: dict, questions: dict, schema_digest: str,
                out_dir: str) -> dict:
    """Project one task and write its three canonical artifacts."""
    sel = pj.select(gold, task, byte_budget=BYTE_BUDGET, record_budget=RECORD_BUDGET)
    budget = {"byte_budget": BYTE_BUDGET, "record_budget": RECORD_BUDGET,
              "unit": "utf8_bytes+records"}
    validity = pj.validate(gold, sel, byte_budget=BYTE_BUDGET, record_budget=RECORD_BUDGET)

    task_dir = os.path.join(out_dir, "tasks", task["task_id"])
    context_json = pj.build_context_json(gold, task, sel)
    context_md = pj.build_context_md(gold, task, sel)
    ctx_json_digest = _write_canonical(os.path.join(task_dir, "context.json"), context_json)
    ctx_md_digest = _write_text(os.path.join(task_dir, "context.md"), context_md)
    artifact_digests = {"context.json": ctx_json_digest, "context.md": ctx_md_digest}

    meta = pj.build_context_meta(
        gold, task,
        task_digest="sha256:" + sha256_hex(canonical_bytes(task)),
        questions_digest="sha256:" + sha256_hex(canonical_bytes(questions)),
        schema_digest=schema_digest,
        sel=sel, budget=budget, artifact_digests=artifact_digests,
        input_observation_digest="sha256:" + sha256_hex(canonical_bytes(gold["observations"])),
        validity=validity)
    ctx_meta_digest = _write_canonical(os.path.join(task_dir, "context.meta.json"), meta)

    # Every question's required observations must actually be in this task's
    # own projection: a task that cannot answer its own questions is a broken
    # selector, and this fails closed rather than reporting a pretty number.
    selected_ids = set(sel["selected_ids"])
    unmet = {}
    for q in questions["questions"]:
        missing = sorted(set(q["required_observation_ids"]) - selected_ids)
        if missing:
            unmet[q["id"]] = missing
    if unmet:
        raise ProjectionError(
            "task %s does not select its own required observations: %s"
            % (task["task_id"], json.dumps(unmet, sort_keys=True)))
    if not validity["valid"]:
        raise ProjectionError(
            "task %s produced an INVALID projection: %s"
            % (task["task_id"], json.dumps(validity, sort_keys=True)))

    return {
        "task_id": task["task_id"],
        "task_statement": task["statement"].strip(),
        "selector": sel["selector_ref"],
        "selectors": sel["selector_spec"],
        "selected_ids": sel["selected_ids"],
        "selected_count": len(sel["selected"]),
        "omitted_count": len(sel["omitted"]),
        "omitted": sel["omitted"],
        "used_bytes": sel["used_bytes"],
        "context_bytes": len(canonical_file_bytes(context_json)),
        "question_ids": [q["id"] for q in questions["questions"]],
        "required_observation_union": sorted(
            {oid for q in questions["questions"] for oid in q["required_observation_ids"]}),
        "projection_validity": validity,
        "artifact_digests": {**artifact_digests, "context.meta.json": ctx_meta_digest},
    }


def run(fixture: str, out_dir: str) -> dict:
    fdir = _fixture_dir(fixture)
    schema_path = default_schema_path()
    schema_digest = "sha256:" + sha256_of_file(schema_path)
    with open(os.path.join(fdir, "gold-state-v0.json"), encoding="utf-8") as fh:
        gold = json.load(fh)
    validate_gold_state(gold, schema_path)

    results = []
    for task_file, questions_file in TASKS:
        task = _load_yaml(os.path.join(fdir, task_file))
        questions = _load_yaml(os.path.join(fdir, questions_file))
        if questions.get("task_id") != task["task_id"]:
            raise ProjectionError(
                "%s declares task_id %r but %s is %r"
                % (questions_file, questions.get("task_id"), task_file, task["task_id"]))
        results.append(project_one(gold, task, questions, schema_digest, out_dir))

    if len(results) < 2:
        raise ProjectionError("task dependence needs at least two tasks")

    # The acceptance invariant, stated as data rather than prose.
    a, b = results[0], results[1]
    sa, sb = set(a["selected_ids"]), set(b["selected_ids"])
    comparison = {
        "schema": "o7.b1.projection-comparison/v0",
        "fixture_id": fixture,
        "selector": a["selector"],
        "gold_state_digest": "sha256:" + sha256_hex(canonical_bytes(gold)),
        "input_observation_digest": "sha256:" + sha256_hex(canonical_bytes(gold["observations"])),
        "eligible_observation_count": sum(1 for o in gold["observations"] if sl.eligible(o)),
        "tasks": results,
        "task_dependence": {
            "task_a": a["task_id"],
            "task_b": b["task_id"],
            "selected_only_by_a": sorted(sa - sb),
            "selected_only_by_b": sorted(sb - sa),
            "selected_by_both": sorted(sa & sb),
            "symmetric_difference_count": len(sa ^ sb),
            "projections_differ": sa != sb,
            "context_json_digests_differ": (
                a["artifact_digests"]["context.json"] != b["artifact_digests"]["context.json"]),
            "neither_is_a_superset": not (sa <= sb) and not (sb <= sa),
        },
    }
    if not comparison["task_dependence"]["projections_differ"]:
        raise ProjectionError(
            "the two tasks selected the SAME observations; selection is not "
            "task-dependent and this tool refuses to report otherwise")
    if not comparison["task_dependence"]["neither_is_a_superset"]:
        raise ProjectionError(
            "one task's selection is a superset of the other's; that is budget "
            "truncation, not task-dependent selection")
    digest = _write_canonical(os.path.join(out_dir, "projection-comparison.json"), comparison)
    return {"comparison": comparison, "projection_comparison_digest": digest,
            "out_dir": out_dir}


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="Project every task of a fixture (offline, no CAS required).")
    ap.add_argument("--fixture", default="case-0001")
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)
    try:
        res = run(args.fixture, args.out)
    except (ProjectionError, sl.SelectorError) as e:
        sys.stderr.write("FAIL CLOSED: %s\n" % e)
        return 2
    td = res["comparison"]["task_dependence"]
    for t in res["comparison"]["tasks"]:
        sys.stderr.write("%s: %d selected, %d omitted, context.json %s\n"
                         % (t["task_id"], t["selected_count"], t["omitted_count"],
                            t["artifact_digests"]["context.json"]))
    sys.stderr.write("projections differ: %s (symmetric difference %d)\n"
                     % (td["projections_differ"], td["symmetric_difference_count"]))
    sys.stderr.write("projection-comparison.json: %s\n"
                     % res["projection_comparison_digest"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
