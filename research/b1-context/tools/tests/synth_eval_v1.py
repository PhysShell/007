"""Synthetic fixture family for the o7.b1.evaluation/v1 evaluator (R4B / R4B.1).

Everything here is SYNTHETIC. No case-0002 gold observation, task id, question id,
selected id, digest or pair count is copied. Only the rev3 CONTRACT STRUCTURE and
schema constants (byte_budget 20000, record_budget 32, canonicalization id, the
arm/family constants) are reused — those are contract shape, not case content.

The fixture exercises the frozen contract literally: question files carry
`required_relation_paths` and question-level `forbidden_stale_as_current` (the
source of truth that gates 06/07/08 compare the contract against); source
compilation spans two derived sessions plus a plane-record raw source with no
derived transcript; the fixture manifest carries raw-source bytes/digests and the
report carries `input_digests`.

`materialize(dest)` writes a coherent all-PASS input set plus a matching
rev3-structured contract with correct digests, and returns the CLI arg list.
"""
from __future__ import annotations

import json
import os

import yaml

from o7b1.canonical import canonical_bytes, sha256_hex

FIXTURE_ID = "synthetic-eval"

# ---- raw sources (raw bytes + digests are independent of derived bodies) ----
SRC1, SRC1_DIG, SRC1_BYTES = "src-synthetic-1", "sha256:" + "11" * 32, 1000
SRC2, SRC2_DIG, SRC2_BYTES = "src-synthetic-2", "sha256:" + "22" * 32, 2000
PLANE, PLANE_DIG, PLANE_BYTES = "plane-record-synth", "sha256:" + "33" * 32, 300
SESSION1, SESSION2 = "synthetic-session-1", "synthetic-session-2"
EXTRACTOR_DIGEST = "sha256:" + "2" * 64


def _sha(b: bytes) -> str:
    return "sha256:" + sha256_hex(b)


def _prov():
    return [{"source_id": SRC1, "source_class": "repo_record", "digest": SRC1_DIG}]


def _obs(oid, status, kind, topics, authority="repo_record", superseded_by=None):
    o = {"observation_id": oid, "status": status, "authority": authority,
         "kind": kind, "topics": list(topics), "provenance": _prov(),
         "statement": "synthetic statement for %s" % oid}
    if superseded_by:
        o["superseded_by"] = superseded_by
    return o


def gold_state():
    obs = [
        _obs("obs-synthetic-a", "current", "status", ["alpha"]),
        _obs("obs-synthetic-b", "current", "next_action", ["alpha"]),
        _obs("obs-synthetic-c", "current", "decision", ["beta"]),
        _obs("obs-synthetic-d", "current", "status", ["beta"]),
        _obs("obs-synthetic-e", "current", "status", ["beta"]),
        _obs("obs-synthetic-dep-1", "current", "status", ["beta"]),
        _obs("obs-synthetic-dep-2", "current", "status", ["beta"]),
        _obs("obs-synthetic-stale", "superseded", "status", ["alpha"],
             authority="agent_claim", superseded_by="obs-synthetic-a"),
    ]
    relations = [
        {"from": "obs-synthetic-a", "kind": "supersedes", "to": "obs-synthetic-stale"},
        {"from": "obs-synthetic-c", "kind": "depends_on", "to": "obs-synthetic-dep-1"},
        {"from": "obs-synthetic-c", "kind": "depends_on", "to": "obs-synthetic-dep-2"},
    ]
    return {"schema": "o7.b1.state-observables/v0", "fixture_id": FIXTURE_ID,
            "cutoff": "synthetic cutoff", "authority_vocabulary": ["repo_record", "agent_claim"],
            "topic_vocabulary": ["alpha", "beta"], "observations": obs, "relations": relations}


