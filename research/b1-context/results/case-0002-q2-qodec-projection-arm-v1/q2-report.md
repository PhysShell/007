# Q2 — generic Qodec relation-aware projection (Q1R/Q1RQ)

Q2 tests whether Qodec can **own** the projection capability Q1 discovered — without
becoming a 007-shaped plugin. It answers yes, with the honest qualifier that Qodec's
projection is a *different but valid* projection, not a transliteration of the control.

`case_role: development`, `holdout_evidence: false`, `generalization_claim_allowed:
false`. No live model; no model-comprehension or generalization claim.

## Producer qualification (route A)

The exact Qodec producer was qualified through the repository's own CI, not local
sharding: `qodec-v2.yml` run **31147734… / 31149037980** against `experiment/project-v0`
(observed head **6a5d4030…** = QA), with `nix build .#checks…qodec-tests` = **success**
and the full flake check (fmt + clippy + all checks) = **success**. The first QA
candidate (`36fe900b`) failed the flake's rustfmt/clippy gate; per protocol Qodec was
fixed on the same branch, a new QA candidate minted, re-qualified green, and every Q1R
artifact regenerated from it. `qodec_QA_authoritatively_qualified: true`. See
`qualification-receipt.json` (Qodec Commit QB `f71871ed`).

## The primitive and the boundary

`qodec project` (v0): one `qodec-project-request-v0` → one `qodec-project-result-v0`.
The protocol carries **no domain meaning** — the 007 adapter converts frozen B1
semantics (in-force AND not an agent claim) into a bare `eligible: bool` plus opaque
`caller_evidence`; Qodec enforces the decision and never redefines it. The projection
recomputes all identities, is relation-aware *closure* (not an optimizer), fails
closed, and is deterministic. **No `case-0002`/`obs-`/`qa`/`o7.b1` literal in Qodec
production code** (source-scan test). The 007 adapter imports no Qodec module and uses
no Qodec repo path; the binary path is explicit; **C1 is never fed as an oracle** — the
request is built from B0 + gold + contract, and Qodec computes the closure itself.

## Result

Kept deliberately separate (a codec token saving is never called projection quality):

```
projection_capability (Q1R raw structural verdict):  overall PASS, every axis PASS
representation_capability (Q1RQ):                     roundtrip PASS
qodec_project_result:            DIFFERENT_BUT_EVALUATION_PASS
qodec_project_semantic_repair:   PASS
qodec_encoding_roundtrip:        PASS
causal_conclusion:               QODEC_PROJECT_PASSES_WITH_DIFFERENT_VALID_PROJECTION
```

**Q1R evaluates authoritatively to `overall: PASS`** (two byte-identical runs,
corrected evaluator `58a7a111`, arm-rebound rev3 contract; digest `fc76d210…`).
Qodec's own projection **repairs the baseline** `relation_support` failure.

**Vs the C1 control** (per `q2-causal-classification.json`):

| | selection vs C1 | relations vs C1 | every relation gold-grounded | fabricated edge |
|---|---|---|---|---|
| all 3 tasks | **equal** | different | **yes** | none |

The selected observation set is **byte-identical** to the plain control on all three
tasks; the relation set differs because the two producers use different, both-valid
edge policies (C1 = B0's incident edges + witness; Qodec = outgoing edges from
selected). The difference is admissible under the frozen rule: every emitted relation
is gold-grounded, all frozen relation requirements pass, no fabricated edge exists, and
every other evaluation axis passes. That a *different* structurally valid projection
also passes is stronger evidence than byte parity — it shows the product boundary
admits more than one correct projection, so Qodec did not merely transliterate C1.

## Representation cost (separate capability)

Deep codec, meter o200k, zero-profile, all cells admissible (byte-exact + canonical
roundtrip, strict token win). Aggregate over the three tasks:

| Q1R raw | Q1RQ warm | Q1RQ cold (bundle, +261 notation) |
|---|---|---|
| 7482 | **4759** | **5020** |

Qodec compresses its own repaired projection ~36% warm.

## Not taken

No B1 evaluator/contract-semantics/schema/selector/projector change; no B1 methodology
rerun; C1 not altered to force parity; no case-0002 literal in Qodec; C1 not used as
Qodec input; no live model; no holdout; no generalization claim; PR #16 untouched; no PR.

## Artifacts

Qodec: `experiment/project-v0` QA `6a5d4030` (impl `657237a9…`), QB `f71871ed`
(qualification receipt). 007: Q2A adapter `f8fa87e3`, Q2B artifacts `d0022044`, this
Q2C evaluation/comparison, Q2D receipt. Q1R eval in CAS `fc76d210` + offsite restic
`bd4251f3`.
