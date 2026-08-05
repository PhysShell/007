# Evaluation contract v1 — revision 3 (schema-validation closure, R4A.2, design-only)

**Status: DESIGN/VALIDATION FREEZE. No evaluator code was written or executed in
R4A.2.** This round makes the JSON Schemas and the negative test suite actually
**enforce** the prose that revision 2 (R4A.1) already froze. It is a **validation
revision, not a semantic revision**: no measurement decision is reopened.

- `contract_revision: 3`, `semantic_revision_from_r2: false`,
  `validation_revision: true`, `supersedes_contract_revision: 2`.
- Revision 2 (`evaluation-contract-v1-r2.yaml`, artifact-bytes `b212463a…`) and all
  R4A / R4A.1 artifacts are preserved unchanged.
- A machine-generated semantic-equivalence record
  (`semantic-equivalence-r2-vs-r3.json`) proves rev2 ≡ rev3 under a normalized
  semantic view; the only differences are versioning fields, the removal of a
  declared non-normative field, and four fields **mechanically restructured** so a
  schema can bind them.

## Why a revision was needed

R4A.1's prose said the schemas were fail-closed; six spots were still permissive:

| # | Defect (R4A.1) | Closure (R4A.2) |
|---|---|---|
| 1 | `bound_inputs: {type: object}` — arbitrary shapes validated | Fully typed: exactly the 12 declared members, `additionalProperties:false`; public structured inputs require both digest domains + `byte_length`; private bodies require artifact-bytes only + `record_count`. Mapping-keyed groups (`task_files`, `question_files`, `task_contexts`) enforce the value shape with the logical id **as the key**. |
| 2 | `family_applicability` nested `additionalProperties` accepted unknown families/arms (`relation_suport` validated) | Exact required families, each with exact arm keys and **const** applicability values. |
| 3 | Output input records made both digests optional (a public structured artifact could carry none) | Each input declares `artifact_kind`; `oneOf` requires `structured` → both digests, `body` → artifact-bytes only and **forbids** a canonical-object digest. |
| 4 | Endpoint-policy evidence fields optional regardless of policy | `if/then` by `endpoint_policy`: `all_current_targets_materialized` **requires** `materialized_targets` + `unmaterialized_current_targets`; `edge_witness` **forbids** them. |
| 5 | `output_must_record: minItems 4` — four copies of one field validated | Restructured to an **object** with exactly the four required identity keys, `additionalProperties:false` (repetition impossible). |
| 6 | `contract_input_consistency_gates` were eleven arbitrary strings | Restructured to `{id, description}` with **exactly** the eleven ids `gate-01…gate-11` (each asserted by a `contains` clause). |

Plus the additional-closure items: `history_rationale` is declared non-normative,
removed from the instance, and now **forbidden** by `additionalProperties:false`;
`source_compilation_requirements_full_derived` became a closed set of five
requirement IDs; `pair_expectations` became a list carrying explicit `left`/`right`
task IDs for **every** pair (including `incomparable`), which removes the
string-key / endpoint-disagreement class by construction; the manifest's
`relative_path` is constrained to a repository-relative, normalized, traversal-free
form.

## What a schema cannot express (declared, not faked)

JSON Schema draft 2020-12 cannot compare two sibling values or assert uniqueness by
one property. These remain **evaluator consistency checks**, frozen as required R4B
verifier tests, and are exercised by `verifier_consistency` fixtures that the schema
**accepts as documented**:

- pair endpoint distinctness (`left != right`) and all-pairs completeness
  (N-choose-2) — `contract_input_consistency` axis;
- manifest `relative_path` uniqueness by normalized path, and entrypoint-presence in
  `files[]` — evaluator manifest verifier;
- `metric_semantics` key completeness (every emitted metric has a prose entry).

Silence about these would be the old failure — prose claiming the schema does more
than it does. They are named, and each has a fixture and a named R4B gate.

## Restructured fields and semantic equivalence

`semantic-equivalence-r2-vs-r3.json` compares rev2 and rev3 as a normalized semantic
view: 17 fields compared **verbatim-equal**; four fields compared via declared
transforms (gate descriptions equal; full-derived requirement IDs mapped to rev2
prose by a total order-preserving crosswalk; pair semantics equal after resolving
rev2's short keys to full task IDs; `output_must_record` field-set equal); versioning
fields and the non-normative `history_rationale` explicitly excluded. Result:
`semantic_equivalent: true`, `semantic_revision: false`.

## Validation results

- Positive: rev3 contract instance, a positive `evaluation-v1.json` (both endpoint
  policies, structured + body inputs), and a positive evaluator manifest all
  **validate**.
- Negative: **23 schema-oracle fixtures** each rejected with the **intended oracle
  confirmed** (validator + json-path recorded, not merely a nonzero result), plus
  **3 verifier-consistency fixtures** accepted-as-documented and frozen as R4B tests.
  See `negative-oracle-results.json`.

## Sequencing after R4A.2

R4A.2 does not touch `evaluate.py`, does not score case-0002, and creates no
expectation. R4B is now a genuinely mechanical build: parse the declared inputs,
validate the exact closure against these schemas, run the consistency gates listed
above, compute the frozen structural gates, emit canonical `evaluation-v1.json`, and
prove itself against the synthetic positive and adversarial fixtures — each reported
as fixture → triggered oracle → observed output → pass/fail, distinguishing evaluator
failure from fixture failure from scoring failure.
