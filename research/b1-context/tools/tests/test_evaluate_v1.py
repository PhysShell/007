"""R4B qualification suite for the standalone o7.b1.evaluation/v1 evaluator.

SYNTHETIC fixtures only (see synth_eval_v1). Each case reports, via `qualify()`:
  fixture / mutation / failure_layer / expected_oracle / observed_oracle /
  operational_exit / evaluation_overall / pass
and distinguishes evaluator failure (refusal) from fixture failure (axis FAIL)
from scoring. Positive cases assert overall PASS; adversarial cases assert the
INTENDED oracle fired, not merely a nonzero result; determinism asserts
byte-identical output across two runs and across input-argument order.
"""
import copy
import io
import json
import os
import re
import shutil
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

import yaml

from o7b1 import evaluate_v1 as ev
from o7b1.canonical import canonical_bytes, sha256_hex
import synth_eval_v1 as S


# --------------------------------------------------------------------------- helpers

def _sha(b):
    return "sha256:" + sha256_hex(b)


def _read(path):
    with open(path, "rb") as fh:
        return fh.read()


def _write_json_like(path, obj):
    data = (json.dumps(obj, sort_keys=True, indent=1, ensure_ascii=False) + "\n").encode("utf-8")
    with open(path, "wb") as fh:
        fh.write(data)


def _write_yaml_like(path, obj):
    data = yaml.safe_dump(obj, sort_keys=True, allow_unicode=True).encode("utf-8")
    with open(path, "wb") as fh:
        fh.write(data)


def _load(path):
    b = _read(path)
    return json.loads(b) if path.endswith(".json") else yaml.safe_load(b)


def _dump(path, obj):
    if path.endswith(".json"):
        _write_json_like(path, obj)
    else:
        _write_yaml_like(path, obj)


def _dual_struct(path):
    b = _read(path)
    obj = json.loads(b) if path.endswith(".json") else yaml.safe_load(b)
    return {"artifact_bytes_sha256": _sha(b),
            "canonical_object_sha256": _sha(canonical_bytes(obj)),
            "byte_length": len(b)}


def _record_count(b):
    lines = b.decode("utf-8").split("\n")
    if lines and lines[-1] == "":
        lines = lines[:-1]
    return len(lines)


def _patch_contract(dest, fn):
    cpath = os.path.join(dest, "contract.yaml")
    c = yaml.safe_load(_read(cpath))
    fn(c)
    _write_yaml_like(cpath, c)


def _patch_input(dest, name, fn):
    """Edit an input file AND keep the contract bound digest consistent (isolates
    a structural oracle instead of tripping the input-integrity gate)."""
    path = os.path.join(dest, name)
    obj = _load(path)
    fn(obj)
    _dump(path, obj)
    return path


def _redual(dest, bound_key, logical=None, name=None, keyed=False, gold=False):
    cpath = os.path.join(dest, "contract.yaml")
    c = yaml.safe_load(_read(cpath))
    d = _dual_struct(os.path.join(dest, name))
    if keyed:
        c["bound_inputs"][bound_key][logical] = dict(visibility="public", **d)
    elif bound_key == "task_contexts":
        c["bound_inputs"]["task_contexts"][logical] = dict(visibility="public", **d)
    else:
        c["bound_inputs"][bound_key] = dict(logical_id=logical, visibility="public", **d)
    if gold:
        c["digest_domains"]["gold_state_dual_binding"] = dict(logical_id=logical, **d)
    _write_yaml_like(cpath, c)


def execute(contract, args, out, mode="dev"):
    argv = ["--contract", contract, "--out", out, "--mode", mode]
    for a in args:
        argv += ["--input", a]
    so, se = io.StringIO(), io.StringIO()
    with redirect_stdout(so), redirect_stderr(se):
        rc = ev.main(argv)
    output = json.loads(_read(out)) if (rc == 0 and os.path.exists(out)) else None
    return rc, output, so.getvalue(), se.getvalue()


CTX = {"t-alpha": "context-t-alpha.json", "t-beta": "context-t-beta.json",
       "t-gamma": "context-t-gamma.json"}


