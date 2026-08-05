# R4A.2 — evaluation-contract v1 schema-validation closure (design-only)

**Design/validation only. No evaluator was implemented or executed, case-0002 was
not scored, and no expectation was created.** This round makes the JSON Schemas and
the negative test suite enforce the prose that R4A.1 (revision 2) already froze.
It is a **validation revision, not a semantic revision**.

- Base: `fdf9fffe3221f027c49e67780202e1a978bf3db7` (R4A.1 receipt).
- Commit A (schemas + rev3 contract + doc + fixtures + equivalence + oracle results):
  `45b46c426e486abf8a75b133b47f017975815a62` / tree `26a7e7dc22cbed58dc6ded3138a08e0a0ef53c6f`.
- Revision 3 contract: artifact-bytes `8f5060f0…`, canonical-object `9784516d…`;
  `contract_revision: 3`, `semantic_revision_from_r2: false`,
  `validation_revision: true`, `supersedes_contract_revision: 2`.
- R4A / R4A.1 artifacts (rev2 contract `b212463a…`, its three schemas, both docs)
  preserved unchanged — verified against the working tree.

## Six schema defects closed

1. **`bound_inputs`** — was `{type: object}`; now fully typed with
   `additionalProperties:false`, exactly the 12 declared members, both digest
   domains + `byte_length` on public structured inputs, artifact-bytes-only +
   `record_count` on private bodies, and the logical id enforced as the mapping key
   for the keyed groups.
2. **`family_applicability`** — exact required families, each with exact arm keys and
   `const` applicability values; `relation_suport` (misspelled) and stray arms now
   rejected.
3. **Input digest identity** — every output input record declares `artifact_kind`;
   `oneOf` forces `structured` → both digests, `body` → artifact-bytes only and
   **forbids** a canonical-object digest.
4. **Endpoint-policy evidence** — `if/then` by `endpoint_policy`;
   `all_current_targets_materialized` requires `materialized_targets` +
   `unmaterialized_current_targets`, `edge_witness` forbids them.
5. **Implementation identity** — `output_must_record` restructured to an object with
   exactly the four required keys, `additionalProperties:false` (four copies of one
   field impossible).
6. **Consistency gates** — `{id, description}` with exactly the eleven ids
   `gate-01…gate-11`, each asserted by a `contains` clause.

**Additional closure:** `history_rationale` declared non-normative, removed from the
instance, and forbidden by the schema; `source_compilation_requirements_full_derived`
a closed set of five requirement IDs; `pair_expectations` a list with explicit
`left`/`right` per pair (removing the key/endpoint-disagreement class);
`relative_path` constrained to a repository-relative, normalized, traversal-free form.

## Honest limits (declared, not faked)

JSON Schema draft 2020-12 cannot compare two sibling values or assert uniqueness by
one property. These stay evaluator consistency checks with `verifier_consistency`
fixtures the schema **accepts as documented**, frozen as required R4B tests: pair
endpoint distinctness + N-choose-2 completeness; manifest `relative_path` uniqueness
+ entrypoint-in-`files`; `metric_semantics` key completeness.

## Semantic equivalence rev2 ≡ rev3

`semantic-equivalence-r2-vs-r3.json`: 17 fields verbatim-equal; four restructured
fields equal under declared transforms (gate descriptions; a total order-preserving
requirement-ID crosswalk; pair semantics after resolving rev2 short keys;
`output_must_record` field-set); versioning fields and `history_rationale` excluded.
`semantic_equivalent: true`, `semantic_revision: false`.

## Validation

| | Result |
|---|---|
| Positive: rev3 contract | VALID |
| Positive: `evaluation-v1.json` (both endpoint policies, structured + body inputs) | VALID |
| Positive: evaluator manifest | VALID |
| Negative: schema-oracle fixtures | **23 / 23** rejected, intended oracle confirmed (validator + json-path recorded) |
| Negative: verifier-consistency fixtures | **3 / 3** accepted-as-documented, frozen as R4B tests |

Every negative record carries `expected_rejection_validator`,
`expected_rejection_keyword`, `observed_validator`, `observed_json_path`, and `pass`
— no reliance on a bare nonzero result.

## Forbidden list — confirmed not taken

No evaluator CLI/module code; no `evaluate.py` edit; no case-0002 scoring; no A1
integration; no R3/R3.1 edit; no state-schema/selector change; no expectation; no
Qodec arm/holdout; qodec PR #16 untouched; no PR opened or merged.

## Next

R4B is now a genuinely mechanical build against these schemas: parse declared
inputs, validate exact closure, run the named consistency gates, compute the frozen
structural gates, emit canonical `evaluation-v1.json`, and prove itself against the
synthetic positive and adversarial fixtures — each reported as fixture → triggered
oracle → observed output → pass/fail, distinguishing evaluator, fixture, and scoring
failure. R4B requires separate authorization.
