# Q1 — case-0002 Qodec causal-decomposition arm

**First product-facing comparison round.** It separates three questions that a
single "Qodec fixed it" number would have fused: (1) can deterministic relation
closure repair the baseline's missing evidence? (2) what does that repair cost
without Qodec? (3) does Qodec reduce that cost while preserving the exact repaired
payload? Nothing frozen was modified — evaluator, contract semantics, selector v0,
gold, questions, sources, baseline projection, and the Qodec repository (PR #16
untouched).

`case_role: development`, `holdout_evidence: false`,
`generalization_claim_allowed: false`. No live model; no model-comprehension claim.

## The causal answer

```
semantic_repair:                 PASS
qodec_roundtrip:                 PASS
plain_control_structural_budget: PASS
qodec_cost_result:               COLD_AT_OR_BELOW_BASELINE
causal_conclusion:               SEMANTIC_REPAIR_IS_RELATION_CLOSURE_NOT_QODEC
```

**Deterministic relation closure repairs the baseline — with no Qodec.** The
relation-closed control (C1), evaluated authoritatively (two byte-identical runs,
corrected evaluator `58a7a111`, arm-rebound rev3 contract), is `overall: PASS` with
every axis PASS, including `relation_support`. The baseline (B0) failed exactly one
axis, `relation_support`; C1 materializes the two missing audit supersession witness
sources (`obs-round0-closed`, `obs-reviewer-durability-resolved`) and their edges,
and the failure clears. **Qodec did not — and by construction could not — cause this
repair: the witnesses were absent before any encoding.** C1 evaluation
`sha256:d8d1d5fb…`.

## Five-cell comparison

| cell | semantics | representation | verdict |
|---|---|---|---|
| **B0** | baseline (R4C.1) | raw | FAIL (`relation_support`) |
| **B0Q** | exact B0 | Qodec deep | FAIL — deep decode is byte-exact to B0, representation only |
| **C1** | relation-closed | raw | **PASS** |
| **C1Q** | exact C1 | Qodec deep | PASS — deep decode is byte-exact to C1, representation only |
| Q1R / Q1RQ | Qodec relation-aware projection | raw / Qodec | **deferred** (see below) |

Because the `deep` codec round-trips **byte-exact** and canonical-equal on every
context (all cells admissible: envelope parses, two runs byte-identical, decode
succeeds, strict token win), B0Q and C1Q carry *exactly* the B0 and C1 semantics —
they change representation, not verdict.

## Structural budget vs representation cost (kept separate)

Structural budget is computed over decoded semantic records, independent of Qodec:
C1 raw fits it (audit 8797 ≤ 20000 bytes, 12 ≤ 32 records) — **Qodec is not necessary
for structural admission.**

Representation cost (o200k tokens), aggregate over the three tasks:

| | B0 (baseline) | C1 (plain repair) | C1Q (Qodec repair) |
|---|---|---|---|
| raw tokens | 7988 | **8250** | — |
| Qodec warm | — | — | **5288** |
| Qodec cold (bundle, +261 notation) | — | — | **5549** |

Frozen cost-neutral ceilings (set before measurement): audit ceiling = B0 audit raw
= 3040; aggregate ceiling = ΣB0 raw = 7988.

- **Plain repair costs *more* than the baseline** (C1 raw 8250 > 7988; audit 3347 >
  3040) — it adds real evidence, so the raw payload grows.
- **Qodec carries the repaired payload *below the baseline raw cost*, warm and cold**
  (C1Q warm 5288 ≤ 7988; C1Q cold bundle 5549 ≤ 7988; per-audit C1Q cold 2400 ≤ 3040).
- **Strict reduction vs plain repair**: C1Q warm 5288 < C1 raw 8250.

So Qodec earns a genuine, *separately attributed* representation result — it makes
the repaired (larger) context cheaper to carry than even the original baseline — with
zero credit for the semantic fix, which relation closure performed.

## Producer + Qodec identity

- Relation-closure control producer `o7.b1.relation-closed-control/v0` (007 Commit A
  `30ca84d4`), generic, no case literal; distinct from selector v0.
- Qodec frozen main `57fc8660` / tree `ede6847c`; release binary
  `bcc2ebb9…` (16,359,800 bytes); meter o200k; **zero-profile** (no learn/train/
  profile/rules/extern legends). Qodec repository unmodified; PR #16 head `940c7629`
  untouched. *(`cargo test --locked` was interrupted by a disk-full event mid-run and
  needs a clean re-run; the interface probe, `cargo build --release --locked`, and
  round-trip admissibility all passed.)*
- Arm-rebound rev3 contract: only 5 input-identity pointers changed
  (`r3_1_report`, `r3_1_projection_comparison`, three `task_contexts`);
  `semantic-binding-equivalence.json` proves every semantic section is byte-equal
  after normalization and the rebound contract validates against the rev3 schema — no
  `Q1_CONTRACT_SEMANTIC_DRIFT`.

## Q1R / Q1RQ — deferred, on purpose

The reviewer's fifth/sixth cells (Qodec's *own* generic relation-aware projection)
require a new `qodec project` runtime primitive — a producer that takes
gold+contract+budget and emits selected observations + relation witnesses + omission
receipts through a versioned JSON contract, with producer-owned bounds, no case
literals, and its own mutation-tested qualification. That is a feature-build of its
own and is scoped as the next sub-round (Q2), on a Qodec experimental branch off
`57fc8660`. Deferring it keeps the causal boundary intact: the current result already
proves the missing component is **relation-aware projection, not compression**, and
Q2 will separately measure whether Qodec-as-backend can subsume that projection
without a hidden case oracle.

## Not taken

No selector/projector v0 change; no evaluator/schema/contract-semantics change; no
extraction or projection rerun; no R3/R3.1/R4C/R4C.1 edit; no case-id hard-coding in
producer code; no Qodec profile/legend/rules; no live model call; no model-
comprehension or generalization claim; no holdout; Qodec PR #16 untouched; no PR.

## Next

Q2: the Qodec relation-aware projection arm (Q1R/Q1RQ) on an experimental Qodec
branch, measured against this C1 control — does Qodec's own projection match or beat
the plain closure, and is any win free of a case-specific oracle?