# --------------------------------------------------------------------------- mutations
# Each mutate(dest, info) -> (args, mode, expect). expect:
#   {"exit":0,"overall":"PASS"}                              positive
#   {"exit":2,"layer":<refusal layer>}                      operational refusal
#   {"exit":0,"overall":"FAIL","axis":<axis>[, "gate":id]}  fixture/axis failure

def m_positive(dest, info):
    return info["input_args"], "dev", {"exit": 0, "overall": "PASS"}


def m_budget_record_boundary(dest, info):
    p = _patch_input(dest, "budget-v0.yaml", lambda o: o.update(record_budget=5))
    _redual(dest, "budget", "budget-v0.yaml", "budget-v0.yaml")
    return info["input_args"], "dev", {"exit": 0, "overall": "PASS"}


def m_present_negative_control(dest, info):
    def nc(o):
        o["negative_control"] = [{"id": "nc-synth", "digest": "sha256:" + "9" * 64, "byte_length": 10}]
    _patch_input(dest, "manifest.yaml", nc)
    _redual(dest, "fixture_manifest", "manifest.yaml", "manifest.yaml")
    return info["input_args"], "dev", {"exit": 0, "overall": "PASS", "arm_nc": "AVAILABLE",
                                       "axis_nc": "UNAVAILABLE"}


# ---- operational refusals
def m_duplicate_mapping(dest, info):
    args = list(info["input_args"])
    args.append(args[0])  # duplicate gold_state mapping
    return args, "dev", {"exit": 2, "layer": "input_closure"}


def m_extra_mapping(dest, info):
    args = list(info["input_args"])
    args.append("task_file:task-extra.yaml=" + os.path.join(dest, "task-alpha.yaml"))
    return args, "dev", {"exit": 2, "layer": "input_closure"}


def m_malformed_input_arg(dest, info):
    return list(info["input_args"]) + ["not-a-valid-mapping"], "dev", {"exit": 2, "layer": "input_closure"}


def m_bad_contract_schema(dest, info):
    _patch_contract(dest, lambda c: c.update(contract_revision=2))
    return info["input_args"], "dev", {"exit": 2, "layer": "contract_consistency"}


def m_blank_jsonl_line(dest, info):
    with open(os.path.join(dest, "derived-body.jsonl"), "rb") as fh:
        b = fh.read()
    with open(os.path.join(dest, "derived-body.jsonl"), "wb") as fh:
        fh.write(b + b"\n\n")  # a blank line
    return info["input_args"], "dev", {"exit": 2, "layer": "input_closure"}


def m_malformed_jsonl(dest, info):
    with open(os.path.join(dest, "derived-body.jsonl"), "ab") as fh:
        fh.write(b"{not json}\n")
    return info["input_args"], "dev", {"exit": 2, "layer": "input_closure"}


def m_missing_input_path(dest, info):
    os.remove(os.path.join(dest, "report.json"))
    return info["input_args"], "dev", {"exit": 2, "layer": "input_closure"}


def m_dirty_worktree_qualification(dest, info):
    # Force a dirty worktree (deterministically, regardless of the real tree) so
    # qualification mode must refuse. force_dirty is honored by run_case.
    return info["input_args"], "qualification", {"exit": 2, "layer": "implementation_identity",
                                                 "force_dirty": True}


