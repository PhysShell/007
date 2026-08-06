"""R4B / R4B.1 qualification suite for the o7.b1.evaluation/v1 evaluator.

SYNTHETIC fixtures only. Each case reports, via `qualify()`:
  fixture / mutation / failure_layer / expected_oracle / observed_oracle /
  operational_exit / evaluation_overall / pass
and distinguishes evaluator failure (refusal) from fixture failure (axis FAIL)
from scoring. Gates 06/07/08 are tested LITERALLY against the frozen contract
(question files as the source of truth), and their exact evidence is recorded.
"""
import copy
import io
import json
import os
import re
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
    with open(path, "wb") as fh:
        fh.write(yaml.safe_dump(obj, sort_keys=True, allow_unicode=True).encode("utf-8"))


def _load(path):
    b = _read(path)
    return json.loads(b) if path.endswith(".json") else yaml.safe_load(b)


def _dump(path, obj):
    (_write_json_like if path.endswith(".json") else _write_yaml_like)(path, obj)


def _dual_struct(path):
    b = _read(path)
    obj = json.loads(b) if path.endswith(".json") else yaml.safe_load(b)
    return {"artifact_bytes_sha256": _sha(b),
            "canonical_object_sha256": _sha(canonical_bytes(obj)), "byte_length": len(b)}


def _patch_contract(dest, fn):
    cpath = os.path.join(dest, "contract.yaml")
    c = yaml.safe_load(_read(cpath))
    fn(c)
    _write_yaml_like(cpath, c)


def _patch_input(dest, name, fn):
    path = os.path.join(dest, name)
    obj = _load(path)
    fn(obj)
    _dump(path, obj)
    return path


def _redual(dest, bound_key, logical=None, name=None, keyed=False, gold=False):
    cpath = os.path.join(dest, "contract.yaml")
    c = yaml.safe_load(_read(cpath))
    d = _dual_struct(os.path.join(dest, name))
    if keyed or bound_key == "task_contexts":
        c["bound_inputs"][bound_key][logical] = dict(visibility="public", **d)
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


def gate_results(contract_path, args):
    """Recompute the consistency gates for evidence + gate-level assertions."""
    ev._INTEGRITY_FAILURES.clear()
    contract = yaml.safe_load(_read(contract_path))
    loaded = ev.load_inputs(contract, args)
    gold = loaded["singleton"].get("gold_state")
    idx = ev._gold_index(gold) if gold else {"obs": {}, "edges": {}, "obs_kinds": set()}
    gates = list(ev.run_gates(contract, loaded, idx))
    if ev._INTEGRITY_FAILURES:  # mirror assemble(): input integrity is a FAIL gate
        gates.append({"gate_id": "gate-input-integrity", "status": "FAIL",
                      "reason": "present-but-contradictory input artifact",
                      "evidence": {"failures": list(ev._INTEGRITY_FAILURES)}})
    return {g["gate_id"]: g for g in gates}


CTX = {"t-alpha": "context-t-alpha.json", "t-beta": "context-t-beta.json",
       "t-gamma": "context-t-gamma.json"}
NCBODY = "nc-body.jsonl"


def _write_nc(dest, digest_ok=True, length_ok=True):
    body = b'{"nc":1}\n{"nc":2}\n'
    with open(os.path.join(dest, NCBODY), "wb") as fh:
        fh.write(body)
    dig = _sha(body) if digest_ok else "sha256:" + "e" * 64
    length = len(body) if length_ok else len(body) + 100
    _patch_input(dest, "manifest.yaml",
                 lambda o: o.__setitem__("negative_control", [{"id": "nc-synth", "digest": dig, "byte_length": length}]))
    _redual(dest, "fixture_manifest", "manifest.yaml", "manifest.yaml")
    return "negative_control:nc-synth=" + os.path.join(dest, NCBODY)


# --------------------------------------------------------------------------- mutations

def m_positive(dest, info):
    return info["input_args"], "dev", {"exit": 0, "overall": "PASS",
                                       "q_support": ("t-alpha", "qa6", 1.0, True)}


def m_budget_record_boundary(dest, info):
    _patch_input(dest, "budget-v0.yaml", lambda o: o.update(record_budget=5))
    _redual(dest, "budget", "budget-v0.yaml", "budget-v0.yaml")
    return info["input_args"], "dev", {"exit": 0, "overall": "PASS"}


