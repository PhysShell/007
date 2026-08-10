# L3B — dictionary-free structural representation vs raw JSON (case-0002)

**Principal: `JSTRUCT_REGRESSION_WITH_TOKEN_WIN`**

`case_role: development` · `holdout_evidence: false` · `generalization_claim_allowed: false`.
Producer qodec `0d5097a7` (tree `e5ec86fa`), route-A CI run `31415814840` fully green.
Frozen pre-results at 007 `e1c26a7`; no substitution on any of the 72 attempts.

## The question

L1 showed `%q1 deep` costs model quality. L2 showed identifier protection removes the
*corruption* half but not the rest. L3A found a saving that needs no dictionary at all.
L3B asks the only question left: with nothing to expand — no legend, no aliases, every
identifier literal, no notation block, and a shape that reads like a table — does the
representation finally hold quality?

It does not.

## Result

Both arms used **identical scaffolding**: `CONTEXT:` / `QUESTIONS:` / `OUTPUT SHAPE:`,
neither representation named, no reading tutorial for either. RAW was re-run fresh.

| model | RAW | JSTRUCT | paired hard regressions | non-inferior |
|---|---|---|---|---|
| DeepSeek-V4-Flash-0731 | 21/33 | 15/33 | 6 | no |
| Gemma-4-31B-it | 14/33 | 14/33 | 3 | no |
| GLM-4.7 | 17/33 | 17/33 | 2 | no |
| Qwen3.5-27B-Anko | 18/33 | **20/33** | 2 | no |

Pooled (132 cells per arm): correct **70 → 66**.

Token win holds: cold **8546 → 7613 = 10.92%**.

## Where it fails — one axis, everywhere

| axis | RAW | JSTRUCT |
|---|---|---|
| answer_correct | 70 | 66 |
| **missing_required_witness** | **60** | **65** |
| unsupported_or_fabricated_claim | 1 | **0** |
| representation_parse_failure | 0 | 0 |
| stale_as_current_error | 0 | 0 |
| malformed_output | 5 | **0** |
| required_relation_evidence_used | 126 | 126 |

**All 13 hard regressions, in all four families, are `missing_required_witness`.** Not one
fabrication, not one stale-as-current. jstruct is not dirtier than raw JSON on any
integrity axis and is *cleaner* on two (fabrications 1→0, malformed 5→0 — all five RAW
malformed responses were Qwen's). Relation evidence is identical at 126/132 in both arms.

The representation does not corrupt references. It costs the model evidence it would
otherwise have cited.

Qwen illustrates why the paired rule matters: it improved on every aggregate — correct
18→20, missing witnesses 15→12, malformed 5→0 — and still carries 2 hard regressions,
because individual cells newly lost a witness while others gained one. A pooled gain does
not excuse a per-model failure, exactly as frozen.

## What the three experiments now say together

```
L1   alias everything          -> references corrupted + evidence missed
L2   protect identifiers       -> corruption GONE (19→0, 23→0); evidence still missed
L3B  remove the dictionary     -> integrity clean, no legend, literal ids
                                  -> evidence STILL missed, and it is the ONLY failure
```

The boundary is located, and the precise claim is narrower than "compression is hard":

> For this evidence-heavy context, changing the structural presentation of otherwise
> equivalent JSON changes **witness-retrieval behaviour** across the tested model families.
> Canonical JSON is therefore part of the effective model-facing evidence interface, not
> merely a serialization detail.

jstruct was not unintelligible. It was intelligible enough to preserve identifiers,
relations and output integrity — and cleaner than RAW on two integrity axes — while still
changing which witnesses the model noticed. The failure is evidence navigation, not decoder
correctness. Scope: case-0002, four community-hosted families, N=3, reasoning off;
reproducible within that domain across L1/L2/L3B, which is why no L4 follows.

## Caveats

1. **No reasoning**, identical in both arms and all models (as L1/L2). Direct legibility
   without a deliberation subsidy.
2. **Development scale** — 33 paired cells/model/arm, 132 pooled. Its force is that the
   failure is one axis across four independent families and agrees with L1 and L2.
3. **Provider identity** verified as `requested == reported` on every attempt; underlying
   community-hosted weights are not independently attested and model self-description is
   treated as non-authoritative output.
4. **Prompt neutrality** required re-running RAW fresh; token accounting was recomputed from
   the final prompts (10.92%), not inherited from L3A (9.61%).
5. jstruct is byte-lossless — exact roundtrip verified through the real decoder before any
   call. The difference is comprehension, not fidelity.

## Next step (per protocol)

**The reader-facing representation line is closed for this B1 workload.** No new codec is to
be invented in response. The surviving product direction:

```
qodec project / semantic projection   +   raw JSON as the reader representation
savings from: selection, omission, retrieval, caching, budget-aware projection
```

The real-source holdout is **not** opened by this result, and `qodec project v1` is **not**
motivated by it. "The representation line closed, so build v1" would be the wrong inference —
a different causal branch. H3 independently showed a mandatory-closure overflow (9 records
against a budget of 8), so budget-aware selection remains evidence-backed future work on its
own footing.

## Surviving capabilities

| capability | status |
|---|---|
| qodec project — relation-aware projection | alive (Q1/Q2) |
| protected spans — referential integrity primitive | alive, qualified (L2, qodec `89d06be4`) |
| jstruct — dictionary-free structural codec | qualified and lossless, **not adopted** for this workload |
| `%q1 deep` — reader representation | rejected (L1) |

## Artifacts

`l3b-freeze-manifest.json` (`f1c48580`, pre-results at `e1c26a7`), `l3b-receipt.json`
(`01c44a82`), `frozen-prompts/`, `models/<name>/{cells,attempts,responses}`.
L3A: `l3a-receipt.json` (`ec314118`), `jstruct-cells/`.
Harnesses: `tools/l3a_feasibility.py`, `tools/l3b_ab.py`.
