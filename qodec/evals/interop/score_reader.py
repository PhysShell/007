#!/usr/bin/env python3
"""score_reader.py — Level-2 decision over UNIQUE questions, with stability.

The decision unit is `(case, question)`, resolved from **repeat 0** (the primary
result). Repeats exist only for flagged questions and are used solely for
*stability* analysis — they never inflate n, eligibility, or the raw/locator
gates. A locator question run 100 times is still one eligible locator unit.

Conditional paired transitions on the primary results: raw → raw+brief →
encoded+brief, with the codec isolated as encoded+brief vs raw+brief. Gates to
INCONCLUSIVE (never a false PASS) when raw competence < 60%, or unique eligible
overall < 10, or unique eligible locator < 4.

Adds a server/local tokenizer parity check (overhead = server_prompt −
local_content should be constant across arms) and separates format compliance
from semantic correctness.

    python3 score_reader.py runs-l2/<id>
"""

from __future__ import annotations

import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path

ARMS = ["raw", "raw+brief", "encoded+brief"]
GROUPS = {"all": None, "facts/counts": {"fact", "count"}, "locator": {"locator"},
          "call_path": {"call_path"}, "actionability": {"actionability"}}
PARITY_TOLERANCE = 8   # tokens of arm-to-arm overhead spread before we distrust savings


def _rate(n, d):
    return (n / d) if d else None


def _pct(x):
    return f"{x*100:.0f}%" if x is not None else "-"


def _mean(xs):
    xs = [x for x in xs if x is not None]
    return statistics.mean(xs) if xs else None