def m_present_valid_negative_control(dest, info):
    arg = _write_nc(dest, digest_ok=True, length_ok=True)
    return info["input_args"] + [arg], "dev", {"exit": 0, "overall": "PASS",
                                               "arms": {"negative_control": "AVAILABLE"},
                                               "axes": {"negative_control_diagnostics": "UNAVAILABLE"}}


def m_nc_metadata_only(dest, info):
    _write_nc(dest, digest_ok=True, length_ok=True)  # declared, but no body supplied
    return info["input_args"], "dev", {"exit": 0, "overall": "PASS",
                                       "arms": {"negative_control": "UNAVAILABLE"},
                                       "axes": {"negative_control_diagnostics": "UNAVAILABLE"}}


def m_nc_body_contradictory(dest, info):
    arg = _write_nc(dest, digest_ok=False, length_ok=False)
    return info["input_args"] + [arg], "dev", {"exit": 0, "overall": "PASS",
                                               "arms": {"negative_control": "UNAVAILABLE"}}


# ---- operational refusals
def m_duplicate_mapping(dest, info):
    return info["input_args"] + [info["input_args"][0]], "dev", {"exit": 2, "layer": "input_closure"}


def m_extra_mapping(dest, info):
    return (info["input_args"] + ["task_file:task-extra.yaml=" + os.path.join(dest, "task-alpha.yaml")],
            "dev", {"exit": 2, "layer": "input_closure"})


def m_malformed_input_arg(dest, info):
    return info["input_args"] + ["not-a-valid-mapping"], "dev", {"exit": 2, "layer": "input_closure"}


def m_bad_contract_schema(dest, info):
    _patch_contract(dest, lambda c: c.update(contract_revision=2))
    return info["input_args"], "dev", {"exit": 2, "layer": "contract_consistency"}


def m_blank_jsonl_line(dest, info):
    with open(os.path.join(dest, "derived-body-1.jsonl"), "ab") as fh:
        fh.write(b"\n\n")
    return info["input_args"], "dev", {"exit": 2, "layer": "input_closure"}


def m_malformed_jsonl(dest, info):
    with open(os.path.join(dest, "derived-body-1.jsonl"), "ab") as fh:
        fh.write(b"{not json}\n")
    return info["input_args"], "dev", {"exit": 2, "layer": "input_closure"}


def m_missing_input_path(dest, info):
    os.remove(os.path.join(dest, "report.json"))
    return info["input_args"], "dev", {"exit": 2, "layer": "input_closure"}


def m_dirty_worktree_qualification(dest, info):
    return info["input_args"], "qualification", {"exit": 2, "layer": "implementation_identity",
                                                 "force_dirty": True}


# ---- input integrity
def m_artifact_byte_mismatch(dest, info):
    _patch_input(dest, "report.json", lambda o: o.update(status="tampered"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": {"gate-input-integrity": "FAIL"}}


def m_body_record_count_mismatch(dest, info):
    _patch_contract(dest, lambda c: c["bound_inputs"]["derived_bodies_private"][0]
                    .__setitem__("record_count", 99))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": {"gate-input-integrity": "FAIL"}}


# ---- gate-01 presence
def m_gate01_missing_fixture_id(dest, info):
    _patch_input(dest, "gold-state.json", lambda o: o.pop("fixture_id", None))
    _redual(dest, "gold_state", "gold-state.json", "gold-state.json", gold=True)
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": {"gate-01": "FAIL"}}


def m_gate01_disagree(dest, info):
    _patch_input(dest, "gold-state.json", lambda o: o.update(fixture_id="other"))
    _redual(dest, "gold_state", "gold-state.json", "gold-state.json", gold=True)
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "contract_input_consistency",
                                       "gate": {"gate-01": "FAIL"}}


# ---- gate-02..05
def m_gate02(dest, info):
    _patch_input(dest, "report.json", lambda o: o.update(registry_digest="sha256:" + "0" * 64))
    _redual(dest, "r3_1_report", "report.json", "report.json")
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-02": "FAIL"}}


def m_gate03(dest, info):
    _patch_input(dest, "report.json", lambda o: o.__setitem__("tasks", [{"task_id": "t-alpha"}]))
    _redual(dest, "r3_1_report", "report.json", "report.json")
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-03": "FAIL"}}


def m_gate04(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-beta"].__setitem__("questions_file", "questions-alpha.yaml"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-04": "FAIL"}}


