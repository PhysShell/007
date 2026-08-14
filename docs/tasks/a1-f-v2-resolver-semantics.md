# A1-F v2 — Resolver observation semantics

**Status: PREREGISTRATION. RS-P0. No implementation exists, and none is
proposed here.**

This document freezes an *epistemic contract* before any code is written against
it: what each observable outcome of the object resolver mechanically licenses
the layer above to claim. It is deliberately merged on its own, reviewed on its
own, and closed on its own, before an implementation branch exists.

> **On the name.** `RS-P0` is the zeroth document of the **R**esolver
> **S**emantics series — a *preregistration*, not a severity. This repository
> also uses `P0`/`P1` for finding severity in review. The two are unrelated, and
> the collision is unfortunate enough to be worth stating once.

## 0. Why this is a separate PR from the implementation

The last two closed series produced the same result twice, from different
directions. In EB-R1 the mechanism was reasonable and the promise about it was
wider than the property. In EC-R1 the capability contract was frozen before the
code — and review still found, twice in one PR, that the guarantee was narrower
than its wording: a grant exceeding the witness that justified it, and a flag
assumed to cover more than it does.

Both times the weakest object under test was **the text of the guarantee**, not
the code. That is now an empirical claim with several observations behind it
rather than a methodological preference, and it justifies giving the guarantee
its own independent closure before an implementation exists to exert pressure on
it. A preregistration that arrives as the first commit of an implementation PR
is read as the introduction to a diff, and it is amended to match the diff. This
one cannot be, because the diff will not exist yet.

**Zero lines of production code. Zero tests.** Test fixtures are omitted
deliberately as well: a fixture is a partial specification wearing overalls, and
the shape it is easiest to write is not necessarily the shape the contract
requires.

## 1. The primary question

> **What is the strongest object-availability claim mechanically licensed by
> each observable resolver outcome?**

Not "how should `_local_object` be fixed". The repair follows from the answer;
proposing the repair first is how the answer gets chosen to suit it.

## 2. The defect this exists to answer

`_local_object` runs `git cat-file -t <oid>`, turns any non-zero return into
`None`, and the layer above renders `None` as **UNRESOLVABLE IN THIS CHECKOUT**,
whose text asserts something about the *object*. At least four distinct
situations arrive at that same `None`:

```text
object genuinely not in the local object database  ─┐
git refused to read the repository (ownership, …)  ─┼─> returncode != 0 ──> None
repository or object damaged                       ─┤
git failed for some other reason                   ─┘
                                                          ↓
                                          "UNRESOLVABLE IN THIS CHECKOUT"
```

The observation is *"no trustworthy result was obtained"*. The rendered claim is
*"the object is not available here"*. The second does not follow from the first,
and the gap is exactly the E-R9 family — except the projection loss is now in
the representation of an **external resolver's result** rather than of a
semantic edge.

This was found by an experiment aimed at something else (EC-R1's `HOME`
witness), and EC-R1 narrowed rather than removed it: `-c safe.directory` takes
away the ordinary reason to reach that branch, so when it is reached, the
misdescription is what remains.

### 2.1 A second live defect, found by reviewing this document

Reviewing the *contract* — with no implementation in the diff — surfaced a
defect in code already merged on `main`. `_local_object` never checks the return
status of the content command; it hashes whatever landed on stdout. Against a
loose object truncated behind an intact header, git writes a partial prefix and
exits 128, and the merged extractor reports:

```text
OBJECT IDENTITY VIOLATED — asked git for f91c0ba6… and got bytes hashing to
2ae5ab1b… This checkout substitutes object contents; nothing derived from it is
evidence of anything.
```

Nothing substituted anything. The object is damaged, and the strongest
diagnostic in the system is issued as a false accusation — the same defect
family as the one this document exists to correct, one state over: an
observation rendered as a claim it does not support.

**Not repaired here.** This document contains no code, and the repair belongs to
the implementation branch under §4.1's success-before-identity ordering. It is
recorded because the contradiction that produced it was not hypothetical, and
because a preregistration that quietly dropped an inconvenient discovery would
be worth less than no preregistration at all.

## 3. What the consumer actually needs

§8.5's consumer does not need to know *why* an object could not be obtained. It
needs one answer:

> **Can I, right now, without network access and inside the accepted trust
> boundary, obtain exact bytes corresponding to the cited OID?**

That question is total: every resolver outcome answers it, and nothing else
about the outcome changes what the consumer may do. This matters for what
follows, because it is the licence to collapse states that are *physically*
different.

**A distinction must be preserved when the next layer's normative action depends
on it — not merely because the situations differ in the world.** Where absent,
refused, damaged and failed all produce the same downstream obligation, keeping
them apart buys diagnostics, not correctness, and paying for those diagnostics
with an invented fact is the trade this series exists to refuse.

