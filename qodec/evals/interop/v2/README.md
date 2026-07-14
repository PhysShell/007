# Qodec Interop Benchmark v2 — coverage design & held-out contract

**Status:** `frozen-before-data` · **Contract:** `interop-benchmark-v2` ·
**Base commit:** `6b3d3d78d1dc04253842b1c15146ac0b477d59cd`

This directory is **design-and-validation only**. It defines *what the v2
benchmark must contain and how it will be judged*, and ships a validator plus
tests that enforce that design. It contains **no real fixtures, no model
answers, and no sealed material**. No model calls are made anywhere in this
scope.

The policy under test is **VG** (codec `fold-grep-guarded`), evaluated as a
*lossless notation layer*. The already-accepted verdicts it inherits are
unchanged by this scope:

```
VG FULL L2 CANDIDATE:        ACCEPTED
PRODUCTION SQUEEZE:          REJECTED
VG PRODUCTION PROMOTION:     NOT DECIDED
```

## Layout

```
v2/
  README.md                     — this file
  benchmark-contract.json       — status card (frozen-before-data)
  coverage-matrix.json          — frozen numeric gates, taxonomy, split policy (digest-protected)
  heldout-policy.md             — sealed held-out lifecycle & freeze contract
  external-baselines.md         — how external benchmarks are (and are not) used
  schemas/
    case.schema.json            — one base case
    question.schema.json        — one rule-scored question
    sealed-manifest.schema.json — the ONLY sealed artifact allowed in Git
  validate_contract.py          — quota/leakage validator (no model calls)
  tests/
    test_contract.py            — validator tests on a synthetic quota-complete manifest
  private/                      — gitignored; real sealed bundle lives here, never committed
```

`qodec/evals/interop/v2/private/` is git-ignored. **No file under `private/`
ever enters Git.**

## Frozen corpus target

| Axis | Target |
|---|---|
| Base cases | **48** (≥) |
| Rule-scored questions | **240** (≥) |
| Primary output families | **12**, 4 cases each |
| Split | 24 public-development · 12 public-validation · 12 sealed-heldout |
| Per family | 2 dev · 1 val · 1 sealed; ≥2 real, ≥1 adversarial synthetic, ≥1 success, ≥1 failure/mixed, ≥1 payload ≥ medium, ≥1 payload large/xl |
| Ecosystems | ≥6 used; `.NET` and Rust mandatory; ≥4 cases per non-neutral; ≤30% of cases *or* questions per ecosystem |

**Families:** compiler-build · test-runner · lint-static-analysis ·
search-listing · git-diff-history · exception-stacktrace · application-ci-log ·
dependency-package · structured-data-query · container-orchestrator ·
network-api · code-exploration-callgraph.

There is exactly **one primary family** per case. Extra properties are `tags`,
never a `misc` family.

**Outcome quotas** (each case has exactly one primary outcome): success-clean 8
· warning-only 6 · single-failure 8 · multi-failure 8 · mixed-warning-failure 6
· empty-or-no-match 4 · timeout/cancel/truncated/malformed 8.

**Size buckets** (target-tokenizer tokens, not bytes; tokenizer identity + SHA
are part of the manifest): tiny `<256` (≥6) · small `256–1023` (≥10) · medium
`1024–4095` (≥12) · large `4096–16383` (≥12) · xl `≥16384` (≥8).

**Hazard quotas** (≥4 cases each): duplicate-basename · windows-path ·
unicode-or-combining · ansi-or-progress-output · crlf · hostile-qodec-markers ·
conflicting-old-and-new-facts · sanitized-secret-like-values · nested-repetition.
Secrets are synthetic or irreversibly sanitized.

**Question taxonomy** (minimums over 240): exact-retrieval+locator 40 · count 20
· exact-set 20 · relation 30 · ordering 20 · comparison 20 · negative-evidence
20 · causality 12 · actionability 12 · cross-section-synthesis 12. Axes:
cross_section ≥80 · disambiguation ≥48 · absence_required ≥24 · critical ≥120.
Critical categories: exact full-path locator, relation, ordering,
negative-evidence, actionability. Gold is grounded in the raw payload; **the
core benchmark uses no LLM judge.** Match policies are explicit and versioned:
exact · exact-set · one-of · contains-all · ordered-path · numeric · boolean ·
relation-set.

## Benchmark arms

- **Primary semantic benchmark (promotion gate):** `raw`, `raw+brief`, `VG+brief`.
- **Diagnostic subset (not a gate):** `V+brief` — runs only on a preselected
  attribution subset, to separate structural shelf from guarded mining.
- **Interoperability experiment (kept apart from the transparency gate):**
  `native tool output`, `RTK-reduced output`, `RTK-reduced output + VG`. RTK is a
  *lossy reducer*; VG is scored as a *lossless notation layer*, so the two are
  never mixed into one number.

A separate **metamorphic robustness suite** (≥8 public source cases → ≥24
position variants, ≥48 total variants; gold answer unchanged) sits outside the
48 base cases and is scored separately.

## Gate summary