def m_gate05(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-beta"]["relation_requirements"][0]
                    .__setitem__("question", "nonexistent-q"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-05": "FAIL"}}


# ---- gate-06 LITERAL: remove an explicit requirement whose question declares the path
def m_gate06_literal(dest, info):
    # qb1 declares required_relation_paths [c/depends_on]; drop its explicit expansion.
    _patch_contract(dest, lambda c: c["tasks"]["t-beta"].pop("relation_requirements", None))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-06": "FAIL"}}


# ---- gate-07 LITERAL: explicit requirement with no matching question path (existing gold edge, existing question)
def m_gate07_literal(dest, info):
    def g(c):
        c["tasks"]["t-alpha"]["relation_requirements"].append({
            "question": "qa2", "from": "obs-synthetic-a", "kind": "supersedes",
            "direction": "outgoing", "depth": 1, "match": "all", "endpoint_policy": "edge_witness",
            "gold_derived_targets": ["obs-synthetic-stale"], "target_status": ["superseded"]})
    _patch_contract(dest, g)
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-07": "FAIL"}}


# ---- gate-08 LITERAL: omission / addition vs question forbidden set
def m_gate08_omission(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-alpha"].pop("forbidden_stale_as_current", None))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-08": "FAIL"}}


def m_gate08_addition(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-alpha"]["forbidden_stale_as_current"][0]
                    .__setitem__("ids", ["obs-synthetic-stale", "obs-synthetic-b"]))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-08": "FAIL"}}


# ---- gate-09/10/11 (gold checks) + consistency (gold-grounding, renamed)
def m_gate09(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-beta"]["relation_requirements"][0]
                    .__setitem__("gold_derived_targets", ["obs-synthetic-dep-1"]))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-09": "FAIL"}}


def m_gate10(dest, info):
    _patch_contract(dest, lambda c: [rr.__setitem__("target_status", ["current"])
                                     for rr in c["tasks"]["t-alpha"]["relation_requirements"]])
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-10": "FAIL"}}


def m_gate11(dest, info):
    _patch_contract(dest, lambda c: c["tasks"]["t-beta"]["relation_requirements"][0]
                    .__setitem__("from", "obs-does-not-exist"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-11": "FAIL"}}


def m_consistency_forbidden_not_stale(dest, info):
    # contract == question forbidden (gate-08 PASS) but the id is CURRENT in gold:
    # only the separate consistency gate fires.
    _patch_input(dest, "questions-alpha.yaml", lambda o: [
        q.__setitem__("forbidden_stale_as_current", ["obs-synthetic-a"])
        for q in o["questions"] if q["id"] == "qa1"])
    _redual(dest, "question_files", "questions-alpha.yaml", "questions-alpha.yaml", keyed=True)
    _patch_contract(dest, lambda c: c["tasks"]["t-alpha"]["forbidden_stale_as_current"][0]
                    .__setitem__("ids", ["obs-synthetic-a"]))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL",
                                       "gate": {"gate-08": "PASS", "consistency-forbidden-stale-grounded": "FAIL"}}


def m_consistency_extra_gold_edge(dest, info):
    # add a gold edge for a governed (from,kind): contract targets no longer exact -> gate-09
    def g(o):
        o["observations"].append({"observation_id": "obs-synthetic-stale-2", "status": "superseded",
                                  "authority": "agent_claim", "kind": "status", "topics": ["alpha"],
                                  "provenance": [{"source_id": S.SRC1, "source_class": "repo_record",
                                                  "digest": S.SRC1_DIG}], "statement": "s",
                                  "superseded_by": "obs-synthetic-a"})
        o["relations"].append({"from": "obs-synthetic-a", "kind": "supersedes", "to": "obs-synthetic-stale-2"})
    _patch_input(dest, "gold-state.json", g)
    _redual(dest, "gold_state", "gold-state.json", "gold-state.json", gold=True)
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "gate": {"gate-09": "FAIL"}}


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
    _patch_contract(dest, lambda c: c["task_dependence"]["pair_expectations"].append(
        {"left": "t-beta", "right": "t-alpha", "expected_shape": "incomparable", "rationale": "dup"}))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "task_dependence"}


