# R4A.1 — evaluation-contract v1 semantic + validation closure

**Design-only. No evaluator code written or executed; no scored result; no expectation.**
Supersedes the R4A contract instance via `contract_revision: 2`; R4A artifacts and
commits (`cec6b39e`, `b7888392`) are preserved unchanged.

## Six gaps closed

1. **Budget** — gating = `selector-v0-canonical-record-jsonl` (canonical selected-record
   bytes + one LF each); rendered `context.json`/`context.md` sizes are non-gating
   diagnostics; limits unchanged (20000/32); the `UNRESOLVED` label is gone from the
   normative revision.
2. **Full-derived** — `family_applicability` freezes it to the source-compilation axis
   only; the projection arm carries projection-validity / observation / relation /
   stale-state gates; full-derived source-compilation verifies **frozen** derived
   artifacts (no extraction rerun, no semantic judgement).
3. **Digest domains** — every structured input carries `artifact_bytes_sha256` +
   `canonical_object_sha256` (`o7-canonical-json-v0`); bodies carry artifact-bytes only;
   gold state bound in both (`7e00505b` / `2589319a`).
4. **Input closure** — an ordered inventory of every file opened (public + private, both
   domains); opening an undeclared file fails closed; relation checks bind the task
   `context.json` artifacts.
5. **Contract↔source consistency** — a new **required axis** with 11 gates; all
   `gold_derived_target`s recomputed from the frozen gold graph.
6. **Shape** — `incomparable` (differ + neither subset + both directional diffs
   non-empty) replaces the tautological `materially_different`; case-0002 pairs frozen
   at intent level (resume/audit + resume/impact `incomparable`, audit/impact
   `left_strict_superset`).

Plus an explicit **evaluator implementation identity** manifest, and three genuinely
fail-closed schemas.

## Validation

Schemas well-formed; contract-r2 instance, a positive `evaluation-v1.json`, and a
positive evaluator manifest all validate; **14 negative fixtures all rejected**
(unknown arm/axis/shape/policy, depth≠1, missing required digest, extra property,
malformed SHA-256, missing evaluator identity, missing/duplicate task pair,
coverage-out-of-range, nullable-as-prose). All-pairs matrix completeness is enforced by
the evaluator's consistency axis, not faked in JSON Schema.

## Remaining non-normative future issue

Multi-hop relation traversal is out of scope (depth-1 only) until a future contract
defines it — a scope boundary, not an unresolved semantic ambiguity.

## Deliverables & commits

`schema/{evaluation-contract-v1-r2, evaluation-output-v1, evaluator-implementation-manifest-v0}.schema.json`,
`docs/evaluation-contract-v1-r2.md`, `fixtures/case-0002/evaluation-contract-v1-r2.yaml`,
schema positive + negative fixtures (Commit A `f7e647f7` / tree `2035c6d5`); this
decision record + report (Commit B).

## Next

R4B is now mechanical: build the evaluator in its implementation closure, parse only
declared inputs, run the consistency gates, compute the frozen structural gates, emit
canonical `evaluation-v1.json`, prove it against synthetic positive + adversarial
fixtures. R4C applies it to the frozen R3.1 bytes. No adjudication left inside the
implementation.
