# L2 — protected-identifier representation ablation (case-0002)

**Mechanism: `IDENTIFIER_PROTECTION_PARTIALLY_RESCUES`**
**Product: `PROTECTED_REPRESENTATION_REGRESSION_WITH_TOKEN_WIN`**

`case_role: development` · `holdout_evidence: false` · `generalization_claim_allowed: false`.
Producer qualified before any call: qodec `89d06be4` (tree `c2c4d461`), route-A CI run
`31407935615` green on qodec-tests + full flake + build-and-smoke; QB receipt
`02edfdb8`, committed at 007 `0815169` **before the first result existed**.

## The hypothesis under test

L1 found that `%q1 deep` degrades model quality despite being byte-lossless, with the
damage concentrated in identifier handling. L2 asks whether that is *caused* by aliasing
machine-significant identifiers — by protecting them and changing nothing else.

Single-factor by construction: the new binary at an **empty** protection set reproduces
the frozen L1 bytes **byte-identically on all three tasks**, the reader notation is the
bound `9d11406e` unchanged, prompts are unchanged, and the scorer is *imported* from
`l1_ab`. The only thing that moved is which source spans may be aliased.

Protection rule (no task oracle): every observation identity the context schema exposes —
`selected[]`/`omitted[].observation_id`, `relations[].from`/`.to` — located by key, deduped
by source range. 36/50/42 spans, ~11–13% of bytes, 24 unique values per context. Derived
from **no** `required_observation_ids`, no expected answers, no L1 failure list.

## Result — corruption eliminated, parity not reached

Pooled over 4 model families — 33 paired cells per model per arm, **132 pooled per arm**:

| axis | RAW_VALID | QODEC_FULL_ALIAS | QODEC_LITERAL_IDENTIFIERS |
|---|---|---|---|
| **answer_correct** | **70** | 48 | **64** |
| fabricated / garbled ids | 0 | 19 | **0** |
| leaked legend glyphs | 0 | 23 | **0** |
| missing_required_witness | 57 | 75 | 68 |
| malformed output | 0 | 0 | 0 |

Identifier protection drove both referential-corruption axes to **exactly the RAW baseline
of zero**, across every model, and recovered most of the correctness gap (48 → 64 of 70).
It did not restore parity.

Per model (`answer_correct` / 33, and paired hard regressions vs RAW):

| model | RAW | FULL_ALIAS | PROTECTED | hard-regr FA → PROT | non-inferior |
|---|---|---|---|---|---|
| DeepSeek-V4-Flash-0731 | 19 | 7 | **20** | 14 → 2 | no |
| Gemma-4-31B-it | 12 | 11 | **13** | 5 → 3 | no |
| GLM-4.7 | 17 | 14 | 15 | 6 → 6 | no |
| Qwen3.5-27B-Anko | 22 | 16 | 16 | 11 → 7 | no |

Two models *exceed* their RAW correct count under protection yet still fail non-inferiority,
because a cell that newly loses a witness counts as a hard regression even when another cell
gains one. That is the frozen rule working as intended, not a scoring accident.

## Where the residual actually lives

The leftover deficit is almost entirely `missing_required_witness` (RAW 57 → PROTECTED 68).
The model still reads the compressed grammar **less thoroughly** than raw JSON even when
every identifier is literal.

**GLM-4.7 is the decisive witness that the residual is not about identifiers at all**: its
FULL_ALIAS arm had *zero* fabrications and *zero* glyph leaks, yet still fell 17 → 14, and
protection moved it only to 15 with hard regressions unchanged at 6. There was no identifier
corruption there to repair; the loss came from somewhere else.

So the two failure modes separate cleanly:

```
%q1 aliasing
 ├─ referential corruption (fabricated ids, glyph leaks)  -> CAUSED by identifier aliasing
 │                                                           FIXED completely by protection
 └─ reduced reading thoroughness (missing witnesses)      -> NOT caused by identifier aliasing
                                                             survives protection
```

## Token economics

| arm | cold o200k | saving vs RAW |
|---|---|---|
| RAW_VALID | 8552 | — |
| QODEC_FULL_ALIAS | 6651 | 1901 (22.2%) |
| QODEC_LITERAL_IDENTIFIERS | 7097 | 1455 (17.0%) |

**`retained_saving_fraction = 0.765`.** Protecting every identity costs ~23% of the original
gain. The "quality returned only because we stopped compressing" branch is therefore closed:
protection keeps a substantial, real token win.

## Caveats

1. **No reasoning**, identically in every arm and model (as in L1). This measures direct
   legibility without a deliberation subsidy; a large reasoning budget was untested and was
   operationally infeasible (provider gateway 502s in L1).
2. **Development scale** — 3 tasks, 11 questions, N=3, 33 cells/model/arm, 132 pooled per arm. Strength
   comes from the consistent direction across four families and from two axes going to
   exactly zero, not from sample size.
3. **Provider identity**: routing identity verified by the qualified boundary
   (`requested == reported`, no substitution on any attempt), but underlying community-hosted
   weights are not independently attested; model self-identification was inconsistent and is
   treated as non-authoritative output. A limit of provider identity assurance, not a
   substitution failure.
4. Both Qodec arms are byte-lossless (exact roundtrip verified). Every difference here is
   comprehension, not fidelity.

## What this licenses

Supported: *on case-0002, under no-reasoning decoding, protecting machine-significant
identifiers eliminated the referential corruption `%q1` aliasing causes — completely, in all
four families — while retaining 76.5% of the token win; but the representation still did not
reach quality parity with raw JSON, and the residual deficit is a reading-thoroughness cost of
the decoding grammar rather than an identifier problem.*

Not claimed: no generalization, no statement about other budgets or reasoning settings, no
claim that protected representation is production-ready.

## Next step (per protocol)

Partial rescue → **do not proceed to the real-source holdout**, and do not build
`qodec project v1`. The protected-span capability is qualified, generic, and economically
live, so it is worth keeping. But the remaining barrier is the decoding grammar's reading
cost, which is a different intervention from identifier protection. No notation was tuned
after seeing these outputs.

## Artifacts

`qb-protected-spans-qualification-receipt.json` (`02edfdb8`), `l2-freeze-manifest.json`,
`l2-protected-spans-manifest.json`, `protected-spans/`, `protected-cells/`, `frozen-prompts/`,
`l2-receipt.json` (`1f6c9113`), `models/<name>/{cells,attempts,responses}`.
Producer: qodec `experiment/deep-protected-spans-v0` @ `89d06be4`.
Harnesses: `tools/build_protected_spans.py`, `tools/l2_ab.py`.