def m_invalid_incomparable(dest, info):
    _patch_contract(dest, lambda c: c["task_dependence"]["pair_expectations"][1]
                    .__setitem__("expected_shape", "incomparable"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "task_dependence"}


def m_invalid_superset(dest, info):
    _patch_contract(dest, lambda c: c["task_dependence"]["pair_expectations"][0]
                    .__setitem__("expected_shape", "left_strict_superset"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "task_dependence"}


def m_budget_only_difference(dest, info):
    _patch_input(dest, CTX["t-gamma"], lambda o: [om.__setitem__("reason", "omitted: byte budget exceeded")
                                                  for om in o["omitted"]])
    _redual(dest, "task_contexts", "t-gamma", CTX["t-gamma"])
    return info["input_args"], "dev", {"exit": 0, "overall": "PASS",
                                       "diff_source": ("t-alpha", "t-gamma", "budget_only")}


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


# ---- question support
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


# ---- stale state
def m_ss_selected_current(dest, info):
    def g(o):
        stale = dict(next(x for x in _load(os.path.join(dest, "gold-state.json"))["observations"]
                          if x["observation_id"] == "obs-synthetic-stale"))
        stale["status"] = "current"
        o["selected"].append(stale)
        o["omitted"] = [x for x in o["omitted"] if x["observation_id"] != "obs-synthetic-stale"]
    _patch_input(dest, CTX["t-alpha"], g)
    _redual(dest, "task_contexts", "t-alpha", CTX["t-alpha"])
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "stale_state_safety"}


# ---- source compilation (named-source evidence)
def _sc_report(dest, mutate):
    _patch_input(dest, "report.json", mutate)
    _redual(dest, "r3_1_report", "report.json", "report.json")


def m_sc_report_status_unavailable(dest, info):
    _sc_report(dest, lambda o: o["input_digests"][S.SRC1].__setitem__("status", "UNAVAILABLE"))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "source_compilation"}


def m_sc_report_digest_mismatch(dest, info):
    _sc_report(dest, lambda o: o["input_digests"][S.SRC1].__setitem__("digest", "sha256:" + "0" * 64))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "source_compilation"}


def m_sc_report_bytes_mismatch(dest, info):
    _sc_report(dest, lambda o: o["input_digests"][S.SRC1].__setitem__("byte_length", 999999))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "source_compilation"}


def m_sc_session_raw_digest_mismatch(dest, info):
    _patch_input(dest, "derived-manifest.json",
                 lambda o: o["sessions"][0].__setitem__("raw_source_digest", "sha256:" + "0" * 64))
    _redual(dest, "r3_1_derived_manifest", "derived-manifest.json", "derived-manifest.json")
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "source_compilation"}


def m_sc_unknown_raw_source(dest, info):
    _patch_input(dest, "derived-manifest.json",
                 lambda o: o["sessions"][0].__setitem__("raw_source", "unknown-raw"))
    _redual(dest, "r3_1_derived_manifest", "derived-manifest.json", "derived-manifest.json")
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "source_compilation"}


def m_sc_requested_omitted_from_report(dest, info):
    _sc_report(dest, lambda o: o["input_digests"].pop(S.SRC1, None))
    return info["input_args"], "dev", {"exit": 0, "overall": "FAIL", "axis": "source_compilation"}


# ---- arm completeness
def m_arm_one_of_two_bodies(dest, info):
    args = [a for a in info["input_args"] if not a.startswith("derived_body:%s=" % S.SESSION2)]
    return args, "dev", {"exit": 0, "overall": "UNAVAILABLE", "arms": {"full_derived": "UNAVAILABLE"}}


def m_arm_one_of_three_contexts(dest, info):
    args = [a for a in info["input_args"] if not a.startswith("task_context:t-gamma=")]
    return args, "dev", {"exit": 0, "overall": "FAIL", "arms": {"projection": "UNAVAILABLE"}}