# ---- input integrity (present-but-contradictory -> FAIL, not refusal)
def m_artifact_byte_mismatch(dest, info):
    # edit report bytes WITHOUT re-dualing the contract -> artifact digest mismatch
    _patch_input(dest, "report.json", lambda o: o.update(status="tampered"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-input-integrity"}


def m_body_record_count_mismatch(dest, info):
    # contract declares record_count 99 but the body has 3 -> input-integrity FAIL
    _patch_contract(dest, lambda c: c["bound_inputs"]["derived_bodies_private"][0]
                    .__setitem__("record_count", 99))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-input-integrity"}


# ---- gates 01..11
def m_gate01(dest, info):
    _patch_input(dest, "gold-state.json", lambda o: o.update(fixture_id="other"))
    _redual(dest, "gold_state", "gold-state.json", "gold-state.json", gold=True)
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-01"}


def m_gate02(dest, info):
    _patch_input(dest, "report.json", lambda o: o.update(registry_digest="sha256:" + "0" * 64))
    _redual(dest, "r3_1_report", "report.json", "report.json")
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-02"}


def m_gate03(dest, info):
    _patch_input(dest, "report.json", lambda o: o.__setitem__("tasks", [{"task_id": "t-alpha"}]))
    _redual(dest, "r3_1_report", "report.json", "report.json")
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-03"}


def m_gate04(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-beta"].__setitem__("questions_file", "questions-alpha.yaml"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-04"}


def m_gate05(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-beta"]["relation_requirements"][0]
                    .__setitem__("question", "nonexistent-q"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-05"}


def m_gate06(dest, info):
    def g(o):
        o["observations"].append({"observation_id": "obs-synthetic-stale-2", "status": "superseded",
                                  "authority": "agent_claim", "kind": "status", "topics": ["alpha"],
                                  "provenance": [{"source_id": S.SRC, "source_class": "repo_record",
                                                  "digest": S.SRC_DIGEST}],
                                  "statement": "s", "superseded_by": "obs-synthetic-a"})
        o["relations"].append({"from": "obs-synthetic-a", "kind": "supersedes", "to": "obs-synthetic-stale-2"})
    _patch_input(dest, "gold-state.json", g)
    _redual(dest, "gold_state", "gold-state.json", "gold-state.json", gold=True)
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-06"}


def m_gate07(dest, info):
    def g(c):
        c["tasks"]["t-gamma"]["relation_requirements"] = [{
            "question": "qg1", "from": "obs-synthetic-a", "kind": "part_of", "direction": "outgoing",
            "depth": 1, "match": "all", "endpoint_policy": "edge_witness",
            "gold_derived_targets": ["obs-synthetic-b"], "target_status": ["current"]}]
    _patch_contract(dest, g)
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-07"}


def m_gate08(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-alpha"]["forbidden_stale_as_current"][0]
                    .__setitem__("ids", ["obs-synthetic-a"]))  # a is current -> forbidden-current
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-08"}


def m_gate09(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-beta"]["relation_requirements"][0]
                    .__setitem__("gold_derived_targets", ["obs-synthetic-dep-1"]))  # drop dep-2
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-09"}


def m_gate10(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-alpha"]["relation_requirements"][0]
                    .__setitem__("target_status", ["current"]))  # target is actually superseded
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-10"}


def m_gate11(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-beta"]["relation_requirements"][0]
                    .__setitem__("from", "obs-does-not-exist"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": "gate-11"}


# ---- verifier constraints
def m_self_pair(dest, info):
    _patch_contract(dest, lambda c: c["task_dependence"]["pair_expectations"].append(
        {"left": "t-alpha", "right": "t-alpha", "expected_shape": "incomparable", "rationale": "self"}))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "task_dependence"}


def m_missing_pair(dest, info):
    _patch_contract(dest, lambda c: c["task_dependence"].__setitem__(
        "pair_expectations", c["task_dependence"]["pair_expectations"][:2]))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "task_dependence"}


def m_duplicate_pair(dest, info):
    # Same UNORDERED pair as [0] (t-alpha,t-beta) but a distinct object (swapped +
    # different rationale) so the contract-schema uniqueItems passes; the
    # verifier-pair-complete gate must still catch the duplicate unordered pair.
    _patch_contract(dest, lambda c: c["task_dependence"]["pair_expectations"].append(
        {"left": "t-beta", "right": "t-alpha", "expected_shape": "incomparable", "rationale": "dup"}))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "task_dependence"}


def m_invalid_incomparable(dest, info):
    _patch_contract(dest, lambda c: c["task_dependence"]["pair_expectations"][1]
                    .__setitem__("expected_shape", "incomparable"))  # (alpha,gamma) is actually superset
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "task_dependence"}


def m_invalid_superset(dest, info):
    _patch_contract(dest, lambda c: c["task_dependence"]["pair_expectations"][0]
                    .__setitem__("expected_shape", "left_strict_superset"))  # (alpha,beta) is incomparable
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "task_dependence"}


def m_budget_only_difference(dest, info):
    # every distinguishing observation for (alpha,gamma) omitted by gamma with a budget reason
    def g(o):
        for om in o["omitted"]:
            om["reason"] = "omitted: byte budget exceeded"
    _patch_input(dest, CTX["t-gamma"], g)
    _redual(dest, "task_contexts", "t-gamma", CTX["t-gamma"])
    return info["input_args"], "dev", {"exit": 0, "overall": "PASS", "diff_source": ("t-alpha", "t-gamma", "budget_only")}


# ---- projection validity
def m_pv_agent_claim(dest, info):
    _patch_input(dest, CTX["t-alpha"], lambda o: o["selected"][0].__setitem__("authority", "agent_claim"))
    _redual(dest, "task_contexts", "t-alpha", CTX["t-alpha"])
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "projection_validity"}


