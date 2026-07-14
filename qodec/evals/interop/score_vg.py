#!/usr/bin/env python3
"""score_vg.py — the full-L2 candidate gate for a VG (fold-grep-guarded) run.

Reuses score_reader.analyze for retention / stable losses / parity / integrity,
then adds the promotion-specific gate: token savings (mean AND median > 0),
malformed and invalid-identifier deltas vs raw+brief, exact roundtrip per case,
and the realized per-case shelf / guarded-mining / VG==V observations. Emits the
verdict without ever moving a threshold.

    python3 score_vg.py --run runs-l2/l2-vg-7b-v1
"""

from __future__ import annotations

import argparse
import json
import statistics
from pathlib import Path

import score_reader as sr
from bench import ablation_policies as ap, qodec

VG_CODEC = "fold-grep-guarded"


def _median(xs):
    return statistics.median(xs) if xs else None


def _primary(records):
    idx = {}
    for r in records:
        if r["repeat"] == 0:
            idx[(r["case"], r["question"], r["arm"])] = r
    return idx


def analyze_vg(meta, records, run_dir) -> dict:
    a = sr.analyze(meta, records)
    prim = _primary(records)
    questions = sorted({(c, q) for (c, q, _a) in prim})

    # token savings vs raw+brief, per unique question (primary).
    sav = []
    for (c, q) in questions:
        rb, eb = prim.get((c, q, "raw+brief")), prim.get((c, q, "encoded+brief"))
        if rb and eb and rb.get("server_prompt_tokens") and eb.get("server_prompt_tokens"):
            sav.append(rb["server_prompt_tokens"] - eb["server_prompt_tokens"])
    rb_tot = sum(prim[(c, q, "raw+brief")]["server_prompt_tokens"] for (c, q) in questions
                 if prim.get((c, q, "raw+brief"), {}).get("server_prompt_tokens"))
    savings = {"total": sum(sav) if sav else None,
               "mean": round(statistics.mean(sav), 1) if sav else None,
               "median": _median(sav), "percent": round(100 * sum(sav) / rb_tot, 1) if sav and rb_tot else None}

    # malformed delta vs raw+brief (primary).
    mal_rb = sum(1 for (c, q) in questions if prim.get((c, q, "raw+brief"), {}).get("malformed"))
    mal_eb = sum(1 for (c, q) in questions if prim.get((c, q, "encoded+brief"), {}).get("malformed"))

    # per-case realized stages (offline): shelf, guarded-mining accounting (with the
    # passthrough-unwrap normalization), VG==V, roundtrip.
    l1 = Path(meta["l1_run"])
    if not l1.is_absolute():
        l1 = Path(__file__).resolve().parent / l1
    meter = (meta.get("tokenizer") or {}).get("meter") or "o200k"
    qb = str(qodec.binary())
    per_case, shelves = {}, {}
    stage2_attempted_cases, guarded_accepted_cases, vg_eq_v, roundtrip_ok = [], [], [], []
    cases = sorted({c for (c, _q) in questions})
    for case in cases:
        m = sorted(l1.glob(f"*/cases/{case}/*/transformed.txt")) or sorted(l1.glob(f"cases/{case}/*/transformed.txt"))
        raw = m[0].read_text(encoding="utf-8")
        st = ap.normalize_realized(ap.realized_stages_for_codec(VG_CODEC, raw, meter, qb, passthrough=True), ap._sha(raw))
        v = ap.realized_stages_for_codec("structural", raw, meter, qb, passthrough=True)
        env = qodec.encode(raw, codec=VG_CODEC, meter=meter, passthrough=True)
        rt = (qodec.decode_envelope(env)[0] == raw)
        roundtrip_ok.append(rt)
        shelf = st["stage1"]["selected_codec"]
        shelves[shelf] = shelves.get(shelf, 0) + 1
        attempted = st["stage2"]["attempted"]
        accepted = st["stage2"]["candidate_accepted"]        # normalized: a real guarded mine, not the unwrap
        passthrough = st["final"]["passthrough_unwrapped"]
        if attempted:
            stage2_attempted_cases.append(case)
        if accepted:
            guarded_accepted_cases.append(case)
        eqv = st["final"]["artifact_sha256"] == v["final"]["artifact_sha256"]
        if eqv:
            vg_eq_v.append(case)
        per_case[case] = {"shelf": shelf, "stage2_attempted": attempted,
                          "guarded_mining_accepted": accepted, "passthrough_unwrapped": passthrough,
                          "vg_equals_v": eqv, "roundtrip_ok": rt}

    overall, loc, fc = a["groups"]["all"], a["groups"]["locator"], a["groups"]["facts/counts"]
    # Full-run competence gates (canonical, unchanged).
    full_gate = {
        "raw_competence>=60%": (overall["raw_competence"] or 0) >= 0.60,
        "eligible_overall>=10": overall["eligible"] >= 10,
        "eligible_locator>=4": loc["eligible"] >= 4,
        "tokenizer_parity_ok": not a["parity"]["mismatch"],
    }
    # VG quality gate (all must hold).
    vg_gate = {
        "stable_vg_losses==0": overall["stable_codec_losses"] == 0,
        "alias_leaks==0": a["integrity"]["alias_leaks"] == 0,
        "invalid_id_delta<=0": a["integrity"]["invalid_id_delta"] <= 0,
        "malformed_delta<=0": (mal_eb - mal_rb) <= 0,
        "mean_savings>0": (savings["mean"] or 0) > 0,
        "median_savings>0": (savings["median"] or 0) > 0,
        "exact_roundtrip_all": all(roundtrip_ok),
    }
    passes = all(full_gate.values()) and all(vg_gate.values())
    verdict = ("VG PASSES FULL L2 CANDIDATE GATE.\nProduction squeeze remains rejected.\n"
               "VG promotion/integration is a separate decision."
               if passes else
               "VG DOES NOT PASS FULL L2.\nProduction squeeze remains rejected.")

    return {
        "unique_questions": a["unique_questions"], "unstable_questions": a["unstable_questions"],
        "retention": {"overall": overall["codec_retention"], "facts_counts": fc["codec_retention"],
                      "locator": loc["codec_retention"]},
        "raw_competence": overall["raw_competence"],
        "eligible": {"overall": overall["eligible"], "locator": loc["eligible"]},
        "stable_vg_losses": overall["stable_codec_losses"],
        "integrity": {"alias_leaks": a["integrity"]["alias_leaks"],
                      "invalid_id_delta_vs_raw_brief": a["integrity"]["invalid_id_delta"],
                      "malformed_raw_brief": mal_rb, "malformed_vg": mal_eb,
                      "malformed_delta": mal_eb - mal_rb},
        "token_savings_vs_raw_brief": savings,
        "latency_ms": {"raw_brief": a["integrity"]["latency_ms_raw+brief"],
                       "vg": a["integrity"]["latency_ms_encoded"]},
        "parity": a["parity"], "shelf_distribution": shelves,
        "stage2_attempted_cases": stage2_attempted_cases,
        "guarded_mining_accepted_cases": guarded_accepted_cases,
        "vg_equals_v_cases": vg_eq_v, "per_case": per_case,
        "roundtrip_all_ok": all(roundtrip_ok),
        "full_run_gate": full_gate, "vg_quality_gate": vg_gate,
        "passes": passes, "verdict": verdict,
    }


