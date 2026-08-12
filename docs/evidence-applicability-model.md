# Contract applicability and the shape of an evidence record

Status: **proposed vocabulary — not ratified, no schema implied** · Scope: how a
future `judge`/`gate` record distinguishes what a contract promises, whether it
applies, what was observed, whether the two agree, and whether it matters.

This document introduces no implementation and mandates no migration. It is the
durable residue of a design discussion, recorded so later work does not
re-derive it or smuggle in the weaker form. Exactly **one** part of it has a
consumer today (§6, execution state in `judge`/`gate`); everything else is
vocabulary held for when the first real inference needs it.

It is a sibling of [`evidence-and-decision-discipline.md`](evidence-and-decision-discipline.md),
and a narrower instance of that document's recurring failure:

> A signal from a lower layer is not a semantic fact of the upper layer.

Here the lower-layer signals are *a contract's text* and *a measurement*, and
the upper-layer fact is *a property of the case in front of you*. Two steps
separate them, and neither is usually written down: whether the contract covers
this case, and whether the measurement is allowed to speak about the contract.

## 1. Why this exists

The discussion started outside this repo — a C compiler question — and the
compiler question turned out to be the disposable part. What survived is a
shape that recurs wherever we reason from a specification to a verdict.

The failure, stated once:

```text
contract says X          ≠  X holds for the case being evaluated
observation found none   ≠  none can occur
observation found one    ≠  the contract permits it
observer returned clean  ≠  observer ran
```

The third line is the one that costs most, and the one an earlier draft of this
document got wrong (§8, counterexample 6).

## 2. The model

```text
CONTRACT
  authority        who issues it, at what level (standard / implementation
                   spec / project convention)
  revision         the exact artifact read — commit or content digest
  scope            what objects and situations the contract speaks about
        │
        ▼
APPLICABILITY      yes | no | unknown
                   does this contract cover the case being evaluated?
        │
        ▼
POSITION           ruled_out | not_ruled_out | silent | undetermined
                   what the applicable contract says about the behaviour
        │
        ├───────────────────────────┐
        ▼                           │
OBSERVATION        observed | clean | not_evaluated | error
  scope            required for observed and clean
        │                           │
        └─────────────┬─────────────┘
                      ▼
CONFORMANCE        conforms | conflicts | unknown        (derived, §7)
                      │
                      ▼
CONSEQUENTIAL      yes | no | unknown
  consumer         which gate or decision the difference would change
```

Each node answers a different question, and no node may answer for its
neighbour:

| Node | Question |
| --- | --- |
| Contract | What is promised, by whom, at what authority, in which revision? |
| Applicability | Does that promise reach this case? |
| Position | What does the applicable contract say about this behaviour? |
| Observation | What did a particular measurement find? |
| Conformance | Do the position and the observation agree? |
| Consequential | Does the difference change a consumer's decision? |

**On the name.** In the discussion this node was called `COULD`. The name is
retired deliberately: "could" reads as both *is permitted to* and *has been
seen to*, and that ambiguity is exactly what let an earlier draft define the
node normatively in one section and settle it with a witness in the next. The
lineage is recorded here so the earlier vocabulary stays traceable.

## 3. Position and observation are independent

Three rules, and they are the core of the document.

> **1. A contract cannot justify `position=ruled_out` unless its applicability
> to the evaluated case has independently been established as `yes`.**

> **2. An observation settles `OBSERVATION` and nothing else. A witness never
> establishes a position.**

> **3. A position is never revised by an observation.** An observation that
> contradicts an applicable position produces `conformance=conflicts` — a
> finding, not a correction.

Rule 3 is what makes the most valuable state in the model expressible at all:

```text
position    = ruled_out       an applicable contract forbids the behaviour
observation = observed        the behaviour happened anyway
conformance = conflicts       the implementation violates its own contract
```

A model that lets a witness set the position cannot represent that; the witness
silently overwrites the norm, and a genuine compiler or runtime bug is recorded
as "well, apparently it's allowed". This is the same collapse the document is
about, and it is the one an earlier draft committed.

A conflict has exactly two honest resolutions, and choosing between them is real
work: either the implementation is defective, or `applicability=yes` was
established wrongly. It is never resolved by weakening the contract to fit the
observation.

The cost asymmetry sits on the position, not on the evidence:

```text
position = not_ruled_out   cheap: the contract addresses this and permits it
position = ruled_out       expensive: a closed contract, plus a demonstrated
                           hit inside its scope
position = undetermined    the correct initial state, not a defect
```

## 4. `unknown` carries its reason — and two of the three now have fields

