# R4B.2 — remove the non-contract consistency gate

**Local implementation/test correction. No contract or schema change; not applied
to case-0002.** R4B.1 added a gate, `consistency-forbidden-stale-grounded`, and
aggregated it into `contract_input_consistency`. That gate is **not** one of the
frozen contract's eleven gates and contradicts the frozen stale-state semantics. It
is removed here, with no replacement.

- Base: `466cdae9…` (R4C receipt). R4B.1 implementation `8fc4d749…` preserved.
- **Commit A** (corrected evaluator + tests + fixtures):
  `58a7a1113c8a649b6952f56b2782d01f887ff3ed` / tree `054dde669d118a5336177ea602653626961c0eee`.
- Rebuilt 8-file self-manifest: canonical `14abcb83…`, artifact `ccd3c8e7…`,
  entrypoint `e41eba05…` (unchanged), module `d5e8bc06…`.

## Why the gate was wrong

The frozen contract's gate-08 checks *exact equality* between the contract and
question-level `forbidden_stale_as_current` sets; it does **not** require every
forbidden observation to be stale in gold. A forbidden observation may legitimately
be in-force when it is non-authoritative — e.g. an `agent_claim` with status
`pending` — which is exactly why a task forbids presenting it as current state. The
removed gate flagged that valid situation as a contract inconsistency, inventing a
normative rule after the ruler was frozen.

`obs-reviewer-acceptance-claim` (authority `agent_claim`, status `pending`) is the
case-0002 instance: it must stay absent from authoritative projection, but its
pending gold status is not an input-consistency defect. Stale-state safety (fails
only on a forbidden id *selected* as current/pending) and projection validity
(authority / in-force checks) already enforce the real requirement.

## What remains

`contract_input_consistency` now aggregates exactly the frozen inputs: gate-01…11,
`verifier-pair-distinct`, `verifier-pair-complete`, `verifier-matrix-complete`, and
`gate-input-integrity` when applicable. gate-08, gate-11, the four-way stale-state
classification, and projection-validity checks are all preserved. The base-fixture
gate id set is exactly the eleven; the removed gate is absent.

## Qualification (clean Commit A checkout)

- **57 / 57 synthetic cases pass.** New cases prove the corrected semantics: a
  pending non-authoritative agent claim forbidden and absent → `contract_input_consistency`
  PASS, `stale_state_safety` PASS, classification `absent`; the same claim selected →
  `stale_state_safety` FAIL and `projection_validity` FAIL; a superseded forbidden id
  absent → PASS; forbidden-set mismatch → gate-08 FAIL; an unknown forbidden id →
  gate-11 FAIL; a relation-only stale endpoint → `stale_relation_endpoint_only`. The
  old removed-gate case was deleted.
- Byte-deterministic (two runs + reversed argument order); full suite `Ran 127 tests …
  OK`; case-0001 CAS-backed verify PASS, `report.json` digest `52f9d2be…` unchanged.

## Not taken

No contract/schema change; no evaluator-v0 change; not applied to case-0002; no
expected result; qodec PR #16 untouched; no PR.

## Next

R4C.1 replays the authoritative baseline from this exact Commit A. Expected effect:
`contract_input_consistency` returns to PASS; `relation_support` FAIL (the real
finding) persists; overall stays FAIL for one genuine reason.
