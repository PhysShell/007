#!/usr/bin/env python3
"""analyze_ablation.py — per-question factorial analysis of the alias-fold run.

Reads a completed run directory and emits a hash-verified canonical artifact:
per-question truth tables over the six arms, an evidence-based factor verdict per
question (main effects, interaction, framing, lexical guard), a candidate-policy
gate against the diagnostic success criteria, and a quality→integrity→tokens→
latency Pareto frontier. Pure/offline; no model, no qodec.

    python3 analyze_ablation.py --run runs-l2/l2-ablation-7b-v1 \
        --out analysis/l2-qwen2.5-coder-7b-alias-fold-ablation-v1
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

ARMS = ["R", "I", "M", "F", "MF", "GF"]
ENCODED = ["I", "M", "F", "MF", "GF"]


def _sha(s: str) -> str:
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def _dump(obj) -> str:
    return json.dumps(obj, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def _sig(r):
    return (r["correct"], r.get("format_compliant", not r["malformed"]),
            tuple(sorted(set(r.get("alias_leaks") or []))),
            tuple(sorted(set(r.get("invalid_identifiers") or []))))


def load(run_dir: Path):
    records = [json.loads(l) for l in (run_dir / "records.jsonl").read_text(encoding="utf-8").splitlines() if l.strip()]
    manifest = json.loads((run_dir / "manifest.json").read_text(encoding="utf-8"))
    return records, manifest


def index(records):
    idx = {}
    for r in records:
        idx.setdefault((r["case"], r["question"], r["arm"]), {})[r["repeat"]] = r
    return idx


def cell(idx, case, q, arm):
    reps = idx.get((case, q, arm), {})
    if not reps:
        return None
    prim = reps.get(0) or reps[min(reps)]
    stable = len({_sig(reps[k]) for k in reps}) == 1 if len(reps) > 1 else None
    return {
        "correct": prim["correct"], "stable": stable, "n_repeats": len(reps),
        "format_compliant": prim.get("format_compliant", not prim["malformed"]),
        "alias_leaks": len(prim.get("alias_leaks") or []),
        "invalid_identifiers": len(prim.get("invalid_identifiers") or []),
        "prompt_tokens": prim.get("server_prompt_tokens"),
        "latency_ms": round(prim.get("total_ms") or 0.0, 0),
        "answer": prim.get("answer_parsed"),
        "finish_reason": prim.get("finish_reason"),
    }


def _ok(c):
    """Correct AND not-unstable (a flip across repeats is not a clean pass)."""
    return bool(c) and c["correct"] and c["stable"] is not False


def question_verdict(cells: dict) -> list[str]:
    """Evidence rules — each requires the truth table of THIS question."""
    v = []
    R, I, M, F, MF, GF = (_ok(cells.get(a)) for a in ARMS)
    if R and not I:
        v.append("%q1 identity framing regression (R pass, I fail)")
    if cells.get("I") and not I:
        v.append("container/brief framing implicated (I fail)")
    if I and not M:
        v.append("alias main effect (I pass, M fail)")
    if I and not F:
        v.append("structural main effect (I pass, F fail)")
    if I and M and F and not MF:
        v.append("alias × structural interaction (I/M/F pass, MF fail)")
    if (cells.get("MF") and not MF) and GF:
        v.append("lexical aliasing implicated (MF fail, GF pass)")
    if I and M and F and MF:
        v.append("no factor breaks this question (all arms pass)")
    return v or ["unresolved (no clean factor pattern)"]


def factorial(records, manifest):
    idx = index(records)
    losses = [tuple(s.split(":", 1)) for s in manifest["losses"]]
    controls = [tuple(s.split(":", 1)) for s in manifest["controls"]]
    weak = set(manifest.get("weakly_matched", []))

    questions = []
    for (case, q) in losses + controls:
        cells = {a: cell(idx, case, q, a) for a in ARMS}
        questions.append({
            "case": case, "question": q,
            "role": "loss" if (case, q) in losses else "control",
            "weakly_matched": f"{case}:{q}" in weak,
            "cells": cells,
            "verdict": question_verdict(cells) if (case, q) in losses else None,
        })
    return questions


def _r_tokens(cells):
    # R has no local token count; use its server prompt tokens as the baseline.
    return (cells["R"] or {}).get("prompt_tokens")


def candidate_gate(questions):
    """A policy may advance to a full L2 rerun only if it rescues 5/5 losses,
    regresses 0 controls, leaks 0 aliases, keeps invalid-id delta ≤ 0 vs R, and
    saves tokens vs R. Reported per encoded arm."""
    losses = [q for q in questions if q["role"] == "loss"]
    controls = [q for q in questions if q["role"] == "control"]
    out = {}
    for arm in ENCODED:
        rescued = sum(1 for q in losses if _ok(q["cells"].get(arm)))
        # a control "regresses" if R was ok but this arm is not
        regressions = [f"{q['case']}:{q['question']}" for q in controls
                       if _ok(q["cells"].get("R")) and not _ok(q["cells"].get(arm))]
        leaks = sum((q["cells"].get(arm) or {}).get("alias_leaks", 0) for q in questions)
        inv_delta = sum((q["cells"].get(arm) or {}).get("invalid_identifiers", 0) for q in questions) \
            - sum((q["cells"].get("R") or {}).get("invalid_identifiers", 0) for q in questions)
        # token savings vs R over questions where both present
        arm_tok = [c["prompt_tokens"] for q in questions if (c := q["cells"].get(arm)) and c["prompt_tokens"]]
        r_tok = [_r_tokens(q["cells"]) for q in questions if _r_tokens(q["cells"])]
        saves = (sum(r_tok) - sum(arm_tok)) if arm_tok and r_tok else None
        passes = (rescued == len(losses) and not regressions and leaks == 0
                  and inv_delta <= 0 and (saves or 0) > 0)
        out[arm] = {"losses_rescued": f"{rescued}/{len(losses)}", "control_regressions": regressions,
                    "alias_leaks": leaks, "invalid_id_delta_vs_R": inv_delta,
                    "token_savings_vs_R": saves, "advances_to_full_rerun": passes}
    return out


def pareto(questions):
    """quality → integrity → tokens → latency, over all 10 questions."""
    rows = []
    for arm in ARMS:
        cells = [q["cells"].get(arm) for q in questions]
        present = [c for c in cells if c]
        correct = sum(1 for c in present if _ok(c))
        leaks = sum(c["alias_leaks"] for c in present)
        invalid = sum(c["invalid_identifiers"] for c in present)
        toks = [c["prompt_tokens"] for c in present if c["prompt_tokens"]]
        lats = [c["latency_ms"] for c in present if c["latency_ms"]]
        rows.append({"arm": arm, "correct": correct, "alias_leaks": leaks,
                     "invalid_identifiers": invalid,
                     "mean_prompt_tokens": round(sum(toks) / len(toks), 1) if toks else None,
                     "mean_latency_ms": round(sum(lats) / len(lats), 0) if lats else None})
    # Pareto order: more correct, fewer leaks, fewer invalid, fewer tokens, lower latency
    order = sorted(rows, key=lambda r: (-r["correct"], r["alias_leaks"], r["invalid_identifiers"],
                                        r["mean_prompt_tokens"] or 1e9, r["mean_latency_ms"] or 1e9))
    return {"rows": rows, "ranked": [r["arm"] for r in order]}


def build_files(run_dir: Path, records, manifest) -> dict:
    questions = factorial(records, manifest)
    gate = candidate_gate(questions)
    par = pareto(questions)
    advancing = [a for a, g in gate.items() if g["advances_to_full_rerun"]]
    conclusion = (f"candidate policy for full 23-question rerun: {', '.join(advancing)}"
                  if advancing else
                  "no arm rescues 5/5 losses without control regressions — current qodec "
                  "notation remains rejected for blind application")

    factorial_json = {"questions": questions, "candidate_gate": gate, "pareto": par,
                      "conclusion": conclusion,
                      "model": manifest.get("model_requested"),
                      "qodec_binary_sha256": manifest.get("qodec_binary_sha256")}

    files = {}
    files["factorial.json"] = _dump(factorial_json)
    mpath = run_dir / "manifest.json"
    files["manifest.json"] = mpath.read_text(encoding="utf-8") if mpath.exists() else _dump(manifest)
    rpath = run_dir / "records.jsonl"
    files["records.jsonl"] = rpath.read_text(encoding="utf-8") if rpath.exists() \
        else "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in records)
    if (run_dir / "preflight.json").exists():
        files["preflight.json"] = (run_dir / "preflight.json").read_text(encoding="utf-8")
    for q in questions:
        files[f"per-question/{q['case']}__{q['question']}.json"] = _dump(q)
    files["README.md"] = _render(factorial_json, manifest)
    files["_meta"] = factorial_json
    return files


def _tt_row(arm, c):
    if not c:
        return f"| {arm} | – | – | – | – | – | – |"
    st = {True: "stable", False: "UNSTABLE", None: "n=1"}[c["stable"]]
    return (f"| {arm} | {'✓' if c['correct'] else '✗'} | {st} | {'ok' if c['format_compliant'] else 'BAD'} "
            f"| {c['alias_leaks']} | {c['invalid_identifiers']} | {c['prompt_tokens']} |")


def _render(fj, manifest) -> str:
    o = ["# Level-2 alias × structural ablation — Qwen2.5-Coder-7B", "",
         f"Same model/tokenizer/determinism as the canonical record "
         f"(model `{manifest.get('model_requested')}`, qodec "
         f"`{manifest.get('qodec_binary_sha256','')[:12]}`). Six arms in one run.", "",
         "Arms: **R** raw+brief · **I** identity (framing only) · **M** alias-only · "
         "**F** structural-only · **MF** squeeze · **GF** guarded squeeze.", "",
         f"**Conclusion: {fj['conclusion']}**", ""]
    o += ["## per-question truth tables", ""]
    for q in fj["questions"]:
        tag = "LOSS" if q["role"] == "loss" else "control" + (" (weak match)" if q["weakly_matched"] else "")
        o.append(f"### {q['case']} / {q['question']} — {tag}")
        o += ["| arm | correct | stable | format | leaks | invalid | ptok |",
              "|-----|---------|--------|--------|-------|---------|------|"]
        for arm in ARMS:
            o.append(_tt_row(arm, q["cells"].get(arm)))
        if q["verdict"]:
            o.append("")
            o.append("verdict: " + "; ".join(q["verdict"]))
        o.append("")
    o += ["## candidate-policy gate (advance to full rerun?)", "",
          "| arm | rescued | control regressions | leaks | invalid Δ vs R | tok savings vs R | advances |",
          "|-----|---------|---------------------|-------|----------------|------------------|----------|"]
    for arm in ENCODED:
        g = fj["candidate_gate"][arm]
        o.append(f"| {arm} | {g['losses_rescued']} | {len(g['control_regressions'])} | {g['alias_leaks']} "
                 f"| {g['invalid_id_delta_vs_R']} | {g['token_savings_vs_R']} | "
                 f"{'YES' if g['advances_to_full_rerun'] else 'no'} |")
    o += ["", "## Pareto (quality → integrity → tokens → latency)", "",
          "ranked: " + " > ".join(fj["pareto"]["ranked"]), "",
          "```json", json.dumps(fj["pareto"]["rows"], indent=2), "```"]
    return "\n".join(o) + "\n"


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--run", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    args = p.parse_args()
    run_dir = args.run if args.run.is_absolute() else (Path(__file__).resolve().parent / args.run)
    out_dir = args.out if args.out.is_absolute() else (Path(__file__).resolve().parent / args.out)

    records, manifest = load(run_dir)
    files = build_files(run_dir, records, manifest)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "per-question").mkdir(exist_ok=True)
    for rel, text in files.items():
        if rel == "_meta":
            continue
        (out_dir / rel).write_text(text, encoding="utf-8")
    sha = [f"{_sha(t)}  ./{rel}" for rel, t in sorted(files.items()) if rel != "_meta"]
    (out_dir / "SHA256SUMS").write_text("\n".join(sha) + "\n", encoding="utf-8")
    print(f"wrote {out_dir}\nconclusion: {files['_meta']['conclusion']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
