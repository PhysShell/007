# L1 — model-facing A/B: RAW_VALID (Q1R) vs QODEC_VALID (Q1RQ) on case-0002

**Principal classification: `MODEL_QUALITY_REGRESSION_DESPITE_STRUCTURAL_PASS`**
(principal model `Gemma-4-31B-it`; same verdict on all four panel models).

`case_role: development` · `holdout_evidence: false` · `generalization_claim_allowed: false`.
Frozen producers unchanged: Qodec QA `6a5d4030`, adapter `f8fa87e3`, Q1R eval
`fc76d210`, evaluator `58a7a111`. Transport through the L0-qualified boundary
(`provider_matrix.send_json` @ `fa93d63`). Freeze manifest `l1-freeze-manifest.json`;
deterministic scorer v1 (no LLM judge), frozen and self-tested before any call.

## Question

Does a real qualified model answer from the **cheaper Qodec representation** of an
already-valid context as well as from the **raw** context? Same semantics; the only
intentional difference is representation. Scored on evidence citations against the
frozen `required_observation_ids` / `forbidden_stale_as_current` / `required_relation_paths`
and gold observation status.

## Result — consistent regression across four families

| model | family | RAW correct | QODEC correct | parse-fail | hard-regr | verdict |
|---|---|---|---|---|---|---|
| Gemma-4-31B-it | Google | 12/33 | 12/33 | 0.18 | 4 | REGRESSION |
| GLM-4.7 | Zhipu | 17/33 | 12/33 | 0.00 | 5 | REGRESSION |
| DeepSeek-V4-Flash-0731 | DeepSeek | 14/33 | 10/33 | 0.30 | 14 | REGRESSION |
| Qwen3.5-27B-Anko | Qwen | 23/33 | 17/33 | 0.00 | 11 | REGRESSION |

Every model reads the raw context **better** than its Qodec encoding, and every model
has hard regressions (a QODEC cell with a stale-as-current / fabricated / missing-witness
error the paired RAW cell did not have). No substitutions on any attempt (reported==requested).

### How QODEC fails (pooled, 132 cells/arm across the 4 models)

| axis | RAW | QODEC |
|---|---|---|
| **answer_correct** | 66 (50%) | 51 (39%) |
| fabricated / wrong observation ids | 3 | **21** |
| representation_parse_failure (alias glyphs leaked) | 0 | **16** |
| missing_required_witness | 64 | 75 |
| malformed JSON | 0 | 6 |
| stale_as_current_error | 0 | 2 |

The degradation is specifically in **accurate identifier extraction** from the `%q1`
legend: models mis-decode the aliased observation ids (hallucinating or garbling them)
and sometimes emit the raw legend glyphs. Reading is worse, not the encoding.

## Token side (the other half)

Qodec **is** cheaper: aggregate cold input **25,656 → 19,953 o200k tokens (−22.2%)**;
provider-reported prompt tokens 30,918 → 22,998. `token_win_cold = true`. But a token
win with a quality regression is not non-inferiority.

```
structural:    Q1R PASS · Q1RQ_roundtrip PASS   (byte-lossless; inherited from Q2)
model_facing:  representation_comprehension PASS · quality_noninferiority FAIL · token QODEC_CHEAPER_COLD
```

## Caveats (load-bearing)

1. **No reasoning.** Model Thinking was disabled at the key level for all models
   (`rlen==0`), because with reasoning on, the Qodec decode blew past the provider's
   gateway (repeated HTTP 502). It is applied identically to both arms, so the
   within-model comparison is controlled — but decoding a compressed legend plausibly
   benefits more from deliberation than reading raw JSON. **The regression is established
   under greedy / no-CoT decoding** and might narrow with a large reasoning budget (which
   was operationally infeasible here).
2. Development scale (N=3, 11 questions, one case). A development signal, not a p-value;
   strength is the consistent direction across four independent families.
3. Modest absolute baseline (RAW 36–70%): small/mid open models, no CoT. The result is
   the **within-model relative** regression.
4. Q1RQ is byte-lossless; the loss is in model comprehension, not encoding fidelity.

## Interpretation

Supported: *On case-0002, under no-reasoning decoding, four qualified models across
families each answered the raw valid context better than its byte-equivalent Qodec
encoding — a consistent quality regression despite Qodec being lossless and ~22% cheaper
in input tokens.*

**Not** claimed: no generalization, no "Qodec helps/hurts LLMs in general", no statement
about other budgets or reasoning settings.

## Next step (per protocol)

`MODEL_QUALITY_REGRESSION_DESPITE_STRUCTURAL_PASS` → **stop generalization work**. Do
**not** build `qodec project v1`, do **not** start the real-source holdout. Investigate
the **representation/instruction boundary** first — the failure is concretely
identifier-extraction from the `%q1` legend. (No notation tuning was done after seeing
outputs; that L1 prohibition is intact, so any encoding/notation change is a fresh,
separately-authorized step.)

## Artifacts

`l1-freeze-manifest.json` (frozen surfaces + scorer + acceptance/classification rules),
`l1-panel-summary.json`, `l1-receipt.json` (principal + panel + caveats),
`models/<name>/{receipt,cells,attempts,responses}`, `frozen-prompts/`, `notation.txt`.
Harness `research/b1-context/tools/l1_ab.py`.