def analyze(meta: dict, records: list[dict]) -> dict:
    # index[(case, q, arm)] = {repeat: record}
    index: dict[tuple, dict] = defaultdict(dict)
    for r in records:
        index[(r["case"], r["question"], r["arm"])][r["repeat"]] = r
    questions = sorted({(r["case"], r["question"]) for r in records})
    categories = {(r["case"], r["question"]): r["category"] for r in records}

    def primary(case, q, arm):
        arms = index.get((case, q, arm), {})
        return arms.get(0) or (arms[min(arms)] if arms else None)

    # Stability: for each (case,q,arm) with >1 repeat, are the repeats consistent?
    unstable_q = set()
    for (case, q, arm), reps in index.items():
        if len(reps) > 1:
            corr = {reps[k]["correct"] for k in reps}
            if len(corr) > 1:
                unstable_q.add((case, q))

    def transitions(cats):
        qs = [(c, q) for (c, q) in questions if cats is None or categories[(c, q)] in cats]
        raw = [primary(c, q, "raw") for (c, q) in qs]
        rb = [primary(c, q, "raw+brief") for (c, q) in qs]
        eb = [primary(c, q, "encoded+brief") for (c, q) in qs]
        rows = [(a, b, e) for a, b, e in zip(raw, rb, eb) if a and b and e]
        raw_ok = [(a, b, e) for (a, b, e) in rows if a["correct"]]
        rb_ok = [(a, b, e) for (a, b, e) in rows if b["correct"]]
        return {
            "n": len(rows),
            "raw_competence": _rate(sum(a["correct"] for a, _, _ in rows), len(rows)),
            "brief_retention": _rate(sum(b["correct"] for _, b, _ in raw_ok), len(raw_ok)),
            "codec_retention": _rate(sum(e["correct"] for _, _, e in rb_ok), len(rb_ok)),
            "eligible": len(rb_ok),
            "codec_losses": sum(1 for _, b, e in rows if b["correct"] and not e["correct"]),
            "codec_rescues": sum(1 for _, b, e in rows if not b["correct"] and e["correct"]),
            # stable loss = a codec loss whose repeats agree (or was never repeated)
            "stable_codec_losses": sum(
                1 for (c, q) in qs
                if (b := primary(c, q, "raw+brief")) and (e := primary(c, q, "encoded+brief"))
                and b["correct"] and not e["correct"] and (c, q) not in unstable_q),
        }

    groups = {name: transitions(cats) for name, cats in GROUPS.items()}

    # Format compliance (primary) per arm over unique questions.
    fmt = {}
    for arm in ARMS:
        ps = [primary(c, q, arm) for (c, q) in questions]
        ps = [p for p in ps if p]
        fmt[arm] = _rate(sum(p.get("format_compliant", not p["malformed"]) for p in ps), len(ps))

    # Server/local tokenizer parity: overhead = server_prompt − local_content.
    # System+template is constant across arms, so overhead should be too; a large
    # arm-to-arm spread means the local tokenizer disagrees with the server's.
    overhead_by_arm = {}
    for arm in ARMS:
        ohs = []
        for (c, q) in questions:
            p = primary(c, q, arm)
            if p and p.get("server_prompt_tokens") is not None:
                ohs.append(p["server_prompt_tokens"] - p["local_content_tokens"])
        overhead_by_arm[arm] = _mean(ohs)
    present = [v for v in overhead_by_arm.values() if v is not None]
    spread = (max(present) - min(present)) if len(present) >= 2 else None
    parity = {
        "overhead_by_arm": {k: (round(v, 1) if v is not None else None) for k, v in overhead_by_arm.items()},
        "arm_overhead_spread": round(spread, 1) if spread is not None else None,
        "tolerance": PARITY_TOLERANCE,
        "mismatch": bool(spread is not None and spread > PARITY_TOLERANCE),
    }

    # Integrity + cost/latency deltas (primary, over questions with both arms).
    def arm_prim(arm):
        return [primary(c, q, arm) for (c, q) in questions if primary(c, q, arm)]
    eb, rb = arm_prim("encoded+brief"), arm_prim("raw+brief")
    alias_leaks = sum(len(p["alias_leaks"]) for p in eb)
    inv_eb = sum(len(p["invalid_identifiers"]) for p in eb)
    inv_rb = sum(len(p["invalid_identifiers"]) for p in rb)
    integrity = {
        "alias_leaks": alias_leaks, "invalid_ids_raw+brief": inv_rb, "invalid_ids_encoded": inv_eb,
        "invalid_id_delta": inv_eb - inv_rb,
        "malformed_total": sum(1 for r in records if r["malformed"]),
        "server_tokens_raw+brief": _mean([p.get("server_prompt_tokens") for p in rb]),
        "server_tokens_encoded": _mean([p.get("server_prompt_tokens") for p in eb]),
        "latency_ms_raw+brief": _mean([p.get("total_ms") for p in rb]),
        "latency_ms_encoded": _mean([p.get("total_ms") for p in eb]),
    }

    overall, loc = groups["all"], groups["locator"]
    inconclusive_reason = None
    if (overall["raw_competence"] or 0) < 0.60:
        inconclusive_reason = f"reader too weak (raw competence {_pct(overall['raw_competence'])} < 60%)"
    elif overall["eligible"] < 10:
        inconclusive_reason = f"insufficient unique eligible overall ({overall['eligible']} < 10)"
    elif loc["eligible"] < 4:
        inconclusive_reason = f"insufficient unique eligible locator ({loc['eligible']} < 4)"

    if inconclusive_reason:
        verdict = f"INCONCLUSIVE: {inconclusive_reason}"
    elif (overall["stable_codec_losses"] == 0 and loc["stable_codec_losses"] == 0
          and alias_leaks == 0 and integrity["invalid_id_delta"] <= 0 and not parity["mismatch"]):
        verdict = "BLIND QODEC PASSES"
    elif loc["stable_codec_losses"] > 0 and (groups["facts/counts"]["codec_retention"] or 0) >= 0.9:
        verdict = f"PROTECTED SPANS NEXT (locator regression on {loc['stable_codec_losses']} unique question(s))"
    elif (overall["codec_retention"] or 1) < 0.9:
        verdict = "DO NOT APPLY BLIND QODEC / change notation (general comprehension drop)"
    elif parity["mismatch"]:
        verdict = "TOKENIZER_OR_TEMPLATE_MISMATCH — token savings not trustworthy; fix tokenizer identity"
    else:
        verdict = "MIXED — inspect per-case"

    return {
        "unique_questions": len(questions),
        "total_executions": len(records),
        "repeated_questions": sum(1 for (c, q) in questions
                                  if any(len(index.get((c, q, a), {})) > 1 for a in ARMS)),
        "unstable_questions": len(unstable_q),
        "groups": groups, "format_compliance": fmt, "parity": parity,
        "integrity": integrity,
        "decision": {"inconclusive": bool(inconclusive_reason), "verdict": verdict},
    }