def m_pv_superseded_selected(dest, info):
    _patch_input(dest, CTX["t-alpha"], lambda o: o["selected"][3].__setitem__("status", "superseded"))
    _redual(dest, "task_contexts", "t-alpha", CTX["t-alpha"])
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "projection_validity"}


def m_pv_incomplete_provenance(dest, info):
    _patch_input(dest, CTX["t-alpha"], lambda o: o["selected"][1].__setitem__("provenance", []))
    _redual(dest, "task_contexts", "t-alpha", CTX["t-alpha"])
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "projection_validity"}


def m_pv_missing_required_kind(dest, info):
    _patch_input(dest, "task-gamma.yaml",
                 lambda o: o["selectors"].__setitem__("required_kinds", ["decision"]))
    _redual(dest, "task_files", "task-gamma.yaml", "task-gamma.yaml", keyed=True)
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "projection_validity"}


def m_pv_record_overflow(dest, info):
    _patch_input(dest, "budget-v0.yaml", lambda o: o.update(record_budget=1))
    _redual(dest, "budget", "budget-v0.yaml", "budget-v0.yaml")
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "projection_validity"}


def m_pv_byte_overflow(dest, info):
    _patch_input(dest, "budget-v0.yaml", lambda o: o.update(byte_budget=1))
    _redual(dest, "budget", "budget-v0.yaml", "budget-v0.yaml")
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "projection_validity"}


# ---- question observation support
def m_qs_missing_required(dest, info):
    _patch_input(dest, "questions-gamma.yaml",
                 lambda o: o["questions"][0].__setitem__("required_observation_ids", ["obs-synthetic-c"]))
    _redual(dest, "question_files", "questions-gamma.yaml", "questions-gamma.yaml", keyed=True)
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "question_observation_support"}


# ---- relations
def m_rel_missing_edge_witness(dest, info):
    _patch_input(dest, CTX["t-alpha"], lambda o: o.__setitem__("relations", []))
    _redual(dest, "task_contexts", "t-alpha", CTX["t-alpha"])
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "relation_support"}


def m_rel_fabricated_edge(dest, info):
    _patch_input(dest, CTX["t-alpha"], lambda o: o["relations"].append(
        {"from": "obs-synthetic-a", "kind": "supersedes", "to": "obs-synthetic-b"}))
    _redual(dest, "task_contexts", "t-alpha", CTX["t-alpha"])
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "relation_support"}


def m_rel_unmaterialized_current(dest, info):
    def g(o):
        o["selected"] = [s for s in o["selected"] if s["observation_id"] != "obs-synthetic-dep-2"]
        o["omitted"].append({"observation_id": "obs-synthetic-dep-2", "reason": "dropped (synthetic)"})
    _patch_input(dest, CTX["t-beta"], g)
    _redual(dest, "task_contexts", "t-beta", CTX["t-beta"])
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "relation_support"}


# ---- stale state safety
def m_ss_selected_current(dest, info):
    def g(o):
        by = {s["observation_id"]: s for s in o["selected"]}
        stale = dict(next(x for x in _load(os.path.join(dest, "gold-state.json"))["observations"]
                          if x["observation_id"] == "obs-synthetic-stale"))
        stale["status"] = "current"
        o["selected"].append(stale)
        o["omitted"] = [x for x in o["omitted"] if x["observation_id"] != "obs-synthetic-stale"]
    _patch_input(dest, CTX["t-alpha"], g)
    _redual(dest, "task_contexts", "t-alpha", CTX["t-alpha"])
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "stale_state_safety"}


