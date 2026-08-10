#!/usr/bin/env python3
"""H2-A — does `task_dependence` have an independent, projection-invariant core?

The rule under suspicion gates on the *shape* of two tasks' selected sets
(equal / superset / incomparable) against a shape frozen into the contract.
Shape is a property of which optional-but-admissible records a particular valid
projector happened to include, so the question is whether any predicate over
these inputs can be:

  1. invariant across the equivalence class of semantically valid projections, and
  2. able to catch a product error no other required axis already owns.

This probe builds that equivalence class by mutating a valid projection only in
ways the contract does not declare mandatory, checks that the other required
axes still pass, and then asks each candidate predicate whether it survived.
No contract, evaluator or historical artifact is modified.
"""
import copy
import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import yaml  # noqa: E402
from o7b1.canonical import canonical_bytes, sha256_hex  # noqa: E402
from o7b1 import evaluate_v1 as ev  # noqa: E402

RB = Path(__file__).resolve().parent.parent
H2 = RB / "holdout" / "h2"
NORMATIVE_AXES = ["projection_validity", "question_observation_support",
                  "relation_support", "stale_state_safety", "contract_input_consistency"]


def sha(b):
    return "sha256:" + sha256_hex(b)


def dual(path):
    b = Path(path).read_bytes()
    o = json.loads(b) if str(path).endswith(".json") else yaml.safe_load(b)
    return {"artifact_bytes_sha256": sha(b),
            "canonical_object_sha256": "sha256:" + sha256_hex(canonical_bytes(o)),
            "byte_length": len(b)}


def selected_ids(ctx):
    return {o["observation_id"] for o in ctx.get("selected", [])}


def shape_of(left, right):
    if left == right:
        return "equal"
    if right <= left:
        return "left_strict_superset"
    if left <= right:
        return "right_strict_superset"
    return "incomparable"


def gold_eligible(gold):
    """Records a projector *may* include: eligible by the frozen B1 rule
    (in force, not an agent claim). Nothing here is required."""
    out = {}
    for o in gold["observations"]:
        if o.get("status") in ("current", "pending") and o.get("authority") != "agent_claim":
            out[o["observation_id"]] = o
    return out


def evaluate(case_dir, contexts, tids, workdir):
    """Run the frozen evaluator over a set of (possibly mutated) contexts."""
    contract = yaml.safe_load((case_dir / "evaluation-contract-v1-r3.yaml").read_bytes())
    reb = copy.deepcopy(contract)
    ctx_paths = {}
    for tid, ctx in contexts.items():
        p = workdir / f"{tid}.context.json"
        p.write_bytes(canonical_bytes(ctx) + b"\n")
        ctx_paths[tid] = p
        reb["bound_inputs"]["task_contexts"][tid] = dict(visibility="public", **dual(p))
    # The arm's own report/comparison bindings, exactly as the holdout run bound
    # them; without these the input-consistency gates fail for a reason that has
    # nothing to do with the projection under test.
    arm_dir = case_dir / "qodec-run" / "qodec-project-arm"
    reb["bound_inputs"]["r3_1_report"] = dict(
        logical_id="report.json", visibility="public", **dual(arm_dir / "report.json"))
    reb["bound_inputs"]["r3_1_projection_comparison"] = dict(
        logical_id="projection-comparison.json", visibility="public",
        **dual(arm_dir / "projection-comparison.json"))
    cpath = workdir / "contract.yaml"
    cpath.write_text(yaml.safe_dump(reb, sort_keys=False, allow_unicode=True, width=120))

    body = workdir / "body.jsonl"
    body.write_bytes((case_dir / "derived-body.jsonl").read_bytes())
    args = ["--contract", str(cpath), "--out", str(workdir / "eval.json"), "--mode", "dev"]

    def inp(role, logical, path):
        args.extend(["--input", f"{role}:{logical}={path}"])
    inp("gold_state", "gold-state.json", case_dir / "gold-state.json")
    inp("fixture_manifest", "manifest.yaml", case_dir / "manifest.yaml")
    inp("source_selectors", "source-selectors.yaml", case_dir / "source-selectors.yaml")
    inp("task_registry", "tasks.yaml", case_dir / "tasks.yaml")
    inp("budget", "budget-v0.yaml", case_dir / "budget-v0.yaml")
    inp("report", "report.json", case_dir / "qodec-run/qodec-project-arm/report.json")
    inp("projection_comparison", "projection-comparison.json",
        case_dir / "qodec-run/qodec-project-arm/projection-comparison.json")
    inp("derived_manifest", "derived-manifest.json", case_dir / "derived-manifest.json")
    for f in sorted(case_dir.iterdir()):
        if f.name.startswith("task-") and f.name.endswith(".yaml"):
            inp("task_file", f.name, f)
        if f.name.startswith("questions-"):
            inp("questions_file", f.name, f)
    for tid in tids:
        inp("task_context", tid, ctx_paths[tid])
    inp("derived_body", "holdout-session", body)
    rc = ev.main(args)
    out = json.loads((workdir / "eval.json").read_text()) if rc == 0 else None
    return rc, out