An earlier draft observed that `unknown` collapses three unrelated states:

```text
not_investigated          nobody has looked yet
applicability_unresolved  the contract may or may not reach this case
contract_silent           an applicable contract makes no such guarantee
```

After the §2 split, two of them stop being reasons and become values in
different nodes:

```text
not_investigated          →  position = undetermined
contract_silent           →  position = silent
applicability_unresolved  →  applicability = unknown  (reason still required)
```

That is the improvement worth noticing: the collapse is prevented by the shape
of the record rather than by a required comment. Where a reason is still the
only carrier — `applicability=unknown`, `consequential=unknown` — it remains a
normative requirement on any future representation, because the states still
prescribe different next actions:

```text
position = undetermined       → look
applicability = unknown       → establish scope, or find a covering contract
position = silent             → this source will never answer; find another,
                                or settle it empirically and record it as
                                observation only
```

A state that does not change the next action is decoration. These do.

## 5. `not_applicable` is not `unknown`

```text
position = undetermined      we do not know the answer
applicability = no           this source cannot produce an answer
```

Distinguished, again, by the next action: the first is answerable by more work
against the same source, the second is not. Encoding the second inside the
position hides that, and makes a dead end look like a backlog item.

## 6. Observation state — the part with a consumer today

This is the only section with a present-tense claim on `judge`/`gate`.

```text
observed        evaluated, witness present
clean           evaluated, witness absent
not_evaluated   the observer did not run, or could not
error           the observer ran and failed
```

`clean` and `not_evaluated` may **never** be conflated. They differ by execution
state, not by depth of observation, so no amount of scope makes them
interchangeable — and no amount of scope is needed to tell them apart.

Scope does a different job:

```text
clean without a recorded scope  →  malformed record, inadmissible
                                →  NOT "not_evaluated"
```

A `clean` with no scope is not weak evidence; it is an invalid record, and takes
a different handling path from a legitimate `not_evaluated`.

And the rule that makes the whole thing durable:

> **Absence of a record means no durable knowledge, never a semantic outcome.**

A process may legitimately refuse to write a verdict it did not earn. A store
may not use file-absence to mean anything, because absence has too many
preimages: not evaluated, never invoked, runner died, upload failed, format
retired.

## 7. Conformance is derived, never asserted

`CONFORMANCE` is a function of position and observation. It is written down
because the join is where the interesting states live, not because anyone sets
it by hand.

| Position | Observation | Conformance | |
| --- | --- | --- | --- |
| `ruled_out` | `observed` | **conflicts** | the implementation violates a contract that applies to it |
| `ruled_out` | `clean` | conforms | |
| `not_ruled_out` | `observed` | conforms | permitted, and exercised |
| `not_ruled_out` | `clean` | conforms | permitted, not exercised *here* — position unchanged |
| `silent` | any | unknown | nothing to conform to |
| `undetermined` | any | unknown | no position yet |
| any | `not_evaluated`, `error` | unknown | nothing measured |

Row 4 is the negative-evidence trap, and the table is how it stops being a
matter of author discipline: a `clean` observation cannot move a
`not_ruled_out` position toward `ruled_out`, because conformance reads both and
writes neither.

## 8. Motivating counterexamples

Inferences produced during the source discussion, each of which lost a boundary
this model makes structural. They are recorded as a **counterexample corpus**,
not as a measurement:

> These failures are not an estimate of an error rate. They are a counterexample
> corpus showing that prose discipline alone does not enforce the
> scope/applicability separation, even when the author is explicitly watching for
> that failure mode. Fields are therefore justified as structural constraints,
> not as reminders.

```text
1  observation      → guarantee            one passing witness read as a language guarantee
2  single target    → architecture class   one ISA datapoint read as an ISA-class law
3  visible freedom  → normative scope      where freedom was visible read as where it exists
4  pointer caveat   → contract exclusion   a hedge read as a scope withdrawal
5  "may violate"    → "explicitly refuses" hedged wording read as categorical
6  witness          → contractual position an earlier draft of THIS document, §3
7  missing revision → applicability_unknown an earlier draft of THIS document, §11
```

Their use is as a mutation corpus for the representation itself:

> If the schema lets any of these be recorded without a missing required field
> or absent provenance, the schema is still too permissive.

4 and 5 are the same underlying case caught at two different strengths — which
is why applicability earns its own node rather than living as a caveat. 6 and 7
were produced *while writing the document that forbids them*, which is the
strongest available argument that the separations must be fields.

## 9. First fixture

