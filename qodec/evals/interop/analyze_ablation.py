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

ARMS = ["R", "I", "M", "F", "MF", "VG"]
ENCODED = ["I", "M", "F", "MF", "VG"]
_RELABEL = {"GF": "VG"}   # the Commit-G run labelled the guarded arm GF; VG is the honest name
STRUCTURAL_CODECS = {"fold", "grep", "diag", "tmpl", "toon", "raw", "identity"}


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
    return normalize(records, manifest)


def normalize(records, manifest):
    """Relabel the as-run arm codes to their honest names (GF → VG) so the whole
    artifact is self-consistent; the underlying data is unchanged."""
    for r in records:
        r["arm"] = _RELABEL.get(r["arm"], r["arm"])
    manifest = dict(manifest)
    manifest["arms"] = [_RELABEL.get(a, a) for a in manifest.get("arms", ARMS)]
    if "arm_receipts" in manifest:
        manifest["arm_receipts"] = {
            "|".join([k.split("|")[0], _RELABEL.get(k.split("|")[1], k.split("|")[1])]): v
            for k, v in manifest["arm_receipts"].items()}
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
        "final_codec": (prim.get("arm_receipt") or {}).get("format_codec"),
        "answer": prim.get("answer_parsed"),
        "finish_reason": prim.get("finish_reason"),
    }


def _ok(c):
    """Correct AND not-unstable (a flip across repeats is not a clean pass)."""
    return bool(c) and c["correct"] and c["stable"] is not False


def question_verdict(cells: dict) -> dict:
    """Causal verdict from THIS question's truth table — deliberately
    conservative. VG differs from MF in BOTH the structural shelf and the mine
    guard, so a `VG pass` is NEVER used as causal evidence here (it is reported
    separately as candidate-policy evidence). A production-stage failure that M
    and F alone survive cannot be split into stage-1 vs stage-2 without the
    stage-matched S/SM/SG arms (Commit I), so it is marked *unresolved*."""
    R, I, M, F, MF, VG = (_ok(cells.get(a)) for a in ARMS)
    causal = "unresolved (no clean factor pattern)"
    if cells.get("I") and not I:
        causal = "container/%q1 framing implicated (I fail)"
    elif I and not M:
        causal = "alias main effect confirmed (I pass, M fail)"
    elif I and not F:
        causal = "structural main effect confirmed (I pass, F fail)"
    elif I and M and F and not MF:
        # Only the full production pipeline breaks it. Whether the culprit is the
        # production structural stage (toon/diag/tmpl) or the mine over it is
        # unresolvable without stage-matched arms.
        mf_codec = (cells.get("MF") or {}).get("final_codec")
        if mf_codec in STRUCTURAL_CODECS:
            causal = "production-stage effect unresolved (only MF fails; its outer codec is structural)"
        else:
            causal = "production-stage / mine interaction unresolved (only MF fails; MF mined)"
    elif I and M and F and MF:
        causal = "no factor breaks this question (all arms pass)"
    # candidate-policy evidence (NOT causal): which restricted arms rescue it.
    rescuers = [a for a in ("F", "VG") if _ok(cells.get(a))] if (cells.get("MF") and not MF) else []
    return {"causal": causal,
            "candidate_policy_rescue": rescuers,
            "note": ("VG/F rescue is candidate-policy evidence only; VG changes the shelf AND "
                     "guards the mine, so it is not a clean lexical-guard attribution"
                     if rescuers else None)}