def render(meta: dict, a: dict) -> str:
    out = []
    out.append("# qodec interop bench — Level 2 (reader comprehension)")
    mi = meta.get("model_identity", {})
    out.append(f"run {meta['run_id']}  model={meta['model_requested']} "
               f"(reported {meta.get('model_reported')})  quant={mi.get('quantization')}")
    out.append(f"model_sha256={str(mi.get('model_file_sha256'))[:12]}  "
               f"tokenizer_sha256={str(meta['tokenizer'].get('sha256'))[:12]}  qodec={meta['qodec_version']}")
    out.append(f"effective={meta.get('effective')}  structured_json={(meta.get('structured_json') or {}).get('mode')}")
    out.append(f"unique_questions={a['unique_questions']}  total_executions={a['total_executions']}  "
               f"repeated_questions={a['repeated_questions']}  unstable_questions={a['unstable_questions']}\n")

    out.append(f"{'group':<15}{'n':>3}{'raw':>7}{'brief_ret':>10}{'codec_ret':>10}"
               f"{'elig':>6}{'loss':>6}{'stbl_loss':>10}{'resc':>6}")
    for name in GROUPS:
        g = a["groups"][name]
        out.append(f"{name:<15}{g['n']:>3}{_pct(g['raw_competence']):>7}{_pct(g['brief_retention']):>10}"
                   f"{_pct(g['codec_retention']):>10}{g['eligible']:>6}{g['codec_losses']:>6}"
                   f"{g['stable_codec_losses']:>10}{g['codec_rescues']:>6}")

    out.append("\nformat compliance (primary, per arm): " +
               "  ".join(f"{arm}={_pct(a['format_compliance'][arm])}" for arm in ARMS))
    p = a["parity"]
    out.append(f"tokenizer parity: overhead/arm={p['overhead_by_arm']} spread={p['arm_overhead_spread']} "
               f"(tol {p['tolerance']}) → {'MISMATCH' if p['mismatch'] else 'ok'}")
    it = a["integrity"]
    out.append(f"integrity: alias_leaks={it['alias_leaks']}  invalid_id Δ={it['invalid_id_delta']}  "
               f"malformed={it['malformed_total']}")
    out.append(f"cost: server_tokens raw+brief={it['server_tokens_raw+brief']:.0f} "
               f"encoded={it['server_tokens_encoded']:.0f}  "
               f"latency_ms raw+brief={it['latency_ms_raw+brief']:.0f} encoded={it['latency_ms_encoded']:.0f}"
               if it["server_tokens_raw+brief"] else "cost: server tokens unavailable")
    out.append(f"\n## decision\n{a['decision']['verdict']}")
    if a["decision"]["inconclusive"]:
        out.append("Run saved as calibration evidence; qodec verdict withheld.")
    return "\n".join(out)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("run_dir", type=Path)
    args = ap.parse_args()
    meta = json.loads((args.run_dir / "meta.json").read_text())
    recs = [json.loads(l) for l in (args.run_dir / "records.jsonl").read_text().splitlines() if l.strip()]
    print(render(meta, analyze(meta, recs)))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