For this consumer, that makes the reason **IRRELEVANT-BY-NORM**: the admissible
coarser representation of the existing rule, rather than a projection loss.

## 4. The decision rule (FROZEN)

Three states. Classification is by **mechanical observation only**.

### 4.1 What counts as "supplied bytes"

**A response counts only if the command that produced it completed
successfully.** Mechanically: every git invocation in the lookup exits zero.
Output written by a command that then failed is **not** a response — it is
debris, and it is discarded unread rather than hashed.

This is not a technicality; the first draft of this document omitted it and was
thereby unimplementable. Review demonstrated the case: against a loose object
truncated in its body but intact in its header,

```text
cat-file -t <oid>      ->  "blob"   exit 0
cat-file blob <oid>    ->           exit 128, after writing 163840 bytes to stdout
```

Those 163840 bytes necessarily hash to something other than the requested oid,
so a rule that says *bytes were supplied and the hash differs* classifies a
damaged object as `IDENTITY_VIOLATION` — while §7's witness 3 requires
`LOOKUP_UNOBTAINABLE`. Both sections were frozen and they could not both be
satisfied.

The rule is therefore **success first, identity second**. Damage must not be
able to manufacture an accusation of substitution, and that ordering is what
prevents it.

### 4.2 The states

```text
every git invocation exited zero
  AND kind and bytes were obtained
  AND recomputed object id == requested oid
      -> RESOLVED(kind, bytes)

every git invocation exited zero
  AND bytes were obtained
  AND recomputed object id != requested oid
      -> IDENTITY_VIOLATION

anything else
      -> LOOKUP_UNOBTAINABLE
```

### 4.3 Prerequisites for `RESOLVED` (FROZEN)

Hash equality alone does **not** establish the property §3 asks for. Two
mechanisms already recorded in the envelope document produce hash-matching bytes
while violating it: an unguarded `cat-file` in a promisor clone fetches the
object **over the network** and then returns bytes that hash correctly; and a
caller-selected counterfeit `git` returns the genuine published bytes for a
repository that does not exist. Both would satisfy a bare identity predicate.

`RESOLVED` may therefore be reported **only** when the observation was produced
under all of:

```text
trusted executable       git resolved from pinned absolute candidates,
                         never from an inherited PATH            (#143)
constructed environment  no ambient variable reaches the child   (#141, #143)
no lazy fetch            GIT_NO_LAZY_FETCH, no network access    (#141)
pinned config scopes     system and global git configuration
                         closed; the repository's own config
                         is the only scope read                  (#143)
no object substitution   GIT_NO_REPLACE_OBJECTS, plus the
                         object-id postcondition below           (#141)
```

These are already-merged properties, not new work. Naming them here binds this
contract to them, so that a future implementation cannot satisfy the letter of
§4.2 while answering a weaker question than §3 asks. **This does not choose the
git command** — that stays out of scope per §10. It forbids `RESOLVED` from
floating free of the guarantees that make it mean anything.

If any prerequisite cannot be established for a given lookup, the outcome is
`LOOKUP_UNOBTAINABLE`: no trustworthy result was obtained, which is exactly what
that state says.

**The cause of a non-zero exit is not classified.** No branch of this rule reads
`stderr`, matches on a message, or consults an exit code beyond
"did we obtain a trustworthy kind and bytes". Enumerating known failure
messages is the instance-by-instance repair this effort has spent six rounds
declining, and a resolver that classifies by diagnostic string inherits every
future change to git's wording as a silent semantic change.

`RESOLVED` carries `kind` because the resolver observed it. It does **not**
judge whether that kind is the one the citation required.

## 5. Explicit non-claims

`LOOKUP_UNOBTAINABLE` does **not** mean, and may never be rendered as:

```text
the object is absent from the repository
the object is absent from the local object database
permission was denied
the repository is corrupt
git malfunctioned
the locator is wrong
a fetch is required
```

It means exactly: **no evidence sufficient for a referential judgment was
obtained.** Any diagnostic text emitted for this state is bound by the same
limit — the failure mode being corrected is a *string* that asserted more than
its observation, so a state name that stays honest while its message does not
would repeat the defect with extra steps.

## 6. Consumer mapping (FROZEN)

```text
RESOLVED(kind, bytes)
    -> continue referential validation
       (object type, encoding, anchor uniqueness — the locator layer's work)

LOOKUP_UNOBTAINABLE
    -> ERROR: referential judgment unavailable
       do NOT re-pin
       do NOT edit the locator on the strength of this result

IDENTITY_VIOLATION
    -> ERROR: the checkout or the ruler is untrustworthy
       strictly stronger diagnostic than LOOKUP_UNOBTAINABLE
       NEVER downgraded to a locator defect
```

