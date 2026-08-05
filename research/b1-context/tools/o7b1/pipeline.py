"""Shared projection + task-dependence pipeline.

Both `project_case.py` (offline, no CAS) and `run_case.py` (CAS-backed) drive the
task-dependent projection through this one module, so their `context.*` artifacts
and their `projection-comparison.json` are produced by identical code and are
byte-identical. `run_case.py` adds the 3-arm evaluation on top; the projection
half is defined here once.

Task-dependence acceptance (generic, corrective round §3)
--------------------------------------------------------
A projection pair is task-dependent iff ALL of:

  1. the selected observation sets differ;
  2. the canonical context.json digests differ;
  3. each task selects every observation its own frozen questions require;
  4. both projections pass validity;
  5. the difference is caused by selector RELEVANCE, not solely by a tighter
     budget — at least one distinguishing observation is omitted by the other
     task for a non-budget reason.

A strict superset is NOT rejected here: if task B legitimately needs everything
A needs plus more, that is valid task-dependence. `neither_is_a_superset` is kept
only as a DIAGNOSTIC, and enforced (for a specific fixture) solely through the
committed `expected-projection-*.yaml`, never as a universal selector law.
"""
from __future__ import annotations

import json
import os

import yaml

from . import projector as pj
from . import registry as rg
from . import selector as sl
from .canonical import canonical_bytes, canonical_file_bytes, sha256_hex

COMPARISON_SCHEMA = "o7.b1.projection-comparison/v1"
EXPECTED_PROJECTION_FILENAME = "expected-projection-v0.yaml"


class PipelineError(Exception):
    pass


def _write_canonical(path: str, obj) -> str:
    data = canonical_file_bytes(obj)
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)
    return "sha256:" + sha256_hex(data)


def _write_text(path: str, text: str) -> str:
    data = text.encode("utf-8")
    if not data.endswith(b"\n"):
        data += b"\n"
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)
    return "sha256:" + sha256_hex(data)


def project_task(gold: dict, task: dict, questions: dict, schema_digest: str,
                 out_dir: str, budget: dict) -> dict:
    """Project one task, write its three canonical artifacts, return a summary.

    Fails closed if a task cannot select the observations its own frozen
    questions require, or if the projection is invalid.
    """
    try:
        sel = pj.select(gold, task, byte_budget=budget["byte_budget"],
                        record_budget=budget["record_budget"])
    except sl.SelectorError as e:
        raise PipelineError("selector refused task %s: %s" % (task.get("task_id"), e))
    validity = pj.validate(gold, sel, byte_budget=budget["byte_budget"],
                           record_budget=budget["record_budget"])

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
        schema_digest=schema_digest, sel=sel, budget=budget,
        artifact_digests=artifact_digests,
        input_observation_digest="sha256:" + sha256_hex(canonical_bytes(gold["observations"])),
        validity=validity)
    ctx_meta_digest = _write_canonical(os.path.join(task_dir, "context.meta.json"), meta)

    selected_ids = set(sel["selected_ids"])
    unmet = {}
    for q in questions["questions"]:
        missing = sorted(set(q["required_observation_ids"]) - selected_ids)
        if missing:
            unmet[q["id"]] = missing
    if unmet:
        raise PipelineError(
            "task %s does not select its own required observations: %s"
            % (task["task_id"], json.dumps(unmet, sort_keys=True)))
    if not validity["valid"]:
        raise PipelineError(
            "task %s produced an INVALID projection: %s"
            % (task["task_id"], json.dumps(validity, sort_keys=True)))

    summary = {
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
    return {"summary": summary, "sel": sel, "task": task, "questions": questions}


def _omit_reason(summary: dict, oid: str) -> str:
    for om in summary["omitted"]:
        if om["observation_id"] == oid:
            return om["reason"]
    return ""


def assess_task_dependence(a: dict, b: dict) -> dict:
    """Generic 5-point acceptance over the first two task summaries."""
    sa, sb = set(a["selected_ids"]), set(b["selected_ids"])
    distinguishing = sa ^ sb

    relevance_hits = 0
    budget_hits = 0
    for oid in distinguishing:
        other = b if oid in sa else a  # the task that did NOT select it
        reason = _omit_reason(other, oid)
        if reason.startswith("budget exceeded"):
            budget_hits += 1
        else:
            relevance_hits += 1

    each_supports_own = all(
        set(t["required_observation_union"]) <= set(t["selected_ids"]) for t in (a, b))
    both_valid = a["projection_validity"]["valid"] and b["projection_validity"]["valid"]
    projections_differ = sa != sb
    digests_differ = (a["artifact_digests"]["context.json"]
                      != b["artifact_digests"]["context.json"])
    difference_is_selector_relevance = relevance_hits > 0

    accepted = (projections_differ and digests_differ and each_supports_own
                and both_valid and difference_is_selector_relevance)
    reasons = []
    if not projections_differ:
        reasons.append("selections are identical")
    if not digests_differ:
        reasons.append("context.json digests are identical")
    if not each_supports_own:
        reasons.append("a task does not select its own required observations")
    if not both_valid:
        reasons.append("a projection is invalid")
    if not difference_is_selector_relevance:
        reasons.append("the difference is solely budget truncation, not selector relevance")
    return {
        "accepted": accepted,
        "rejection_reasons": reasons,
        "each_task_supports_own_questions": each_supports_own,
        "both_valid": both_valid,
        "difference_is_selector_relevance": difference_is_selector_relevance,
        "distinguishing_by_relevance_count": relevance_hits,
        "distinguishing_by_budget_only_count": budget_hits,
    }


def build_comparison(fixture: str, gold: dict, summaries: list[dict],
                     registry_digest: str, registry_filename: str | None = None) -> dict:
    if len(summaries) < 2:
        raise PipelineError("task dependence needs at least two tasks")
    a, b = summaries[0], summaries[1]
    sa, sb = set(a["selected_ids"]), set(b["selected_ids"])
    acceptance = assess_task_dependence(a, b)
    comparison = {
        "schema": COMPARISON_SCHEMA,
        "fixture_id": fixture,
        "registry_digest": registry_digest,
        "selector": a["selector"],
        "gold_state_digest": "sha256:" + sha256_hex(canonical_bytes(gold)),
        "input_observation_digest": "sha256:" + sha256_hex(canonical_bytes(gold["observations"])),
        "eligible_observation_count": sum(1 for o in gold["observations"] if sl.eligible(o)),
        "tasks": [s for s in summaries],
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
            "acceptance": acceptance,
        },
    }
    # Record a NON-DEFAULT registry filename only; the default keeps every
    # existing comparison (e.g. case-0001) byte-identical.
    if registry_filename is not None and registry_filename != rg.REGISTRY_FILENAME:
        comparison["registry_filename"] = registry_filename
    return comparison


