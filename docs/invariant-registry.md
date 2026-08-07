# Invariant registry — named invariants and their executable witnesses

Status: **ratified (maintainer, interactive session)** ·
Ratified design revision: `1f17b49cd6530cad1fb22d8225e69af8d3aa5d88` ·
Scope: 007 in v0, federated per-repo by design · Implementation: not started

This design was ratified by the maintainer in an interactive session, under the
carve-out in `docs/evidence-and-decision-discipline.md` rule 3. The ratification
is bound to design revision `1f17b49cd6530cad1fb22d8225e69af8d3aa5d88`;
later changes are not covered by that decision unless separately ratified when
they cross the governance threshold.

## The gap

007 states many normative invariants — in `AGENTS.md`, `docs/security-layers.md`,
`docs/evidence-and-decision-discipline.md`, `docs/decision-and-admission-protocol.md`,
`docs/architecture/*` — and enforces a good number of them, via the `Cargo.toml`
lint set, `crates/o7-harness-policy`'s negative-compilation probe, the `proptest!`
blocks, three fuzz targets, and two Kani proofs (`docs/verification.md`).

What is missing is the join. There is no artifact that answers:

```text
Which normative invariant has NO enforcement site?
Which normative invariant has NO executable witness that goes red when
    its enforcement is removed?
Which "enforced" invariant is bound to a governing artifact that has since moved?
```

Today that question is answered by reading prose and grepping. That is exactly the
condition under which a declared invariant quietly stops being an executed one.

Two external instances are recorded in
`docs/neuro-symbolic-transplant-record.md` — an architecture whose formal axioms
existed in documentation while their computational enforcement did not, and, two
generations later, the same system losing its axioms to *agent divergence* and
regressing to 0 % safety coverage. Evidence of recurrence, not authority.

## What this is not

Not a new testing philosophy. `docs/evidence-and-decision-discipline.md` rule 1
already chose the discipline — a **shared positive/negative semantic conformance
corpus** — and scoped it to projection-bound contracts, deferred to MG-C+. This
generalizes that already-chosen discipline to named invariants; it does not invent
a second one.

Not one-invariant-one-test. A rule of "each invariant gets a regression test"
degrades within two quarters into a checkbox ritual in which the box is ticked and
the universe is therefore safe. The registry's unit is the **witness**, and the
gate is stated over *enforcement-sensitive* witnesses specifically.

Not a governance register. Rule 3's decision register (a `docs/adr/` candidate)
tracks *decisions*. This tracks *invariants and their enforcement*. Different
objects; do not merge them.

Not a coverage percentage. Report the absolute list of entries failing the gate.
A ratio invites the ritual, and "we are 83 % formally safe" is the exact sentence
this document exists to prevent.

## Authority

Three roles, kept separate. Collapsing any two of them is how a registry becomes
a small church of contract generation.

```text
governing artifact      authoritative for the invariant's SEMANTICS
                        (README.md, security-layers.md, verification.md,
                         public-governance.md, architecture/*, …)

invariant registry      authoritative for the JOIN ONLY:
                          - stable invariant ID
                          - binding to the governing artifact
                          - enforcement-site bindings
                          - witness bindings
                          - lifecycle state

AGENTS.md               review-facing human/agent summary — unchanged
```

`AGENTS.md` already declares itself "the review-facing summary of them, not a
replacement", and this proposal does not disturb that. `Invariant.statement` in v0
is a **normalized restatement for indexing**, never a new normative source: where
it and the governing artifact disagree, the governing artifact wins and the entry
is a bug.

Generating `AGENTS.md` (or any agent-facing projection) *from* the registry would
make the registry a projection authority and drag in all of rule 1 — generator
identity, projection digests, conformance corpus. Explicitly out of scope; if ever
wanted, it is a separate ratified migration that pays that price honestly.

## Scope — federated, 007 only in v0

Each repository owns the registry of its own invariants, because the governing
artifacts, enforcement sites, and CI witnesses all live there. 007 does not become
the Pope of Own.NET's invariants.

```text
007/invariants.*          authoritative for 007
Own.NET/invariants.*      authoritative for Own.NET       (later)
OwnAudit/invariants.*     authoritative for OwnAudit      (later)

        ↓ read-only aggregation, later

     o7 invariants …       a projection, never an authority
```

This also composes with revision binding: each repo knows for itself which of its
entries went STALE. **v0 implements 007 only.** Cross-repo aggregation is a
consumer, and per `docs/autonomy-controller.md`'s rule about projections, it may
never become an alternate authority.

## Registry entry

One entry per normative invariant, in a machine-readable file
(`invariants.toml` or equivalent — the format is an implementation choice):