CASES = [
    ("p1_all_pass", "output_schema", m_positive),
    ("p2_edge_witness_stale_endpoint", "relation_support", m_positive),
    ("p3_all_current_materialized", "relation_support", m_positive),
    ("p4_three_task_matrix", "task_dependence", m_positive),
    ("p5_record_budget_boundary", "projection_validity", m_budget_record_boundary),
    ("p6_present_negative_control", "negative_control", m_present_negative_control),
    ("r_duplicate_mapping", "input_closure", m_duplicate_mapping),
    ("r_extra_mapping", "input_closure", m_extra_mapping),
    ("r_malformed_input_arg", "input_closure", m_malformed_input_arg),
    ("r_bad_contract_schema", "contract_consistency", m_bad_contract_schema),
    ("r_blank_jsonl_line", "input_closure", m_blank_jsonl_line),
    ("r_malformed_jsonl", "input_closure", m_malformed_jsonl),
    ("r_missing_input_path", "input_closure", m_missing_input_path),
    ("r_dirty_worktree_qualification", "implementation_identity", m_dirty_worktree_qualification),
    ("f_artifact_byte_mismatch", "contract_consistency", m_artifact_byte_mismatch),
    ("f_body_record_count_mismatch", "contract_consistency", m_body_record_count_mismatch),
    ("g01_fixture_id_mismatch", "contract_consistency", m_gate01),
    ("g02_registry_digest_mismatch", "contract_consistency", m_gate02),
    ("g03_task_set_mismatch", "contract_consistency", m_gate03),
    ("g04_questions_filename_mismatch", "contract_consistency", m_gate04),
    ("g05_referenced_question_missing", "contract_consistency", m_gate05),
    ("g06_dropped_gold_edge", "contract_consistency", m_gate06),
    ("g07_invented_requirement", "contract_consistency", m_gate07),
    ("g08_forbidden_current", "contract_consistency", m_gate08),
    ("g09_wrong_target", "contract_consistency", m_gate09),
    ("g10_wrong_target_status", "contract_consistency", m_gate10),
    ("g11_unknown_identifier", "contract_consistency", m_gate11),
    ("v_self_pair", "task_dependence", m_self_pair),
    ("v_missing_pair", "task_dependence", m_missing_pair),
    ("v_duplicate_pair", "task_dependence", m_duplicate_pair),
    ("td_invalid_incomparable", "task_dependence", m_invalid_incomparable),
    ("td_invalid_superset", "task_dependence", m_invalid_superset),
    ("td_budget_only_difference", "task_dependence", m_budget_only_difference),
    ("pv_agent_claim", "projection_validity", m_pv_agent_claim),
    ("pv_superseded_selected", "projection_validity", m_pv_superseded_selected),
    ("pv_incomplete_provenance", "projection_validity", m_pv_incomplete_provenance),
    ("pv_missing_required_kind", "projection_validity", m_pv_missing_required_kind),
    ("pv_record_overflow", "projection_validity", m_pv_record_overflow),
    ("pv_byte_overflow", "projection_validity", m_pv_byte_overflow),
    ("qs_missing_required", "question_observation_support", m_qs_missing_required),
    ("rel_missing_edge_witness", "relation_support", m_rel_missing_edge_witness),
    ("rel_fabricated_edge", "relation_support", m_rel_fabricated_edge),
    ("rel_unmaterialized_current", "relation_support", m_rel_unmaterialized_current),
    ("ss_selected_as_current", "stale_state_safety", m_ss_selected_current),
]


def _observed_oracle(rc, output, stderr, expect):
    if rc != 0:
        m = re.search(r"REFUSAL\[([a-z_]+)\]", stderr)
        return "refusal:%s" % (m.group(1) if m else "unknown")
    axes = output["outcome_axes"]
    failed = sorted(k for k, v in axes.items() if v == "FAIL")
    return "overall:%s;fail_axes:%s" % (output["overall"], ",".join(failed) or "-")