CASES = [
    ("p1_all_pass", "positive", m_positive),
    ("p5_record_budget_boundary", "projection_validity", m_budget_record_boundary),
    ("p6_present_valid_negative_control", "negative_control", m_present_valid_negative_control),
    ("p7_budget_only_difference", "task_dependence", m_budget_only_difference),
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
    ("g01_missing_fixture_id", "contract_consistency", m_gate01_missing_fixture_id),
    ("g01_fixture_id_disagree", "contract_consistency", m_gate01_disagree),
    ("g02_registry_digest_mismatch", "contract_consistency", m_gate02),
    ("g03_task_set_mismatch", "contract_consistency", m_gate03),
    ("g04_questions_filename_mismatch", "contract_consistency", m_gate04),
    ("g05_referenced_question_missing", "contract_consistency", m_gate05),
    ("g06_literal_unexpanded_path", "contract_consistency", m_gate06_literal),
    ("g07_literal_invented_requirement", "contract_consistency", m_gate07_literal),
    ("g08_forbidden_omission", "contract_consistency", m_gate08_omission),
    ("g08_forbidden_addition", "contract_consistency", m_gate08_addition),
    ("g09_wrong_target", "contract_consistency", m_gate09),
    ("g10_wrong_target_status", "contract_consistency", m_gate10),
    ("g11_unknown_identifier", "contract_consistency", m_gate11),
    ("c_forbidden_not_stale_grounded", "contract_consistency", m_consistency_forbidden_not_stale),
    ("c_extra_gold_edge_gate09", "contract_consistency", m_consistency_extra_gold_edge),
    ("v_self_pair", "task_dependence", m_self_pair),
    ("v_missing_pair", "task_dependence", m_missing_pair),
    ("v_duplicate_pair", "task_dependence", m_duplicate_pair),
    ("td_invalid_incomparable", "task_dependence", m_invalid_incomparable),
    ("td_invalid_superset", "task_dependence", m_invalid_superset),
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
    ("sc_report_status_unavailable", "source_compilation", m_sc_report_status_unavailable),
    ("sc_report_digest_mismatch", "source_compilation", m_sc_report_digest_mismatch),
    ("sc_report_bytes_mismatch", "source_compilation", m_sc_report_bytes_mismatch),
    ("sc_session_raw_digest_mismatch", "source_compilation", m_sc_session_raw_digest_mismatch),
    ("sc_unknown_raw_source", "source_compilation", m_sc_unknown_raw_source),
    ("sc_requested_omitted_from_report", "source_compilation", m_sc_requested_omitted_from_report),
    ("nc_metadata_only_unavailable", "negative_control", m_nc_metadata_only),
    ("nc_body_contradictory", "negative_control", m_nc_body_contradictory),
    ("arm_one_of_two_bodies", "arm_completeness", m_arm_one_of_two_bodies),
    ("arm_one_of_three_contexts", "arm_completeness", m_arm_one_of_three_contexts),
]


def _observed_oracle(rc, output, stderr):
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
        ev._git = lambda a, _o=orig: "M forced-dirty" if a[:1] == ["status"] else _o(a)
        try:
            rc, output, so, se = execute(info["contract"], args, out, mode)
        finally:
            ev._git = orig
    else:
        rc, output, so, se = execute(info["contract"], args, out, mode)
    observed = _observed_oracle(rc, output, se)
    ok, reasons = True, []
    gate_evidence = None

    if rc != expect["exit"]:
        ok = False; reasons.append("exit %d != %d" % (rc, expect["exit"]))
    if expect["exit"] == 2:
        if output is not None or os.path.exists(out):
            ok = False; reasons.append("output left after refusal")
        m = re.search(r"REFUSAL\[([a-z_]+)\]", se)
        if not m or (expect.get("layer") and m.group(1) != expect["layer"]):
            ok = False; reasons.append("refusal layer %s != %s" % (m and m.group(1), expect.get("layer")))
    elif output is None:
        ok = False; reasons.append("no output emitted")
    else:
        if output["overall"] != expect["overall"]:
            ok = False; reasons.append("overall %s != %s" % (output["overall"], expect["overall"]))
        if "axis" in expect and output["outcome_axes"].get(expect["axis"]) != "FAIL":
            ok = False; reasons.append("axis %s not FAIL (%s)" % (expect["axis"], output["outcome_axes"].get(expect["axis"])))
        for a, want in expect.get("arms", {}).items():
            if output["arms"].get(a) != want:
                ok = False; reasons.append("arm %s=%s != %s" % (a, output["arms"].get(a), want))
        for a, want in expect.get("axes", {}).items():
            if output["outcome_axes"].get(a) != want:
                ok = False; reasons.append("axis %s=%s != %s" % (a, output["outcome_axes"].get(a), want))
        if "diff_source" in expect:
            L, R, want = expect["diff_source"]
            row = next((m for m in output["task_dependence_matrix"]
                        if set(m["pair"]) == {L, R}), None)
            if not row or row["difference_source"] != want:
                ok = False; reasons.append("diff_source %s" % (row and row["difference_source"]))
        if "q_support" in expect:
            tid, qid, cov, full = expect["q_support"]
            task = next((t for t in output["tasks"] if t["task_id"] == tid), {})
            q = next((x for x in task.get("question_observation_support", []) if x["question_id"] == qid), None)
            if not q or q["required_observation_coverage"] != cov or q["fully_supported"] != full:
                ok = False; reasons.append("q_support %s/%s=%s" % (tid, qid, q))
        if "gate" in expect:
            try:
                gr = gate_results(info["contract"], args)
                gate_evidence = {gid: {"status": gr.get(gid, {}).get("status"),
                                       "reason": gr.get(gid, {}).get("reason"),
                                       "evidence": gr.get(gid, {}).get("evidence")}
                                 for gid in expect["gate"]}
                for gid, want in expect["gate"].items():
                    if gr.get(gid, {}).get("status") != want:
                        ok = False; reasons.append("gate %s=%s != %s" % (gid, gr.get(gid, {}).get("status"), want))
            except Exception as e:
                ok = False; reasons.append("gate_results error %r" % e)

    return {"fixture": name, "failure_layer": expect.get("layer") or expect.get("axis")
            or (",".join(expect.get("gate", {})) if expect.get("gate") else "positive"),
            "expected_oracle": expect, "observed_oracle": observed,
            "operational_exit": rc, "evaluation_overall": (output or {}).get("overall"),
            "gate_evidence": gate_evidence, "pass": ok, "reasons": reasons}


