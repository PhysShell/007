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

Every branch below presupposes §4.3. Where its prerequisites do not hold, the
classification is not entered at all. No branch carries a replacement-guard
condition: that condition was added in an earlier revision on a justification
§4.3.2 records as false, and withdrawn with it.

```text
every git invocation exited zero
  AND kind and bytes were obtained
  AND recomputed object id == requested oid
      -> RESOLVED(kind, bytes)
         CLAIMS: these bytes were obtained locally under §4.3 and their
                 object id equals the cited oid.
         DOES NOT CLAIM: that they are the bytes the citer intended.
                 See §4.3.2 — that is a property of the citation, not an
                 observation available here.

every git invocation exited zero
  AND bytes were obtained
  AND recomputed object id != requested oid
      -> IDENTITY_VIOLATION
         (no guard condition: this branch rests on SOUNDNESS, and a
          detected mismatch is a positive observation)

anything else
      -> LOOKUP_UNOBTAINABLE
```

This table is the single normative statement of the classification. §4.3.2's
four-line grid is an exposition of the same rule, not a second one — an earlier
revision left the two disagreeing about the guard-absent matching case, which is
how the pattern of this document's defects usually presents: a repair made in
one section and not propagated to the other place that states the same thing.

### 4.3 Prerequisites for ANY byte-derived state (FROZEN)

**Checked before hashing, and gating both `RESOLVED` and `IDENTITY_VIOLATION`.**

The first version of this section gated `RESOLVED` alone, which left the
stronger state ungoverned. Review supplied the case: a counterfeit `git` exits
zero twice and returns a plausible `kind` with arbitrary bytes that do not hash
to the requested oid. Under a rule that gates only `RESOLVED`, that is
classified `IDENTITY_VIOLATION`, and §6 renders it as *the checkout or the ruler
is untrustworthy* — about a checkout that need not exist. The one thing actually
established is that **an untrusted program returned mismatching bytes**, which
is a statement about the program, not about any repository.

Both byte-derived states are claims *about the object store*. Neither may be
reached from bytes whose provenance was never established, so the prerequisites
gate entry to the classification rather than one of its outcomes. If any
prerequisite is absent there is no observation to classify, and the outcome is
`LOOKUP_UNOBTAINABLE` — which is precisely what that state means.

Hash equality alone does **not** establish the property §3 asks for. Two
mechanisms already recorded in the envelope document produce hash-matching bytes
while violating it: an unguarded `cat-file` in a promisor clone fetches the
object **over the network** and then returns bytes that hash correctly; and a
caller-selected counterfeit `git` returns the genuine published bytes for a
repository that does not exist. Both would satisfy a bare identity predicate.

No byte-derived state may therefore be reported unless the observation satisfies
every requirement below.

**These requirements are derived from §3's question, not from the
implementation.** That is the second derivation of this subsection and the
change is deliberate. The first version was written from recollection; when four
findings landed here, it was re-derived by reading the merged extractor and
reporting what that code does. A fifth finding landed anyway — a resolver can
satisfy every property the current code has and still answer a different
question — which falsified the diagnosis. Describing an implementation, however
accurately, cannot establish what the implementation is *for*. So the list is
now obtained by decomposing the question itself, and the code appears afterwards
as something that satisfies it rather than as its definition.

§3 asks: *can I, right now, without network access and inside the accepted trust
boundary, obtain exact bytes corresponding to the cited OID?* Each clause of
that sentence carries a requirement.

The requirements fall into two kinds, and conflating them has produced findings
in both directions:

```text
PROVENANCE      is this an observation OF AN OBJECT STORE at all?
                "I", no network, trust boundary.
                Absent -> nothing to classify. Gates BOTH hash outcomes,
                including an observed mismatch: bytes from an untrusted
                program say nothing about a repository, whatever they
                hash to.

TRANSFORMATION  were the bytes altered on the way back?
                the "exact bytes" clause.
                Scoped by §4.3.1: it may exclude only alteration that can
                leave the object id MATCHING, because alteration yielding
                a different id is already visible to the postcondition and
                is the guard case.
```

