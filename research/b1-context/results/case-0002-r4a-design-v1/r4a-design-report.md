# R4A — evaluation-contract v1 design freeze

**Design-only. No evaluator code was written or executed; no scored result; no expectation.**

Bound to the source-complete R3.1 baseline (artifact `3bf00a67`, receipt `0f78e1cd`,
report `sha256:7f72133a…`). `case_role: development`, `generalization_claim_allowed: false`.

## What was frozen (design)

- **Capability-based arms** — `full_derived` + `projection` required, `negative_control`
  optional. Missing optional → `NOT_APPLICABLE` (never zero, never `PARTIAL`, never a
  ceremonial `PASS`); missing required → `UNAVAILABLE`. No synthetic negative-control arm.
  The gold state is an oracle, not an arm.
- **Separated metric families** — source compilation; **structural** observation support
  (not semantic recall / not answerability, no LLM judge); projection validity (reusing
  v0 meanings, selector unchanged).
- **Depth-1 relation requirements** — `edge_witness` (audit supersession → stale targets)
  and `all_current_targets_materialized` (impact `depends_on` → current targets). Targets
  derived from the **frozen gold graph**, never from observed output; fabricated edge fails
  closed; a relation-only stale reference is not "current".
- **Forbidden stale-as-current** — 4-way classification; only `selected_as_current_or_pending`
  fails.
- **All-unordered-pairs task dependence** — intent shapes only (resume/audit
  `materially_different`, audit/impact `left_strict_superset`, resume/impact
  `materially_different`); no exact IDs/counts frozen.
- **Budget** bound to `budget-v0.yaml` (digest), values unchanged (20000/32), no tuning.
- **Outcome lattice** `PASS | FAIL | UNAVAILABLE`; `PARTIAL` not used in eval v1.
- **Standalone `evaluation-v1.json`** output, computable from frozen inputs without
  rerunning extraction/projection.

## Deliverables

`schema/evaluation-contract-v1.schema.json`, `docs/evaluation-contract-v1.md`,
`fixtures/case-0002/evaluation-contract-v1.yaml` (Commit A `cec6b39e` / tree `489abdf4`);
this decision record + report (Commit B).

## Unresolved design issues (recorded, not invented)

byte-budget counted representation; `full_derived` structural-support metric; multi-hop
relations (out of scope, depth-1 only). Carried to R4B.

## Migration

Eval v0 stays for case-0001 (no case-0001 bytes change); R3/R3.1 immutable; eval v1
produces a separate artifact; `report.json` integration needs a later explicit
report-contract revision (`o7.b1.report/v1` NOT redefined).

## Next

R4B builds a standalone evaluator against synthetic fixtures first; R4C applies it to the
already-frozen R3.1 bytes; R4D is optional report integration. Ruler designed here; not yet
built or used.
