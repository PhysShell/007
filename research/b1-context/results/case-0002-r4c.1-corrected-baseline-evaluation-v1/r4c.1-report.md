# R4C.1 — corrected authoritative baseline evaluation

**Measurement-only replay after R4B.2 removed the non-contract gate. No contract,
schema, projection, extraction, fixture, selector or report change.** The original
R4C remains an immutable historical measurement; this reissues the verdict with the
phantom failure removed. See `r4c-erratum.json`.

## Corrected baseline classification

```
BASELINE_EVALUATION_FAIL
```

Follows `evaluation-v1.json`; not an expectation. `case_role: development`,
`holdout_evidence: false`, `generalization_claim_allowed: false`.

The overall verdict is still **FAIL** — but now for **one real reason**
(`relation_support`) rather than one real reason plus one measurement bug.

## Execution identity

- Evaluator: R4B.2 Commit A `58a7a111…` / tree `054dde66…`, clean detached worktree;
  self-manifest canonical `14abcb83…` (matched the R4B.2 record).
- Contract revision 3 (`8f5060f0…` / `9784516d…`), frozen R3.1 report `7f72133a…`,
  the exact same 19 R4C inputs and the same private CAS bodies (reviewer `374bbac6`
  1,101,185 B / 303 rec; coder `56a294f8` 4,237,924 B / 2,081 rec).
- **Two-run authoritative determinism**: run 1 and run 2 (reversed `--input` order)
  both exit 0, **byte-identical** `evaluation-v1.json`, identical stdout/stderr,
  output schema independently validated.
- Corrected artifact:
  `sha256:963edb459fb742359051e3a5386ce6b6f1026e1d01b5a8a951a9b0bf99541d76` (16030 bytes),
  19 inputs.

## Result

**arms** — `full_derived: AVAILABLE`, `projection: AVAILABLE`, `negative_control: NOT_APPLICABLE`.

| axis | R4C (original) | R4C.1 (corrected) |
|---|---|---|
| contract_input_consistency | ~~FAIL~~ (invalid) | **PASS** |
| source_compilation | PASS | PASS |
| projection_validity | PASS | PASS |
| question_observation_support | PASS | PASS |
| relation_support | **FAIL** | **FAIL** |
| stale_state_safety | PASS | PASS |
| task_dependence | PASS | PASS |
| negative_control_diagnostics | NOT_APPLICABLE | NOT_APPLICABLE |

**overall: FAIL.**

`contract_input_consistency` returns to PASS: gates 01–11 and the pair verifiers all
pass, and the invalid `consistency-forbidden-stale-grounded` gate is gone. In
particular `obs-reviewer-acceptance-claim` (authority `agent_claim`, status
`pending`) is correctly treated as an in-force but non-authoritative claim — forbidden
from authoritative projection (stale-state / projection-validity concern), not an
input-consistency defect.

### The single real failure — relation_support

Audit task `qa3-superseded` requires three `edge_witness` supersession witnesses; the
frozen R3.1 projection carried **one of three**:

| from | kind | gold edge | present | satisfied |
|---|---|---|---|---|
| obs-round0-closed | supersedes | obs-plane-record-not-frozen-v6 | — | **no** |
| obs-reviewer-durability-resolved | supersedes | obs-reviewer-durability-unresolved-v6 | — | **no** |
| obs-profiler-fail-closed | supersedes | obs-profiler-prior-defect | obs-profiler-prior-defect | yes |

No fabricated edges. This is the genuine, non-theoretical baseline result: **perfect
structural question-ID coverage and valid selected records, yet two supersession
witnesses needed to explain evidence history are lost.** Plain observation coverage
reported "everything supported"; relation-aware evaluation found the missing
evidentiary structure. (Structural — not semantic recall or answerability.)

## Freeze + provenance

`evaluation-v1.json` in private CAS
(`cas:sha256:963edb45…`, verify OK) and encrypted offsite (restic→R2 snapshot
`cf002ed5`, parent `4e504424`). Private derived bodies not committed. Original R4C
files unedited; corrected via erratum.

## Not taken

No evaluator-code / contract / schema change in this round; no extraction or
projection rerun; no R3/R3.1 or original-R4C edit; no expected result; no
`run_case`/report integration; no Qodec generation or scoring; no holdout; qodec
PR #16 untouched; no PR.

## Next — methodology track closed

The scale is built, calibrated, and the frozen baseline verdict is on record:
**FAIL, for one real reason.** The Qodec arm now has one concrete job — recover the
two missing audit supersession witnesses (`obs-round0-closed → obs-plane-record-not-frozen-v6`,
`obs-reviewer-durability-resolved → obs-reviewer-durability-unresolved-v6`) without
breaking validity, budget, task differentiation, or stale-state safety. No more ruler
design.