def check_expected_projection(comparison: dict, fixture_dir: str) -> None:
    """Fail closed if the projection drifts from the committed fixture expectation.

    The expectation is fixture-specific (it includes `neither_is_a_superset` for
    THIS pair). A mismatch is a hard error: numbers change only by editing the
    expectation with an explicit explanation, never by silent drift.
    """
    path = os.path.join(fixture_dir, EXPECTED_PROJECTION_FILENAME)
    if not os.path.isfile(path):
        return
    with open(path, encoding="utf-8") as fh:
        exp = yaml.safe_load(fh)
    td = comparison["task_dependence"]
    by_id = {t["task_id"]: t for t in comparison["tasks"]}
    diffs = []
    for tid, want in (exp.get("tasks") or {}).items():
        got = by_id.get(tid)
        if got is None:
            diffs.append("expected task %s absent from comparison" % tid)
        elif got["selected_count"] != want.get("selected_count"):
            diffs.append("%s selected_count %d != expected %d"
                         % (tid, got["selected_count"], want.get("selected_count")))
    want_td = exp.get("task_dependence") or {}
    actual_td = {
        "selected_only_by_a_count": len(td["selected_only_by_a"]),
        "selected_only_by_b_count": len(td["selected_only_by_b"]),
        "selected_by_both_count": len(td["selected_by_both"]),
        "symmetric_difference_count": td["symmetric_difference_count"],
        "neither_is_a_superset": td["neither_is_a_superset"],
        "both_valid": td["acceptance"]["both_valid"],
    }
    for k, want_v in want_td.items():
        if actual_td.get(k) != want_v:
            diffs.append("task_dependence.%s = %r != expected %r"
                         % (k, actual_td.get(k), want_v))
    if diffs:
        raise PipelineError(
            "projection no longer matches committed %s (update it only with an "
            "explicit explanation): %s" % (EXPECTED_PROJECTION_FILENAME, "; ".join(diffs)))


def run_projection(fixture: str, fixture_dir: str, gold: dict, schema_digest: str,
                   out_dir: str, budget: dict,
                   registry_filename: str = rg.REGISTRY_FILENAME) -> dict:
    """Load the task registry, project every task, build + check the comparison.

    `registry_filename` selects a versioned registry inside the fixture dir
    (default `tasks-v0.yaml`). Returns {"entries", "summaries", "comparison",
    "registry_digest", "registry_filename"}. The caller writes
    projection-comparison.json (identically in both CLIs).
    """
    reg = rg.load_task_registry(fixture_dir, fixture, registry_filename)
    entries = [project_task(gold, e["task"], e["questions"], schema_digest, out_dir, budget)
               for e in reg["entries"]]
    summaries = [e["summary"] for e in entries]
    comparison = build_comparison(fixture, gold, summaries, reg["registry_digest"],
                                  reg["registry_filename"])
    acc = comparison["task_dependence"]["acceptance"]
    if not acc["accepted"]:
        raise PipelineError(
            "task-dependence acceptance failed: %s" % "; ".join(acc["rejection_reasons"]))
    check_expected_projection(comparison, fixture_dir)
    return {
        "entries": entries,
        "summaries": summaries,
        "comparison": comparison,
        "registry_digest": reg["registry_digest"],
        "registry_filename": reg["registry_filename"],
    }
