#!/usr/bin/env python3
"""Evaluator for contract revision 4 — forward-only over the frozen rev3 evaluator.

`evaluate_v1` is not imported here to be *changed*; it is imported to be *reused
unmodified*, so that every measurement rev4 reports is the same measurement rev3
made. The single semantic difference is which measurements are allowed to decide
an outcome.

Rev3 gated `task_dependence` on whether two tasks' selected sets stood in a
contract-pinned relationship. H2 showed that a different but equally valid
projection -- one whose relation-aware closure legitimately admits an optional
record -- changes that relationship while satisfying every normative obligation.
The axis was therefore measuring the projector's realization, not the task's
requirements, and a metamorphic search found no replacement predicate that was
both projection-invariant and not already owned by another axis.

So rev4 keeps the number and drops its authority. Selected-set relationship is
reported as descriptive topology, and the overall verdict is decided by the axes
that own the normative properties:

    required observations present      question_observation_support
    required relation witnesses        relation_support
    stale material as current          stale_state_safety
    eligibility, admissibility, budget projection_validity

This is not a retraction. Under rev3, H2's `task_dependence` was FAIL, and that
record stands byte-intact. Under rev4 the same evidence is classified as a
non-projection-invariant measurement artifact. The two statements do not
conflict because the semantics changed explicitly, forward-only, under a new
identity.
"""
import argparse
import hashlib
import json
import sys
from pathlib import Path

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from o7b1 import evaluate_v1 as base  # noqa: E402
from o7b1.canonical import canonical_bytes, sha256_hex  # noqa: E402

REPORT_SCHEMA = "o7.b1.evaluation-report/v1r4"
CONTRACT_REVISION = 4

#: Axes that decide the outcome under rev4. `task_dependence` is absent by
#: design, and its absence is the whole revision.
GATING_AXES = ("contract_input_consistency", "source_compilation", "projection_validity",
               "question_observation_support", "relation_support", "stale_state_safety")

DIAGNOSTIC_AXES = ("task_dependence", "negative_control_diagnostics")


def module_sha256() -> str:
    return "sha256:" + hashlib.sha256(Path(__file__).read_bytes()).hexdigest()


def _topology(matrix: list) -> list:
    """The selected-set relationship, as description rather than judgement."""
    rows = []
    for row in matrix:
        if row.get("pair") == ["", ""]:
            continue
        if row.get("set_equality"):
            shape = "equal"
        elif row.get("left_superset_of_right"):
            shape = "left_strict_superset"
        elif row.get("right_superset_of_left"):
            shape = "right_strict_superset"
        else:
            shape = "incomparable"
        rows.append({
            "pair": row.get("pair"),
            "observed_topology": shape,
            "symmetric_difference_count": row.get("symmetric_difference_count"),
            "difference_source": row.get("difference_source"),
            "rev3_expected_shape": row.get("expected_shape"),
            "rev3_shape_satisfied": row.get("shape_satisfied"),
            "gating": False,
        })
    return rows


def relation_groundedness(argv: list) -> dict:
    """Is every relation a context asserts actually present in gold?

    Discovered by rev4's own negative controls, and it is a real hole rather
    than one this revision opened: under rev3 a context could assert an edge to
    an observation that does not exist and no axis objected. That mutant's run
    still came out FAIL, but only because `task_dependence` was failing for an
    unrelated reason — the fabricated edge was never the thing being caught.
    Removing the shape gate takes that accidental cover away, so the property
    has to be owned explicitly.

    It belongs to `projection_validity`: an edge with no counterpart in gold is
    not a different valid projection, it is an assertion about the world that
    the evidence does not support. And it is projection-invariant — grounding
    does not depend on which optional records a projector chose.
    """
    inputs = []
    for i, token in enumerate(argv):
        if token == "--input" and i + 1 < len(argv):
            inputs.append(argv[i + 1])
    gold_path, contexts = None, {}
    for spec in inputs:
        role, _, rest = spec.partition(":")
        logical, _, path = rest.partition("=")
        if role == "gold_state":
            gold_path = path
        elif role == "task_context":
            contexts[logical] = path
    if gold_path is None or not contexts:
        return {"checked": False, "reason": "gold or contexts not supplied"}
    gold = json.loads(Path(gold_path).read_text())
    grounded = {(r["from"], r["kind"], r["to"]) for r in gold.get("relations", [])}
    known = {o["observation_id"] for o in gold.get("observations", [])}
    findings = []
    for tid, path in sorted(contexts.items()):
        ctx = json.loads(Path(path).read_text())
        for rel in ctx.get("relations", []) or []:
            triple = (rel.get("from"), rel.get("kind"), rel.get("to"))
            if triple in grounded:
                continue
            findings.append({
                "task_id": tid, "relation": {"from": triple[0], "kind": triple[1], "to": triple[2]},
                "reason": ("endpoint unknown to gold" if triple[0] not in known or triple[2] not in known
                           else "edge absent from gold"),
            })
    return {"checked": True, "ungrounded_relations": findings,
            "status": "PASS" if not findings else "FAIL"}


def rev4_overall(axes: dict) -> str:
    """Only gating axes decide. A diagnostic axis can neither fail a run nor
    rescue one -- otherwise 'diagnostic' would be a label rather than a rule."""
    statuses = [axes.get(a) for a in GATING_AXES]
    if any(s == "FAIL" for s in statuses):
        return "FAIL"
    if any(s in (None, "UNAVAILABLE") for s in statuses):
        return "UNAVAILABLE"
    return "PASS"


