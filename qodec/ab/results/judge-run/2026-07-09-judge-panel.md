# Judge-grade A/B — run record, 2026-07-09

The real task, not retrieval QA: `own-check` FP-triage over the OwnAudit
oracle, judged from raw vs qodec-encoded source. This is next-steps item 1
from `docs/token-codec.md`, executed at oracle scale — and it doubles as the
007 Phase-1 FP-direction gate from `TODO.md`.

## Setup

- **Task:** classify 4 real own-check findings as `real` / `false_positive` /
  `uncertain` — 2 genuine leaks (`WatchlistViewModel:32` OWN001,
  `TickerViewModel:27` OWN-TIMER) and 2 deliberately-false controls against
  the `Fixed*` counterparts (`oracle/fixtures/findings.json` +
  `findings-fp-control.json`).
- **Prompt:** the production judge contract — `judge/prompt.template.md`
  structure with the full OwnAudit rubric (`docs/fp-judge/rubric.md`)
  injected verbatim — in `o7 judge`'s batched form (all 4 files, one prompt;
  see the per-file finding below). Prompts archived beside this record.
- **Panel:** 6 fresh-context Claude subagents, 3 per side, tools limited to
  reading their own prompt file. Encoded side saw only the `%q1` container
  (deep codec, 18 legend entries) plus the notation brief.
- **Ground truth:** real, real, false_positive, false_positive.

## Results

| side | verdicts correct | evidence quality |
|---|---:|---|
| raw ×3 | 12/12 | teardown sites cited with `:line` on every FP |
| encoded ×3 | 12/12 | same — teardown citations survived encoding |

Full agreement raw ↔ encoded, all seeds, all findings. The encoded judges
correctly attributed the *aliased* subscription line (`记` in the legend) to
both Watchlist variants and keyed their FP verdicts on the raw-visible
`Dispose()` teardowns — the deciding facts were never garbled.

**Phase-1 gate (TODO.md): PASS** — both FP-control findings came back
`false_positive` from every judge, reasons citing the `-=` in `Dispose` /
`_timer.Dispose()`, which is exactly the gate's PASS criterion. (Panel
edition: fresh Claude contexts, not the `o7 judge` binary — re-run through
`o7` for the record when a `claude` CLI box is handy.)

## Codec economics on this task (o200k)

- **Per-file prompts fall below the payoff line.** All four oracle files
  (198–481 tokens) honestly fell back to `raw` (+2–5% container tax, refused
  by `ab emit`'s fail-closed check). Finding: per-file judging of *small*
  files is not where the codec pays.
- **Batched prompts pay.** The 4-file batch (`o7 judge --max-files` shape):
  1430 → 1287 tokens cold (−10.0%), body-only 1070 (−25.2% warm). Leaky and
  Fixed variants share almost everything, so the miner aliased the common
  lines while the differences — the teardown code that decides the verdict —
  stayed raw and visually prominent, diff-style.
- **Real Claude token accounting** (per-judge totals incl. harness):
  raw ×3 ≈ 26,088 avg; encoded ×3 ≈ 25,891 avg — encoded cheaper even with
  the extra decode-thinking. Wall time again asymmetric: raw 24–29 s,
  encoded 40–54 s (reasoning models trade some input savings for thinking).

## Honest caveats

- The oracle self-documents its ground truth in doc comments ("DELIBERATELY
  LEAKY", "the corrected counterpart") — this run proves notation
  transparency on a real prompt contract, not judge skill on hostile input.
  The 156-finding STS run (source not in this workspace) is the real test.
- One batch, one model family, 3 seeds per side.
