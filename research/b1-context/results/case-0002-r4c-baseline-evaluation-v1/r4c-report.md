# R4C — authoritative standalone baseline evaluation

**Measurement-only round. Nothing was modified — evaluator, contract, schema,
projection, extraction, fixture, selector and report are all untouched.** This is
the first content-bearing result: whether the frozen R3.1 baseline satisfies the
frozen baseline contract v1.

## Baseline classification

```
BASELINE_EVALUATION_FAIL
```

This follows `evaluation-v1.json`; it is **not** an expectation. The frozen R3.1
projection does **not** satisfy the frozen baseline structural contract v1 on two
required axes.

`case_role: development`, `designed_after_observing_case_0002: true`,
`holdout_evidence: false`, `generalization_claim_allowed: false`. A FAIL here — like a
PASS — establishes only a development-structural fact; it does not prove or disprove
Qodec benefit.

## Execution identity

- Evaluator: corrected R4B.1 Commit A `8fc4d749…` / tree `bd6aa0cb…`, run from a clean
  detached worktree; self-manifest canonical `40cd3b55…` matched the R4B.1 record.
- Contract revision 3: artifact `8f5060f0…`, canonical `9784516d…`.
- Only frozen R3.1 artifacts + the two CAS-materialized private derived bodies were used.
- **Two-run authoritative determinism**: run 1 and run 2 (reversed `--input` order) both
  exit 0 and produce **byte-identical** `evaluation-v1.json`, identical stdout/stderr.
- Evaluation artifact: `sha256:ab95f39e0c8d389f326f097ec177c5c22a751da016e173796a8dde645dd882df`
  (16030 bytes), 19 input artifacts; output schema independently validated both runs.

## The result, in full

**arms** — `full_derived: AVAILABLE`, `projection: AVAILABLE`, `negative_control: NOT_APPLICABLE`.

**outcome_axes**

| axis | value |
|---|---|
| contract_input_consistency | **FAIL** |
| source_compilation | PASS |
| projection_validity | PASS |
| question_observation_support | PASS |
| relation_support | **FAIL** |
| stale_state_safety | PASS |
| task_dependence | PASS |
| negative_control_diagnostics | NOT_APPLICABLE |

**overall: FAIL.**

### Why contract_input_consistency FAILs

Gates 01–11 and the pair verifiers all PASS. The failing check is the gold-grounding
consistency gate added in R4B.1, `consistency-forbidden-stale-grounded`:

> `case-0002-audit-r21-evidence-provenance` forbids `obs-reviewer-acceptance-claim` as
> stale-as-current, but that observation is **in-force (current)** in the frozen gold
> state.

The contract and the question file agree on the forbidden set (gate-08 PASS); the
disagreement is between the forbidden list and the gold status of that observation. This
is a real inconsistency in the frozen baseline, surfaced — not remediated.

### Why relation_support FAILs

Audit task `qa3-superseded` carries three `edge_witness` supersession requirements. The
R3.1 projection materialized **one of three**; the other two required gold edges are
absent from the projected context (no fabrication):

| from | kind | gold edge | present | satisfied |
|---|---|---|---|---|
| obs-round0-closed | supersedes | obs-plane-record-not-frozen-v6 | — | **no** |
| obs-reviewer-durability-resolved | supersedes | obs-reviewer-durability-unresolved-v6 | — | **no** |
| obs-profiler-fail-closed | supersedes | obs-profiler-prior-defect | obs-profiler-prior-defect | yes |

Impact task `qi1-dependencies` (`all_current_targets_materialized`) is **satisfied**: all
three current targets (`obs-output-path-dependence`, `obs-repo-authority-04108e7`,
`obs-window-invariant`) present, materialized, none fabricated.

### Source compilation (PASS)

Raw sources, from the fixture manifest (raw bytes/digests) with availability from the
frozen report — never derived-body evidence:

| id | requested_bytes (raw) | cas_resolvable | round_trip |
|---|---|---|---|
| reviewer-export | 5,963,644 | OK | true |
| coder-prefix | 87,564,394 | OK | true |
| plane-record | 94,084 | OK | true |

Derived-session completeness (bodies reproduce): `reviewer` — present, byte/sha256/record
all match; `coder` — present, byte/sha256/record all match.

### Question observation support (PASS)

Every question in all three tasks is `fully_supported` with `required_observation_coverage
= 1.0` and no missing required observations. (Structural support — **not** semantic recall
or answerability.)

### Stale-state safety (PASS)

- audit/qa4: `obs-plane-record-not-frozen-v6` → absent; `obs-reviewer-acceptance-claim` →
  stale_relation_endpoint_only; `obs-reviewer-durability-unresolved-v6` → absent.
- resume/qr2: `obs-plane-record-not-frozen-v6` → stale_relation_endpoint_only.

No forbidden observation is selected as current/pending, so the axis passes (this is
independent of the gold-grounding consistency gate above).

### Task dependence (PASS)

| pair | expected | satisfied | set_eq / L⊃R / R⊃L | symdiff | difference |
|---|---|---|---|---|---|
| audit, impact | left_strict_superset | yes | F / T / F | 1 | selector_relevance |
| resume, impact | incomparable | yes | F / F / F | 13 | selector_relevance |
| resume, audit | incomparable | yes | F / F / F | 14 | selector_relevance |

## Freeze + provenance

`evaluation-v1.json` stored in private CAS
(`cas:sha256:ab95f39e…`, verify OK) and encrypted offsite (restic→R2 snapshot
`4e504424`). Run captures were empty (no stdout/stderr) and stored. Private derived
bodies are **not** committed.

## Not taken

No evaluator/contract/schema change; no extraction or projection rerun; no R3/R3.1
modification; no expected evaluation result; no selector/question/relation/budget/pair
tuning; no `run_case`/report integration; no R4D; no Qodec arm/holdout; qodec PR #16
untouched; no PR. The FAIL was **not** remediated.

## Next

The methodology track closes here. The frozen baseline verdict is on record. The next
authorized round is the Qodec arm — putting a product on the scale that has now been
built and calibrated.