def run_case(name, mutate, tmproot):
    dest = os.path.join(tmproot, name)
    info = S.materialize(dest)
    args, mode, expect = mutate(dest, info)
    out = os.path.join(dest, "out", "evaluation-v1.json")
    if expect.get("force_dirty"):
        orig = ev._git
        def fake(a, _o=orig):
            return "M forced-dirty" if a[:1] == ["status"] else _o(a)
        ev._git = fake
        try:
            rc, output, so, se = execute(info["contract"], args, out, mode)
        finally:
            ev._git = orig
    else:
        rc, output, so, se = execute(info["contract"], args, out, mode)
    observed = _observed_oracle(rc, output, se, expect)
    ok = True
    reasons = []
    if rc != expect["exit"]:
        ok = False; reasons.append("exit %d != %d" % (rc, expect["exit"]))
    if expect["exit"] == 2:
        if output is not None or os.path.exists(out):
            ok = False; reasons.append("output file left after refusal")
        m = re.search(r"REFUSAL\[([a-z_]+)\]", se)
        if not m or (expect.get("layer") and m.group(1) != expect["layer"]):
            ok = False; reasons.append("refusal layer %s != %s" % (m and m.group(1), expect.get("layer")))
    else:
        if output is None:
            ok = False; reasons.append("no output emitted")
        else:
            if output["overall"] != expect["overall"]:
                ok = False; reasons.append("overall %s != %s" % (output["overall"], expect["overall"]))
            if "axis" in expect and output["outcome_axes"].get(expect["axis"]) != "FAIL":
                ok = False; reasons.append("axis %s not FAIL (%s)" % (
                    expect["axis"], output["outcome_axes"].get(expect["axis"])))
            if expect.get("arm_nc") and output["arms"]["negative_control"] != expect["arm_nc"]:
                ok = False; reasons.append("nc arm %s" % output["arms"]["negative_control"])
            if expect.get("axis_nc") and output["outcome_axes"]["negative_control_diagnostics"] != expect["axis_nc"]:
                ok = False; reasons.append("nc axis %s" % output["outcome_axes"]["negative_control_diagnostics"])
            if "diff_source" in expect:
                L, R, want = expect["diff_source"]
                row = next((m for m in output["task_dependence_matrix"]
                            if m["pair"] == [L, R] or m["pair"] == [R, L]), None)
                if not row or row["difference_source"] != want:
                    ok = False; reasons.append("diff_source %s" % (row and row["difference_source"]))
    return {"fixture": name, "expected_layer": expect.get("layer") or expect.get("axis") or "positive",
            "expected_oracle": expect, "observed_oracle": observed,
            "operational_exit": rc, "evaluation_overall": (output or {}).get("overall"),
            "pass": ok, "reasons": reasons}


class QualificationMatrixTest(unittest.TestCase):
    def test_all_cases(self):
        with tempfile.TemporaryDirectory() as tmp:
            failures = []
            for name, layer, mutate in CASES:
                rec = run_case(name, mutate, tmp)
                if not rec["pass"]:
                    failures.append((name, rec["reasons"]))
            self.assertEqual(failures, [], "qualification cases failed: %r" % failures)


class DeterminismTest(unittest.TestCase):
    def test_byte_identical_and_arg_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            info = S.materialize(os.path.join(tmp, "det"))
            o1 = os.path.join(tmp, "a", "evaluation-v1.json")
            o2 = os.path.join(tmp, "b", "evaluation-v1.json")
            rc1, _, so1, se1 = execute(info["contract"], info["input_args"], o1)
            shuffled = list(reversed(info["input_args"]))
            rc2, _, so2, se2 = execute(info["contract"], shuffled, o2)
            self.assertEqual((rc1, rc2), (0, 0))
            self.assertEqual(_read(o1), _read(o2), "output not byte-identical across arg order")
            self.assertEqual((so1, se1), (so2, se2), "stdout/stderr differ")


