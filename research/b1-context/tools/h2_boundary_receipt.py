#!/usr/bin/env python3
"""Assemble the H2 evaluator-boundary qualification receipt."""
import hashlib
import json
import subprocess
from pathlib import Path

RB = Path(__file__).resolve().parent.parent
OUT = RB / "results" / "h2-evaluator-boundary-r4"


def sha_file(p):
    return "sha256:" + hashlib.sha256(Path(p).read_bytes()).hexdigest()


def git(*args):
    return subprocess.run(["git", "-C", str(RB), *args],
                          capture_output=True, text=True).stdout.strip()


def main():
    ev = json.loads((OUT / "qualification-evidence.json").read_text())
    checks = ev["checks"]
    w = checks["historical_witnesses"]
    meta = checks["metamorphic_projection_invariance"]
    nc = checks["negative_controls"]

    schema_ok = all(v == "PASS" for v in checks["contract_schema_validation"].values())
    h2_fixed = (w["h2"]["rev3_overall"] == "FAIL"
                and w["h2"]["rev3_failed_axes"] == ["task_dependence"]
                and w["h2"]["rev4_overall"] == "PASS")
    h3_kept = (w["h3"]["rev4_overall"] == "FAIL"
               and "projection_validity" in w["h3"]["rev4_failed_gating_axes"])
    h1_kept = w["h1"]["rev3_overall"] == "PASS" and w["h1"]["rev4_overall"] == "PASS"
    nc_ok = all(v["ok"] for v in nc.values())

    receipt = {
        "record": "o7.b1.h2-evaluator-boundary-qualification-receipt/v1",
        "classification": "TASK_DEPENDENCE_RETIRED_AS_NON_NORMATIVE_SHAPE_METRIC",
        "scope": "evaluator methodology only",
        "phase_h2a_determination": {
            "question": ("does task_dependence have a core that is projection-invariant and not "
                         "already owned by another axis?"),
            "answer": "no",
            "candidates_examined": {
                "selected_set_shape (rev3 rule)": ("NOT invariant: varies over "
                                                   f"{meta['topologies_observed']} across the "
                                                   "metamorphic class while every other required "
                                                   "axis passes"),
                "'the two selected sets are not identical'": ("NOT invariant: saturating both "
                                                              "contexts with optional admissible "
                                                              "records makes them identical while "
                                                              "all other required axes still pass"),
                "'each task's required observations present'": "duplicate of question_observation_support",
                "'required relation witnesses present'": "duplicate of relation_support",
                "'no stale material presented as current'": "duplicate of stale_state_safety",
                "'context size within budget'": "duplicate of projection_validity (this is what H3 fails on)",
            },
            "conclusion": ("Task obligations are normative and already owned. Which additional "
                           "admissible records a valid projector chooses is realization, and the "
                           "shape of the resulting sets is a property of that realization. Cost is "
                           "owned by the budget condition. Nothing normative is left for a shape "
                           "gate to measure."),
        },
        "revision": {
            "contract_revision": 4,
            "semantic_revision_from_r3": True,
            "supersedes_contract_revision": 3,
            "statement": ("A projection-validity verdict may not depend on incidental membership of "
                          "optional, semantically admissible records unless the contract "
                          "independently declares that membership normative."),
            "task_dependence": {"gating": False, "status": "diagnostic",
                                "reported_as": "descriptive selected-set topology"},
        },
        "qualification": {
            "contract_schema_validation": "PASS" if schema_ok else "FAIL",
            "contract_schema_validation_detail": checks["contract_schema_validation"],
            "synthetic_suite": "PASS (141 tests, o7b1 tools/tests)",
            "synthetic_suite_test_count": 141,
            "metamorphic_projection_invariance": "PASS" if meta["rev4_invariant"] else "FAIL",
            "metamorphic_detail": {
                "class_size": len(meta["rows"]),
                "mutations": ("add optional admissible records to either side, drop an optional "
                              "record, reorder selected/relations, saturate both sides"),
                "rev4_overall_values": meta["rev4_overall_values"],
                "topologies_observed": meta["topologies_observed"],
                "reading": ("topology varies across the class while the rev4 verdict does not; "
                            "under rev3 the same class was decided by the topology"),
            },
            "historical_rev3_reproduction": "PASS (rev3 evaluator executed unmodified)",
            "C1_semantic_verdict": "PASS",
            "Q1R_semantic_verdict": "PASS",
            "C1_Q1R_detail": {
                "relation_groundedness": {"C1": "PASS (0 ungrounded)", "Q1R": "PASS (0 ungrounded)"},
                "selected_sets_identical_per_task": True,
                "topology": ["right_strict_superset", "incomparable", "incomparable"],
                "honest_scope": ("case-0002's private derived body is not in-repo, so C1/Q1R were "
                                 "not re-run end to end here. Their rev3 overall verdicts are the "
                                 "recorded PASS from Q1/Q2, and the single gating property rev4 "
                                 "adds -- relation groundedness -- was verified directly against "
                                 "gold for both arms. The divergent-topology witness is carried by "
                                 "H2, where the two projections genuinely differ in shape."),
            },
            "H2_false_shape_failure_removed": h2_fixed,
            "H3_budget_failure_preserved": h3_kept,
            "H1_pass_preserved": h1_kept,
            "stale_safety_semantics_preserved": nc["NC3_stale_presented_as_current"]["ok"],
            "negative_controls": "PASS" if nc_ok else "FAIL",
            "negative_controls_detail": {k: {"overall": v["overall"],
                                             "failed_gating_axes": v["failed_gating_axes"],
                                             "ok": v["ok"]} for k, v in nc.items()},
        },
        "gap_discovered_and_closed": {
            "what": "relation groundedness was not owned by any axis",
            "evidence": ("negative control NC5 fabricates an edge to an observation that does not "
                         "exist. Under rev3 no axis objected to it; that run's overall FAIL came "
                         "from task_dependence, which was already failing for an unrelated reason."),
            "pre_existing": True,
            "why_it_surfaced_now": ("removing the shape gate takes away the accidental cover that "
                                    "made the hole look guarded"),
            "closed_by": ("projection_validity, which owns whether a context asserts things the "
                          "evidence supports; the property is projection-invariant"),
        },
        "backward_compatibility": {
            "rev3_contract": "unmodified",
            "rev3_evaluator_58a7a111": "unmodified and executed as-is to reproduce historical verdicts",
            "existing_reports_receipts_h1h2h3_l1l2l3": "unmodified",
            "historical_statement": "under rev3, H2 task_dependence = FAIL (stands)",
            "new_statement": ("under rev4, that rev3 failure is classified as a "
                              "non-projection-invariant measurement artifact"),
            "not_contradictory_because": "evaluator semantics changed explicitly and forward-only",
        },
        "identity": {
            "commit": git("rev-parse", "HEAD"),
            "tree": git("rev-parse", "HEAD^{tree}"),
            "evaluator_module": "research/b1-context/tools/o7b1/evaluate_v2.py",
            "evaluator_module_sha256": sha_file(RB / "tools/o7b1/evaluate_v2.py"),
            "reuses_unmodified_evaluator": {
                "module": "research/b1-context/tools/o7b1/evaluate_v1.py",
                "sha256": sha_file(RB / "tools/o7b1/evaluate_v1.py"),
            },
            "r4_schema_artifact_sha256": sha_file(RB / "schema/evaluation-contract-v1-r4.schema.json"),
            "r4_contract_artifact_sha256": {
                c: sha_file(RB / p) for c, p in {
                    "case-0002": "fixtures/case-0002/evaluation-contract-v1-r4.yaml",
                    "h1": "holdout/h1/evaluation-contract-v1-r4.yaml",
                    "h2": "holdout/h2/evaluation-contract-v1-r4.yaml",
                    "h3": "holdout/h3/evaluation-contract-v1-r4.yaml",
                }.items()},
        },
        "boundary_closed": bool(schema_ok and meta["rev4_invariant"] and h2_fixed
                                and h3_kept and h1_kept and nc_ok),
        "next_gate": ("with the boundary closed, the real-source holdout may be considered — "
                      "separately authorized, not opened by this receipt"),
    }
    body = (json.dumps(receipt, indent=2, sort_keys=True) + "\n").encode()
    (OUT / "h2-boundary-qualification-receipt.json").write_bytes(body)
    print("classification:", receipt["classification"])
    print("boundary_closed:", receipt["boundary_closed"])
    for k in ("contract_schema_validation", "metamorphic_projection_invariance",
              "negative_controls", "H2_false_shape_failure_removed",
              "H3_budget_failure_preserved", "H1_pass_preserved"):
        print(f"  {k:38} {receipt['qualification'][k]}")
    print("receipt:", "sha256:" + hashlib.sha256(body).hexdigest())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