def main():
    case = H2
    gold = json.loads((case / "gold-state.json").read_bytes())
    eligible = gold_eligible(gold)
    arm = case / "qodec-run/qodec-project-arm/tasks"
    tids = sorted(p.name for p in arm.iterdir())
    base = {t: json.loads((arm / t / "context.json").read_bytes()) for t in tids}

    # Every task's normative obligations, from the frozen question files.
    required = {}
    for f in case.glob("questions-*.yaml"):
        q = yaml.safe_load(f.read_bytes())
        req = set()
        for item in q["questions"]:
            req |= set(item.get("required_observation_ids", []))
        required[q["task_id"]] = req

    print("=== base projection (Q1R) ===")
    sel = {t: selected_ids(base[t]) for t in tids}
    for t in tids:
        print(f"  {t:22} |sel|={len(sel[t])} required={sorted(required.get(t, []))}")
    print(f"  shape({tids[0]},{tids[1]}) = {shape_of(sel[tids[0]], sel[tids[1]])}")

    # --- the metamorphic equivalence class -------------------------------
    # Only mutations the contract does not declare mandatory: optional
    # admissible records added or removed, and record order.
    mutants = {}

    def add_optional(ctx, tid, n):
        """Add eligible, non-required observations this task does not already carry."""
        c = copy.deepcopy(ctx)
        have = selected_ids(c)
        cands = [o for oid, o in sorted(eligible.items())
                 if oid not in have and oid not in required.get(tid, set())]
        for o in cands[:n]:
            c["selected"].append({
                "observation_id": o["observation_id"], "kind": o.get("kind"),
                "status": o.get("status"), "authority": o.get("authority"),
                "topics": o.get("topics", []), "statement": o.get("statement", ""),
                "provenance": o.get("provenance", []),
                "selection_reason": "metamorphic: optional admissible record",
                "selection_score": 1.0,
            })
        c["omitted"] = [x for x in c.get("omitted", [])
                        if x["observation_id"] not in selected_ids(c)]
        return c

    def drop_optional(ctx, tid):
        """Remove an optional record this task's obligations do not demand."""
        c = copy.deepcopy(ctx)
        keep, dropped = [], None
        for row in c["selected"]:
            oid = row["observation_id"]
            if dropped is None and oid not in required.get(tid, set()):
                dropped = oid
                continue
            keep.append(row)
        c["selected"] = keep
        if dropped:
            c.setdefault("omitted", []).append(
                {"observation_id": dropped, "reason": "not_selected"})
            c["omitted"].sort(key=lambda x: x["observation_id"])
        return c

    def reorder(ctx):
        c = copy.deepcopy(ctx)
        c["selected"] = list(reversed(c["selected"]))
        c["relations"] = list(reversed(c.get("relations", [])))
        return c

    left, right = tids[0], tids[1]
    mutants["M1_add_optional_to_left"] = {left: add_optional(base[left], left, 2), right: base[right]}
    mutants["M2_add_optional_to_right"] = {left: base[left], right: add_optional(base[right], right, 3)}
    mutants["M3_drop_optional_from_left"] = {left: drop_optional(base[left], left), right: base[right]}
    mutants["M4_reorder_both"] = {t: reorder(base[t]) for t in tids}
    mutants["M5_saturate_both"] = {t: add_optional(base[t], t, 99) for t in tids}

    print("\n=== metamorphic equivalence class ===")
    rows = []
    with tempfile.TemporaryDirectory() as td:
        for name, ctxs in [("BASE", base)] + list(mutants.items()):
            wd = Path(td) / name
            wd.mkdir()
            rc, out = evaluate(case, ctxs, tids, wd)
            if out is None:
                print(f"  {name:28} evaluator refused (rc={rc})")
                continue
            ax = out["outcome_axes"]
            s = {t: selected_ids(ctxs[t]) for t in tids}
            shp = shape_of(s[left], s[right])
            normative_ok = all(ax.get(a) == "PASS" for a in NORMATIVE_AXES)
            rows.append({"name": name, "shape": shp, "td": ax.get("task_dependence"),
                         "normative_ok": normative_ok,
                         "identical": s[left] == s[right],
                         "axes": {a: ax.get(a) for a in NORMATIVE_AXES}})
            print(f"  {name:28} shape={shp:22} task_dependence={ax.get('task_dependence'):4} "
                  f"other_required_all_PASS={normative_ok}  sets_identical={s[left] == s[right]}")

    valid = [r for r in rows if r["normative_ok"]]
    print("\n=== candidate predicates over the valid class ===")
    print(f"  members of the class that pass all other required axes: {len(valid)}/{len(rows)}")
    shapes = {r["shape"] for r in valid}
    tds = {r["td"] for r in valid}
    idents = {r["identical"] for r in valid}
    print(f"  P1 selected-set SHAPE          -> values {sorted(shapes)}  invariant={len(shapes) == 1}")
    print(f"  P1' current task_dependence    -> values {sorted(tds)}  invariant={len(tds) == 1}")
    print(f"  P2 'sets not identical'        -> values {sorted(idents)}  invariant={len(idents) == 1}")
    print("  P3 'each task's required observations present' -> owned by question_observation_support")
    print("  P4 'required relations present'                -> owned by relation_support")
    print("  P5 'no stale material as current'              -> owned by stale_state_safety")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
