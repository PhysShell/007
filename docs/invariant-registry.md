# Invariant registry — named invariants and their executable witnesses

Status: **proposal, pending ratification** · Scope: cross-cutting · Implementation: not started

Per `docs/evidence-and-decision-discipline.md` rule 3, this proposes a new
authoritative source of truth and was drafted autonomously, so it is **pending**
until a maintainer ratifies or rejects it. It is not normative authority and must
not be cited to justify a further autonomous decision.

## The gap

007 states many normative invariants — in `AGENTS.md`, `docs/security-layers.md`,
`docs/evidence-and-decision-discipline.md`, `docs/decision-and-admission-protocol.md`,
`docs/architecture/*` — and enforces a good number of them, via the `Cargo.toml`
lint set, `crates/o7-harness-policy`'s negative-compilation probe, the `proptest!`
blocks, three fuzz targets, and two Kani proofs (`docs/verification.md`).

What is missing is the join. There is no artifact that answers:

```text
Which normative invariant has NO enforcement site?
Which normative invariant has NO executable witness that FAILS when it is violated?
Which enforcement site guards an invariant nobody wrote down?
```

Today that question is answered by reading prose and grepping. That is exactly the
condition under which a declared invariant quietly stops being an executed one.

The external instance is recorded in
`docs/neuro-symbolic-transplant-record.md`: an architecture whose formal axioms
existed in documentation while their computational enforcement did not, at 0 %
coverage of the violations they named. It is evidence of recurrence, not authority.

## What this is not

Not a new testing philosophy. `docs/evidence-and-decision-discipline.md` rule 1
already chose the discipline — a **shared positive/negative semantic conformance
corpus** — and scoped it to projection-bound contracts, deferred to MG-C+. This
proposal generalizes that already-chosen discipline to named invariants; it does
not invent a second one.

Not one-invariant-one-test. A rule of "each invariant gets a regression test"
degrades within two quarters into a checkbox ritual in which the box is ticked and
the universe is therefore safe. The registry's unit is the **witness**, and the
gate below is stated over *negative* witnesses specifically, because only a test
that fails when the invariant is violated proves the invariant is executed at all.

Not a governance register. Rule 3's decision register (a `docs/adr/` candidate)
tracks *decisions*. This tracks *invariants and their enforcement*. Different
objects; do not merge them.

## Registry entry

One entry per normative invariant, in a machine-readable file
(`invariants.toml` or equivalent — format is an implementation choice, not part of
this proposal):

```text
Invariant
  id                  stable, never reused, never renumbered  (e.g. INV-VERDICT-003)
  statement           the invariant itself, one sentence, testable as written
  authority           the artifact that makes it normative + its revision binding
  scope               where it must hold (crate, boundary, wire format, lifecycle)
  enforcement_site[]  where it is actually enforced: lint, type, CI job, runtime check
  evidence_kind       what class of evidence establishes it (per the epistemic algebra)
  positive_vectors[]  executable cases that must pass while it holds
  negative_vectors[]  executable cases that must FAIL if it is violated
  property_harnesses[] proptest properties, if any
  bounded_proofs[]    Kani harnesses, if any
  status              proposed | enforced | superseded | withdrawn
```

`authority` carries a revision binding, so rule 4 applies to the registry itself:
an entry whose governing artifact has moved is STALE until re-checked, and the
registry can report that mechanically rather than rotting silently.

## The meta-invariant

The registry earns its existence only if a machine checks it. The minimum gate:

```text
for every entry with status = enforced:
    len(enforcement_site) >= 1
    AND len(negative_vectors) >= 1        # an executable witness that fails on violation
    AND authority.revision is current     # else STALE, not enforced
```

and, in the other direction, so the registry cannot drift into fiction:

```text
every negative_vector referenced by an entry must exist and must be executed by CI
every enforcement_site must name a real, locatable artifact
```

This is the property the external failure case did not have: it is not enough to
hold an axiom; the system must mechanically prove that for that axiom there exists
both an enforcement site and executable evidence of enforcement.

An entry that cannot satisfy the gate is not deleted — it is `proposed`, which is
the honest state and the useful one. The registry's value is precisely its ability
to say *"this invariant is written down and not enforced"* out loud.

## Deliberately out of scope for v0

- Proving the enforcement site actually enforces the stated invariant. The gate
  proves a witness *exists*; whether it is the *right* witness is review, not CI.
  Claiming otherwise would repeat the paper's mistake one level up.
- Coverage percentages. A ratio invites the ritual. Report the absolute list of
  entries failing the gate.
- Retrofitting every invariant in the tree at once. Seed with the ones already
  enforced (the lint set, the harness-policy probe, verdict semantics, the fuzzed
  parsers) so the first version describes reality, then let the gap grow visible.

## Open questions for ratification

1. Does the registry live in 007 only, or is it cross-repo (Own.NET / OwnAudit
   carry their own, same schema)? Cross-repo multiplies the value and the cost.
2. Is `AGENTS.md` the authority for review-facing invariants, or does the registry
   become the authority and `AGENTS.md` its projection? If the latter, rule 1's
   projection-bound-contract requirements apply to the generation.
3. Does the gate block CI from the start, or report-only until the seeded set
   stops moving?

No code follows from this document.