Only the **transformation** requirement is scoped by the detectability
criterion. An earlier revision applied that scoping to every requirement, which
made the provenance clauses unable to exclude different-id interference — so a
counterfeit executable returning mismatching bytes fell into the guard case and
licensed `IDENTITY_VIOLATION`, contradicting §4.3.1's gate. Provenance comes
first and answers a prior question; detectability only becomes meaningful once
there is an observation to detect anything in.

Both statements are about **effects, never mechanisms**: the same mechanism can
produce either effect depending on composition, which §4.3.1 demonstrates.

```text
"I"                      the program that produced the answer is chosen by
                         THIS layer — not by the caller, not by the
                         repository under examination

"exact bytes             the response is not passed through machinery that can
 corresponding to        alter content while leaving the object id MATCHING —
 the cited OID"          filters, textconv, any conversion layer. Substitution
                         that CHANGES the id is not excluded here: it is
                         detectable, so §4.3.1 makes it a guard, not a
                         prerequisite. The id is recomputed either way.

"without network         nothing in the lookup path performs network I/O:
 access"                 not the object lookup itself, and not any program
                         the lookup is able to invoke

"inside the accepted     the repository under examination is the SUBJECT of
 trust boundary"         the inquiry, so nothing it controls may select a
                         program to run, or transform the response in a way
                         the identity check cannot detect
```

**Repository configuration can execute programs, and an earlier draft said it
could not matter.** That draft argued the repository's own `local` and
`worktree` scopes need no exclusion "since configuration cannot add an object
directory". Configuration cannot add an object directory — and it can run
commands. With **every** guard of the previous list in place, a
repository-defined `filter.<driver>.smudge` executed during
`git cat-file --filters --path=…`:

```text
env -i  PATH=…  GIT_NO_LAZY_FETCH=1  GIT_NO_REPLACE_OBJECTS=1
        GIT_CONFIG_NOSYSTEM=1  GIT_CONFIG_GLOBAL=/dev/null
        git -c safe.directory=<repo> -C <repo> cat-file --filters --path=f.txt <oid>

  -> smudge filter executed: YES        bytes unchanged, oid matches
  -> raw `cat-file blob <oid>`:  smudge filter executed: no
```

A filter that passes bytes through unchanged and exits zero leaves the object id
matching, so the frozen rule reports `RESOLVED` while an arbitrary program has
run — a program the *subject of the inquiry* selected, which may have reached
the network. Every environment prerequisite held throughout. The hole was never
in the environment; it was in the assumption that the lookup command is a
detail.

**The lookup must therefore be a raw, non-transforming read**, and that is a
requirement rather than an implementation preference. §10's exclusion of "git
command choice" is narrowed accordingly: choosing *among raw reads* stays out of
scope; whether the read is raw at all is in scope, and is this requirement.

**How the merged implementation satisfies these** — recorded as satisfaction,
not as definition, so a future change can be checked against the requirement
rather than against a description of its predecessor:

All rows below are in `tools/a1_v2_extract_graph.py`, function `_local_object`
and the module constants above it, as of `e70d019`. Each names the revision that
introduced the property, so a later reader can re-check the predecessor rather
than trust this summary of it:

```text
"I"              GIT_CANDIDATES, pinned absolute paths, no PATH lookup
                     034f4f2  "pin which git answers"
                 GIT_ENV_ALLOW = (), child PATH synthesized from the
                 resolved binary
                     1e7a4ae  "construct git's environment, never inherit it"
                     cdaed35  "narrow the grant to its witness"  (allowlist -> empty)

exact bytes      raw `cat-file <type> <oid>` — no --filters, no --textconv
                     51b2d6a  resolver introduced with the raw form
                 object id recomputed from the response and compared
                     2d372a7  "check the bytes against the name asked for"

no network       GIT_NO_LAZY_FETCH=1
                     52574e4  "enforce no-network with GIT_NO_LAZY_FETCH"

trust boundary   GIT_CONFIG_NOSYSTEM=1, GIT_CONFIG_GLOBAL=/dev/null
                     e051882  "an empty environment is not an empty git
                               configuration"
                 exactly one command-scope key,
                 -c safe.directory=<the repository being read>
                     cdaed35  "narrow the grant to its witness"

route closure   GIT_NO_REPLACE_OBJECTS=1 — closes the refs/replace ROUTE,
 (not a          which can produce either a detectable or an undetectable
 prerequisite)   effect depending on composition (§4.3.1). Not a guarantee
                 of detection.
                     2d372a7  route closure and postcondition added together
```