def closure_verdict(cells: dict) -> dict:
    """Stage-matched causal verdict for the closure run (R/S/SM/SG/V/VG). SM and
    SG share the exact production stage-1 and differ ONLY in the mine guard, so
    an SM-fail / SG-pass flip IS a clean stage-2 lexical-mining attribution."""
    R, S, SM, SG, V, VG = (_ok(cells.get(a)) for a in ("R", "S", "SM", "SG", "V", "VG"))
    have = {a: cells.get(a) is not None for a in ("R", "S", "SM", "SG", "V", "VG")}
    if have["S"] and not S:
        causal = "production structural stage itself causes the loss (S fails)"
        if (V or VG):
            causal += "; V/VG pass → rescue comes from dropping the diag/tmpl/toon shelf, not the guard"
    elif S and have["SM"] and not SM and SG:
        causal = "stage-2 lexical mining effect confirmed (S pass, SM fail, SG pass; stage-1 matched)"
    elif have["SM"] and not SM and have["SG"] and not SG and S:
        causal = "non-guarded mining behaviour not covered by the lexical classes (S pass, SM & SG fail)"
    elif have["SM"] and not SM and not SG and not S:
        causal = "guard in stage 2 cannot repair stage-1 damage (S, SM, SG all fail)"
    elif have["SM"] and SM:
        causal = "no factor breaks this question (production squeeze passes)"
    else:
        causal = "unresolved (no clean stage pattern)"
    rescuers = [a for a in ("SG", "V", "VG") if _ok(cells.get(a))] if (cells.get("SM") and not SM) else []
    return {"causal": causal, "candidate_policy_rescue": rescuers,
            "stage1_matched_guard_pair": "SM vs SG (production stage-1 shared)"}


def factorial(records, manifest):
    idx = index(records)
    arms = manifest.get("arms", ARMS)
    is_closure = "SM" in arms
    losses = [tuple(s.split(":", 1)) for s in manifest["losses"]]
    controls = [tuple(s.split(":", 1)) for s in manifest["controls"]]
    weak = set(manifest.get("weakly_matched", []))

    questions = []
    for (case, q) in losses + controls:
        cells = {a: cell(idx, case, q, a) for a in arms}
        verdict = None
        if (case, q) in losses:
            verdict = closure_verdict(cells) if is_closure else question_verdict(cells)
        questions.append({
            "case": case, "question": q,
            "role": "loss" if (case, q) in losses else "control",
            "weakly_matched": f"{case}:{q}" in weak,
            "cells": cells, "verdict": verdict,
        })
    return questions


def _r_tokens(cells):
    # R has no local token count; use its server prompt tokens as the baseline.
    return (cells["R"] or {}).get("prompt_tokens")


def candidate_gate(questions, encoded=None):
    """A policy may advance to a full L2 rerun only if it rescues 5/5 losses,
    regresses 0 controls, leaks 0 aliases, keeps invalid-id delta ≤ 0 vs R, and
    saves tokens (positive total AND median) vs R. Reported per encoded arm."""
    losses = [q for q in questions if q["role"] == "loss"]
    controls = [q for q in questions if q["role"] == "control"]
    encoded = encoded or ENCODED
    out = {}
    for arm in encoded:
        rescued = sum(1 for q in losses if _ok(q["cells"].get(arm)))
        # a control "regresses" if R was ok but this arm is not
        regressions = [f"{q['case']}:{q['question']}" for q in controls
                       if _ok(q["cells"].get("R")) and not _ok(q["cells"].get(arm))]
        leaks = sum((q["cells"].get(arm) or {}).get("alias_leaks", 0) for q in questions)
        inv_delta = sum((q["cells"].get(arm) or {}).get("invalid_identifiers", 0) for q in questions) \
            - sum((q["cells"].get("R") or {}).get("invalid_identifiers", 0) for q in questions)
        # token savings vs R over questions where both present
        sv = _token_savings(questions, arm)
        passes = (rescued == len(losses) and not regressions and leaks == 0
                  and inv_delta <= 0 and (sv["total_vs_R"] or 0) > 0
                  and (sv["median_per_question_vs_R"] or 0) > 0)
        out[arm] = {"losses_rescued": f"{rescued}/{len(losses)}", "control_regressions": regressions,
                    "alias_leaks": leaks, "invalid_id_delta_vs_R": inv_delta,
                    "token_savings_vs_R": sv, "advances_to_full_rerun": passes}
    return out