Both error states are `ERROR`, never `FAIL`: no judgment about the realization
was obtained, and reporting one would blame the target for a defect of the
machinery. This is the verdict algebra already frozen in the envelope document,
applied one floor down.

`IDENTITY_VIOLATION` preserves the existing family-level postcondition
(recompute the object id; whatever substitutes the bytes, the substitute does
not hash to the name). This preregistration does not weaken it and does not
propose to.

**`MALFORMED_LOCATOR` stays one floor up.** A resolver that successfully
supplies a *commit* where a blob was cited has done its job perfectly: it
returns `RESOLVED("commit", bytes)`, and the locator layer says the citation
names the wrong object type. Moving that judgment down would put the locator's
lawyer back inside the resolver, which §2.5.1 exists to prevent.

## 7. Falsification witnesses, preregistered

Required before the contract can be called satisfied by any implementation.
Listed here so the implementation cannot choose the experiments that suit it.

1. **A healthy, present blob** → `RESOLVED`, with bytes hashing to the request.
2. **A well-formed OID this checkout does not supply** → `LOOKUP_UNOBTAINABLE`,
   and the emitted text contains no claim of absence.
3. **An object entry that exists but yields no trustworthy object** — a
   deliberately damaged loose object → also `LOOKUP_UNOBTAINABLE`, and
   specifically **not** `IDENTITY_VIOLATION`. The construction must be the hard
   one: a body truncated behind an intact header, so that `cat-file -t`
   succeeds and the content command fails *after emitting a partial prefix*. A
   damaged object whose header is also destroyed exercises nothing, because
   both commands fail immediately and no debris is produced to misclassify.
4. **Substituted bytes** (replace-style identity violation) →
   `IDENTITY_VIOLATION`, existing postcondition intact.
5. **A non-blob object that resolves** → `RESOLVED(kind, bytes)`; the *locator*
   layer, not the resolver, classifies the wrong type.
6. **No witness may use `stderr` text as a semantic discriminator.** A witness
   that distinguishes states by matching git's prose is testing git's release
   notes.

7. **A `RESOLVED` result must fail to be reported when any §4.3 prerequisite is
   absent** — the promisor-fetch and counterfeit-executable constructions
   already carried by the merged corpus, re-read as resolver-state witnesses
   rather than as environment witnesses.

**Witness 3 is the load-bearing one.** It mechanically refutes the implication

```text
cat-file failed  =>  the object is absent
```

with a present-but-unreadable object, and it needs no root, no `chown`, and no
platform-specific ownership behaviour — unlike the EC-R1 experiment that
surfaced this defect, whose foreign-ownership arm could not be carried in the
corpus for exactly those reasons.

## 8. What reviewers are asked to attack

Not git, and not the eventual code.

> **Find two resolver outcomes that this preregistration collapses but that
> require different authoritative downstream judgments or remediation.
> Conversely, find any claimed distinction for which the proposed observations
> do not mechanically justify the stronger state.**

Both directions are live, and they fail in opposite ways:

- **too coarse** — a distinction the next layer normatively needs has been
  merged away, and the consumer will act identically on situations that demand
  different action;
- **too rich** — a state has been invented that the observation cannot
  establish, which is the original defect wearing a new name.

If someone shows that *absent locally* is normatively required downstream, this
preregistration owes an answer to a prior question — **which observation
licenses that claim** — before any implementation may be designed around it.

## 9. What may be added later, and on what evidence

`NOT_PRESENT_LOCALLY` was an obvious fourth state and is deliberately **not**
frozen here. It may be added only on a positive mechanical witness of absence —
an oracle that establishes the object is not in the local object database —
never on the inference

```text
git exited non-zero
therefore the object is absent
```

which is the defect itself, restated. Enumerating the object database directly
is a candidate oracle; whether it is a sound one, under partial clones and
alternates, is a question for that investigation and not an assumption for this
one.

If no such oracle exists, nothing is lost that was ever really held. **A poorer
but truthful state machine beats a richer one that occasionally invents facts**,
and this effort's revision record is already a reasonably complete museum of the
second kind.

## 10. Scope

```text
base: e70d019

IN
  resolver result semantics
  strongest licensed claims
  state distinctions
  consumer mapping
  planned falsification witnesses

OUT
  tools/*.py
  corpus changes
  git command choice
  stderr parsing
  Phase G
  pin state
  the parent Python startup residual
```

The envelope document is deliberately **not** amended to point here. Its record
of this residual stays exactly as EC-R1 left it — OPEN and unqualified — until
there is an accepted contract to point at. A pointer added now would read as
progress on the residual, and no code has moved.