class QualificationMatrixTest(unittest.TestCase):
    def test_all_cases(self):
        with tempfile.TemporaryDirectory() as tmp:
            failures = [(n, r["reasons"]) for n, _, mut in CASES
                        for r in [run_case(n, mut, tmp)] if not r["pass"]]
            self.assertEqual(failures, [], "cases failed: %r" % failures)


class DeterminismTest(unittest.TestCase):
    def test_byte_identical_and_arg_order(self):
        with tempfile.TemporaryDirectory() as tmp:
            info = S.materialize(os.path.join(tmp, "det"))
            o1, o2 = os.path.join(tmp, "a.json"), os.path.join(tmp, "b.json")
            rc1, _, so1, se1 = execute(info["contract"], info["input_args"], o1)
            rc2, _, so2, se2 = execute(info["contract"], list(reversed(info["input_args"])), o2)
            self.assertEqual((rc1, rc2), (0, 0))
            self.assertEqual(_read(o1), _read(o2), "output not byte-identical across arg order")
            self.assertEqual((so1, se1), (so2, se2))


class NoCaseLiteralTest(unittest.TestCase):
    FORBIDDEN = ["case-0002", "obs-round0-closed", "obs-oracle-topology-constraint",
                 "obs-plane-record", "obs-reviewer-durability", "resume-product-integration"]

    def test_production_code_has_no_case_literals(self):
        for rel in ("evaluate_v1.py", os.path.join("o7b1", "evaluate_v1.py")):
            with open(os.path.join(ev.TOOLS_DIR, rel), encoding="utf-8") as fh:
                text = fh.read()
            for tok in self.FORBIDDEN:
                self.assertNotIn(tok, text, "%s contains forbidden literal %r" % (rel, tok))


class ManifestIdentityTest(unittest.TestCase):
    def _good(self):
        return {"schema": "o7.b1.evaluator-implementation-manifest/v0",
                "files": [{"relative_path": "research/b1-context/tools/evaluate_v1.py",
                           "artifact_bytes_sha256": _sha(b"x")}],
                "entrypoint": "research/b1-context/tools/evaluate_v1.py",
                "python_version": "3.14.0", "dependency_versions": {}, "schema_note": "note"}

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
        m = self._good(); m["files"][0]["artifact_bytes_sha256"] = _sha(b"not-real")
        with self.assertRaises(ev.Refusal):
            ev.verify_manifest_files(m)

    def test_unlisted_import_detected(self):
        declared = [r for r in ev.IMPL_CLOSURE_RELPATHS if not r.endswith("canonical.py")]
        self.assertTrue(any(x.endswith("canonical.py") for x in ev.unlisted_production_imports(declared)))

    def test_real_manifest_builds_clean(self):
        self.assertEqual(ev.build_and_validate_manifest()["entrypoint"], ev.ENTRYPOINT_REL)


class OutputSchemaTest(unittest.TestCase):
    def test_corrupted_output_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            info = S.materialize(os.path.join(tmp, "x"))
            out = os.path.join(tmp, "out.json")
            rc, output, _, _ = execute(info["contract"], info["input_args"], out)
            self.assertEqual(rc, 0)
            corrupted = copy.deepcopy(output); corrupted["overall"] = "MAYBE"
            with self.assertRaises(ev.Refusal):
                ev.validate_output(corrupted)


def qualify(tmproot):
    return [run_case(name, mutate, tmproot) for name, _, mutate in CASES]


if __name__ == "__main__":
    unittest.main()
