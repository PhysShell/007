# Gortex admissibility — follow-up work

Status: planned R&D follow-up

This note records the agreed execution order so the current evaluation work does not dissolve into unrelated side tracks.

## Immediate order

1. Close the remaining Package 1 instrument defects before adding more cases:
   - make the TypeScript independent-oracle claim executable rather than treating `tsc --strict` as a reference/caller oracle;
   - repair `case-0003-interface-dispatch` so the runtime-selected implementor actually flows into the call under test, and make the closed-world fixture assumption explicit;
   - strengthen `case-0006-generated-members` with a deterministic miniature codegen check if practical, so persistence/safety is demonstrated rather than asserted.
2. Build Package 2: index lifecycle.
3. Freeze the fixture corpus as v1 once Package 1 and Package 2 self-check cleanly.
4. Open a draft PR for the frozen measurement instrument.
5. Only then install/run Gortex against the corpus and produce the first observer-admission measurement.

Do not expand Package 1 further before the first real Gortex run unless a defect in the measuring instrument requires it.

## Package 2 — index lifecycle

Package 2 is not optional cleanup. It tests whether a previously valid observation loses admissibility when the indexed world changes.

Minimum lifecycle scenarios:

```text
fresh indexed symbol
file modified after observation
symbol deleted after observation
file or repository disappears after indexing
```

Each scenario should separate these questions:

```text
graph freshness
staleness signaling
old-observation invalidation
action admissibility after invalidation
```

A fast reindex is not sufficient. If an agent consumed evidence at state A and the workspace moved to state B, the A-bound observation must become stale or otherwise non-admissible even if the graph has already converged to B.

Package 2 should therefore test both current graph state and what happens to evidence already consumed by a session, including any Gortex `stale_refs` / working-set notifications that are available at measurement time.

## Boundary after Package 2

The intended milestone is:

```text
Package 1 instrument repaired
+ Package 2 lifecycle fixtures complete
+ self-check clean
= corpus v1 frozen
```

At that point the work changes phase from instrument construction to measurement. The next artifact should be a draft PR and then a first Gortex run, not another round of speculative fixture growth.

## Separate deferred track: GCX1

GCX1 is deliberately not part of this corpus. It is a separate Qodec research object concerned with compact, round-trippable representation rather than code-graph admissibility.

The corresponding follow-up is tracked in `PhysShell/qodec` on a separate branch. It should compare GCX1 against JSON and relevant Qodec/native compact representations on at least:

- exact round-trip fidelity;
- canonicalization / determinism;
- bytes and token counts across selected tokenizers;
- parse / serialize cost;
- malformed-input behavior;
- pathological fixtures;
- schema-evolution behavior.

Whether Gortex itself passes this admissibility harness must not gate that experiment, and the GCX1 experiment must not delay the Package 2 → corpus-v1 → first-run sequence here.