```text
Invariant
  id                  stable, never reused, never renumbered  (e.g. INV-VERDICT-003)
  statement           normalized restatement for indexing (NOT the norm itself)
  authority           governing-artifact binding, by content identity (below)
  scope               where it must hold (crate, boundary, wire format, lifecycle)
  enforcement_site[]  where it is actually enforced: lint, type, CI job, runtime check
  witnesses[]         executable evidence (below)
  status              proposed | enforced | superseded | withdrawn
```

### `authority` — bind by content, not by commit

```text
authority
  repo
  path
  selector          the exact section or machine-checkable property
  blob_oid          content identity of that artifact
```

A repo-commit SHA is the wrong anchor: a neighbouring README edit would age a
governing artifact that did not change, and the resulting STALE noise trains
everyone to ignore the gate. Bind the **blob/content digest**. For an external
artifact that no commit of ours captures, bind its own digest — see
`docs/neuro-symbolic-transplant-record.md` for the worked case.

### `witnesses[]` — one kind is not enough

```text
witness
  id
  kind              compile_fail | rejection_vector | regression_vector
                    | property_harness | fuzz_harness | bounded_proof
  locator           the harness, test, or job — resolvable, not prose
  ci_binding        which CI job actually executes it
  expected_semantics  what it demonstrates about the invariant
```

A narrow "negative vector" requirement would exclude exactly the witnesses we most
want to seed with. *"This parser never panics on arbitrary input"* may have no
fixed bad input at all — that is why it has a fuzz target and a Kani proof. A
bounded proof is an excellent executable witness and a poor negative vector.
Negative and conformance vectors become **one kind among several**, not the only
admissible form of evidence.

## The meta-invariant

The registry earns its existence only if a machine checks it:

```text
for every entry with status = enforced:
    len(enforcement_site) >= 1
    AND at least one ENFORCEMENT-SENSITIVE executable witness
    AND authority.blob_oid is current
```

**Enforcement-sensitive** means: *if the violation became admissible, the witness
would go red.* It does not mean the witness is red now. In the healthy state every
witness is green and CI passes — the `o7-harness-policy` probe is the reference
shape:

```text
healthy:            invalid program → compiler rejects → probe job PASSES
enforcement removed: invalid program → compiles        → probe job FAILS
```

Stating this explicitly, because "negative vector must FAIL" read literally
produces a deliberately-red job and a team that has learned to ignore red.

And in the other direction, so the registry cannot drift into fiction:

```text
every witness locator must resolve, and its ci_binding must name a job that runs it
every enforcement_site must name a real, locatable artifact
```

This is what the external failure case lacked: it is not enough to hold an axiom;
the system must mechanically prove that for that axiom there exists both an
enforcement site and executable evidence that the enforcement is live.

## Admission — blocking from day one, for `enforced` only

No global report-only phase. A meta-gate that may be ignored on day one is a
decorative control against decorative controls: elegant, recursive, useless.
Instead the gate is narrow and the seed is allowed to be small.

```text
proposed      may be incomplete; reported, never blocking
              NEVER counts as established enforcement
enforced      MUST satisfy the meta-invariant; failure BLOCKS CI
superseded    structurally checked; no live enforcement required
withdrawn     structurally checked; no live enforcement required

STALE         a DERIVED condition, never a hand-written status:
              an `enforced` entry whose authority binding moved => CI failure
```

The seed can therefore be tiny and move as much as it likes. The only thing
forbidden is calling an entry `enforced` while it fails its own contract.

## v0 completeness — stated limit

The reverse gate verifies that **referenced** enforcement sites and witnesses
exist and run. It does **not** discover enforcement sites the registry never
mentions. Orphan-enforcement-site discovery would need invariant-ID annotations at
each site, an enumerable set of sites, or bespoke scanners over lints/CI/runtime
checks — **out of scope for v0**, and the third question in [The gap](#the-gap)
was narrowed accordingly.

Recorded here because the alternative is a small semantic lie that becomes, four
documents later, "the registry proves complete enforcement coverage."

## Resolved dispositions (ratified)

```text
1. Scope        v0 = 007 only; architecture federated per-repo;
                cross-repo aggregation is a read-only projection
2. Authority    governing artifact owns semantics; registry owns the join;
                AGENTS.md stays a summary; no generated AGENTS.md in v0
3. Admission    proposed non-blocking; enforced blocking immediately;
                stale enforced binding blocking; no global report-only phase
4. Witnesses    enforcement-sensitive executable witnesses; negative/conformance
                vectors are one kind among several
5. Identity     bind governing artifacts by content/blob identity, not repo commit
6. Completeness no claim of detecting unregistered/orphan enforcement sites in v0
```

Seed order for v0, so the first version describes reality rather than aspiration:
the compile/lint boundaries, the `o7-harness-policy` compile-fail probe, verdict
semantics, then the parser fuzz/Kani/proptest witnesses.

No code follows from this document.