The raw form is the oldest of these and the only one never introduced by a
review finding. It has been correct since `51b2d6a` by accident of drafting
rather than by decision — which is why it is written down here as a requirement
now, and why nothing in this document treats its survival as evidence that it
was ever load-bearing on purpose.

**On that command-scope key.** `git --show-scope` reports `command` as a scope
of its own, so an earlier draft claiming the repository's config was "the only
scope read" was false, and put an implementation in a bind: keep the grant and
be ineligible to report `RESOLVED`, or drop it and fail on a foreign-owned
checkout. The grant is not an exception to these requirements — it satisfies the
`"I"` clause. It is narrow (one key), explicit (visible in argv), scoped to the
single repository under examination, and supplied by this layer rather than
inherited. An implementation that widened it would be violating the list.

The repository's `local` and `worktree` scopes are still read, necessarily —
without them the directory is not a repository. What the requirements forbid is
not reading that configuration but letting it **choose what runs**.

### 4.3.1 What decides prerequisite from guard

The trust-boundary clause is narrow on purpose, and the first version of it was
not. It said *nothing the repository controls may execute or transform anything*
— which swallows `refs/replace`, since a replace ref is repository-controlled
and does substitute the bytes returned for a name. Where such a substitution
yields a **different** object id, the rows below place it in the guard case and
§7's witness 4 requires `IDENTITY_VIOLATION`. A blanket clause made those
unsatisfiable — the same contradiction as finding 4, reintroduced by a broader
rule while repairing something else.

The criterion that separates them is **what the postcondition can detect**:

**The rows below operate inside §4.3's gate, not instead of it.** Where a
provenance prerequisite is absent there is no observation to classify at all,
and the outcome is `LOOKUP_UNOBTAINABLE` — including when a mismatch was
*observed*. A counterfeit executable returning bytes that recompute to a
different id shows the difficulty plainly: the effect matches the guard row, but
nothing establishes that any object store was consulted, so there is no
substitution to report. **A missing prerequisite overrides both byte-derived
states.**

Within that gate, classification is by **effect on the object id** and by
nothing else. The rows are defined by what the postcondition observes; the
mechanisms in them are **illustrations, not memberships**.

```text
GUARD          the substitution yields a DIFFERENT object id, so recomputing
               it reveals the substitution

PREREQUISITE   the interference can leave the object id MATCHING, and the
               resolver can exclude it by construction
               e.g. counterfeit executable, lazy fetch, smudge/textconv
               filters — each can return correct-hashing bytes

NEITHER        the interference leaves the object id MATCHING and no
               construction at this layer excludes it
               e.g. an object file overwritten with a PREPARED COLLIDING blob
               -> not detectable, not excludable, and therefore outside what
                  RESOLVED claims at all (§4.3.2)
```

**No mechanism belongs categorically to a row, and this contract has now been
wrong about that five times.** Revisions of this section successively asserted
that `refs/replace` is always detectable, then never, then always again. All
three were false, because a mechanism's effect depends on what it is composed
with.

**What was OBSERVED** — git 2.43.0, in a scratch repository, three distinct
non-colliding blobs:

```text
A = d1006e4be4c5fb8a694105ed74c3167fbe7f094c   "AUTHENTIC AUTHORITY"
C = 261426e18e847c2fa3ed25516a8477926c793e91   "DECOY OBJECT"
D = d97e4ba5af8bc5436bddc013b4733eeed1c1a2e7   "ATTACKER CONTENT VIA TWO HOPS"

git replace <A> <C>                          replace ref: A -> a DIFFERENT oid
cp <D's object file> <C's object file path>  C's path now holds D's object
git cat-file blob <A>
  -> "ATTACKER CONTENT VIA TWO HOPS"
```

