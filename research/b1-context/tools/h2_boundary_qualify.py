#!/usr/bin/env python3
"""Qualify the rev4 evaluator boundary.

Three things must hold together, and each of them is a way the repair could be
wrong:

  * the historical witnesses come out right -- H2's false shape failure is gone,
    H3's genuine budget failure is NOT (a repair that turns everything green is
    not a repair);
  * the outcome is invariant across a metamorphic class of valid projections
    built by varying only what the contract does not declare mandatory;
  * the negative controls still fail, through the axes that own them -- rev4
    must not have opened a "different projection = valid" escape hatch.

No rev3 artifact is modified. The rev3 evaluator is executed unchanged to
reproduce its historical statements alongside the new ones.
"""
import copy
import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import yaml  # noqa: E402
from jsonschema import Draft202012Validator  # noqa: E402
from o7b1 import evaluate_v1 as ev3  # noqa: E402
from o7b1 import evaluate_v2 as ev4  # noqa: E402
from o7b1.canonical import canonical_bytes, sha256_hex  # noqa: E402
import h2a_invariance_probe as probe  # noqa: E402

RB = Path(__file__).resolve().parent.parent
GATING = ev4.GATING_AXES


def sha(b):
    return "sha256:" + sha256_hex(b)


def dual(path):
    b = Path(path).read_bytes()
    o = json.loads(b) if str(path).endswith(".json") else yaml.safe_load(b)
    return {"artifact_bytes_sha256": sha(b),
            "canonical_object_sha256": "sha256:" + sha256_hex(canonical_bytes(o)),
            "byte_length": len(b)}


def run(case_dir, contexts, tids, workdir, revision, arm_subdir="qodec-run/qodec-project-arm"):
    """Evaluate one set of contexts under rev3 or rev4."""
    name = "evaluation-contract-v1-r3.yaml" if revision == 3 else "evaluation-contract-v1-r4.yaml"
    contract = yaml.safe_load((case_dir / name).read_bytes())
    reb = copy.deepcopy(contract)
    ctx_paths = {}
    for tid, ctx in contexts.items():
        p = workdir / f"{tid}.context.json"
        p.write_bytes(canonical_bytes(ctx) + b"\n")
        ctx_paths[tid] = p
        reb["bound_inputs"]["task_contexts"][tid] = dict(visibility="public", **dual(p))
    arm = case_dir / arm_subdir
    reb["bound_inputs"]["r3_1_report"] = dict(
        logical_id="report.json", visibility="public", **dual(arm / "report.json"))
    reb["bound_inputs"]["r3_1_projection_comparison"] = dict(
        logical_id="projection-comparison.json", visibility="public",
        **dual(arm / "projection-comparison.json"))
    cpath = workdir / "contract.yaml"
    cpath.write_text(yaml.safe_dump(reb, sort_keys=False, allow_unicode=True, width=120))

    body = workdir / "body.jsonl"
    body.write_bytes((case_dir / "derived-body.jsonl").read_bytes())
    out = workdir / "eval.json"
    args = ["--contract", str(cpath), "--out", str(out), "--mode", "dev"]

    def inp(role, logical, path):
        args.extend(["--input", f"{role}:{logical}={path}"])
    inp("gold_state", "gold-state.json", case_dir / "gold-state.json")
    inp("fixture_manifest", "manifest.yaml", case_dir / "manifest.yaml")
    inp("source_selectors", "source-selectors.yaml", case_dir / "source-selectors.yaml")
    inp("task_registry", "tasks.yaml", case_dir / "tasks.yaml")
    inp("budget", "budget-v0.yaml", case_dir / "budget-v0.yaml")
    inp("report", "report.json", arm / "report.json")
    inp("projection_comparison", "projection-comparison.json", arm / "projection-comparison.json")
    inp("derived_manifest", "derived-manifest.json", case_dir / "derived-manifest.json")
    for f in sorted(case_dir.iterdir()):
        if f.name.startswith("task-") and f.name.endswith(".yaml"):
            inp("task_file", f.name, f)
        if f.name.startswith("questions-"):
            inp("questions_file", f.name, f)
    for tid in tids:
        inp("task_context", tid, ctx_paths[tid])
    inp("derived_body", "holdout-session", body)
    rc = (ev3.main(args) if revision == 3 else ev4.main(args))
    return rc, (json.loads(out.read_text()) if rc == 0 and out.exists() else None)


