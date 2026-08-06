# R4B.1 — evaluator-v1 contract-conformance correction

**Local implementation/test fix. No new design; no contract or schema semantics
changed; the evaluator was NOT applied to case-0002; R4C remains blocked.** This
round corrects four production defects the original R4B synthetic tests had frozen
as a simplified implementation (they proved conformance to the *tests*, not to the
frozen contract).

- Base: `251cda45…` (R4B receipt). Original R4B: implementation `49c758fd…`,
  receipt `251cda45…` — both preserved unchanged.
- **Commit A** (corrected evaluator + tests + fixtures + README):
  `8fc4d749b8aeaa4518e271a9ea53d3e41a040bf4` / tree `bd6aa0cb22b836d1ddf5e5d0a35da2bdb6cd31cd`.
- Contract revision 3 (`8f5060f0…` / `9784516d…`) and all schemas consumed
  **unchanged** — the truthful source-compilation representation fit the existing
  field names, so no schema change was needed.

## The four defects, corrected

1. **Gates 06/07/08 now enforce the frozen contract literally.** The question files'
   `required_relation_paths` and question-level `forbidden_stale_as_current` are the
   source of truth:
   - **gate-06** — every legacy question relation path must have an explicit contract
     requirement of the exact shape (outgoing, depth 1, match all). Removing an
     expansion whose question still declares the path now FAILs, even when every
     remaining requirement is internally valid.
   - **gate-07** — every explicit contract requirement must have a matching
     question-level path. A requirement on an existing gold edge, attached to an
     existing question that does not declare the path, now FAILs (a gold edge existing
     is not sufficient).
   - **gate-08** — exact set equality between contract and question
     `forbidden_stale_as_current` per (task, question); omission, addition,
     wrong-question and post-normalization duplicates all FAIL.
   Gold-graph grounding is preserved under **accurately named** oracles: gate-09
   (targets recomputed exactly), gate-10 (target status), and
   `consistency-forbidden-stale-grounded` (forbidden ids exist in gold and are stale).
   The last is proven independent of gate-08 by a case where the sets are equal but
   the id is current.
2. **gate-01** fails on a *missing* fixture id — missing values are no longer dropped
   before comparison.
3. **source_compilation uses its named sources.** `requested_bytes` / `requested_digest`
   are the RAW source bytes/digest from the fixture manifest `raw_sources`; availability
   comes from the frozen report's `input_digests`. `cas_resolvable` is OK only when the
   report records the raw source OK and its digest+length match the manifest (MISMATCH /
   UNAVAILABLE otherwise); `round_trip` is true only when availability is OK and every
   bound derived session reproduces its body. A plane-record raw source is valid without
   a derived body. Raw and derived evidence are never conflated. Field names preserved,
   populated truthfully, and documented in `metric_semantics`.
4. **Negative-control body identity.** A new `negative_control:<id>=PATH` input role,
   validated against the fixture manifest's declaration: no NC → `NOT_APPLICABLE`;
   declared + valid body → arm `AVAILABLE`; declared but body absent or contradictory →
   `UNAVAILABLE`; more than one declaration fails closed. Diagnostics stay `UNAVAILABLE`
   (no v0 diagnostics imported) and never move a required axis. The former metadata-only
   positive is now an adversarial `UNAVAILABLE` case.

Plus: **arm availability requires the whole set** — `full_derived` AVAILABLE only when
every selector session has a valid body; `projection` AVAILABLE only when every task has
a digest-valid context (partial → UNAVAILABLE). And an **empty** `required_observation_ids`
yields coverage 1.0 / `fully_supported` true (vacuous structural support), exercised by a
relation-only question.

## Qualification (from the clean Commit A checkout)

- **Synthetic matrix: 55 / 55 cases pass** — 6 PASS positives, 8 operational refusals,
  40 axis failures, 1 UNAVAILABLE (one-of-two derived bodies). Base gates 06/07/08 and
  the gold-grounding consistency gate all PASS on the valid fixture; gates 06/07/08 are
  tested literally (removed expansion, invented requirement, forbidden omission/addition)
  with their exact contract-vs-question evidence recorded in the matrix.
- **Determinism**: byte-identical output across two runs and reversed argument order,
  identical stdout/stderr, `execution_checkout_commit` = Commit A.
- **Regression**: full suite `Ran 127 tests … OK`; case-0001 CAS-backed verify PASS with
  `report.json` digest `52f9d2be…` unchanged; worktree clean after all runs.

## Forbidden — confirmed not taken

No case-0002/R3.1 evaluation; no contract or schema semantic change; no evaluator-v0
change; no `run_case`/pipeline/report integration; no expected result; no Qodec arm or
holdout; qodec PR #16 untouched; no PR.

## Next

R4C (separately authorized) applies this exact Commit A evaluator + rev3 contract to the
frozen R3.1 inputs to emit one standalone `evaluation-v1.json` — the baseline verdict.
