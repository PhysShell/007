#!/usr/bin/env python3
"""score_reader.py — Level-2 report + decision (B6).

Per case × arm: target-tokenizer raw/cold/warm tokens; fact score; exact-locator
(files+symbols) score; call-path score; invalid-identifier count; alias leakage;
mean output tokens; TTFT; total latency. Then the design doc's decision tree.

    python3 score_reader.py runs-l2/<id>
"""

from __future__ import annotations

import argparse
import json
import statistics
from collections import defaultdict
from pathlib import Path

ARMS = ["raw", "raw+brief", "encoded+brief"]
LOCATOR = {"files", "symbols"}


def _mean(xs):
    xs = [x for x in xs if x is not None]
    return statistics.mean(xs) if xs else None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("run_dir", type=Path)
    args = ap.parse_args()

    meta = json.loads((args.run_dir / "meta.json").read_text())
    recs = [json.loads(l) for l in (args.run_dir / "records.jsonl").read_text().splitlines() if l.strip()]

    print("# qodec interop bench — Level 2 (reader comprehension)")
    print(f"run {meta['run_id']}  model={meta['model']}  tokenizer={meta['tokenizer']}\n")

    by = defaultdict(list)
    for r in recs:
        by[(r["case"], r["arm"])].append(r)

    hdr = (f"{'case':<22}{'arm':<15}{'tok':>6}{'fact':>7}{'loc':>7}{'cnt':>7}"
           f"{'inv':>5}{'leak':>5}{'out':>6}{'ttft':>7}{'tot_ms':>8}")
    for case in dict.fromkeys(r["case"] for r in recs):
        ct = meta["case_tokens"].get(case, {})
        print(f"\n{case}   raw={ct.get('raw')} cold={ct.get('cold_encoded')} "
              f"warm={ct.get('warm_encoded')} tok (codec={ct.get('codec')})")
        print(hdr)
        for arm in ARMS:
            rs = by.get((case, arm), [])
            if not rs:
                continue
            def rate(pred):
                sub = [r for r in rs if pred(r)]
                return f"{100*sum(r['correct'] for r in sub)/len(sub):.0f}%" if sub else "-"
            tok = ct.get("warm_encoded" if arm == "encoded+brief" else "raw")
            out = _mean([r["usage"].get("completion_tokens") for r in rs])
            ttft = _mean([r["ttft_ms"] for r in rs])
            tot = _mean([r["total_ms"] for r in rs])
            inv = sum(len(r["invalid_identifiers"]) for r in rs)
            leak = sum(r["alias_leak"] for r in rs)
            print(f"{'':<22}{arm:<15}{tok if tok is not None else '-':>6}"
                  f"{rate(lambda r: r['qtype']=='facts'):>7}"
                  f"{rate(lambda r: r['qtype'] in LOCATOR):>7}"
                  f"{rate(lambda r: r['qtype']=='count'):>7}"
                  f"{inv:>5}{leak:>5}"
                  f"{(f'{out:.0f}' if out else '-'):>6}"
                  f"{(f'{ttft:.0f}' if ttft else '-'):>7}"
                  f"{(f'{tot:.0f}' if tot else '-'):>8}")

    # Decision tree (design doc). Compare encoded+brief vs raw on locator + overall.
    enc = [r for r in recs if r["arm"] == "encoded+brief"]
    raw = [r for r in recs if r["arm"] == "raw"]
    def acc(rs, pred=lambda r: True):
        sub = [r for r in rs if pred(r)]
        return sum(r["correct"] for r in sub) / len(sub) if sub else 0.0
    enc_loc, raw_loc = acc(enc, lambda r: r["qtype"] in LOCATOR), acc(raw, lambda r: r["qtype"] in LOCATOR)
    enc_all, raw_all = acc(enc), acc(raw)
    enc_leak = sum(r["alias_leak"] for r in enc)
    enc_inv = sum(len(r["invalid_identifiers"]) for r in enc)

    print("\n## decision")
    print(f"overall accuracy: raw {raw_all*100:.0f}% vs encoded+brief {enc_all*100:.0f}%  "
          f"| locator: raw {raw_loc*100:.0f}% vs encoded {enc_loc*100:.0f}%  "
          f"| alias leaks {enc_leak}  invalid ids {enc_inv}")
    if enc_all >= raw_all - 0.01 and enc_loc >= raw_loc - 0.01 and enc_leak == 0:
        print("→ blind qodec passes: protected spans not needed yet.")
    elif enc_loc < raw_loc - 0.01 and enc_all >= raw_all - 0.05:
        print("→ exact paths/symbols drop while reasoning holds: NEXT increment = protected spans.")
    elif enc_all < raw_all - 0.05:
        print("→ general comprehension drops: do not apply qodec to this evidence / change notation.")
    else:
        print("→ mixed; inspect per-case. (Latency: compare tot_ms columns for the warm-only caveat.)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