# Selected sets per task drive the N-choose-2 shapes (see R4B).
SELECTED = {
    "t-alpha": ["obs-synthetic-a", "obs-synthetic-b", "obs-synthetic-c", "obs-synthetic-d"],
    "t-beta": ["obs-synthetic-c", "obs-synthetic-d", "obs-synthetic-e",
               "obs-synthetic-dep-1", "obs-synthetic-dep-2"],
    "t-gamma": ["obs-synthetic-a", "obs-synthetic-b"],
}
CONTEXT_RELATIONS = {
    "t-alpha": [{"from": "obs-synthetic-a", "kind": "supersedes", "to": "obs-synthetic-stale"}],
    "t-beta": [{"from": "obs-synthetic-c", "kind": "depends_on", "to": "obs-synthetic-dep-1"},
               {"from": "obs-synthetic-c", "kind": "depends_on", "to": "obs-synthetic-dep-2"}],
    "t-gamma": [],
}
TASK_FILES = {"t-alpha": "task-alpha.yaml", "t-beta": "task-beta.yaml", "t-gamma": "task-gamma.yaml"}
Q_FILES = {"t-alpha": "questions-alpha.yaml", "t-beta": "questions-beta.yaml",
           "t-gamma": "questions-gamma.yaml"}

# Question files ARE the source of truth for gates 06/07/08: required_relation_paths
# and question-level forbidden_stale_as_current.
QUESTIONS = {
    "t-alpha": [
        {"id": "qa1", "text": "?", "required_observation_ids": ["obs-synthetic-a"],
         "required_relation_paths": [{"from": "obs-synthetic-a", "kind": "supersedes"}],
         "forbidden_stale_as_current": ["obs-synthetic-stale"]},
        {"id": "qa2", "text": "?", "required_observation_ids": ["obs-synthetic-b"]},
        # relation-only question: no required observation IDs (vacuous support).
        {"id": "qa6", "text": "?", "required_observation_ids": [],
         "required_relation_paths": [{"from": "obs-synthetic-a", "kind": "supersedes"}]},
    ],
    "t-beta": [
        {"id": "qb1", "text": "?", "required_observation_ids": ["obs-synthetic-c"],
         "required_relation_paths": [{"from": "obs-synthetic-c", "kind": "depends_on"}]},
    ],
    "t-gamma": [
        {"id": "qg1", "text": "?", "required_observation_ids": ["obs-synthetic-a"]},
    ],
}


def _relreq(question, frm, kind, policy, targets, status):
    return {"question": question, "from": frm, "kind": kind, "direction": "outgoing",
            "depth": 1, "match": "all", "endpoint_policy": policy,
            "gold_derived_targets": list(targets), "target_status": list(status)}


RELATION_REQS = {
    "t-alpha": [
        _relreq("qa1", "obs-synthetic-a", "supersedes", "edge_witness", ["obs-synthetic-stale"], ["superseded"]),
        _relreq("qa6", "obs-synthetic-a", "supersedes", "edge_witness", ["obs-synthetic-stale"], ["superseded"]),
    ],
    "t-beta": [
        _relreq("qb1", "obs-synthetic-c", "depends_on", "all_current_targets_materialized",
                ["obs-synthetic-dep-1", "obs-synthetic-dep-2"], ["current"]),
    ],
    "t-gamma": [],
}
# Contract-level forbidden mirrors the question-level forbidden exactly (gate-08).
FORBIDDEN = {
    "t-alpha": [{"question": "qa1", "ids": ["obs-synthetic-stale"]}],
    "t-beta": [], "t-gamma": [],
}
PAIRS = [
    {"left": "t-alpha", "right": "t-beta", "expected_shape": "incomparable", "rationale": "synthetic"},
    {"left": "t-alpha", "right": "t-gamma", "expected_shape": "left_strict_superset", "rationale": "synthetic"},
    {"left": "t-beta", "right": "t-gamma", "expected_shape": "incomparable", "rationale": "synthetic"},
]


def _obs_by_id():
    return {o["observation_id"]: o for o in gold_state()["observations"]}


