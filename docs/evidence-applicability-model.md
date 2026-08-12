# Contract applicability and the shape of an evidence record

Status: **proposed vocabulary — not ratified, no schema implied** · Scope: how a
future `judge`/`gate` record distinguishes what a contract promises, whether it
applies, what was observed, and whether it matters.

This document introduces no implementation and mandates no migration. It is the
durable residue of a design discussion, recorded so later work does not
re-derive it or smuggle in the weaker form. Exactly **one** part of it has a
consumer today (§6, execution state in `judge`/`gate`); everything else is
vocabulary held for when the first real inference needs it.

It is a sibling of [`evidence-and-decision-discipline.md`](evidence-and-decision-discipline.md),
and a narrower instance of that document's recurring failure:

> A signal from a lower layer is not a semantic fact of the upper layer.

Here the lower-layer signal is *a contract's text*, and the upper-layer fact is
*a property of the case in front of you*. The two are separated by a step that
is almost never written down: whether the contract covers this case at all.

## 1. Why this exists

The discussion started outside this repo — a C compiler question — and the
compiler question turned out to be the disposable part. What survived is a
shape that recurs wherever we reason from a specification to a verdict.

The failure, stated once:

```text
contract says X          ≠  X holds for the case being evaluated
observation found none   ≠  none can occur
observer returned clean   ≠  observer ran
```

Each line is a distinct loss, and each was produced live during the discussion
by an author who had just finished arguing against that exact loss (§7).

## 2. The model

```text
CONTRACT
  authority        who issues it, at what level (standard / implementation
                   spec / project convention), at which revision
  scope            what objects and situations the contract speaks about
        │
        ▼
APPLICABILITY      yes | no | unknown
                   does this contract cover the case being evaluated?
        │
        ▼
COULD              yes | no | unknown
  derivation       how the verdict was reached (witness / contract / neither)
        │
        ▼
OBSERVATION        observed | clean | not_evaluated | error
  scope            required for observed and clean
        │
        ▼
CONSEQUENTIAL      yes | no | unknown
  consumer         which gate or decision the difference would change
```

Each floor answers a different question, and no floor may answer for its
neighbour:

| Floor | Question |
| --- | --- |
| Contract | What is promised, by whom, at what authority? |
| Applicability | Does that promise reach this case? |
| Could | Is the behaviour of interest permitted? |
| Observation | What did a particular measurement find? |
| Consequential | Does the difference change a consumer's decision? |

## 3. The binding rule

> **A contract cannot justify `COULD=no` unless its applicability to the
> evaluated case has independently been established as `yes`.**

`COULD=yes` is cheap: one valid witness settles it. `COULD=no` is expensive: it
needs a closed contract *and* a demonstrated hit inside that contract's scope.
`COULD=unknown` is the correct initial state, not a defect of the analysis.

The asymmetry matters because the cheap direction is the safe one. Without the
rule, the expensive verdict is reachable by the cheap route — read a strong
sentence, skip the scope check, record `no`.

## 4. `unknown` carries its reason

`unknown` collapses faster than any other value, because three unrelated states
all render as the same word:

```text
not_investigated          nobody has looked yet
applicability_unresolved  the contract may or may not reach this case
contract_silent           an applicable contract makes no such guarantee
```

They prescribe different next actions, which is the whole reason to keep them
apart:

```text
not_investigated          → look
applicability_unresolved  → establish scope, or find a contract that covers it
contract_silent           → this source will never answer; find another, or test
```

A state that does not change the next action is decoration. These three do, so:
**`unknown` must retain provenance.** This is a normative requirement on any
future representation, not a request for three enums today.

## 5. `not_applicable` is not `unknown`

```text
COULD = unknown              we do not know the answer
applicability = no           this source cannot produce an answer
```

Epistemically distinct, and again distinguished by the next action: the first is
answerable by more work against the same source, the second is not. Encoding the
second inside `COULD` hides that, and makes a dead end look like a backlog item.

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

## 7. Motivating counterexamples

Five inferences produced during the source discussion, each of which lost a
boundary this model makes structural. They are recorded as a **counterexample
corpus**, not as a measurement:

> These five failures are not an estimate of an error rate. They are a
> counterexample corpus showing that prose discipline alone does not enforce the
> scope/applicability separation, even when the author is explicitly watching for
> that failure mode. Fields are therefore justified as structural constraints,
> not as reminders.

