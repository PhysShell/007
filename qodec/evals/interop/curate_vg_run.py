#!/usr/bin/env python3
"""Curate the VG (fold-grep-guarded) full-L2 run into a committed artifact.

Reads a completed run dir (runs-l2/l2-vg-7b-v1) and assembles
results/l2-cpu-qwen2.5-coder-7b-vg-v1/ with the canonical file set: meta.json,
manifest.json, preflight.json, records.jsonl, report.txt, stability.txt,
realized-stage-receipts.json, snapshots/reader-tasks.json, README.md, SHA256SUMS.

Offline: no model calls. Re-derives realized VG stages from the L1 artifacts via
the same code the harness records, and re-runs the gate for report.txt.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path

import score_vg
from bench import ablation_policies as ap, qodec

HERE = Path(__file__).resolve().parent
VG_CODEC = "fold-grep-guarded"


def _sha(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def _stability(records) -> str:
    """Canonical stability.txt: repeated (case,question,arm) and repeat agreement."""
    grp: dict = {}
    for r in records:
        grp.setdefault((r["case"], r["question"], r["arm"]), []).append(r)
    lines = ["# stability — repeated (case,question,arm) and whether repeats agree on correct"]
    for (case, q, arm) in sorted(grp):
        reps = sorted(grp[(case, q, arm)], key=lambda r: r["repeat"])
        if len(reps) < 2:
            continue
        correct = [bool(r["correct"]) for r in reps]
        verdict = "STABLE" if len(set(correct)) == 1 else "UNSTABLE"
        lines.append(f"{case:<21} {q:<13} {arm:<14} repeats={len(reps)} correct={correct} {verdict}")
    return "\n".join(lines) + "\n"


def _realized_receipts(meta, cases) -> dict:
    """Per-case realized VG stages, read from the L1 tool artifacts (offline)."""
    l1 = Path(meta["l1_run"])
    if not l1.is_absolute():
        l1 = HERE / l1
    meter = (meta.get("tokenizer") or {}).get("meter") or "o200k"
    qb = str(qodec.binary())
    out = {}
    for case in cases:
        m = (sorted(l1.glob(f"*/cases/{case}/*/transformed.txt"))
             or sorted(l1.glob(f"cases/{case}/*/transformed.txt")))
        raw = m[0].read_text(encoding="utf-8")
        out[case] = ap.realized_stages_for_codec(VG_CODEC, raw, meter, qb, passthrough=True)
    return {"codec": VG_CODEC, "l1_run": meta["l1_run"],
            "qodec_binary_sha256": meta["qodec_version"].split(":")[-1], "receipts": out}


def curate(run: Path, out: Path, canon_snapshot: Path) -> None:
    meta = json.loads((run / "meta.json").read_text(encoding="utf-8"))
    records = [json.loads(l) for l in (run / "records.jsonl").read_text(encoding="utf-8").splitlines() if l.strip()]
    cases = sorted({r["case"] for r in records})

    out.mkdir(parents=True, exist_ok=True)
    (out / "snapshots").mkdir(exist_ok=True)

    # verbatim copies from the run
    shutil.copyfile(run / "meta.json", out / "meta.json")
    shutil.copyfile(run / "preflight.json", out / "preflight.json")
    shutil.copyfile(run / "records.jsonl", out / "records.jsonl")
    shutil.copyfile(run / "run-manifest.json", out / "manifest.json")

    # tasks snapshot — reuse the canonical one (manifest tasks_snapshot_sha256 pins it)
    manifest = json.loads((run / "run-manifest.json").read_text(encoding="utf-8"))
    assert _sha(canon_snapshot) == manifest["tasks_snapshot_sha256"], "tasks snapshot SHA mismatch"
    shutil.copyfile(canon_snapshot, out / "snapshots" / "reader-tasks.json")

    # derived, offline
    v = score_vg.analyze_vg(meta, records, run)
    (out / "report.txt").write_text(score_vg.render(meta, v), encoding="utf-8")
    (out / "stability.txt").write_text(_stability(records), encoding="utf-8")
    (out / "realized-stage-receipts.json").write_text(
        json.dumps(_realized_receipts(meta, cases), indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (out / "README.md").write_text(_readme(meta, v), encoding="utf-8")

    # SHA256SUMS over every file except itself
    files = sorted(p for p in out.rglob("*") if p.is_file() and p.name != "SHA256SUMS")
    lines = [f"{_sha(p)}  ./{p.relative_to(out)}" for p in files]
    (out / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")


def _readme(meta, v) -> str:
    s = v["token_savings_vs_raw_brief"]
    ret = v["retention"]
    intg = v["integrity"]
    return f"""# VG candidate record: l2-cpu-qwen2.5-coder-7b-vg-v1