- **L0 (all cases):** exact byte roundtrip 100% · deterministic encoding 100%
  (≥3 consecutive encodes) · 0 decoder/encoder crashes · 0 alias collisions · 0
  accepted-artifact token regressions · no-gain ⇒ passthrough (never negative
  savings).
- **L1 (target tokenizer):** mean/median savings > 0 · p10 ≥ 0 · every accepted
  case tokens_out < tokens_in · tokenizer parity spread ≤ 8 · exact roundtrip all
  cases. Runtime is a *required observation*: preprocessing time must beat
  measured inference time saved on ≥75% of non-passthrough promotion cases on
  pinned hardware — latency is neither an advertising win nor a hidden fail.
- **L2 candidate eligibility:** raw competence ≥60% · eligible overall ≥50% ·
  parity ok · alias leaks 0 · invalid-identifier Δ ≤0 · malformed Δ ≤0 · exact
  roundtrip · mean & median savings > 0.
- **L2 public candidate:** overall retention ≥98% · per-family ≥95% · 0 stable
  critical losses · 0 unstable critical · unresolved-unstable ≤2%.
- **L2 sealed promotion:** 0 stable codec losses · 0 stable critical losses · 0
  alias leaks · invalid Δ ≤0 · malformed Δ ≤0 · overall retention 100% · exact
  roundtrip 100% · mean & median savings > 0.
- **Cross-reader (before production promotion):** 1 primary (full 240) · 1
  secondary local (stratified) · 1 strong (stratified) · ≥2 tokenizer families.
- **Agent A/B pilot (documented only, not run here):** 18 tasks × 2 conditions ×
  3 repeats = 108 sessions, with its own frozen pilot gate.

Gates cannot be changed after the first scored request. See
`coverage-matrix.json` for exact numbers.

## Using the validator

```bash
python validate_contract.py MANIFEST.json          # exits non-zero on any violation
python validate_contract.py MANIFEST.json --coverage coverage-matrix.json --base-dir DIR
```

The validator checks schema validity, unique IDs, SHA formats, split/family/
ecosystem/outcome/size/origin/hazard quotas, question-category and axis quotas,
per-case question requirements, sealed-content leakage, public path existence
and gate immutability — **without any model, tokenizer, qodec or RTK
dependency**.

```bash
python -m unittest discover -s tests -p 'test_contract.py'
```

## Contract immutability

Numeric gates, taxonomy and split policy are frozen and digest-protected in
`coverage-matrix.json`. Once the first v2 results appear, any change to them
requires a **new `contract_version`**. Old results are always judged by the
contract they were produced under.

## Scope M1 — execution substrate & RTK comparison (non-gating addendum)

Scope M1 adds a Nix execution substrate and an RTK↔qodec comparison, **without
changing any frozen numeric gate** in `coverage-matrix.json` (its
`contract_version`, `gates_digest`, quotas, gates, `agent_ab_pilot` and
`results_ledger` are untouched). New material:

- **Execution environment:** [`execution-environment.md`](execution-environment.md)
  — reproducibility identity + Nix-as-canonical policy.
- **Nix / GitHub Actions:** root [`/flake.nix`](../../../../flake.nix) adds
  `packages.qodec`, `packages.rtk-pinned` (built from a pinned RTK commit — no
  mutable release binary), `devShells.qodec-bench`, apps and checks; the
  workflow [`/.github/workflows/qodec-v2.yml`](../../../../.github/workflows/qodec-v2.yml)
  runs them read-only, SHA-pinned, with no model calls.
- **RTK source audit:** [`rtk-implementation-map.md`](rtk-implementation-map.md),
  [`rtk-output-grammar.json`](rtk-output-grammar.json),
  [`rtk-qodec-composition-risks.md`](rtk-qodec-composition-risks.md).
- **Comparison contract (non-gating):**
  [`rtk-comparison-contract.json`](rtk-comparison-contract.json) — four logical
  arms (RAW, QODEC, RTK, RTK+QODEC), two separate leaderboards (transparency
  excludes RTK; utility includes all four), all tokens via the same real target
  tokenizer.
- **Dataset map:** [`dataset-source-map.md`](dataset-source-map.md) — candidate
  sources investigated (not downloaded), with ingest decisions.
- **Non-scoring smoke suite:** [`smoke/`](smoke/) — NON-BENCHMARK, NON-GATING,
  not part of the 48 base cases or held-out; proves qodec losslessness and
  token accounting over arbitrary and RTK-shaped input.

## Scope N0 — reproducible corpus compiler (compiler-only, zero benchmark cases)

Scope N0 adds a reproducible corpus compiler under [`corpus/`](corpus/) that
defines, captures, verifies and regenerates benchmark case bundles. It builds
**only the compiler** — it contains **zero** real Benchmark v2 cases and ships
one NON-BENCHMARK demonstration bundle. Contract:
`interop-corpus-compiler-v1` (see [`corpus/corpus-contract.json`](corpus/corpus-contract.json),
[`corpus/README.md`](corpus/README.md), [`corpus/decisions.md`](corpus/decisions.md)).
Raw and RTK snapshots are canonical corpus evidence; qodec/VG/hybrid outputs are
derived and rejected inside a bundle.