So git followed the replace ref to `C` and returned the bytes found at `C`'s
path **without validating them against `C`** — no validation at either hop.

**What is INFERRED, and was NOT executed:** that if `D` were chosen to collide
with `A`, the resolver would recompute `A`'s oid and the postcondition would see
a match. That half requires SHA-1 collision material, which was not available in
this environment; the observation above establishes only that the *route*
delivers unvalidated bytes of the attacker's choosing. The inference from there
is arithmetic, not an experiment, and it is labelled so rather than folded into
the word "verified".

So a replace ref lands in the guard row when its target resolves to bytes of a
different id, and in the third row when composition makes those bytes collide
with the citation. Same mechanism, different effect, different row. Findings 8,
13, 16, 21, 22 and 27 were all this error in different clothes: a statement
about a mechanism standing in for a statement about an effect.

### 4.3.2 What RESOLVED claims, and what the hash properties are for

No cryptographic assumption is checkable inside a repository, and this document
went through several revisions trying to find one that was. The resolution is
not a better premise but a narrower claim.

**`RESOLVED` needs no hash premise at all.** It reports two things, both
directly observed:

```text
RESOLVED CLAIMS
    these bytes were obtained locally, under every §4.3 requirement, and
    their object id equals the cited oid

RESOLVED DOES NOT CLAIM
    that these are the bytes the citer intended
```

Both remain true observations even if practical second preimages existed, so
neither hash property gates the state. Making one gate it would leave an
implementation with two options — silently assume an undischargeable property,
or never report `RESOLVED` — which is the bind §4.3's prerequisites exist to
prevent, and which earlier revisions of this section walked into twice.

`IDENTITY_VIOLATION` needs no premise either, for a different reason: it fires
on an **observed** mismatch, and the postcondition's *soundness* — a reported
mismatch is a real one — does not depend on the hash's strength.

**The hash properties bear on what a CONSUMER may infer**, not on what the
resolver may report:

```text
second-preimage resistance   what lets a consumer treat RESOLVED bytes as
                             THE object for an already-trusted citation
                             -> SHA-1, as of 2026-08: no practical attack
                                publicly known. SEE THE CAVEAT BELOW.

collision resistance         what would let a consumer treat the citation as
                             denoting uniquely at all. Settled when the oid
                             was chosen, not when it is resolved.
                             -> SHA-1: broken in practice (identical-prefix
                                2017, chosen-prefix 2020)
                             Inherited by every consumer of the pin,
                             PINNED_BLOB included.
```

Both are risks the reader of a verdict carries. They are declared here so the
reader knows what they are carrying, and neither is dischargeable at this layer.

**The cryptographic status above is an as-of statement, and this document cites
no authority for it.** It is the author's understanding at 2026-08, not a
finding this contract establishes, and it is the kind of claim that changes
without the document changing. A consumer relying on it must re-check it against
current cryptanalytic literature rather than against this file. The *structure*
— which property underwrites which inference — is what this document freezes;
the *status* of each property is a fact about the world that this document only
reports, and reports without a source.

**The prepared-collision case, and why it is not a `refs/replace` case.** An
adversary who authored an artifact before it was pinned can prepare a colliding
pair and cause the locator to cite the shared oid. Delivery is by **overwriting
the object file**, not by a ref: colliding blobs share an oid, so a replace ref
would map the oid to itself. Verified against git 2.43.0 —

```text
git replace <oid> <oid>     creates the ref
git cat-file blob <oid>     fatal: replace depth too high   (exit 128)
```

— which is `LOOKUP_UNOBTAINABLE`. A replace ref aimed at a *different* oid
usually mismatches and is detected, but not necessarily: §4.3.1 records the
composition that makes it undetectable. The channel that carries the
prepared-collision case in every form is the object file itself:

```text
git cat-file DOES NOT VERIFY that the object stored at an oid's path hashes
to that oid. With A's object file overwritten by a valid object for B:

    cat-file blob <A>  ->  "SUBSTITUTED CONTENT ENTIRELY"
    git fsck           ->  error: hash-path mismatch
```