```text
1  observation      → guarantee          one passing witness read as a language guarantee
2  single target    → architecture class one ISA datapoint read as an ISA-class law
3  visible freedom  → normative scope    where freedom was visible read as where it exists
4  pointer caveat   → contract exclusion a hedge read as a scope withdrawal
5  "may violate"    → "explicitly refuses" hedged wording read as categorical
```

Their use is as a mutation corpus for the representation itself:

> If the schema lets any of these five be recorded without a missing required
> field or absent provenance, the schema is still too permissive.

Note that 4 and 5 are the same underlying case, caught at two different
strengths — which is why applicability earns its own node rather than living as
a caveat inside `COULD`.

## 8. First fixture

The case that motivated the applicability node. It is a good first fixture
precisely because it cannot be recorded as a single verdict — an implementation
that allows `contract → COULD=no` without the intermediate step cannot express
it at all.

Contract: the .NET runtime memory model, which states under *Side-effects and
optimizations of memory accesses* that its assumption "applies to all reads and
writes - volatile or not", and derives from it `Reads cannot be introduced` and
`A read cannot be re-done`. Authority: implementation specification, issued by
`dotnet/runtime`; explicitly stronger than the ECMA-335 model it cites as weak.

```text
CASE A   runtime-managed ordinary memory, managed references
         applicability = yes
         COULD         = no        derivation: contract

CASE B   ordinary coherent shared mapping via unmanaged access
         applicability = unknown   reason: applicability_unresolved
         COULD         = unknown   derivation: none admissible
         toward-covered: the rule is stated over all reads and writes
         toward-excluded: nothing on the introduction axis — the unmanaged
                          caveat is hedged ("may violate") and its only
                          example concerns alignment and atomicity
         → insufficient for no; insufficient for yes

CASE C   device / incoherent memory
         applicability = no        the contract declares this outside its model
         COULD         = unknown   derivation: none — needs a different contract
```

Case B also shows why one scenario can need two contract lookups on two axes,
resolved independently:

| Axis | Source | Status |
| --- | --- | --- |
| introduced / repeated access | the runtime memory model | applicability unresolved |
| aggregate snapshot atomicity | the accessor's public API docs | contract silent |

Merging the axes is how a caveat about atomicity gets applied to a question
about read introduction — counterexample 4 above.

## 9. What applies now

```text
applies now      §6 execution state, for judge/gate:
                 clean ≠ not_evaluated; clean requires a recorded scope

proposed         §2 the model, §3 the binding rule, §4 unknown-with-reason,
                 §5 applicability vs unknown

deferred         materialising authority and applicability as fields — do this
                 when the first inference actually derives COULD from a
                 contract, using §8 as the fixture, and not before
```

The order is deliberate. Building the full vocabulary into a schema ahead of a
consumer produces an ontology whose fields nobody is obliged to fill correctly,
which is the failure this document is about, one level up.

## 10. Provenance of the claims in this document

Per rule 4 of [`evidence-and-decision-discipline.md`](evidence-and-decision-discipline.md),
the factual claims here about external artifacts are bound to this document's
revision, and each is an `OBSERVED` record with a scope — not a standing fact.

```text
.NET memory model     dotnet/runtime, docs/design/specs/Memory-model.md
                      read 2026-08-12 from the moving ref `main` — UNPINNED
kernel READ_ONCE      torvalds/linux, include/asm-generic/rwonce.h and
                      tools/include/linux/compiler.h
                      read 2026-08-12 from the moving ref `master` — UNPINNED
compiler behaviour    gcc 13.3.0 (Ubuntu 13.3.0-6ubuntu2~24.04.1),
                      clang 18.1.3, x86-64, -O2
```

The two repository references were read from moving branches, so they identify a
file but not a revision — by this document's own §4 that is
`applicability_unresolved` for any later reader, and re-verification is required
before citing them. The observation that motivates it: the same project, at the
same commit, carries two different `READ_ONCE` contracts in two directories —
one restricting to native word size by static assertion, one falling back to
`memcpy` — so "the kernel guarantees X" is not a proposition until the header is
named.

The compiler observations are `OBSERVED` on one toolchain and one target. Under
§3 they establish `COULD=yes` for the transformations they exhibit, and nothing
at all about other toolchains — the direction counterexamples 1 and 2 went
wrong.