def evaluate(argv: list) -> tuple[int, dict | None]:
    """Run the frozen rev3 evaluator, then re-decide under rev4 semantics."""
    ap = argparse.ArgumentParser(add_help=False)
    ap.add_argument("--contract", required=True)
    ap.add_argument("--out", required=True)
    known, _rest = ap.parse_known_args(argv)

    contract = yaml.safe_load(Path(known.contract).read_bytes())
    revision = contract.get("contract_revision")
    if revision != CONTRACT_REVISION:
        raise SystemExit(
            f"evaluate_v2 evaluates contract revision {CONTRACT_REVISION}; this contract is "
            f"revision {revision!r}. Rev3 artifacts are evaluated by the rev3 evaluator, which "
            "is retained unmodified.")
    if contract.get("axis_requirements", {}).get("task_dependence") != "diagnostic":
        raise SystemExit("a revision 4 contract must declare task_dependence diagnostic")

    # The rev3 evaluator validates against the rev3 schema and pins revision 3,
    # so it is handed a rev3-shaped view of this contract. Every *measurement*
    # it makes is unchanged; only the verdict is re-decided below.
    shim = dict(contract)
    shim["contract_revision"] = 3
    # The rev3 view declares rev3's own lineage (it superseded rev2), not rev4's.
    shim["supersedes_contract_revision"] = 2
    shim.pop("semantic_revision_from_r3", None)
    shim.pop("r4_schema_artifact_sha256", None)
    # rev3's schema refuses unknown keys, so the rev4-only annotations are
    # stripped from the view handed to it. The rev4 file keeps them.
    # Reconstruct rev3's own provenance block, which rev4 preserved verbatim:
    # the rev3 schema pins that pointer, and rewriting history to satisfy a
    # validator is exactly what this revision refuses to do.
    if contract.get("rev3_supersedes_block"):
        shim["supersedes"] = dict(contract["rev3_supersedes_block"])
    shim.pop("rev3_supersedes_block", None)
    shim["semantic_revision_from_r2"] = contract.get("semantic_revision_from_r2", False)
    shim["axis_requirements"] = dict(contract["axis_requirements"])
    shim["axis_requirements"]["task_dependence"] = "required"
    td = dict(contract["task_dependence"])
    for k in ("gating", "status", "non_normative_statement", "retirement_rationale",
              "reported_topology_values", "pair_expectations_are_historical_only"):
        td.pop(k, None)
    shim["task_dependence"] = td
    shim["overall_rules"] = {k: v for k, v in contract["overall_rules"].items()
                             if k != "diagnostic_axes_cannot_gate"}
    shim["overall_rules"]["any_required_axis_FAIL"] = "overall FAIL"

    shim_path = Path(known.out).parent / "_r3view-contract.yaml"
    shim_path.parent.mkdir(parents=True, exist_ok=True)
    shim_path.write_text(yaml.safe_dump(shim, sort_keys=False, allow_unicode=True, width=120))

    inner_out = Path(known.out).parent / "_r3view-evaluation.json"
    inner_argv = []
    skip = False
    for i, token in enumerate(argv):
        if skip:
            skip = False
            continue
        if token == "--contract":
            inner_argv.extend(["--contract", str(shim_path)])
            skip = True
        elif token == "--out":
            inner_argv.extend(["--out", str(inner_out)])
            skip = True
        else:
            inner_argv.append(token)
    rc = base.main(inner_argv)
    if rc != 0 or not inner_out.exists():
        return rc, None

    inner = json.loads(inner_out.read_text())
    axes = dict(inner["outcome_axes"])
    rev3_task_dependence = axes.get("task_dependence")
    axes["task_dependence"] = "DIAGNOSTIC"
    grounding = relation_groundedness(argv)
    if grounding.get("status") == "FAIL":
        axes["projection_validity"] = "FAIL"

    report = dict(inner)
    report["schema"] = REPORT_SCHEMA
    report["contract_revision"] = CONTRACT_REVISION
    report["outcome_axes"] = axes
    report["gating_axes"] = list(GATING_AXES)
    report["diagnostic_axes"] = list(DIAGNOSTIC_AXES)
    report["overall"] = rev4_overall(axes)
    report["selected_set_topology"] = _topology(inner.get("task_dependence_matrix", []))
    report["relation_groundedness"] = grounding
    report["rev3_comparison"] = {
        "rev3_overall": inner.get("overall"),
        "rev3_task_dependence": rev3_task_dependence,
        "reclassified": (rev3_task_dependence == "FAIL"),
        "note": ("under rev3 this axis could decide the outcome; under rev4 the same measurement "
                 "is descriptive. The rev3 record is retained byte-intact and is not rewritten."),
    }
    report["evaluator_implementation_identity"] = {
        "evaluator_module": "o7b1/evaluate_v2.py",
        "evaluator_module_sha256": module_sha256(),
        "reuses_unmodified": {
            "module": "o7b1/evaluate_v1.py",
            "sha256": "sha256:" + hashlib.sha256(
                Path(base.__file__).read_bytes()).hexdigest(),
        },
    }
    report["canonical_object_sha256"] = "sha256:" + sha256_hex(canonical_bytes(report))
    Path(known.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0, report


def main(argv: list) -> int:
    rc, _ = evaluate(argv)
    return rc


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