def context(task_id):
    by = _obs_by_id()
    sel = [dict(by[oid], selection_reason="synthetic relevance", selection_score=1.0)
           for oid in SELECTED[task_id]]
    selected_ids = set(SELECTED[task_id])
    omitted = [{"observation_id": oid, "reason": "not relevant to this task (synthetic)"}
               for oid in sorted(by) if oid not in selected_ids]
    return {"schema": "o7.b1.context/v0", "fixture_id": FIXTURE_ID, "task_id": task_id,
            "cutoff": "synthetic cutoff", "selectors": task_file(task_id)["selectors"],
            "selector": {"selector_id": "o7.b1.selector/v0"},
            "selected": sel, "omitted": omitted, "relations": CONTEXT_RELATIONS[task_id]}


def task_file(task_id):
    return {"schema": "o7.b1.task/v0", "task_id": task_id, "fixture_id": FIXTURE_ID,
            "statement": "synthetic task %s" % task_id,
            "selectors": {"required_topics": [], "preferred_topics": [], "excluded_topics": [],
                          "required_kinds": [], "anchor_observation_ids": [], "anchor_rationale": {}},
            "cutoff": "synthetic cutoff"}


def questions_file(task_id):
    return {"schema": "o7.b1.questions/v0", "task_id": task_id, "questions": QUESTIONS[task_id]}


def registry():
    return {"schema": "o7.b1.fixture-tasks/v0", "fixture_id": FIXTURE_ID,
            "tasks": [{"task_file": TASK_FILES[t], "questions_file": Q_FILES[t]}
                      for t in ("t-alpha", "t-beta", "t-gamma")]}


def budget():
    return {"schema": "o7.b1.budget/v0", "fixture_id": FIXTURE_ID,
            "byte_budget": 20000, "record_budget": 32, "unit": "utf8_bytes+records",
            "note": "synthetic frozen budget"}


def source_selectors():
    return {"schema": "o7.b1.source-selectors/v0", "fixture_id": FIXTURE_ID,
            "sessions": [{"session_id": SESSION1, "raw_source": SRC1, "coverage": {}},
                         {"session_id": SESSION2, "raw_source": SRC2, "coverage": {}}]}


def fixture_manifest():
    return {"schema": "o7.b1.fixture-manifest/v0", "fixture_id": FIXTURE_ID,
            "negative_control": [],
            "raw_sources": [
                {"id": SRC1, "digest": SRC1_DIG, "byte_length": SRC1_BYTES, "source_system": "synthetic"},
                {"id": SRC2, "digest": SRC2_DIG, "byte_length": SRC2_BYTES, "source_system": "synthetic"},
                {"id": PLANE, "digest": PLANE_DIG, "byte_length": PLANE_BYTES, "source_system": "synthetic"}],
            "status": "synthetic"}


def derived_body_text(n):
    recs = [{"event": i, "session": n} for i in range(n)]
    return "".join(json.dumps(r, sort_keys=True) + "\n" for r in recs)


BODY1 = derived_body_text(3).encode("utf-8")
BODY2 = derived_body_text(2).encode("utf-8")


def _session(sid, raw, raw_dig, body):
    return {"session_id": sid, "derived_byte_length": len(body), "derived_digest": _sha(body),
            "record_count": body.decode().count("\n"), "extractor_id": "o7-synthetic-extractor-v0",
            "extractor_version": "0", "extractor_impl_digest": EXTRACTOR_DIGEST,
            "raw_source": raw, "raw_source_digest": raw_dig}


def derived_manifest():
    return {"schema": "o7.b1.derived-manifest/v0", "fixture_id": FIXTURE_ID,
            "sessions": [_session(SESSION1, SRC1, SRC1_DIG, BODY1),
                         _session(SESSION2, SRC2, SRC2_DIG, BODY2)]}