def case_contexts(case_dir, arm_subdir="qodec-run/qodec-project-arm"):
    arm = case_dir / arm_subdir / "tasks"
    tids = sorted(p.name for p in arm.iterdir())
    return tids, {t: json.loads((arm / t / "context.json").read_bytes()) for t in tids}


def main():
    result = {"record": "o7.b1.h2-evaluator-boundary-qualification/v1", "checks": {}}

    # ---- schema validation of every rev4 contract --------------------------
    schema = json.loads((RB / "schema" / "evaluation-contract-v1-r4.schema.json").read_text())
    validator = Draft202012Validator(schema)
    schema_rows = {}
    for case in [RB / "fixtures" / "case-0002", RB / "holdout" / "h1",
                 RB / "holdout" / "h2", RB / "holdout" / "h3"]:
        p = case / "evaluation-contract-v1-r4.yaml"
        errs = sorted(validator.iter_errors(yaml.safe_load(p.read_bytes())),
                      key=lambda e: list(e.path))
        schema_rows[case.name] = "PASS" if not errs else f"FAIL: {errs[0].message[:120]}"
    result["checks"]["contract_schema_validation"] = schema_rows
    print("=== rev4 contract schema validation ===")
    for k, v in schema_rows.items():
        print(f"  {k:12} {v}")

    # ---- historical witnesses ---------------------------------------------
    print("\n=== historical witnesses (rev3 reproduced, rev4 decided) ===")
    witnesses = {}
    with tempfile.TemporaryDirectory() as td:
        for case in [RB / "holdout" / "h1", RB / "holdout" / "h2", RB / "holdout" / "h3"]:
            tids, ctxs = case_contexts(case)
            w3 = Path(td) / f"{case.name}-r3"
            w3.mkdir(parents=True)
            _rc3, r3 = run(case, ctxs, tids, w3, 3)
            w4 = Path(td) / f"{case.name}-r4"
            w4.mkdir(parents=True)
            _rc4, r4 = run(case, ctxs, tids, w4, 4)
            row = {
                "rev3_overall": r3["overall"] if r3 else None,
                "rev3_failed_axes": sorted(k for k, v in (r3 or {}).get("outcome_axes", {}).items()
                                           if v == "FAIL"),
                "rev4_overall": r4["overall"] if r4 else None,
                "rev4_failed_gating_axes": sorted(
                    k for k, v in (r4 or {}).get("outcome_axes", {}).items()
                    if v == "FAIL" and k in GATING),
                "rev4_topology": [t["observed_topology"] for t in (r4 or {}).get("selected_set_topology", [])],
            }
            witnesses[case.name] = row
            print(f"  {case.name}: rev3 {str(row['rev3_overall']):11} {row['rev3_failed_axes']}"
                  f"   ->   rev4 {str(row['rev4_overall']):11} {row['rev4_failed_gating_axes']}"
                  f"  topology={row['rev4_topology']}")
    result["checks"]["historical_witnesses"] = witnesses

    # ---- metamorphic projection invariance --------------------------------
    print("\n=== metamorphic projection invariance (rev4 overall must not move) ===")
    case = RB / "holdout" / "h2"
    tids, base = case_contexts(case)
    gold = json.loads((case / "gold-state.json").read_bytes())
    eligible = probe.gold_eligible(gold)
    required = {}
    for f in case.glob("questions-*.yaml"):
        q = yaml.safe_load(f.read_bytes())
        req = set()
        for item in q["questions"]:
            req |= set(item.get("required_observation_ids", []))
        required[q["task_id"]] = req

    def add_optional(ctx, tid, n):
        c = copy.deepcopy(ctx)
        have = probe.selected_ids(c)
        for o in [o for oid, o in sorted(eligible.items())
                  if oid not in have and oid not in required.get(tid, set())][:n]:
            c["selected"].append({
                "observation_id": o["observation_id"], "kind": o.get("kind"),
                "status": o.get("status"), "authority": o.get("authority"),
                "topics": o.get("topics", []), "statement": o.get("statement", ""),
                "provenance": o.get("provenance", []),
                "selection_reason": "metamorphic: optional admissible record",
                "selection_score": 1.0})
        c["omitted"] = [x for x in c.get("omitted", []) if x["observation_id"] not in probe.selected_ids(c)]
        return c

    def drop_optional(ctx, tid):
        c = copy.deepcopy(ctx)
        keep, dropped = [], None
        for row in c["selected"]:
            if dropped is None and row["observation_id"] not in required.get(tid, set()):
                dropped = row["observation_id"]
                continue
            keep.append(row)
        c["selected"] = keep
        if dropped:
            c.setdefault("omitted", []).append({"observation_id": dropped, "reason": "not_selected"})
            c["omitted"].sort(key=lambda x: x["observation_id"])
        return c

    def reorder(ctx):
        c = copy.deepcopy(ctx)
        c["selected"] = list(reversed(c["selected"]))
        c["relations"] = list(reversed(c.get("relations", [])))
        return c

    left, right = tids[0], tids[1]
    mutants = {
        "BASE": base,
        "M1_add_optional_left": {left: add_optional(base[left], left, 2), right: base[right]},
        "M2_add_optional_right": {left: base[left], right: add_optional(base[right], right, 3)},
        "M3_drop_optional_left": {left: drop_optional(base[left], left), right: base[right]},
        "M4_reorder_both": {t: reorder(base[t]) for t in tids},
        "M5_saturate_both": {t: add_optional(base[t], t, 99) for t in tids},
    }
    meta_rows, r4_overalls, r3_overalls, topologies = {}, set(), set(), set()
    with tempfile.TemporaryDirectory() as td:
        for name, ctxs in mutants.items():
            w = Path(td) / name
            w.mkdir(parents=True)
            _rc, r4 = run(case, ctxs, tids, w, 4)
            w3 = Path(td) / (name + "-r3")
            w3.mkdir(parents=True)
            _rc3, r3 = run(case, ctxs, tids, w3, 3)
            topo = [t["observed_topology"] for t in (r4 or {}).get("selected_set_topology", [])]
            gating_ok = all((r4 or {}).get("outcome_axes", {}).get(a) == "PASS" for a in GATING)
            meta_rows[name] = {"rev4_overall": r4["overall"] if r4 else None,
                               "rev3_overall": r3["overall"] if r3 else None,
                               "topology": topo, "all_gating_pass": gating_ok}
            r4_overalls.add(meta_rows[name]["rev4_overall"])
            r3_overalls.add(meta_rows[name]["rev3_overall"])
            topologies.update(topo)
            print(f"  {name:24} rev3={str(meta_rows[name]['rev3_overall']):8} "
                  f"rev4={str(meta_rows[name]['rev4_overall']):8} topology={topo}")
    invariant = len(r4_overalls) == 1
    print(f"  rev4 overall values across the class: {sorted(r4_overalls)}  invariant={invariant}")
    print(f"  rev3 overall values across the class: {sorted(r3_overalls)}  "
          f"(shape-sensitive: {len(r3_overalls) > 1 or 'FAIL' in r3_overalls})")
    print(f"  observed topologies varied over: {sorted(topologies)}")
    result["checks"]["metamorphic_projection_invariance"] = {
        "rows": meta_rows, "rev4_invariant": invariant,
        "rev4_overall_values": sorted(r4_overalls),
        "topologies_observed": sorted(topologies),
        "note": ("topology varies across the class while the rev4 verdict does not; that is the "
                 "repair, stated as a measurement"),
    }

    # ---- negative controls -------------------------------------------------
    print("\n=== negative controls (must still FAIL, through owning axes) ===")
    nc = {}
    with tempfile.TemporaryDirectory() as td:
        def check(label, ctxs, expect_axis):
            w = Path(td) / label
            w.mkdir(parents=True)
            _rc, r4 = run(case, ctxs, tids, w, 4)
            axes = (r4 or {}).get("outcome_axes", {})
            failed = sorted(k for k, v in axes.items() if v == "FAIL" and k in GATING)
            ok = (r4 or {}).get("overall") == "FAIL" and expect_axis in failed
            nc[label] = {"overall": (r4 or {}).get("overall"), "failed_gating_axes": failed,
                         "expected_axis": expect_axis, "ok": ok}
            print(f"  {label:34} overall={str(nc[label]['overall']):8} failed={failed}  ok={ok}")

        # 1. remove a required observation
        c = copy.deepcopy(base)
        req_left = sorted(required.get(left, []))[0]
        c[left]["selected"] = [r for r in c[left]["selected"] if r["observation_id"] != req_left]
        c[left].setdefault("omitted", []).append({"observation_id": req_left, "reason": "not_selected"})
        check("NC1_required_observation_removed", c, "question_observation_support")

        # 2. remove a required relation witness
        c = copy.deepcopy(base)
        for t in tids:
            c[t]["relations"] = []
        check("NC2_relation_witnesses_removed", c, "relation_support")

        # 3. materialize forbidden stale state as current
        c = copy.deepcopy(base)
        stale = [o for o in gold["observations"] if o.get("status") == "superseded"]
        if stale:
            o = stale[0]
            c[left]["selected"].append({
                "observation_id": o["observation_id"], "kind": o.get("kind"),
                "status": "current", "authority": o.get("authority"),
                "topics": o.get("topics", []), "statement": o.get("statement", ""),
                "provenance": o.get("provenance", []),
                "selection_reason": "negative control: stale presented as current",
                "selection_score": 1.0})
            c[left]["omitted"] = [x for x in c[left].get("omitted", [])
                                  if x["observation_id"] != o["observation_id"]]
            check("NC3_stale_presented_as_current", c, "stale_state_safety")

        # 4. include an ineligible authoritative record (agent claim)
        c = copy.deepcopy(base)
        inelig = [o for o in gold["observations"] if o.get("authority") == "agent_claim"]
        if inelig:
            o = inelig[0]
            c[left]["selected"].append({
                "observation_id": o["observation_id"], "kind": o.get("kind"),
                "status": o.get("status"), "authority": o.get("authority"),
                "topics": o.get("topics", []), "statement": o.get("statement", ""),
                "provenance": o.get("provenance", []),
                "selection_reason": "negative control: ineligible record",
                "selection_score": 1.0})
            c[left]["omitted"] = [x for x in c[left].get("omitted", [])
                                  if x["observation_id"] != o["observation_id"]]
            check("NC4_ineligible_record_included", c, "projection_validity")

        # 5. fabricate a relation
        c = copy.deepcopy(base)
        c[left]["relations"] = list(c[left].get("relations", [])) + [
            {"from": "obs-h2-authority", "kind": "supports", "to": "obs-does-not-exist"}]
        check("NC5_fabricated_relation", c, "projection_validity")

        # 6. exceed the record budget
        budget = yaml.safe_load((case / "budget-v0.yaml").read_bytes())
        c = copy.deepcopy(base)
        c[left] = add_optional(c[left], left, 99)
        extra = budget["record_budget"] + 5 - len(c[left]["selected"])
        for i in range(max(0, extra)):
            c[left]["selected"].append(dict(c[left]["selected"][0],
                                            observation_id=f"obs-h2-authority",
                                            selection_reason=f"negative control: budget {i}"))
        check("NC6_record_budget_exceeded", c, "projection_validity")

    result["checks"]["negative_controls"] = nc

    out = RB / "results" / "h2-evaluator-boundary-r4"
    out.mkdir(parents=True, exist_ok=True)
    (out / "qualification-evidence.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n")
    print("\nevidence written:", out / "qualification-evidence.json")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