def render(meta, v) -> str:
    o = ["# qodec interop bench — full L2 VG candidate gate (fold-grep-guarded)"]
    o.append(f"run {meta['run_id']}  model={meta['model_requested']}  policy=VG (best(fold,grep)+guarded mine)")
    o.append(f"unique_questions={v['unique_questions']}  unstable={v['unstable_questions']}  "
             f"raw_competence={_pct(v['raw_competence'])}  eligible overall={v['eligible']['overall']} locator={v['eligible']['locator']}")
    o.append(f"retention overall={_pct(v['retention']['overall'])} facts/counts={_pct(v['retention']['facts_counts'])} "
             f"locator={_pct(v['retention']['locator'])}")
    o.append(f"stable VG losses={v['stable_vg_losses']}  leaks={v['integrity']['alias_leaks']}  "
             f"invalid Δ={v['integrity']['invalid_id_delta_vs_raw_brief']}  malformed Δ={v['integrity']['malformed_delta']}")
    s = v["token_savings_vs_raw_brief"]
    o.append(f"token savings vs raw+brief: total={s['total']} mean={s['mean']} median={s['median']} {s['percent']}%")
    o.append(f"latency ms raw+brief={v['latency_ms']['raw_brief']:.0f} vg={v['latency_ms']['vg']:.0f} (observation only)"
             if v['latency_ms']['raw_brief'] else "latency: n/a")
    o.append(f"shelf distribution: {v['shelf_distribution']}")
    o.append(f"stage-2 attempted in {len(v['stage2_attempted_cases'])} case(s): {v['stage2_attempted_cases']}")
    o.append(f"guarded mining accepted in {len(v['guarded_mining_accepted_cases'])} case(s): {v['guarded_mining_accepted_cases']}"
             " (passthrough-unwrap no-gain cases excluded)")
    o.append(f"VG == V (structural) byte-identical in {len(v['vg_equals_v_cases'])} case(s): {v['vg_equals_v_cases']}")
    o.append(f"exact roundtrip all cases: {v['roundtrip_all_ok']}")
    o.append(f"tokenizer parity: {'MISMATCH' if v['parity']['mismatch'] else 'ok'} (spread {v['parity']['arm_overhead_spread']})")
    o.append("\nfull-run gate: " + "  ".join(f"{k}={'✓' if val else '✗'}" for k, val in v["full_run_gate"].items()))
    o.append("VG quality gate: " + "  ".join(f"{k}={'✓' if val else '✗'}" for k, val in v["vg_quality_gate"].items()))
    o.append(f"\n## verdict\n{v['verdict']}")
    return "\n".join(o) + "\n"


def _pct(x):
    return f"{x*100:.0f}%" if x is not None else "-"


def main() -> int:
    ap_ = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap_.add_argument("--run", type=Path, required=True)
    args = ap_.parse_args()
    meta = json.loads((args.run / "meta.json").read_text(encoding="utf-8"))
    records = [json.loads(l) for l in (args.run / "records.jsonl").read_text(encoding="utf-8").splitlines() if l.strip()]
    v = analyze_vg(meta, records, args.run)
    print(render(meta, v))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