The identity postcondition is therefore not defence in depth over a check git
already performs — it **is** the check. Against a *colliding* overwrite it sees
nothing, which is §4.3.1's third row and precisely what `RESOLVED` declines to
claim.

**What §2.5.1 says, and what this document infers, kept apart:**

```text
WHAT THE SOURCE SAYS
  docs/tasks/a1-f-v2-envelope.md §2.5.1, introduced 51b2d6a:
      blob:  <immutable git blob oid>     # the ONLY identity
  It does NOT mention any hash property, and says nothing about PINNED_BLOB.

WHAT THIS DOCUMENT INFERS
  treating an oid as an artifact's identity presupposes hash properties the
  citation scheme never stated. That inference is this document's; it is not
  a ratification recorded anywhere else, and this is the first place the
  reliance is written down — for consumers of the pin, not for this resolver.
```

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
       claims hash equality under §4.3, NOT the citer's intent (§4.3.2)
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

`IDENTITY_VIOLATION` preserves the existing postcondition — recompute the
object id and compare — and states exactly what that establishes: **the bytes
returned recomputed to a different object id.** It does NOT establish that every
substitution is detectable; §4.3.1 records a composition that is not. The state
rests on the postcondition's SOUNDNESS and carries no hash premise at all
(§4.3.2). This preregistration does not weaken the postcondition and does not
propose to; it narrows the claim made *from* it.

**`MALFORMED_LOCATOR` stays one floor up.** A resolver that successfully
supplies a *commit* where a blob was cited has done its job perfectly: it
returns `RESOLVED("commit", bytes)`, and the locator layer says the citation
names the wrong object type. Moving that judgment down would put the locator's
lawyer back inside the resolver, which §2.5.1 exists to prevent.

## 7. Falsification witnesses, preregistered

Required before the contract can be called satisfied by any implementation.
Listed here so the implementation cannot choose the experiments that suit it.

1. **A healthy, present blob**, with every §4.3 prerequisite satisfied →
   `RESOLVED`, with bytes hashing to the request.
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
   `IDENTITY_VIOLATION`, existing postcondition intact. The §4.3 prerequisites
   all hold throughout; only the §4.3.1 *guard* is removed, which is what makes
   this witness reachable at all. What the witness requires is the **observed
   effect**, not a mechanism: the bytes returned must recompute to a
   different object id. A substitution engineered to recompute to the cited
   id — by whatever route — is §4.3.1's third row and is not what this
   witness exercises.
5. **A non-blob object that resolves** → `RESOLVED(kind, bytes)`; the *locator*
   layer, not the resolver, classifies the wrong type.
6. **No witness may use `stderr` text as a semantic discriminator.** A witness
   that distinguishes states by matching git's prose is testing git's release
   notes.

7ter. **Object-database substitution that the postcondition CAN see** — an
   object file overwritten by a different valid object → `IDENTITY_VIOLATION`,
   since `cat-file` performs no hash verification of its own. This is the
   channel the postcondition exists for, and it is independent of
   `refs/replace`.

7bis. **A lookup that engages filter or conversion machinery must not report
   `RESOLVED`** — a repository-defined smudge filter that passes bytes through
   unchanged leaves the object id matching, so this witness cannot be satisfied
   by checking the returned bytes. It must observe that no repository-selected
   program ran.

7. **With any §4.3 prerequisite absent, NEITHER `RESOLVED` NOR
   `IDENTITY_VIOLATION` may be reported** — the outcome is
   `LOOKUP_UNOBTAINABLE`. Both arms are required: a witness that only checks
   `RESOLVED` is not withheld would pass while the stronger accusation remained
   reachable from untrusted bytes, which is the defect this witness exists for.
   The promisor-fetch and counterfeit-executable constructions already carried
   by the merged corpus supply the fixtures, re-read as resolver-state witnesses
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
  the choice AMONG raw lookup commands
  stderr parsing
  Phase G
  pin state
  the parent Python startup residual
```

The envelope document is deliberately **not** amended to point here. Its record
of this residual stays exactly as EC-R1 left it — OPEN and unqualified — until
there is an accepted contract to point at. A pointer added now would read as
progress on the residual, and no code has moved.