The case that motivated the applicability node. It is a good first fixture
precisely because it cannot be recorded as a single verdict — an implementation
that allows `contract → ruled_out` without the intermediate step cannot express
it at all.

Contract: the .NET runtime memory model (pinned in §11), which states under
*Side-effects and optimizations of memory accesses* that its assumption "applies
to all reads and writes - volatile or not", and derives from it `Reads cannot be
introduced` and `A read cannot be re-done`. Authority: implementation
specification, issued by `dotnet/runtime`; explicitly stronger than the ECMA-335
model it cites as weak.

```text
CASE A   runtime-managed ordinary memory, managed references,
         NOT PROVEN THREAD-LOCAL (potentially cross-thread)
         applicability = yes
         position      = ruled_out
         condition:    the same contract permits duplicating and removing
                       accesses where an optimizer has PROVEN the data is
                       reachable by a single thread. The position holds only
                       while that proof is unavailable — which is the normal
                       case for memory a second party writes, but is a
                       condition of the record, not a background assumption.

CASE B   ordinary coherent shared mapping via unmanaged access
         applicability = unknown   reason: applicability_unresolved
         position      = undetermined
         toward-covered: the rule is stated over all reads and writes
         toward-excluded: nothing on the introduction axis — the unmanaged
                          caveat is hedged ("may violate") and its only
                          example concerns alignment and atomicity
         → insufficient for ruled_out; insufficient for not_ruled_out

CASE C   device / incoherent memory
         applicability = no        the contract declares this outside its model
         position      = undetermined — needs a different contract
```

Case B also shows why one scenario can need two contract lookups on two axes,
resolved independently:

| Axis | Source | Status |
| --- | --- | --- |
| introduced / repeated access | the runtime memory model | applicability unresolved |
| aggregate snapshot atomicity | the accessor's public API docs | position silent |

Merging the axes is how a caveat about atomicity gets applied to a question
about read introduction — counterexample 4 above.

## 10. What applies now

```text
applies now      §6 execution state, for judge/gate:
                 clean ≠ not_evaluated; clean requires a recorded scope

proposed         §2 the model, §3 the three rules, §4 unknown-with-reason,
                 §5 applicability vs undetermined, §7 derived conformance

deferred         materialising contract, applicability, position and
                 conformance as fields — do this when the first inference
                 actually derives a position from a contract, using §9 as the
                 fixture, and not before
```

The order is deliberate. Building the full vocabulary into a schema ahead of a
consumer produces an ontology whose fields nobody is obliged to fill correctly,
which is the failure this document is about, one level up.

## 11. Provenance of the claims in this document

Per rule 4 of [`evidence-and-decision-discipline.md`](evidence-and-decision-discipline.md),
factual claims about external artifacts need a revision anchor of their own.
This document's commit binds the *text of the claim*; it does not capture the
*artifact the claim is about*, so each external source is pinned by commit and
content digest:

```text
.NET memory model
  dotnet/runtime  docs/design/specs/Memory-model.md
  commit 633ab1a41439ad2405ba2eb241295ba1842fcf5a
  blob   5c727e6ec30de20fbc3e1a9b09a4803b64cd6c28

kernel READ_ONCE, enforced contract
  torvalds/linux  include/asm-generic/rwonce.h
  commit f5bbbfec59b4e2fb7520a91de3df8a6174325d6a
  blob   52b969c7cef9359e997e1e24df247f59187ccd59

kernel READ_ONCE, tools/ copy
  torvalds/linux  tools/include/linux/compiler.h
  commit f5bbbfec59b4e2fb7520a91de3df8a6174325d6a
  blob   f40bd2b04c29872ff1ee10442a8615f59c42f78d

compiler behaviour (OBSERVED, one toolchain, one target)
  gcc 13.3.0 (Ubuntu 13.3.0-6ubuntu2~24.04.1), clang 18.1.3, x86-64, -O2
```

The blob digests were verified by hashing the content fetched at each pinned
commit; the commit ids were the branch heads at the time of writing, but it is
the pin that binds, not the branch.

The rule this section is an instance of:

```text
missing contract revision   ≠  applicability unknown
missing contract revision   →  grounding incomplete; inadmissible as
                               contract evidence
```

Those are independent axes. Applicability asks *whether this contract covers
this case*; a revision anchor asks *which artifact was read at all*. An earlier
draft of this document conflated them — counterexample 7 — which is the same
shape as `clean` without a scope: not weak evidence, but a malformed record.

The compiler observations carry no contract. Under rule 2 of §3 they establish
`observation=observed` for the transformations they exhibit, and no position
whatsoever — the direction counterexamples 1 and 2 went wrong.
