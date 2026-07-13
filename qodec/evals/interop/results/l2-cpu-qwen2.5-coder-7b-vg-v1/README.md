# VG candidate record: l2-cpu-qwen2.5-coder-7b-vg-v1

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
`encoded+brief` uses **fold-grep-guarded** instead of `squeeze`. Same L1 evidence
(`results/rtk-codegraph-clap-v1`), same tasks snapshot (`manifest.tasks_snapshot_sha256`),
same three arms (`raw`, `raw+brief`, `encoded+brief`).

## Result — VG PASSES FULL L2 CANDIDATE GATE

```
VG PASSES FULL L2 CANDIDATE GATE.
Production squeeze remains rejected.
VG promotion/integration is a separate decision.
```

- **Reader is decision-capable:** raw competence 74%
  (≥ 60%), eligible overall 17 (≥ 10),
  locator 9 (≥ 4), tokenizer parity ok.
- **No comprehension loss under VG:** codec_retention overall
  100% / facts-counts 100% /
  locator 100%, **0 stable VG losses**.
- **Clean integrity:** 0 alias leaks, invalid-identifier
  Δ 0, malformed Δ 0
  vs raw+brief; exact roundtrip on all cases.
- **It still saves tokens:** vs raw+brief total 3379, mean 146.9,
  median 91 (5.3%).

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

- **shelf distribution:** {'raw': 3, 'grep': 2}
- **guarded mining applied** in 4 case(s):
  ['build-log-rtk-log', 'clap-derive-explore', 'rg-output-rtk-grep', 'rtk-rg-derive-clap']
- **VG == V (structural) byte-identical** in 3 case(s):
  ['build-log-rtk-log', 'rg-output-rtk-grep', 'rtk-rg-parser-clap'] (the guard removed every mine candidate there)

## Gates (never moved after the result)

```
full-run gate:  raw_competence>=60%=PASS  eligible_overall>=10=PASS  eligible_locator>=4=PASS  tokenizer_parity_ok=PASS
VG quality gate: stable_vg_losses==0=PASS  alias_leaks==0=PASS  invalid_id_delta<=0=PASS  malformed_delta<=0=PASS  mean_savings>0=PASS  median_savings>0=PASS  exact_roundtrip_all=PASS
```

The same gate applied to the canonical squeeze run **fails** (5 stable losses,
alias leaks) — pinned by `tests/test_score_vg.py::CanonicalSqueezeFailsVGGate`.

## Caveats

- **CPU-served, single quant, one model, one L1 evidence set.** A candidate-grade
  signal, not a universal claim. Promotion is a separate decision.
- Latency is observation-only (raw+brief 39668ms,
  vg 32198ms) — CPU wall-clock, not a scored metric.

## Contents

`meta.json` (identities, determinism, contract, per-case tokens),
`manifest.json` (immutable run manifest: arms, codec=VG, policy_name, artifact +
realized-stage-receipt SHAs), `preflight.json`, `records.jsonl` (75
requests/responses/parsed-answers/scores), `report.txt` (the gate),
`stability.txt`, `realized-stage-receipts.json`, `snapshots/reader-tasks.json`,
`SHA256SUMS` (`sha256sum -c SHA256SUMS`).