class NoCaseLiteralTest(unittest.TestCase):
    FORBIDDEN = ["case-0002", "obs-round0-closed", "obs-oracle-topology-constraint",
                 "obs-plane-record", "obs-reviewer-durability", "resume-product-integration"]

    def test_production_code_has_no_case_literals(self):
        for rel in ("evaluate_v1.py", os.path.join("o7b1", "evaluate_v1.py")):
            path = os.path.join(ev.TOOLS_DIR, rel)
            with open(path, encoding="utf-8") as fh:
                text = fh.read()
            for tok in self.FORBIDDEN:
                self.assertNotIn(tok, text, "%s contains forbidden literal %r" % (rel, tok))


class ManifestIdentityTest(unittest.TestCase):
    def _good(self):
        return {"schema": "o7.b1.evaluator-implementation-manifest/v0",
                "files": [{"relative_path": "research/b1-context/tools/evaluate_v1.py",
                           "artifact_bytes_sha256": _sha(b"x")}],
                "entrypoint": "research/b1-context/tools/evaluate_v1.py",
                "python_version": "3.14.0", "dependency_versions": {},
                "schema_note": "note"}

    def test_unsafe_path_rejected(self):
        m = self._good(); m["files"][0]["relative_path"] = "/etc/passwd"; m["entrypoint"] = "/etc/passwd"
        with self.assertRaises(ev.Refusal):
            ev.check_manifest_schema_and_mechanics(m)

    def test_traversal_path_rejected(self):
        m = self._good(); m["files"][0]["relative_path"] = "a/../b"; m["entrypoint"] = "a/../b"
        with self.assertRaises(ev.Refusal):
            ev.check_manifest_schema_and_mechanics(m)

    def test_duplicate_path_same_digest_rejected(self):
        m = self._good(); m["files"] = m["files"] + [dict(m["files"][0])]
        with self.assertRaises(ev.Refusal):
            ev.check_manifest_schema_and_mechanics(m)

    def test_duplicate_path_different_digest_rejected(self):
        m = self._good()
        m["files"] = [dict(m["files"][0], artifact_bytes_sha256=_sha(b"x")),
                      dict(m["files"][0], artifact_bytes_sha256=_sha(b"y"))]
        with self.assertRaises(ev.Refusal):
            ev.check_manifest_schema_and_mechanics(m)

    def test_entrypoint_absent_rejected(self):
        m = self._good(); m["entrypoint"] = "research/b1-context/tools/o7b1/evaluate_v1.py"
        with self.assertRaises(ev.Refusal):
            ev.check_manifest_schema_and_mechanics(m)

    def test_listed_file_digest_mismatch_rejected(self):
        m = self._good()
        m["files"][0]["artifact_bytes_sha256"] = _sha(b"definitely-not-the-real-bytes")
        with self.assertRaises(ev.Refusal):
            ev.verify_manifest_files(m)

    def test_unlisted_import_detected(self):
        # declare a closure that omits canonical.py, which IS imported at runtime.
        declared = [r for r in ev.IMPL_CLOSURE_RELPATHS if not r.endswith("canonical.py")]
        self.assertTrue(any(x.endswith("canonical.py")
                            for x in ev.unlisted_production_imports(declared)))

    def test_real_manifest_builds_clean(self):
        m = ev.build_and_validate_manifest()
        self.assertEqual(m["entrypoint"], ev.ENTRYPOINT_REL)


class OutputSchemaTest(unittest.TestCase):
    def test_corrupted_output_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            info = S.materialize(os.path.join(tmp, "x"))
            out = os.path.join(tmp, "out.json")
            rc, output, _, _ = execute(info["contract"], info["input_args"], out)
            self.assertEqual(rc, 0)
            corrupted = copy.deepcopy(output)
            corrupted["overall"] = "MAYBE"  # not in the closed enum
            with self.assertRaises(ev.Refusal):
                ev.validate_output(corrupted)


# ---- qualification-record emitter (used by run_r4b_qualification.py) ------------

def qualify(tmproot):
    records = [run_case(name, mutate, tmproot) for name, _, mutate in CASES]
    return records


if __name__ == "__main__":
    unittest.main()