def report(reg_filename, reg_digest):
    return {"schema": "o7.b1.report/v1", "fixture_id": FIXTURE_ID,
            "registry_filename": reg_filename, "registry_digest": reg_digest,
            "tasks": [{"task_id": t} for t in ("t-alpha", "t-beta", "t-gamma")],
            "input_digests": {
                SRC1: {"digest": SRC1_DIG, "byte_length": SRC1_BYTES, "status": "OK"},
                SRC2: {"digest": SRC2_DIG, "byte_length": SRC2_BYTES, "status": "OK"},
                PLANE: {"digest": PLANE_DIG, "byte_length": PLANE_BYTES, "status": "OK"}},
            "status": "synthetic"}


def comparison(reg_filename, reg_digest):
    return {"schema": "o7.b1.projection-comparison/v1", "fixture_id": FIXTURE_ID,
            "registry_filename": reg_filename, "registry_digest": reg_digest,
            "tasks": [{"task_id": t} for t in ("t-alpha", "t-beta", "t-gamma")]}


GATE_DESCRIPTIONS = [
    "fixture identity agreement", "registry filename and digest agreement",
    "exact task-set agreement", "questions filename and dual-digest agreement",
    "referenced question existence", "every legacy relation path expanded",
    "no invented explicit relation requirement", "forbidden-stale set equality",
    "gold-derived targets recomputed exactly", "target-status agreement",
    "unknown identifiers or closed-vocabulary values rejected",
]


def _write_json(path, obj):
    data = (json.dumps(obj, sort_keys=True, indent=1, ensure_ascii=False) + "\n").encode("utf-8")
    with open(path, "wb") as fh:
        fh.write(data)
    return data


def _write_yaml(path, obj):
    data = yaml.safe_dump(obj, sort_keys=True, allow_unicode=True).encode("utf-8")
    with open(path, "wb") as fh:
        fh.write(data)
    return data


def _dual(data_bytes, obj):
    return {"artifact_bytes_sha256": _sha(data_bytes),
            "canonical_object_sha256": _sha(canonical_bytes(obj)),
            "byte_length": len(data_bytes)}


