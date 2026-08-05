# R4B — standalone evaluation-v1 evaluator + synthetic qualification

**Implementation and tests only. The evaluator was NOT applied to case-0002; no
case-0002 `evaluation-v1.json` exists; R4C was not begun; no expected evaluation
result was created.** The ruler designed in R4A/R4A.1/R4A.2 is now built and
proven against independent synthetic fixtures.

- Base: `3a31f7deb2f7afc031f99be19c90122155f4b6a8` (R4A.2 receipt).
- **Commit A** (implementation + tests + synthetic fixtures + README):
  `49c758fd8349a6af52e0b6b085bc370048a8c33f` / tree `73738598dbfdebdbc86c3f113dbf38ceb6ff058e`.
- Contract revision 3 (`8f5060f0…` / canonical `9784516d…`) and the three rev3
  schemas are consumed unchanged; no design artifact was modified.

## What was built

`research/b1-context/tools/evaluate_v1.py` (CLI) over
`research/b1-context/tools/o7b1/evaluate_v1.py` (module), reusing `o7b1/canonical.py`
and `o7b1/validity.py`. It is **contract-driven, deterministic, structural**:

- validates the exact input closure — dual-digest structured inputs, artifact-bytes
  + JSONL bodies (UTF-8, non-empty lines, record count = valid lines; blank/malformed
  fail closed);
- runs contract-input consistency **gates 01–11** independently, plus the declared
  verifier-only constraints (pair endpoint distinctness, N-choose-2 completeness of
  the contract pair list and the output matrix, manifest path-uniqueness,
  entrypoint-in-closure, metric-key completeness);
- computes the frozen structural gates: question-observation support; relation
  support (`edge_witness` — every exact gold edge present, none fabricated, no
  materialization fields; `all_current_targets_materialized` — every current target
  present, materialized, none fabricated, depth exactly 1); projection validity
  reusing v0 meanings with byte usage **recomputed** from the selected list
  (`sum(len(canonical_json_bytes(o)) + 1)`), never trusting a serialized `used_bytes`;
  stale-state safety (four classifications, only `selected_as_current_or_pending`
  fails); all-pairs task dependence with directional `left_strict_superset` and
  `incomparable`, plus recomputed `difference_source`;
- applies the frozen outcome lattice (required FAIL → FAIL; else required
  UNAVAILABLE → UNAVAILABLE; else PASS; optional negative-control diagnostics never
  move overall; no `PARTIAL`);
- honors the negative-control boundary: absent NC → arm `NOT_APPLICABLE`,
  diagnostics `NOT_APPLICABLE`; present, identity-valid NC → arm `AVAILABLE`,
  diagnostics `UNAVAILABLE` — no v0 diagnostic semantics imported as norm;
- emits canonical `evaluation-v1.json` and validates it against
  `evaluation-output-v1-r2.schema.json` before an atomic temp+rename write.

**Operational vs evaluation result are distinct.** A schema-valid artifact is exit
0 whether `overall` is PASS / FAIL / UNAVAILABLE. Nonzero exit is reserved for an
inability to emit a trustworthy artifact: bad contract schema, bad evaluator
identity, malformed invocation, undeclared input access, output-schema failure,
internal exception. `qualification`/`authoritative` modes refuse on a dirty
worktree. A present-but-contradictory input (wrong digest, wrong record count) is a
FAIL, not a refusal.

**No case literal in production code**, enforced by a test. The self-identity
manifest is built from a fixed closure (8 files: 2 evaluator sources + `__init__` +
canonical + validity + 3 schemas), validated against the manifest schema, and
mechanically checked for path-uniqueness, entrypoint membership, existence, digest
match, and **no unlisted import** via an AST scan of the evaluator's own source
(pollution-free, unlike `sys.modules`).

## Qualification (from the clean Commit A checkout)

- **Synthetic qualification matrix: 44 / 44 cases pass** — 7 PASS positives (incl.
  edge-witness stale endpoint, all-current materialized, three-task N-choose-2
  matrix, record-budget boundary, present valid negative control, budget-only
  difference-source), **8 operational refusals** (duplicate/extra/malformed mapping,
  bad contract schema, blank/malformed JSONL, missing input path, dirty-worktree
  qualification), and **29 axis failures** covering input integrity, every gate
  01–11, the verifier constraints (self-pair, missing pair, duplicate unordered
  pair), task-dependence shape violations, all eight projection-validity failures,
  observation support, relation edge-witness/fabricated/unmaterialized, and
  stale-state. Each case records fixture → intended oracle → observed oracle →
  operational_exit → evaluation_overall → pass, and the matrix distinguishes
  evaluator failure (refusal) from fixture failure (axis FAIL) from scoring.
- **Determinism**: two runs from clean output directories, and reversed input-argument
  order, produce **byte-identical** `evaluation-v1.json` and identical stdout/stderr;
  output digest and `execution_checkout_commit` (= Commit A) recorded.
- **Regression**: full suite `Ran 127 tests … OK`; case-0001 CAS-backed verify
  **PASS** with `report.json` digest `sha256:52f9d2be…` unchanged; worktree clean
  after all runs; Commit A added only new files (README.md the sole modification).

See `synthetic-qualification-matrix.json`, `synthetic-determinism-record.json`,
`regression-record.json`, `evaluator-implementation-manifest.json`, and
`r4b-receipt.json`.

## Forbidden — confirmed not taken

No `evaluate.py` edit; no evaluator-v0 change; no `run_case`/pipeline/report
integration; no case-0002 `evaluation-v1.json`; no expected evaluation result; no
selector/task/relation/budget/pair-shape tuning; no Qodec arm or holdout; qodec
PR #16 untouched; no PR opened or merged.

## Next

R4C (separately authorized) is now very narrow: take this exact Commit A evaluator,
the exact rev3 contract, and the frozen R3.1 inputs, and emit one standalone
`evaluation-v1.json`. No projection, extraction, or design moves.