The first **full 23-question** Level-2 run of the **VG** policy
(`fold-grep-guarded` = best(fold, grep) structural shelf + **guarded** mine/deep),
scored against the promotion gate. VG **passes**. Production `squeeze` remains
rejected (canonical record `l2-cpu-qwen2.5-coder-7b-v1`); VG promotion or
integration is a separate decision this run does not make.

## Setup

Identical reader environment to the canonical 7B record — same GGUF
(`509287f78cb4…`), same target tokenizer (`c0382117ea32…`), same negotiated
contract (`stream=False`, `seed=0`, `response_format=json_object`),
temperature 0. The only change is the encoded arm's codec:
`encoded+brief` uses **{VG_CODEC}** instead of `squeeze`. Same L1 evidence
(`{meta['l1_run']}`), same tasks snapshot (`manifest.tasks_snapshot_sha256`),
same three arms (`raw`, `raw+brief`, `encoded+brief`).

## Result — VG PASSES FULL L2 CANDIDATE GATE

```
{v['verdict']}
```

- **Reader is decision-capable:** raw competence {v['raw_competence']*100:.0f}%
  (≥ 60%), eligible overall {v['eligible']['overall']} (≥ 10),
  locator {v['eligible']['locator']} (≥ 4), tokenizer parity ok.
- **No comprehension loss under VG:** codec_retention overall
  {ret['overall']*100:.0f}% / facts-counts {ret['facts_counts']*100:.0f}% /
  locator {ret['locator']*100:.0f}%, **{v['stable_vg_losses']} stable VG losses**.
- **Clean integrity:** {intg['alias_leaks']} alias leaks, invalid-identifier
  Δ {intg['invalid_id_delta_vs_raw_brief']}, malformed Δ {intg['malformed_delta']}
  vs raw+brief; exact roundtrip on all cases.
- **It still saves tokens:** vs raw+brief total {s['total']}, mean {s['mean']},
  median {s['median']} ({s['percent']}%).

## Why VG, and how it differs from squeeze

VG is **not** "guarded squeeze". Its structural shelf is best(fold, grep) only —
it drops squeeze's `toon`/`diag`/`tmpl` stage-1 candidates — and its mine/deep
stage runs with the **lexical guard** on (rejecting code-shaped candidate phrases:
backtick, `»`, `::`, `/`, file extensions, snake_case, Camel/Pascal humps). The
closure ablation (`analysis/l2-qwen2.5-coder-7b-alias-fold-closure-v1`) showed the
guard alone (SG) rescues 4/5 canonical losses but the simplified structural shelf
is what carries the 5th; VG combines both.

Realized per-case stages (`realized-stage-receipts.json`, re-derived offline from
the L1 artifacts):

- **shelf distribution:** {v['shelf_distribution']}
- **guarded mining applied** in {len(v['guarded_mining_applied_cases'])} case(s):
  {v['guarded_mining_applied_cases']}
- **VG == V (structural) byte-identical** in {len(v['vg_equals_v_cases'])} case(s):
  {v['vg_equals_v_cases']} (the guard removed every mine candidate there)

## Gates (never moved after the result)

```
full-run gate:  {'  '.join(f"{k}={'PASS' if val else 'FAIL'}" for k, val in v['full_run_gate'].items())}
VG quality gate: {'  '.join(f"{k}={'PASS' if val else 'FAIL'}" for k, val in v['vg_quality_gate'].items())}
```

The same gate applied to the canonical squeeze run **fails** (5 stable losses,
alias leaks) — pinned by `tests/test_score_vg.py::CanonicalSqueezeFailsVGGate`.

## Caveats

- **CPU-served, single quant, one model, one L1 evidence set.** A candidate-grade
  signal, not a universal claim. Promotion is a separate decision.
- Latency is observation-only (raw+brief {v['latency_ms']['raw_brief']:.0f}ms,
  vg {v['latency_ms']['vg']:.0f}ms) — CPU wall-clock, not a scored metric.

## Contents

`meta.json` (identities, determinism, contract, per-case tokens),
`manifest.json` (immutable run manifest: arms, codec=VG, policy_name, artifact +
realized-stage-receipt SHAs), `preflight.json`, `records.jsonl` ({meta['n_records']}
requests/responses/parsed-answers/scores), `report.txt` (the gate),
`stability.txt`, `realized-stage-receipts.json`, `snapshots/reader-tasks.json`,
`SHA256SUMS` (`sha256sum -c SHA256SUMS`).
"""


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--run", type=Path, default=HERE / "runs-l2" / "l2-vg-7b-v1")
    p.add_argument("--out", type=Path, default=HERE / "results" / "l2-cpu-qwen2.5-coder-7b-vg-v1")
    p.add_argument("--canon-snapshot", type=Path,
                   default=HERE / "results" / "l2-cpu-qwen2.5-coder-7b-v1" / "snapshots" / "reader-tasks.json")
    args = p.parse_args()
    curate(args.run, args.out, args.canon_snapshot)
    print(f"curated -> {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