def _median(xs):
    xs = sorted(xs)
    n = len(xs)
    if not n:
        return None
    return xs[n // 2] if n % 2 else (xs[n // 2 - 1] + xs[n // 2]) / 2


def _token_savings(questions, arm):
    """Per-question token savings vs R: total, mean, median, and percent."""
    per_q, r_total, a_total = [], 0, 0
    for q in questions:
        a, r = q["cells"].get(arm), q["cells"].get("R")
        if a and r and a["prompt_tokens"] and r["prompt_tokens"]:
            per_q.append(r["prompt_tokens"] - a["prompt_tokens"])
            r_total += r["prompt_tokens"]
            a_total += a["prompt_tokens"]
    if not per_q:
        return {"total_vs_R": None, "mean_per_question_vs_R": None,
                "median_per_question_vs_R": None, "percent_vs_R": None}
    return {"total_vs_R": sum(per_q),
            "mean_per_question_vs_R": round(sum(per_q) / len(per_q), 1),
            "median_per_question_vs_R": _median(per_q),
            "percent_vs_R": round(100 * (r_total - a_total) / r_total, 1) if r_total else None}


def priority_ranking(questions, arms=None):
    """Priority ranking (NOT a Pareto frontier — this is a lexicographic ordering
    of dominated candidates): quality → integrity → tokens → latency, over all
    10 questions."""
    rows = []
    for arm in (arms or ARMS):
        present = [c for c in (q["cells"].get(arm) for q in questions) if c]
        toks = [c["prompt_tokens"] for c in present if c["prompt_tokens"]]
        lats = [c["latency_ms"] for c in present if c["latency_ms"]]
        rows.append({"arm": arm, "correct": sum(1 for c in present if _ok(c)),
                     "alias_leaks": sum(c["alias_leaks"] for c in present),
                     "invalid_identifiers": sum(c["invalid_identifiers"] for c in present),
                     "mean_prompt_tokens": round(sum(toks) / len(toks), 1) if toks else None,
                     "mean_latency_ms": round(sum(lats) / len(lats), 0) if lats else None})
    order = sorted(rows, key=lambda r: (-r["correct"], r["alias_leaks"], r["invalid_identifiers"],
                                        r["mean_prompt_tokens"] or 1e9, r["mean_latency_ms"] or 1e9))
    return {"rows": rows, "ranked": [r["arm"] for r in order]}


def _byte_identical(stage_receipts) -> list:
    """Per case, arm pairs whose final artifact is byte-identical — so 'both pass'
    is not mistaken for two independent results (e.g. F == VG when the guard left
    the fold/grep artifact untouched)."""
    receipts = stage_receipts.get("receipts", {})
    by_case: dict = {}
    for key, st in receipts.items():
        case, arm = key.split("|")
        by_case.setdefault(case, {})[arm] = st["final"]["artifact_sha256"]
    out = []
    for case, shas in sorted(by_case.items()):
        arms = [a for a in ARMS if a in shas]
        for i, a in enumerate(arms):
            for b in arms[i + 1:]:
                if shas[a] and shas[a] == shas[b]:
                    out.append({"case": case, "arms": [a, b]})
    return out


def _closure_conclusion(questions, gate, advancing):
    losses = [q for q in questions if q["role"] == "loss"]
    stage1 = [q for q in losses if "production structural stage" in q["verdict"]["causal"]]
    mining = [q for q in losses if "stage-2 lexical mining effect confirmed" in q["verdict"]["causal"]]
    sg_ok = gate.get("SG", {}).get("advances_to_full_rerun")
    vg_ok = gate.get("VG", {}).get("advances_to_full_rerun")
    if sg_ok:
        head = ("SG (stage-matched guarded mining) rescues 5/5 and passes the gate — the guard "
                "in the mine stage is the viable candidate for a full 23-question rerun.")
    elif vg_ok:
        head = ("lexical guard alone is insufficient; the viable candidate is the simplified "
                "structural shelf (VG: fold/grep + guarded mine), which passes the gate.")
    else:
        head = ("no stage-matched policy rescues 5/5 without control regressions — current qodec "
                "notation remains rejected for blind application.")
    conclusion = (f"{head} By stage: {len(mining)}/5 losses are a confirmed stage-2 lexical-mining "
                  f"effect (S pass, SM fail, SG pass); {len(stage1)}/5 fail already at the production "
                  f"structural stage (S fails), where the stage-2 guard cannot help. Gate winners: "
                  f"{', '.join(advancing) if advancing else 'none'}. Full rerun is a separate decision.")
    guard_iso = {"stage1_matched_pair_present": True,
                 "note": ("SM and SG share production's exact stage-1 (squeeze_stage1) and differ "
                          "only in the mine guard, so SM-fail / SG-pass IS a clean stage-2 lexical-"
                          "mining attribution.")}
    return conclusion, guard_iso


def build_files(run_dir: Path, records, manifest, stage_receipts=None) -> dict:
    arms = manifest.get("arms", ARMS)
    is_closure = "SM" in arms
    encoded = [a for a in arms if a != "R"]
    questions = factorial(records, manifest)
    gate = candidate_gate(questions, encoded)
    ranking = priority_ranking(questions, arms)
    advancing = [a for a, g in gate.items() if g["advances_to_full_rerun"]]

    if is_closure:
        conclusion, guard_iso = _closure_conclusion(questions, gate, advancing)
    else:
        confirmed = [q for q in questions if q["role"] == "loss" and "confirmed" in q["verdict"]["causal"]]
        unresolved = [q for q in questions if q["role"] == "loss" and "unresolved" in q["verdict"]["causal"]]
        conclusion = (
            f"{len(confirmed)}/5 losses have a confirmed factor (alias main effect); "
            f"{len(unresolved)}/5 are production-stage effects unresolved without a "
            f"stage-matched S/SM/SG comparison (Commit I). Candidate policies that pass the "
            f"gate: {', '.join(advancing) if advancing else 'none'} — candidate-policy evidence, "
            f"NOT a causal claim that a lexical guard fixed production squeeze. "
            f"Blind production squeeze remains rejected.")
        guard_iso = {"stage1_matched_pair_present": False,
                     "note": ("VG's shelf (fold/grep) differs from MF's — no stage-1-matched pair "
                              "here; the guard's effect is NOT isolated. Commit I isolates it.")}

    byte_identical = _byte_identical(stage_receipts) if stage_receipts else []
    factorial_json = {"questions": questions, "candidate_gate": gate,
                      "priority_ranking": ranking, "byte_identical_arms": byte_identical,
                      "conclusion": conclusion, "guard_isolation": guard_iso,
                      "model": manifest.get("model_requested"),
                      "qodec_binary_sha256": manifest.get("qodec_binary_sha256")}

    files = {}
    files["factorial.json"] = _dump(factorial_json)
    if stage_receipts is not None:
        files["realized-stage-receipts.json"] = _dump(stage_receipts)
    # Normalized manifest (GF→VG relabelled) so the artifact is self-consistent.
    # Its arm_receipts are the run's INTENT-based receipts; the realized applied
    # stages live in realized-stage-receipts.json.
    man_out = dict(manifest)
    man_out["_relabel_note"] = "arm GF (as-run) → VG (fold-grep-guarded); arm_receipts are intent-based, see realized-stage-receipts.json"
    files["manifest.json"] = _dump(man_out)
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
    arms = manifest.get("arms", ARMS)
    encoded = [a for a in arms if a != "R"]
    is_closure = "SM" in arms
    if is_closure:
        armline = ("Arms: **R** raw+brief · **S** production stage-1 only · **SM** production "
                   "squeeze · **SG** stage-1 + guarded mine (SM/SG share stage-1, differ only in "
                   "the guard) · **V** fold/grep structural · **VG** fold/grep + guarded mine.")
    else:
        armline = ("Arms: **R** raw+brief · **I** identity (framing only) · **M** mine over raw "
                   "(alias only) · **F** structural fold/grep only · **MF** production squeeze · "
                   "**VG** fold-grep-guarded (fold/grep shelf + guarded mine).")
    o = [f"# Level-2 {'stage-matched closure' if is_closure else 'alias × structural'} ablation — Qwen2.5-Coder-7B", "",
         f"Same model/tokenizer/determinism as the canonical record "
         f"(model `{manifest.get('model_requested')}`, qodec "
         f"`{manifest.get('qodec_binary_sha256','')[:12]}`). Six arms in one run.", "",
         armline, "",
         "> " + fj["guard_isolation"]["note"], "",
         f"**Conclusion: {fj['conclusion']}**", ""]
    o += ["## per-question truth tables", ""]
    for q in fj["questions"]:
        tag = "LOSS" if q["role"] == "loss" else "control" + (" (weak match)" if q["weakly_matched"] else "")
        o.append(f"### {q['case']} / {q['question']} — {tag}")
        o += ["| arm | correct | stable | format | leaks | invalid | ptok |",
              "|-----|---------|--------|--------|-------|---------|------|"]
        for arm in arms:
            o.append(_tt_row(arm, q["cells"].get(arm)))
        if q["verdict"]:
            o.append("")
            o.append("causal verdict: " + q["verdict"]["causal"])
            if q["verdict"]["candidate_policy_rescue"]:
                o.append(f"candidate-policy rescue (non-causal): {', '.join(q['verdict']['candidate_policy_rescue'])}")
        o.append("")
    o += ["## candidate-policy gate (may a policy advance to a full rerun?)", "",
          "Candidate-policy evidence only — passing this gate is NOT a causal claim.", "",
          "| arm | rescued | control regr. | leaks | invalid Δ vs R | tok save total / median / % vs R | advances |",
          "|-----|---------|---------------|-------|----------------|----------------------------------|----------|"]
    for arm in encoded:
        g = fj["candidate_gate"][arm]
        s = g["token_savings_vs_R"]
        o.append(f"| {arm} | {g['losses_rescued']} | {len(g['control_regressions'])} | {g['alias_leaks']} "
                 f"| {g['invalid_id_delta_vs_R']} | {s['total_vs_R']} / {s['median_per_question_vs_R']} / "
                 f"{s['percent_vs_R']}% | {'YES' if g['advances_to_full_rerun'] else 'no'} |")
    if fj.get("byte_identical_arms"):
        o += ["", "## byte-identical arms (not independent results)", ""]
        for bi in fj["byte_identical_arms"]:
            o.append(f"- {bi['case']}: {' == '.join(bi['arms'])} (same artifact bytes)")
    o += ["", "## priority ranking (lexicographic: quality → integrity → tokens → latency)", "",
          "Not a Pareto frontier — a priority ordering of the arms.", "",
          "ranked: " + " > ".join(fj["priority_ranking"]["ranked"]), "",
          "```json", json.dumps(fj["priority_ranking"]["rows"], indent=2), "```"]
    return "\n".join(o) + "\n"


def _realized_stage_receipts(manifest, l1_run: Path):
    """Realized codec-stage receipts per (case, arm), recomputed offline from the
    L1 payloads. Verifies each arm's recomputed artifact SHA matches the run's
    recorded receipt (so the rename/rebuild did not change the codecs)."""
    from bench import ablation_policies as ap, qodec
    meter = (manifest.get("tokenizer") or {}).get("meter") or "o200k"
    qb = str(qodec.binary())
    cases = sorted({k.split("|")[0] for k in manifest.get("arm_receipts", {})})
    recorded = manifest.get("arm_receipts", {})
    arms = manifest.get("arms", ARMS)
    receipts, mismatches = {}, []
    for case in cases:
        matches = sorted(l1_run.glob(f"*/cases/{case}/*/transformed.txt")) \
            or sorted(l1_run.glob(f"cases/{case}/*/transformed.txt"))
        raw = matches[0].read_text(encoding="utf-8")
        for arm in arms:
            st = ap.realized_stages(arm, raw, meter, qb)
            receipts[f"{case}|{arm}"] = st
            rec = recorded.get(f"{case}|{arm}")
            if rec and rec.get("artifact_sha256") and st["final"]["artifact_sha256"] != rec["artifact_sha256"]:
                mismatches.append(f"{case}|{arm}")
    return {"receipts": receipts,
            "consistency": {"recomputed_matches_run": not mismatches, "mismatches": mismatches}}


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--run", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--l1-run", type=Path, default=Path(__file__).resolve().parent / "results" / "rtk-codegraph-clap-v1",
                   help="L1 run with the raw tool payloads, for realized stage receipts")
    args = p.parse_args()
    run_dir = args.run if args.run.is_absolute() else (Path(__file__).resolve().parent / args.run)
    out_dir = args.out if args.out.is_absolute() else (Path(__file__).resolve().parent / args.out)

    records, manifest = load(run_dir)
    stage_receipts = None
    try:
        stage_receipts = _realized_stage_receipts(manifest, args.l1_run)
    except Exception as exc:  # noqa: BLE001 — receipts are best-effort provenance
        print(f"realized stage receipts skipped: {exc}")
    files = build_files(run_dir, records, manifest, stage_receipts)
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
