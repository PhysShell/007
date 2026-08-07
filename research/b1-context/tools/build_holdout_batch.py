#!/usr/bin/env python3
"""Freeze the first B1 holdout batch: H1 (relation-light), H2 (relation-heavy),
H3 (budget-pressure).

Each case is authored here, then its BASELINE is produced by the frozen B1
selector/projector v0 (never Qodec). The frozen inputs (gold, tasks, questions,
contract, budget, synthetic sources) plus the projector-v0 baseline and the
authoritative baseline verdict are written under research/b1-context/holdout/.

This script does NOT invoke Qodec and does NOT modify selector/projector v0,
the evaluator, or the contract semantics. It only materializes new cases so a
FROZEN producer (Qodec QA 6a5d4030 + adapter f8fa87e3) can later be measured on
material it has never seen.
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import yaml  # noqa: E402
from o7b1.canonical import canonical_bytes, canonical_file_bytes, sha256_hex  # noqa: E402
from o7b1 import projector as pj  # noqa: E402
from o7b1 import evaluate_v1 as ev  # noqa: E402

ROOT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "holdout")
SRC = "src-holdout"
SRC_DIG = "sha256:" + "1" * 64
EXTRACTOR_DIG = "sha256:" + "2" * 64
IN_FORCE = ("current", "pending")


def sha(b):
    return "sha256:" + sha256_hex(b)


def canon(o):
    return "sha256:" + sha256_hex(canonical_bytes(o))


def prov():
    return [{"source_id": SRC, "source_class": "repo_record", "digest": SRC_DIG}]


def obs(oid, status, kind, topics, authority="repo_record", superseded_by=None):
    o = {"observation_id": oid, "status": status, "authority": authority, "kind": kind,
         "topics": list(topics), "provenance": prov(), "statement": "statement for " + oid}
    if superseded_by:
        o["superseded_by"] = superseded_by
    return o


# =============================================================================
# Case definitions (authored BEFORE any Qodec run)
# =============================================================================

def case_h1():
    """Relation-light: almost everything is decided by observations; one relation."""
    fid = "holdout-h1"
    observations = [
        obs("obs-h1-state", "current", "status", ["ops"]),
        obs("obs-h1-next", "current", "next_action", ["ops"]),
        obs("obs-h1-decision", "current", "decision", ["ops"]),
        obs("obs-h1-detail", "current", "status", ["ops"]),
        obs("obs-h1-support-src", "current", "evidence", ["ops"]),
        obs("obs-h1-review-a", "current", "status", ["review"]),
        obs("obs-h1-review-b", "current", "decision", ["review"]),
    ]
    relations = [{"from": "obs-h1-support-src", "kind": "supports", "to": "obs-h1-state"}]
    tasks = {
        "holdout-h1-operate": {
            "topics_required": ["ops"], "kinds_required": [],
            "questions": [
                {"id": "qh1-state", "text": "?", "required_observation_ids": ["obs-h1-state"]},
                {"id": "qh1-next", "text": "?", "required_observation_ids": ["obs-h1-next"]},
                {"id": "qh1-support", "text": "?", "required_observation_ids": ["obs-h1-state"],
                 "required_relation_paths": [{"from": "obs-h1-support-src", "kind": "supports"}]},
            ],
            "relation_requirements": [
                {"question": "qh1-support", "from": "obs-h1-support-src", "kind": "supports",
                 "endpoint_policy": "all_current_targets_materialized",
                 "gold_derived_targets": ["obs-h1-state"], "target_status": ["current"]}],
            "forbidden": [],
        },
        "holdout-h1-review": {
            "topics_required": ["review"], "kinds_required": [],
            "questions": [{"id": "qh1r-a", "text": "?", "required_observation_ids": ["obs-h1-review-a"]}],
            "relation_requirements": [], "forbidden": [],
        },
    }
    return fid, {"schema": "o7.b1.state-observables/v0", "fixture_id": fid, "cutoff": "holdout",
                 "authority_vocabulary": ["repo_record", "agent_claim"],
                 "topic_vocabulary": ["ops", "review"], "observations": observations,
                 "relations": relations}, tasks


def case_h2():
    """Relation-heavy: supersedes/derived_from + stale history; supersession sources
    are on a topic the task does not require, so plain relevance drops the witnesses
    (the case-0002 failure class, on new material)."""
    fid = "holdout-h2"
    observations = [
        obs("obs-h2-current-a", "current", "status", ["audit"]),
        obs("obs-h2-current-b", "current", "evidence", ["audit"]),
        obs("obs-h2-authority", "current", "decision", ["audit"]),
        # supersession SOURCES live on 'history' topic, NOT required by the audit task
        obs("obs-h2-super-src-1", "current", "status", ["history"]),
        obs("obs-h2-super-src-2", "current", "evidence", ["history"]),
        obs("obs-h2-stale-1", "superseded", "status", ["history"], superseded_by="obs-h2-super-src-1"),
        obs("obs-h2-stale-2", "superseded", "risk", ["history"], superseded_by="obs-h2-super-src-2"),
        obs("obs-h2-claim", "pending", "status", ["audit"], authority="agent_claim"),
        obs("obs-h2-derived", "current", "evidence", ["audit"]),
        obs("obs-h2-derived-src", "current", "status", ["audit"]),
    ]
    relations = [
        {"from": "obs-h2-super-src-1", "kind": "supersedes", "to": "obs-h2-stale-1"},
        {"from": "obs-h2-super-src-2", "kind": "supersedes", "to": "obs-h2-stale-2"},
        {"from": "obs-h2-derived", "kind": "derived_from", "to": "obs-h2-derived-src"},
    ]
    tasks = {
        "holdout-h2-audit": {
            "topics_required": ["audit"], "kinds_required": [],
            "questions": [
                {"id": "qh2-active", "text": "?", "required_observation_ids": ["obs-h2-current-a"]},
                {"id": "qh2-support", "text": "?", "required_observation_ids": ["obs-h2-current-b"]},
                {"id": "qh2-superseded", "text": "?", "required_observation_ids": ["obs-h2-authority"],
                 "required_relation_paths": [
                     {"from": "obs-h2-super-src-1", "kind": "supersedes"},
                     {"from": "obs-h2-super-src-2", "kind": "supersedes"}]},
                {"id": "qh2-forbid", "text": "?", "required_observation_ids": ["obs-h2-authority"],
                 "forbidden_stale_as_current": ["obs-h2-stale-1", "obs-h2-claim"]},
            ],
            "relation_requirements": [
                {"question": "qh2-superseded", "from": "obs-h2-super-src-1", "kind": "supersedes",
                 "endpoint_policy": "edge_witness", "gold_derived_targets": ["obs-h2-stale-1"],
                 "target_status": ["superseded"]},
                {"question": "qh2-superseded", "from": "obs-h2-super-src-2", "kind": "supersedes",
                 "endpoint_policy": "edge_witness", "gold_derived_targets": ["obs-h2-stale-2"],
                 "target_status": ["superseded"]}],
            "forbidden": [{"question": "qh2-forbid", "ids": ["obs-h2-stale-1", "obs-h2-claim"]}],
        },
        "holdout-h2-history": {
            "topics_required": ["history"], "kinds_required": [],
            "questions": [{"id": "qh2h-src", "text": "?", "required_observation_ids": ["obs-h2-super-src-1"]}],
            "relation_requirements": [], "forbidden": [],
        },
    }
    return fid, {"schema": "o7.b1.state-observables/v0", "fixture_id": fid, "cutoff": "holdout",
                 "authority_vocabulary": ["repo_record", "agent_claim"],
                 "topic_vocabulary": ["audit", "history"], "observations": observations,
                 "relations": relations}, tasks


def case_h3():
    """Budget-pressure: more eligible relevant evidence than a tight record budget
    admits, plus a relation requirement whose source is eligible but crowded out —
    exercises where closure v0 (no eviction) meets the budget wall."""
    fid = "holdout-h3"
    observations = [obs("obs-h3-%02d" % i, "current", "status", ["load"]) for i in range(1, 13)]
    # a supersedes witness whose source is relevant but its stale target is not selected
    observations.append(obs("obs-h3-super-src", "current", "decision", ["load"]))
    observations.append(obs("obs-h3-stale", "superseded", "status", ["load"], superseded_by="obs-h3-super-src"))
    observations.append(obs("obs-h3-meta-a", "current", "status", ["meta"]))
    observations.append(obs("obs-h3-meta-b", "current", "decision", ["meta"]))
    relations = [{"from": "obs-h3-super-src", "kind": "supersedes", "to": "obs-h3-stale"}]
    tasks = {
        "holdout-h3-load": {
            "topics_required": ["load"], "kinds_required": [],
            "questions": [
                {"id": "qh3-any", "text": "?", "required_observation_ids": ["obs-h3-01"]},
                {"id": "qh3-super", "text": "?", "required_observation_ids": ["obs-h3-01"],
                 "required_relation_paths": [{"from": "obs-h3-super-src", "kind": "supersedes"}]},
            ],
            "relation_requirements": [
                {"question": "qh3-super", "from": "obs-h3-super-src", "kind": "supersedes",
                 "endpoint_policy": "edge_witness", "gold_derived_targets": ["obs-h3-stale"],
                 "target_status": ["superseded"]}],
            "forbidden": [],
            "tight_budget": {"byte_budget": 20000, "record_budget": 8},  # < the eligible set
        },
        "holdout-h3-meta": {
            "topics_required": ["meta"], "kinds_required": [],
            "questions": [{"id": "qh3m-a", "text": "?", "required_observation_ids": ["obs-h3-meta-a"]}],
            "relation_requirements": [], "forbidden": [],
        },
    }
    return fid, {"schema": "o7.b1.state-observables/v0", "fixture_id": fid, "cutoff": "holdout",
                 "authority_vocabulary": ["repo_record", "agent_claim"],
                 "topic_vocabulary": ["load", "meta"], "observations": observations,
                 "relations": relations}, tasks


# =============================================================================
# Materialization (freeze inputs + projector-v0 baseline)
# =============================================================================

def _task_yaml(fid, tid, spec):
    return {"schema": "o7.b1.task/v0", "task_id": tid, "fixture_id": fid,
            "statement": "holdout task " + tid,
            "selectors": {"required_topics": spec["topics_required"], "preferred_topics": [],
                          "excluded_topics": [], "required_kinds": spec["kinds_required"],
                          "anchor_observation_ids": [], "anchor_rationale": {}},
            "cutoff": "holdout"}


def _questions_yaml(fid, tid, spec):
    return {"schema": "o7.b1.questions/v0", "task_id": tid, "questions": spec["questions"]}


def _write(path, b):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(b)
    return b


def materialize(fid, gold, tasks, out_dir):
    p = lambda *a: os.path.join(out_dir, *a)
    files = {}

    def wy(name, o):
        files[name] = (_write(p(name), yaml.safe_dump(o, sort_keys=True, allow_unicode=True).encode()), o)

    def wj(name, o):
        files[name] = (_write(p(name), canonical_file_bytes(o)), o)

    wj("gold-state.json", gold)
    task_ids = sorted(tasks)
    default_budget = {"byte_budget": 20000, "record_budget": 32}
    budget = tasks[task_ids[0]].get("tight_budget", default_budget)
    wy("budget-v0.yaml", {"schema": "o7.b1.budget/v0", "fixture_id": fid, **budget,
                          "unit": "utf8_bytes+records", "note": "holdout frozen budget"})
    registry_tasks = []
    task_file_names, q_file_names = {}, {}
    for tid in task_ids:
        tfn, qfn = "task-%s.yaml" % tid, "questions-%s.yaml" % tid
        task_file_names[tid], q_file_names[tid] = tfn, qfn
        wy(tfn, _task_yaml(fid, tid, tasks[tid]))
        wy(qfn, _questions_yaml(fid, tid, tasks[tid]))
        registry_tasks.append({"task_file": tfn, "questions_file": qfn})
    wy("tasks.yaml", {"schema": "o7.b1.fixture-tasks/v0", "fixture_id": fid, "tasks": registry_tasks})

    # synthetic sources (one derived session + one raw source)
    body = "".join(json.dumps({"event": i}, sort_keys=True) + "\n" for i in range(3)).encode()
    _write(p("derived-body.jsonl"), body)
    session = "holdout-session"
    wy("source-selectors.yaml", {"schema": "o7.b1.source-selectors/v0", "fixture_id": fid,
                                 "sessions": [{"session_id": session, "raw_source": SRC, "coverage": {}}]})
    wy("manifest.yaml", {"schema": "o7.b1.fixture-manifest/v0", "fixture_id": fid, "negative_control": [],
                         "raw_sources": [{"id": SRC, "digest": SRC_DIG, "byte_length": 500,
                                          "source_system": "holdout-synthetic"}], "status": "holdout"})
    wj("derived-manifest.json", {"schema": "o7.b1.derived-manifest/v0", "fixture_id": fid,
                                 "sessions": [{"session_id": session, "derived_byte_length": len(body),
                                               "derived_digest": sha(body), "record_count": 3,
                                               "extractor_id": "o7-holdout-extractor-v0", "extractor_version": "0",
                                               "extractor_impl_digest": EXTRACTOR_DIG, "raw_source": SRC,
                                               "raw_source_digest": SRC_DIG}]})
    report = {"schema": "o7.b1.report/v1", "fixture_id": fid, "registry_filename": "tasks.yaml",
              "registry_digest": sha(files["tasks.yaml"][0]), "tasks": [{"task_id": t} for t in task_ids],
              "input_digests": {SRC: {"digest": SRC_DIG, "byte_length": 500, "status": "OK"}}, "status": "holdout"}
    wj("report.json", report)
    comparison = {"schema": "o7.b1.projection-comparison/v1", "fixture_id": fid, "registry_filename": "tasks.yaml",
                  "registry_digest": sha(files["tasks.yaml"][0]), "tasks": [{"task_id": t} for t in task_ids]}
    wj("projection-comparison.json", comparison)

    # ---- projector v0 baseline (frozen B1 selector; NOT Qodec) ----
    baseline_ctx = {}
    for tid in task_ids:
        task = files[task_file_names[tid]][1]
        sel = pj.select(gold, task, byte_budget=budget["byte_budget"], record_budget=budget["record_budget"])
        ctxj = pj.build_context_json(gold, task, sel)
        cb = canonical_file_bytes(ctxj)
        _write(p("baseline", "tasks", tid, "context.json"), cb)
        baseline_ctx[tid] = {"context": ctxj, "sha": sha(cb), "path": p("baseline", "tasks", tid, "context.json")}

    return {"files": files, "task_ids": task_ids, "task_file_names": task_file_names,
            "q_file_names": q_file_names, "budget": budget, "baseline_ctx": baseline_ctx,
            "body_bytes": body, "session": session}


def build_contract(fid, gold, tasks, mat):
    """rev3-structured contract for the holdout, bound to the projector-v0 baseline."""
    files = mat["files"]

    def dual_name(name):
        b, o = files[name]
        return {"artifact_bytes_sha256": sha(b), "canonical_object_sha256": canon(o), "byte_length": len(b)}

    def ctx_dual(tid):
        c = mat["baseline_ctx"][tid]
        return {"artifact_bytes_sha256": c["sha"],
                "canonical_object_sha256": canon(c["context"]), "byte_length": len(canonical_file_bytes(c["context"]))}

    gold_dual = dual_name("gold-state.json")
    bud_dual = dual_name("budget-v0.yaml")
    body = mat["body_bytes"]
    bound = {
        "gold_state": dict(logical_id="gold-state.json", visibility="public", **gold_dual),
        "fixture_manifest": dict(logical_id="manifest.yaml", visibility="public", **dual_name("manifest.yaml")),
        "source_selectors": dict(logical_id="source-selectors.yaml", visibility="public", **dual_name("source-selectors.yaml")),
        "task_registry": dict(logical_id="tasks.yaml", visibility="public", **dual_name("tasks.yaml")),
        "budget": dict(logical_id="budget-v0.yaml", visibility="public", **bud_dual),
        "r3_1_report": dict(logical_id="report.json", visibility="public", **dual_name("report.json")),
        "r3_1_projection_comparison": dict(logical_id="projection-comparison.json", visibility="public", **dual_name("projection-comparison.json")),
        "r3_1_derived_manifest": dict(logical_id="derived-manifest.json", visibility="public", **dual_name("derived-manifest.json")),
        "task_files": {mat["task_file_names"][t]: dict(visibility="public", **dual_name(mat["task_file_names"][t])) for t in mat["task_ids"]},
        "question_files": {mat["q_file_names"][t]: dict(visibility="public", **dual_name(mat["q_file_names"][t])) for t in mat["task_ids"]},
        "task_contexts": {t: dict(visibility="public", **ctx_dual(t)) for t in mat["task_ids"]},
        "derived_bodies_private": [{"session_id": mat["session"], "visibility": "private",
                                    "artifact_bytes_sha256": sha(body), "byte_length": len(body), "record_count": 3}],
    }
    ctasks = {}
    for tid in mat["task_ids"]:
        spec = tasks[tid]
        entry = {"questions_file": mat["q_file_names"][tid]}
        if spec.get("relation_requirements"):
            entry["relation_requirements"] = [dict(rr, direction="outgoing", depth=1, match="all")
                                              for rr in spec["relation_requirements"]]
        if spec.get("forbidden"):
            entry["forbidden_stale_as_current"] = spec["forbidden"]
        ctasks[tid] = entry
    gates = ["fixture identity agreement", "registry filename and digest agreement",
             "exact task-set agreement", "questions filename and dual-digest agreement",
             "referenced question existence", "every legacy relation path expanded",
             "no invented explicit relation requirement", "forbidden-stale set equality",
             "gold-derived targets recomputed exactly", "target-status agreement",
             "unknown identifiers or closed-vocabulary values rejected"]
    # pairs: N-choose-2 over tasks (holdout cases are single-task -> zero pairs allowed? schema needs minItems 1)
    task_list = mat["task_ids"]
    pairs = [{"left": task_list[i], "right": task_list[j], "expected_shape": "incomparable", "rationale": "holdout"}
             for i in range(len(task_list)) for j in range(i + 1, len(task_list))]
    return {
        "schema": "o7.b1.evaluation-contract/v1", "contract_id": "o7.b1.evaluation-contract/v1",
        "contract_revision": 3, "semantic_revision_from_r2": False, "validation_revision": True,
        "supersedes_contract_revision": 2,
        "supersedes": {"instance": "evaluation-contract-v1-r2.yaml", "artifact_bytes_sha256": "sha256:" + "3" * 64,
                       "r4a1_contract_commit": "0" * 40},
        # provenance_role uses the FROZEN rev3 schema's const values (case_role
        # "development", holdout_evidence false). Those fields are const in the schema,
        # which is frozen and must not change; the HOLDOUT status of this case is
        # asserted at the freeze-manifest / classification level, not in the contract.
        "provenance_role": {"case_role": "development", "designed_after_observing_case_0002": True,
                            "holdout_evidence": False, "generalization_claim_allowed": False},
        "digest_domains": {"canonicalization": {"id": "o7-canonical-json-v0", "definition": "holdout"},
                           "policy": "dual", "gold_state_dual_binding": dict(logical_id="gold-state.json", **gold_dual)},
        "arms": {"full_derived": {"requirement": "required"}, "projection": {"requirement": "required"},
                 "negative_control": {"requirement": "optional"}},
        "arm_semantics": {"missing_required_arm": "UNAVAILABLE", "missing_optional_arm": "NOT_APPLICABLE",
                          "non_applicable_metrics": None, "synthetic_negative_control": "forbidden", "gold_state": "oracle"},
        "family_applicability": {
            "source_compilation": {"full_derived": "required", "projection": "required", "negative_control": "optional"},
            "projection_validity": {"projection": "required", "full_derived": "NOT_APPLICABLE", "negative_control": "NOT_APPLICABLE"},
            "question_observation_support": {"projection": "required", "full_derived": "NOT_APPLICABLE", "negative_control": "NOT_APPLICABLE"},
            "relation_support": {"projection": "required", "full_derived": "NOT_APPLICABLE", "negative_control": "NOT_APPLICABLE"},
            "stale_state_safety": {"projection": "required", "full_derived": "NOT_APPLICABLE", "negative_control": "NOT_APPLICABLE"},
            "negative_control_diagnostics": {"negative_control": "optional"}},
        "full_derived_semantics": "holdout",
        "source_compilation_requirements_full_derived": ["session_matches_derived_manifest", "derived_body_available",
            "derived_byte_length_and_sha256_match", "record_count_reproducible", "extractor_identity_and_raw_binding_present"],
        "budget": {"authoritative_artifact": "budget-v0.yaml", "artifact_bytes_sha256": bud_dual["artifact_bytes_sha256"],
                   "canonical_object_sha256": bud_dual["canonical_object_sha256"], "byte_length": bud_dual["byte_length"],
                   "byte_budget": 20000, "record_budget": 32, "unit": "utf8_bytes+records",
                   "counted_representation": {"id": "selector-v0-canonical-record-jsonl", "definition": "holdout", "gating": True},
                   "diagnostics": {"rendered_context_sizes": {"gating": False}}},
        "input_closure": {"required_public_structured": sorted(mat["task_file_names"].values()),
                          "required_private": [mat["session"]], "negative_control_body": "none",
                          "raw_source_bodies": "frozen", "fail_closed": "undeclared inputs refuse"},
        "bound_inputs": bound, "tasks": ctasks,
        "contract_input_consistency_gates": [{"id": "gate-%02d" % (i + 1), "description": d} for i, d in enumerate(gates)],
        "task_dependence": {"all_unordered_pairs": True,
                            "pair_expectations": pairs or [{"left": task_list[0], "right": task_list[0],
                                                            "expected_shape": "incomparable", "rationale": "single-task holdout: self-pair placeholder"}],
                            "not_frozen": "holdout"},
        "shape_semantics": {"incomparable": "sets differ, neither subset", "left_strict_superset": "left contains every right item plus more",
                            "unknown_shape": "fail closed"},
        "evaluator_implementation_identity": {"manifest_schema": "o7.b1.evaluator-implementation-manifest/v0",
            "manifest_digest_field": "evaluator_implementation_manifest_digest",
            "output_must_record": {"evaluator_entrypoint_sha256": "required", "evaluator_module_sha256": "required",
                                   "execution_checkout_commit": "required", "execution_checkout_tree": "required"}, "note": "holdout"},
        "outcome_axes": ["contract_input_consistency", "source_compilation", "projection_validity",
                         "question_observation_support", "relation_support", "stale_state_safety",
                         "task_dependence", "negative_control_diagnostics"],
        "axis_requirements": {"contract_input_consistency": "required", "source_compilation": "required",
                              "projection_validity": "required", "question_observation_support": "required",
                              "relation_support": "required", "stale_state_safety": "required",
                              "task_dependence": "required", "negative_control_diagnostics": "optional"},
        "overall_rules": {"any_required_axis_FAIL": "overall FAIL",
                          "any_required_axis_UNAVAILABLE_without_FAIL": "overall UNAVAILABLE",
                          "all_required_axes_PASS": "overall PASS",
                          "optional_diagnostics_cannot_upgrade_or_downgrade_required_gates": True,
                          "PARTIAL": "forbidden in evaluation v1"},
    }


if __name__ == "__main__":
    print("use build_holdout_freeze.py")
