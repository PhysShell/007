#!/usr/bin/env python3
"""score_reader.py — Level-2 paired scoring, report, and decision.

Conditional paired scoring (not a raw-vs-encoded accuracy race): for the same
question+repeat, follow raw → raw+brief → encoded+brief and measure

  raw_competence   P(raw correct)
  brief_retention  P(raw+brief correct | raw correct)
  codec_retention  P(encoded+brief correct | raw+brief correct)   <- the qodec effect
  codec_losses     raw+brief correct & encoded+brief wrong
  codec_rescues    raw+brief wrong  & encoded+brief correct

isolating the codec by comparing encoded+brief against raw+brief (both carry the
brief). A weak reader is a valid calibration reader: questions it cannot answer
in the control arms are not eligible and cannot move the qodec verdict.

Gates → INCONCLUSIVE (never PASS on a reader that cannot do the task):
  raw_competence < 60%  |  eligible overall < 10  |  eligible locator < 4

    python3 score_reader.py runs-l2/<id>
"""

from __future__ import annotations

import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path

ARMS = ["raw", "raw+brief", "encoded+brief"]
GROUPS = {"facts/counts": {"fact", "count"}, "locator": {"locator"},
          "call_path": {"call_path"}, "actionability": {"actionability"}}


def _mean(xs):
    xs = [x for x in xs if x is not None]
    return statistics.mean(xs) if xs else None


def _rate(n, d):
    return (n / d) if d else None


