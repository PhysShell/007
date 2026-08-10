#!/usr/bin/env python3
"""Build contract revision 4 — forward-only, derived from rev3, never editing it.

Rev4 changes exactly one thing about what a verdict may depend on:

    A projection-validity verdict may not depend on incidental membership of
    optional, semantically admissible records unless the contract independently
    declares that membership normative.

`task_dependence` gated on the *shape* of two tasks' selected sets against a
shape pinned into the contract. Shape is a property of which optional-but-
admissible records one particular projector chose, so a different but equally
valid projection could fail it while satisfying every normative obligation --
which is what H2 demonstrated. Rev4 therefore keeps the measurement and drops
its authority: selected-set relationship is reported as descriptive topology
and cannot decide an outcome.

Nothing is retracted. Rev3 remains an immutable record of what the old
evaluator said; rev4 is a new identity that says something different about the
same evidence, and says so explicitly.
"""
import copy
import hashlib
import json
import sys
from pathlib import Path

import yaml

RB = Path(__file__).resolve().parent.parent
CASES = [RB / "fixtures" / "case-0002", RB / "holdout" / "h1",
         RB / "holdout" / "h2", RB / "holdout" / "h3"]

STATEMENT = ("A projection-validity verdict may not depend on incidental membership of optional, "
             "semantically admissible records unless the contract independently declares that "
             "membership normative. Selected-set topology is therefore not a gating oracle.")

RETIREMENT_RATIONALE = {
    "classification": "TASK_DEPENDENCE_RETIRED_AS_NON_NORMATIVE_SHAPE_METRIC",
    "why": ("Under rev3 the axis gated on whether two tasks' selected sets stood in a "
            "contract-pinned relationship. That relationship is a property of the projector's "
            "realization, not of the tasks' obligations: a valid projection whose relation-aware "
            "closure legitimately admits an optional record can change the shape without "
            "violating anything the contract declares mandatory."),
    "search_for_a_replacement": ("Every candidate predicate examined either failed metamorphic "
                                 "projection-invariance over mutations the contract does not "
                                 "declare mandatory, or collapsed into an axis that already owns "
                                 "the property."),
    "ownership_after_retirement": {
        "task obligations (required observations)": "question_observation_support",
        "required relation witnesses": "relation_support",
        "stale material presented as current": "stale_state_safety",
        "eligibility, admissibility and budget/cost": "projection_validity",
    },
    "future_reopening": ("Projection topology may become normative again only if a future contract "
                         "supplies an independent normative selection or minimality objective. "
                         "Until then it is descriptive."),
}


def sha(b):
    return "sha256:" + hashlib.sha256(b).hexdigest()


def build_schema():
    src = json.loads((RB / "schema" / "evaluation-contract-v1-r3.schema.json").read_text())
    s = copy.deepcopy(src)
    props = s["properties"]
    props["contract_revision"] = {"const": 4}
    props["supersedes_contract_revision"] = {"const": 3}
    props["semantic_revision_from_r3"] = {"const": True}
    # The one substantive schema change: the axis may no longer be declared gating.
    props["axis_requirements"]["properties"]["task_dependence"] = {"const": "diagnostic"}
    td = props.setdefault("task_dependence", {})
    td.setdefault("properties", {})["gating"] = {"const": False}
    td["properties"]["status"] = {"const": "diagnostic"}
    td["properties"]["non_normative_statement"] = {"type": "string"}
    td["properties"]["retirement_rationale"] = {"type": "object"}
    td["properties"]["reported_topology_values"] = {"type": "array", "items": {"type": "string"}}
    td["properties"]["pair_expectations_are_historical_only"] = {"const": True}
    req = td.get("required", [])
    td["required"] = sorted(set(req) | {"gating", "status"})
    # The schema refuses unknown keys, so every field rev4 adds is declared here.
    props["r4_schema_artifact_sha256"] = {"type": "string"}
    orules = props.get("overall_rules", {})
    if isinstance(orules.get("properties"), dict):
        orules["properties"]["diagnostic_axes_cannot_gate"] = {"const": True}
    sup = props.get("supersedes", {})
    if isinstance(sup.get("properties"), dict):
        sup["properties"]["note"] = {"type": "string"}
        sup["properties"]["instance"] = sup["properties"].get("instance", {"type": "string"})
        sup["properties"]["artifact_bytes_sha256"] = sup["properties"].get(
            "artifact_bytes_sha256", {"type": "string"})
        sup["required"] = ["instance", "artifact_bytes_sha256"]
        sup["properties"]["instance"] = {"const": "evaluation-contract-v1-r3.yaml"}
    props["rev3_supersedes_block"] = {"type": "object"}
    s["required"] = sorted(set(s["required"]) | {"semantic_revision_from_r3"})
    s["title"] = "o7.b1 evaluation contract v1 revision 4 (forward-only from r3)"
    s["description"] = STATEMENT
    return s


def build_contract(case_dir, schema_digest):
    src_path = case_dir / "evaluation-contract-v1-r3.yaml"
    src_bytes = src_path.read_bytes()
    c = copy.deepcopy(yaml.safe_load(src_bytes))
    c["contract_revision"] = 4
    c["supersedes_contract_revision"] = 3
    c["semantic_revision_from_r3"] = True
    # Retarget the pointer at rev3 while carrying every field rev3's own schema
    # requires, so the rev3 evaluator can still read this contract's provenance.
    # rev3's own provenance block is preserved verbatim under its own key, so
    # the rev3 view of this contract can be reconstructed exactly; `supersedes`
    # then points where rev4 actually points.
    c["rev3_supersedes_block"] = dict(c.get("supersedes") or {})
    sup = dict(c.get("supersedes") or {})
    sup["instance"] = "evaluation-contract-v1-r3.yaml"
    sup["artifact_bytes_sha256"] = sha(src_bytes)
    sup["note"] = "rev3 is retained byte-intact; no historical verdict is rewritten"
    c["supersedes"] = sup
    c["axis_requirements"]["task_dependence"] = "diagnostic"
    td = c["task_dependence"]
    td["gating"] = False
    td["status"] = "diagnostic"
    td["non_normative_statement"] = STATEMENT
    td["retirement_rationale"] = RETIREMENT_RATIONALE
    td["reported_topology_values"] = ["equal", "left_strict_superset",
                                      "right_strict_superset", "incomparable"]
    # The pinned expectations stay in the file as the historical record of what
    # rev3 asserted, explicitly marked as no longer deciding anything.
    td["pair_expectations_are_historical_only"] = True
    # `any_required_axis_FAIL` keeps its rev3 wording verbatim: under rev4
    # task_dependence simply is not a required axis, so the rule needs no
    # rewording to stop applying to it.
    c["overall_rules"]["diagnostic_axes_cannot_gate"] = True
    c["r4_schema_artifact_sha256"] = schema_digest
    return c


def main():
    schema = build_schema()
    sbytes = (json.dumps(schema, indent=2, sort_keys=True) + "\n").encode()
    spath = RB / "schema" / "evaluation-contract-v1-r4.schema.json"
    spath.write_bytes(sbytes)
    print(f"schema  {spath.relative_to(RB)}  {sha(sbytes)}")
    for case in CASES:
        if not (case / "evaluation-contract-v1-r3.yaml").exists():
            print(f"  skip {case.name}: no rev3 contract")
            continue
        c = build_contract(case, sha(sbytes))
        out = case / "evaluation-contract-v1-r4.yaml"
        body = yaml.safe_dump(c, sort_keys=False, allow_unicode=True, width=120).encode()
        out.write_bytes(body)
        print(f"contract {out.relative_to(RB)}  {sha(body)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
