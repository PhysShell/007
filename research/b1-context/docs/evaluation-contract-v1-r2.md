# Evaluation contract v1 — revision 2 (R4A.1 closure, design-only)

**Status: DESIGN CLOSURE. No evaluator code written or executed.** Supersedes the
R4A contract instance (`evaluation-contract-v1.yaml`, artifact-bytes
`sha256:feef81c6…`, commit `cec6b39e`) via `contract_revision: 2`
(`fixtures/case-0002/evaluation-contract-v1-r2.yaml`). The R4A artifacts and
commits are preserved unchanged. Same contract family `o7.b1.evaluation-contract/v1`.

Closes the six R4A gaps the reviewer flagged:

## 1. Budget representation resolved (normative)

Gating byte-accounting **preserves selector v0**:
`selector-v0-canonical-record-jsonl` — sum over every selected observation in
canonical selector order of `len(canonical_json_bytes(observation)) + 1` (LF);
`record_count` = number of selected observations. Limits unchanged (`20000`/`32`,
`utf8_bytes+records`). Rendered `context.json`/`context.md` sizes are separate
**non-gating diagnostics**. `counted_representation_UNRESOLVED` is removed from the
normative revision (kept only in R4A history + this rationale).

## 2. Full-derived semantics resolved (normative)

`family_applicability` freezes arm↔family: `full_derived` is **required for the
source-compilation axis only**; it is `NOT_APPLICABLE` for projection-validity,
question-observation-support, relation-support and stale-state-safety — those
task-level structural gates apply to the **projection** arm. `full_derived` is not
a compiled-observation arm; v1 never infers semantic answerability from raw
transcript text. Its source-compilation check verifies **frozen** derived
artifacts (session↔derived-manifest match, body availability, byte-length + SHA-256
match, record count reproducible from the JSONL, extractor identity + raw-source
binding present) — it does not rerun extraction or judge semantics.

## 3. Digest domains disambiguated

Every structured input carries **both** `artifact_bytes_sha256` (exact file bytes)
and `canonical_object_sha256` (parsed → `o7-canonical-json-v0`). Text/binary bodies
(derived JSONL) carry only `artifact_bytes_sha256`. Any field named merely
`sha256`/`digest` states its domain in the schema. Gold state is bound in both
domains: artifact-bytes `sha256:7e00505b…`, canonical-object `sha256:2589319a…` —
the same duality the R4A contract (artifact-bytes) and the R3.1 report
(canonical-object) exhibited. They are not competing values.

## 4. Complete evaluator input closure

An ordered input inventory of every file the evaluator opens (role, logical_id,
visibility, both digest domains, byte length). Required public structured inputs:
evaluation contract, gold state, fixture manifest, source selectors, task registry,
budget, R3.1 report/projection-comparison/derived-manifest, every registered
task + questions file, every projected task `context.json`. Required private:
every derived JSONL body named by the derived manifest (for the full-derived
source-compilation arm). Negative-control body only when that optional arm is
`AVAILABLE`. Raw source bodies are not inputs unless actually read — no decorative
inputs. **Opening an undeclared file fails closed.** (Relation checks need the task
`context.json` artifacts, since projected relation edges live there, not in
`report.json`.)

## 5. Contract↔source consistency gates (new required axis)

`contract_input_consistency` is a required axis with 11 gates: fixture-id agreement;
registry filename+digest; task-id set equality; questions filename + both digests;
question existence; every legacy `required_relation_paths` expanded; no invented
relation requirement; forbidden-stale set equality; every `gold_derived_target`
recomputed from the frozen gold graph; target-status agreement; unknown
task/question/observation/kind/policy/direction/depth/shape fails closed.

## 6. Task-shape semantics corrected

`materially_different` (which was "sets differ and are not equal" — the same
condition twice) is replaced by **`incomparable`**: selected sets differ, neither is
a subset of the other, and both directional differences are non-empty. case-0002:
`resume__audit` and `resume__impact` → `incomparable`; `audit__impact` →
`left_strict_superset` (audit ⊃ impact). Unknown shapes fail closed. No exact IDs /
counts / symmetric differences are frozen.

## Evaluator implementation identity (new)

`o7.b1.evaluator-implementation-manifest/v0` (files + entrypoint + python +
dependency versions); its `canonical_object_sha256` is
`evaluator_implementation_manifest_digest`. The output additionally records
`evaluator_entrypoint_sha256`, `evaluator_module_sha256`,
`execution_checkout_commit`, `execution_checkout_tree`. CLI + module both belong to
the closure.

## Fail-closed schemas

Three separate schemas — `evaluation-contract-v1-r2.schema.json`,
`evaluation-output-v1.schema.json`, `evaluator-implementation-manifest-v0.schema.json`
— with `additionalProperties: false` throughout, closed enums, exact
`^sha256:[0-9a-f]{64}$` patterns, non-negative integer counts, coverage in `[0,1]`,
task pairs of exactly two distinct IDs, `uniqueItems` on the pair matrix, and JSON
`null` (never a prose `"null"`) for nullable metrics. Validated against a positive
instance and against negative fixtures for unknown arm/axis/shape/policy, depth≠1,
missing required digest, extra property, malformed SHA-256, missing evaluator
identity, and missing/duplicate task pair — every negative is rejected.
Combinatorial completeness of the all-pairs matrix is enforced by the evaluator's
`contract_input_consistency` axis, not by JSON Schema alone (documented, not faked).

## Outcome lattice (revised)

Required axes: `contract_input_consistency`, `source_compilation`,
`projection_validity`, `question_observation_support`, `relation_support`,
`stale_state_safety`, `task_dependence`. Optional: `negative_control_diagnostics`
(may be `NOT_APPLICABLE`). Overall `PASS` (all required PASS), `FAIL` (any required
FAIL), `UNAVAILABLE` (any required UNAVAILABLE without a FAIL). Optional diagnostics
cannot upgrade or downgrade required gates. `PARTIAL` forbidden.

## Remaining non-normative future issue

Multi-hop relation traversal stays out of scope (depth-1 only) until a future
contract defines it. This is not an unresolved *semantic* ambiguity — it is an
explicit scope boundary.

## Next

R4B can now be mechanical: build the evaluator (in its implementation closure),
parse only declared inputs, run the consistency gates, compute the frozen
structural gates, emit canonical `evaluation-v1.json`, and prove itself against
synthetic positive + adversarial fixtures. R4C applies it to the frozen R3.1 bytes.
No adjudication remains inside the implementation.