def materialize(dest):
    """Write the full synthetic input set + contract into `dest`. Returns dict with
    `contract` path and `input_args` (role:logical=path list)."""
    os.makedirs(dest, exist_ok=True)
    p = lambda name: os.path.join(dest, name)

    reg = registry()
    reg_bytes = _write_yaml(p("tasks.yaml"), reg)
    reg_digest = _sha(reg_bytes)

    files = {"tasks.yaml": (reg, reg_bytes)}
    def wj(name, obj):
        files[name] = (obj, _write_json(p(name), obj))
    def wy(name, obj):
        files[name] = (obj, _write_yaml(p(name), obj))

    wj("gold-state.json", gold_state())
    wy("manifest.yaml", fixture_manifest())
    wy("source-selectors.yaml", source_selectors())
    wy("budget-v0.yaml", budget())
    with open(p("derived-body-1.jsonl"), "wb") as fh:
        fh.write(BODY1)
    with open(p("derived-body-2.jsonl"), "wb") as fh:
        fh.write(BODY2)
    wj("derived-manifest.json", derived_manifest())
    wj("report.json", report("tasks.yaml", reg_digest))
    wj("projection-comparison.json", comparison("tasks.yaml", reg_digest))
    for t in ("t-alpha", "t-beta", "t-gamma"):
        wy(TASK_FILES[t], task_file(t))
        wy(Q_FILES[t], questions_file(t))
        wj("context-%s.json" % t, context(t))

    def dual(name):
        obj, b = files[name]
        return _dual(b, obj)

    gold_dual = dual("gold-state.json")
    bud_dual = dual("budget-v0.yaml")

    def ctx_dual(t):
        obj, b = files["context-%s.json" % t]
        return {"artifact_bytes_sha256": _sha(b),
                "canonical_object_sha256": _sha(canonical_bytes(obj)), "byte_length": len(b)}

    bound_inputs = {
        "gold_state": dict(logical_id="gold-state.json", visibility="public", **gold_dual),
        "fixture_manifest": dict(logical_id="manifest.yaml", visibility="public", **dual("manifest.yaml")),
        "source_selectors": dict(logical_id="source-selectors.yaml", visibility="public", **dual("source-selectors.yaml")),
        "task_registry": dict(logical_id="tasks.yaml", visibility="public", **dual("tasks.yaml")),
        "budget": dict(logical_id="budget-v0.yaml", visibility="public", **bud_dual),
        "r3_1_report": dict(logical_id="report.json", visibility="public", **dual("report.json")),
        "r3_1_projection_comparison": dict(logical_id="projection-comparison.json", visibility="public", **dual("projection-comparison.json")),
        "r3_1_derived_manifest": dict(logical_id="derived-manifest.json", visibility="public", **dual("derived-manifest.json")),
        "task_files": {TASK_FILES[t]: dict(visibility="public", **dual(TASK_FILES[t])) for t in TASK_FILES},
        "question_files": {Q_FILES[t]: dict(visibility="public", **dual(Q_FILES[t])) for t in Q_FILES},
        "task_contexts": {t: dict(visibility="public", **ctx_dual(t)) for t in TASK_FILES},
        "derived_bodies_private": [
            {"session_id": SESSION1, "visibility": "private", "artifact_bytes_sha256": _sha(BODY1),
             "byte_length": len(BODY1), "record_count": 3},
            {"session_id": SESSION2, "visibility": "private", "artifact_bytes_sha256": _sha(BODY2),
             "byte_length": len(BODY2), "record_count": 2}],
    }

    tasks = {}
    for t in ("t-alpha", "t-beta", "t-gamma"):
        entry = {"questions_file": Q_FILES[t]}
        if RELATION_REQS[t]:
            entry["relation_requirements"] = RELATION_REQS[t]
        if FORBIDDEN[t]:
            entry["forbidden_stale_as_current"] = FORBIDDEN[t]
        tasks[t] = entry

    contract = {
        "schema": "o7.b1.evaluation-contract/v1",
        "contract_id": "o7.b1.evaluation-contract/v1",
        "contract_revision": 3, "semantic_revision_from_r2": False, "validation_revision": True,
        "supersedes_contract_revision": 2,
        "supersedes": {"instance": "evaluation-contract-v1-r2.yaml",
                       "artifact_bytes_sha256": "sha256:" + "3" * 64, "r4a1_contract_commit": "0" * 40},
        "provenance_role": {"case_role": "development", "designed_after_observing_case_0002": True,
                            "holdout_evidence": False, "generalization_claim_allowed": False},
        "digest_domains": {
            "canonicalization": {"id": "o7-canonical-json-v0", "definition": "synthetic"},
            "policy": "dual", "gold_state_dual_binding": dict(logical_id="gold-state.json", **gold_dual)},
        "arms": {"full_derived": {"requirement": "required"}, "projection": {"requirement": "required"},
                 "negative_control": {"requirement": "optional"}},
        "arm_semantics": {"missing_required_arm": "UNAVAILABLE", "missing_optional_arm": "NOT_APPLICABLE",
                          "non_applicable_metrics": None, "synthetic_negative_control": "forbidden",
                          "gold_state": "oracle"},
        "family_applicability": {
            "source_compilation": {"full_derived": "required", "projection": "required", "negative_control": "optional"},
            "projection_validity": {"projection": "required", "full_derived": "NOT_APPLICABLE", "negative_control": "NOT_APPLICABLE"},
            "question_observation_support": {"projection": "required", "full_derived": "NOT_APPLICABLE", "negative_control": "NOT_APPLICABLE"},
            "relation_support": {"projection": "required", "full_derived": "NOT_APPLICABLE", "negative_control": "NOT_APPLICABLE"},
            "stale_state_safety": {"projection": "required", "full_derived": "NOT_APPLICABLE", "negative_control": "NOT_APPLICABLE"},
            "negative_control_diagnostics": {"negative_control": "optional"}},
        "full_derived_semantics": "synthetic",
        "source_compilation_requirements_full_derived": [
            "session_matches_derived_manifest", "derived_body_available",
            "derived_byte_length_and_sha256_match", "record_count_reproducible",
            "extractor_identity_and_raw_binding_present"],
        "budget": {"authoritative_artifact": "budget-v0.yaml",
                   "artifact_bytes_sha256": bud_dual["artifact_bytes_sha256"],
                   "canonical_object_sha256": bud_dual["canonical_object_sha256"],
                   "byte_length": bud_dual["byte_length"],
                   "byte_budget": 20000, "record_budget": 32, "unit": "utf8_bytes+records",
                   "counted_representation": {"id": "selector-v0-canonical-record-jsonl",
                                              "definition": "synthetic", "gating": True},
                   "diagnostics": {"rendered_context_sizes": {"gating": False}}},
        "input_closure": {"required_public_structured": sorted(TASK_FILES.values()),
                          "required_private": [SESSION1, SESSION2], "negative_control_body": "none",
                          "raw_source_bodies": "frozen", "fail_closed": "undeclared inputs refuse"},
        "bound_inputs": bound_inputs,
        "tasks": tasks,
        "contract_input_consistency_gates": [
            {"id": "gate-%02d" % (i + 1), "description": d} for i, d in enumerate(GATE_DESCRIPTIONS)],
        "task_dependence": {"all_unordered_pairs": True, "pair_expectations": PAIRS,
                            "not_frozen": "shapes are intent-level"},
        "shape_semantics": {"incomparable": "sets differ, neither subset",
                            "left_strict_superset": "left contains every right item plus more",
                            "unknown_shape": "fail closed"},
        "evaluator_implementation_identity": {
            "manifest_schema": "o7.b1.evaluator-implementation-manifest/v0",
            "manifest_digest_field": "evaluator_implementation_manifest_digest",
            "output_must_record": {"evaluator_entrypoint_sha256": "required",
                                   "evaluator_module_sha256": "required",
                                   "execution_checkout_commit": "required",
                                   "execution_checkout_tree": "required"},
            "note": "synthetic"},
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
    _write_yaml(p("contract.yaml"), contract)

    input_args = [
        "gold_state:gold-state.json=" + p("gold-state.json"),
        "fixture_manifest:manifest.yaml=" + p("manifest.yaml"),
        "source_selectors:source-selectors.yaml=" + p("source-selectors.yaml"),
        "task_registry:tasks.yaml=" + p("tasks.yaml"),
        "budget:budget-v0.yaml=" + p("budget-v0.yaml"),
        "report:report.json=" + p("report.json"),
        "projection_comparison:projection-comparison.json=" + p("projection-comparison.json"),
        "derived_manifest:derived-manifest.json=" + p("derived-manifest.json"),
        "derived_body:%s=%s" % (SESSION1, p("derived-body-1.jsonl")),
        "derived_body:%s=%s" % (SESSION2, p("derived-body-2.jsonl")),
    ]
    for t in ("t-alpha", "t-beta", "t-gamma"):
        input_args.append("task_file:%s=%s" % (TASK_FILES[t], p(TASK_FILES[t])))
        input_args.append("questions_file:%s=%s" % (Q_FILES[t], p(Q_FILES[t])))
        input_args.append("task_context:%s=%s" % (t, p("context-%s.json" % t)))
    return {"contract": p("contract.yaml"), "input_args": input_args, "dest": dest}


if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    base = os.path.join(here, "fixtures", "evaluation-v1-synthetic", "base")
    info = materialize(base)
    print("materialized base synthetic fixture at", base)