def _pct(x):
    return f"{x*100:.0f}%" if x is not None else "-"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("run_dir", type=Path)
    args = ap.parse_args()

    meta = json.loads((args.run_dir / "meta.json").read_text())
    recs = [json.loads(l) for l in (args.run_dir / "records.jsonl").read_text().splitlines() if l.strip()]

    # Pair by (case, question, repeat) → the three arms.
    pairs: dict[tuple, dict] = defaultdict(dict)
    for r in recs:
        pairs[(r["case"], r["question"], r["repeat"])][r["arm"]] = r
    complete = [p for p in pairs.values() if all(a in p for a in ARMS)]

    print("# qodec interop bench — Level 2 (reader comprehension, CPU calibration)")
    print(f"run {meta['run_id']}  model={meta['model_requested']} "
          f"(reported {meta.get('model_reported')})  tokenizer={meta['tokenizer'].get('meter')}")
    print(f"tokenizer sha256={str(meta['tokenizer'].get('sha256'))[:12]}  "
          f"qodec={meta['qodec_version']}  determinism={meta['determinism']['contract']}")
    if not meta.get("streaming_usage_supported", True):
        print(f"endpoint streams content but not usage → server tokens from non-stream requests; "
              f"TTFT is the preflight streaming sample ({meta.get('preflight_ttft_ms')} ms)")
    print(f"pairs={len(complete)}\n")

    def transitions(sel):
        ps = [p for p in complete if sel(p)]
        raw_ok = [p for p in ps if p["raw"]["correct"]]
        rb_ok = [p for p in ps if p["raw+brief"]["correct"]]
        return {
            "n": len(ps),
            "raw_competence": _rate(sum(p["raw"]["correct"] for p in ps), len(ps)),
            "brief_retention": _rate(sum(p["raw+brief"]["correct"] for p in raw_ok), len(raw_ok)),
            "codec_retention": _rate(sum(p["encoded+brief"]["correct"] for p in rb_ok), len(rb_ok)),
            "eligible": len(rb_ok),
            "codec_losses": sum(1 for p in ps if p["raw+brief"]["correct"] and not p["encoded+brief"]["correct"]),
            "codec_rescues": sum(1 for p in ps if not p["raw+brief"]["correct"] and p["encoded+brief"]["correct"]),
        }

    overall = transitions(lambda p: True)
    print("paired transitions (raw → raw+brief → encoded+brief):")
    hdr = f"  {'group':<15}{'n':>3}{'raw':>7}{'brief_ret':>10}{'codec_ret':>10}{'elig':>6}{'loss':>6}{'resc':>6}"
    print(hdr)
    grp_metrics = {"all": overall}
    for gname, cats in [("all", None), *GROUPS.items()]:
        m = overall if gname == "all" else transitions(lambda p, c=cats: p["raw"]["category"] in c)
        grp_metrics[gname] = m
        print(f"  {gname:<15}{m['n']:>3}{_pct(m['raw_competence']):>7}{_pct(m['brief_retention']):>10}"
              f"{_pct(m['codec_retention']):>10}{m['eligible']:>6}{m['codec_losses']:>6}{m['codec_rescues']:>6}")

    # Token accounting — actual server prompt tokens per arm, never the raw count
    # in the raw+brief row nor the warm artifact as the encoded prompt.
    print("\ntokens (local content vs actual server prompt; warm = amortization estimate):")
    print(f"  {'case':<22}{'arm':<15}{'local':>7}{'server':>8}{'out':>6}{'ttft':>7}{'tot_ms':>8}")
    by_ca = defaultdict(list)
    for r in recs:
        by_ca[(r["case"], r["arm"])].append(r)
    for case in dict.fromkeys(r["case"] for r in recs):
        ct = meta["case_tokens"].get(case, {})
        warm = ct.get("encoded_artifact_warm")
        for arm in ARMS:
            rs = by_ca.get((case, arm), [])
            if not rs:
                continue
            local = rs[0]["local_content_tokens"]
            server = _mean([r["server_prompt_tokens"] for r in rs])
            out = _mean([r["completion_tokens"] for r in rs])
            ttft = _mean([r["ttft_ms"] for r in rs])
            tot = _mean([r["total_ms"] for r in rs])
            tag = f"  (warm est. {warm})" if arm == "encoded+brief" else ""
            print(f"  {case:<22}{arm:<15}{local:>7}{(f'{server:.0f}' if server else '-'):>8}"
                  f"{(f'{out:.0f}' if out else '-'):>6}{(f'{ttft:.0f}' if ttft else '-'):>7}"
                  f"{(f'{tot:.0f}' if tot else '-'):>8}{tag}")

    enc = [p["encoded+brief"] for p in complete]
    rb = [p["raw+brief"] for p in complete]
    alias_leaks = sum(len(r["alias_leaks"]) for r in enc)
    inv_enc = sum(len(r["invalid_identifiers"]) for r in enc)
    inv_rb = sum(len(r["invalid_identifiers"]) for r in rb)
    malformed = sum(1 for r in recs if r["malformed"])
    print(f"\nintegrity: alias_leaks(encoded)={alias_leaks}  invalid_ids raw+brief={inv_rb} "
          f"encoded={inv_enc}  malformed_json={malformed}")

    # Decision.
    loc = grp_metrics["locator"]
    print("\n## decision")
    inconclusive = (
        (overall["raw_competence"] or 0) < 0.60
        or overall["eligible"] < 10
        or loc["eligible"] < 4
    )
    if inconclusive:
        if (overall["raw_competence"] or 0) < 0.60:
            print(f"INCONCLUSIVE: reader too weak for this task set "
                  f"(raw competence {_pct(overall['raw_competence'])} < 60%).")
        else:
            print(f"INCONCLUSIVE: insufficient eligible sample "
                  f"(overall {overall['eligible']}<10 or locator {loc['eligible']}<4).")
        print("Run saved as calibration evidence; qodec verdict withheld.")
        return 0

    ok = (overall["codec_losses"] == 0 and loc["codec_losses"] == 0
          and alias_leaks == 0 and inv_enc <= inv_rb)
    if ok:
        print("BLIND QODEC PASSES: no codec losses, no locator loss, zero alias leakage, "
              "no rise in invalid identifiers. Protected spans stay unnecessary.")
    elif loc["codec_losses"] > 0 and (grp_metrics["facts/counts"]["codec_retention"] or 0) >= 0.9:
        print("PROTECTED SPANS NEXT: exact locator regression (raw+brief correct → encoded+brief "
              f"wrong on {loc['codec_losses']} pair(s)) while facts/counts comprehension holds.")
    elif (overall["codec_retention"] or 1) < 0.9:
        print("DO NOT APPLY BLIND QODEC to this evidence (general comprehension drop) — or change "
              "the notation. This is not a protected-spans case.")
    else:
        print(f"MIXED: codec_losses={overall['codec_losses']} alias_leaks={alias_leaks} "
              f"inv Δ={inv_enc - inv_rb}. Inspect per-case before deciding.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
