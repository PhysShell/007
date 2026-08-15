# A1-F v2 — Resolver observation contract (RS-P0)

**Status:** preregistration. Normative. Docs-only: this change adds no production
code, no tests and no fixtures.

**What this file is.** The frozen contract for what the object resolver may
*conclude* from a lookup, and what each conclusion obliges its consumer to do. It
is written before an implementation exists so that the implementation can be
judged against a target it did not choose.

**Precedence.** `docs/tasks/a1-resolver-semantics-evidence.md` records the
observations this contract rests on. It is **non-normative**. Where that file and
this one disagree, this one governs and the disagreement is a defect in the
evidence record.

**Relation to the terminated attempt.** PR #144 preregistered the same question
and was terminated without merge after an independent reviewer found a semantic
self-contradiction in it at its final head. That attempt is retained as a
**falsification corpus** for this document — a list of defects this contract must
be checkable against — and explicitly **not** as source text. The disposition of
each surviving finding is recorded in the evidence file, which is where a coverage
argument belongs; it is not part of this contract.

---

## 1. Purpose — the primary question

> Can this layer, without network access and inside its accepted trust boundary,
> obtain a **complete** and **trustworthy** response for the cited object id?

Everything below is an answer to that question and to nothing else. The resolver
does not decide whether the cited object is the right object, whether the citation
is well-formed, or whether the artifact means what the citer thought. Those are
other layers' questions (§8).

---

## 2. Observation prerequisites

Three independent conditions must hold before any byte-derived conclusion is
admissible. They are checked in the order given.

### 2.1 PROVENANCE admissible

- the executable is **selected by this layer**, from a fixed list of absolute
  paths — never resolved through `PATH` and never named by the repository;
- the child environment is **constructed**, not inherited: it carries only names
  this layer grants explicitly;
- **the operation performs NO NETWORK ACCESS AT ALL** — not merely that no
  network-delivered bytes satisfy the lookup. The prohibition is on *contacting a
  remote during the operation*, for any purpose: reachability probe,
  authentication, metadata, negotiation, or lazy/promisor fetch.

  The weaker form — "no lazy or network acquisition may **satisfy the lookup**" —
  was insufficient, and this is why: an implementation could contact a remote for
  a reachability or authentication check, then serve the object from the local
  store. No network-delivered byte would satisfy the lookup, §2.1 would hold, and
  §3 would emit `RESOLVED` — while §1 asks whether this layer can answer **without
  network access**, and §4 has `RESOLVED` claim that an admissible **local**
  acquisition occurred. The prerequisite was weaker than the question the whole
  document answers, so a conforming implementation could perform network I/O
  during resolution and still be conforming;
- the read is **raw** — no filter, conversion or textual transformation is
  applied to the returned bytes. This is a requirement on **this layer's own
  invocation**: the transformation path is opt-in, so the obligation is that the
  resolver never requests it (evidence E-8);
- the repository under inspection **cannot choose** which program runs or which
  transformation path is taken.

### 2.2 RESPONSE complete

The response is complete **when, and only when, BOTH of these were obtained**:

- a **usable kind** for the requested oid, and
- the **complete bytes** for it,

each through an operation that **succeeded**.

**`usable kind` is defined here, in resolver-only terms**, because leaving it to
be read naturally makes the emitted state ambiguous:

```text
a usable kind is a value obtained from a successful operation that names
one of the git object types

    blob    tree    commit    tag

USABILITY IS NOT SUITABILITY. Whether the kind is the one some other layer
wanted has no bearing on this clause.
```

Read as "usable *for the caller's purpose*", a `commit` obtained where the locator
expected a `blob` would fail §2.2, so §2 would fail and §3 would emit
`LOOKUP_UNOBTAINABLE` — while §8 and W5 require `RESOLVED` for exactly that input.
The same input would carry two states. §8 already says a resolver result is never
about suitability; the undefined word had quietly readmitted suitability into §2,
which is the one place it must not appear.

The four types are enumerated for the same reason §3.1 enumerates the object
formats: an unenumerated "recognised kind" leaves the same gap one level down.
Widening the set is a change to this contract under §10.

**Completeness is defined by the evidence obtained, not by a set of operations the
implementation nominates as required.** That distinction is load-bearing, and two
separate defects came from getting it wrong:

- an implementation that obtains the kind and skips or short-circuits the body has
  succeeded at every operation *it* considered required, while the bytes are
  absent. Quantifying over an implementation-selected set lets it satisfy the
  clause by choosing a smaller set;
- an operation can exit **zero** while reporting the object unavailable. **Git
  offers lookup forms that report absence through a zero exit status** —
  `cat-file --batch` and `--batch-check` answer `<oid> missing` and exit `0`
  (evidence E-8.2).

So neither command success nor the implementation's own notion of "required" can
establish completeness. Success is a property of a command; availability is a
property of the answer; this clause requires the answer.

**The output of a failed command is debris, never evidence** — it is never hashed,
never parsed, and never compared against the citation.

A partial response is not a weak observation. It is **no observation**. Neither is
an absent one.

### 2.3 IDENTITY FUNCTION available

The digest function `oid(kind, bytes)` of §3.1 was **obtained**: the repository's
object format was read through an operation that **succeeded**, and the value it
returned names a format in the enumerated set §3.1 permits.

This is a prerequisite rather than a step inside §3, for a structural reason. Both
of §3's positive branches compare `oid(kind, bytes)` against the requested id, so
both are meaningless unless that function exists. Establishing its existence
downstream — "the format read failed, therefore the lookup failed too" — requires
an argument that must hold for **every** input, and it does not hold for two:

- the format read is a **separate operation** from the lookup. The kind and the
  bytes can be obtained successfully while `rev-parse --show-object-format` fails;
  §2.2 quantifies over the kind and the bytes only, so §2 would still hold;
- **"unsupported by git" and "unrecognised by this layer" are different
  conditions.** A format that git implements and this layer does not leaves every
  lookup operation succeeding, with `oid` still having no algorithm.

In either case §2 would hold with `oid` undefined: §3 would point at its two
positive branches while §3.1 demanded `LOOKUP_UNOBTAINABLE`, and the same input
would carry two states. Making availability a prerequisite closes that by
construction — the input fails §2, falls to §3's `ANYTHING ELSE` branch, and
`LOOKUP_UNOBTAINABLE` is the only state it can carry.

**Totality is not availability.** §3.1 requires the chosen function to be total,
so the comparison is defined for every `(kind, bytes)`. §2.3 requires that there
be a chosen function at all. The first is a property of an algorithm; the second
is a precondition for having one. Conflating the two is what left the gap.

---

## 3. Normative state table

This is the **only** statement of the state partition in this document. There is
no second table, and no alternative vocabulary that re-expresses it.

```text
§2 holds  AND  oid(kind, bytes) == requested oid
    -> RESOLVED(kind, bytes)

§2 holds  AND  oid(kind, bytes) != requested oid
    -> IDENTITY_VIOLATION

ANYTHING ELSE
    -> LOOKUP_UNOBTAINABLE
```

**Exhaustiveness is STRUCTURAL here, not argued.** The third case is the
complement of the first two: any input that does not satisfy one of the two
positive conditions falls to `LOOKUP_UNOBTAINABLE` by construction. No input can
escape the partition, and no future clause can open a gap in it.

This replaces an earlier form in which all three branches carried positive
conditions and exhaustiveness had to be *demonstrated*. That form failed twice, in
two different ways, and both failures were the same shape: an input that satisfied
no branch, because the conditions did not quite tile the space. A partition whose
completeness depends on an argument is only as sound as the argument, and this one
was twice wrong.

The two positive branches remain mutually exclusive, since `==` and `!=` cannot
both hold. They are also **jointly reachable** only when `oid(kind, bytes)` is
defined, and that takes **both** prerequisites: §2.3 establishes that a digest
function was obtained at all, and §3.1 requires the obtained function to be
**total**. Either alone is insufficient. If totality were relaxed, `§2 holds`
inputs could satisfy neither positive branch and a genuine identity result would
be silently reported as unobtainable. If availability were merely argued rather
than required, `§2 holds` inputs could arrive with no function to apply — which
is the defect §2.3 was added to close, after this paragraph cited totality alone
as if it settled the question.

**A missing prerequisite outranks both byte-derived states.** If §2 does not hold,
the outcome is `LOOKUP_UNOBTAINABLE` **even when a mismatch was observed** — an
untrustworthy channel cannot support a report about the object store, in either
direction.

### 3.1 What `oid(kind, bytes)` denotes

`oid(kind, bytes)` is the digest of the git object encoding — the header
`"<kind> <length>\0"` followed by the bytes — computed by this layer, under the
same §2.1 provenance requirements as the lookup itself. A recomputation performed
by a program the repository could choose is not a check; it is a second thing to
trust.

**The algorithm is the REPOSITORY'S OBJECT FORMAT, not a fixed choice.** A git
repository declares its object format in the `extensions.objectFormat`
configuration variable, and the object ids it issues are digests under that
format. This layer must compute the same function the repository used.

**The enumerated set is exactly these two:**

```text
sha1     -> SHA-1,   object ids 40 hex characters
sha256   -> SHA-256, object ids 64 hex characters
```

Any other returned value is **outside the set**, whatever git may do with it.
Widening this set is a change to this contract under §10, not an implementation
decision — which is the point of enumerating it here rather than deferring to
whatever the installed git happens to accept.

**The read is fixed, and it is not the configuration variable.** The resolver
obtains the format from `rev-parse --show-object-format`, invoked under the same
§2.1 requirements as the lookup. Reading `extensions.objectFormat` directly would
be wrong in the ordinary case: a `sha1` repository normally leaves it **unset**,
and `git config --get extensions.objectformat` exits 1 while
`--show-object-format` still answers `sha1` (evidence E-8.3). The variable names
the property; the command is how the property is read.

**Absent and unsupported values.** An absent value is not an error — it is the
`sha1` default, and `--show-object-format` reports it as such, so the resolver
simply uses what it is told. A read that **fails**, or that returns a format
outside the enumerated set, means no identity function was obtained: **§2.3 does
not hold, so §2 does not hold**, and §3's default branch yields
`LOOKUP_UNOBTAINABLE`.

That routing is **structural — it does not depend on how git behaves in such a
repository.** Git is expected to refuse to operate on an object format it does not
implement, which would make the lookup operations fail as well; this contract does
not rely on it. An earlier form of this paragraph made exactly that behavioural
claim load-bearing, and it is both unversioned and too narrow: it says nothing
about the case where the lookup operations succeed and only the format read fails,
nor about a format git implements and this layer does not recognise. §2.3 covers
all of them without asserting anything about a program's behaviour.

The resolver must **never** guess a format when the read fails or returns
something it does not recognise — guessing reintroduces exactly the mismatch this
clause exists to prevent, and `LOOKUP_UNOBTAINABLE` is the honest answer for a
repository whose identity function is unknown.

Fixing the algorithm at SHA-1 would be wrong rather than merely narrow: in a
`sha256` repository, a SHA-1 recomputation cannot equal the requested 64-character
oid for **any** input, so §3 would report `IDENTITY_VIOLATION` for every authentic
object and the consumer would return `ERROR` on a lookup that fully succeeded.
This also contradicts the frozen scalar definition in
`docs/q-deck/a1-authority-contracts.md`, **pinned to the reviewed base commit
`e70d019923a958bb18d8dbb266da007c6e93a88c`** — line 1216 at that revision, blob `e22539ddf4f7c9ab260e16835eef8ef18abbe726`:

```text
| `CommitId` | full object id, the repository's object-format width | never abbreviated … |
```

The pin is the point, not decoration. Cited against the mutable path alone, this
paragraph would go on asserting a contradiction after the q-deck had changed, and
a reader could not re-check the premise that justified the requirement. If a later
revision of that document alters the definition, this clause does not silently
follow it: the disagreement becomes a **contract question under §10**, to be
settled deliberately rather than by whichever file was edited last.

**Whatever the format, the chosen function must be TOTAL and must not be
collision-detecting.** Totality is what keeps the comparison defined whenever §2
holds, so a lookup that genuinely succeeded is reported as such.

**Reading the object format is not a repository-controlled program choice.** §2.1
forbids the repository selecting *what runs*; the format selects only among a
fixed, enumerated set of digest functions this contract already permits. A
repository that misreports its format produces ids that do not match its own
objects, which is precisely what the §3 comparison detects — so the failure mode
is caught rather than trusted.

**A collision-*detecting* variant is therefore NOT permitted here.** It is a
*partial* function: on input exhibiting a known collision pattern it may refuse to
produce a digest at all. Since §3's third case is a structural default, such an
input would **not** go unassigned — it would fall to `LOOKUP_UNOBTAINABLE`. That is
the precise harm: a lookup that satisfied every prerequisite and returned complete
bytes would be reported as *unobtainable*, which §6 says means the layer did not
obtain a response. It did. The report would be false, and it would be false in the
direction that hides a successful acquisition rather than inventing one.

Two resolvers conforming to this contract must not be able to emit different states
for the same input. That is why the algorithm is **determined by the repository's
declared format** rather than left to the implementation: both resolvers read the
same format and therefore compute the same function. Adopting a partial variant
would require amending §3 to give its refusal outcome a state — a different
contract, out of scope for this document.

**This document does not establish which variant any particular git build uses.**
That is a fact about a build, not about this contract, and no observation here
speaks to it. That ignorance is precisely why the function is **named here** rather
than inherited from whichever tool an implementation happens to call: an
implementation must compute this function, not adopt one by accident.

---

## 4. Exact claims per state

```text
RESOLVED CLAIMS
    an admissible local acquisition occurred, under every §2 requirement
    these returned bytes hash to the requested object id

RESOLVED DOES NOT CLAIM
    that the citer intended these particular bytes
    that the object id uniquely denotes one semantic artifact
    that collisions do not exist
```

`RESOLVED` therefore rests on **no cryptographic premise**. Both of its claims are
direct observations and remain true whatever the status of any hash property.

`IDENTITY_VIOLATION` likewise rests on no premise: it reports that a recomputation
disagreed with the citation, which is a fact about the arithmetic performed, not
about what an adversary can achieve.

---

## 5. Prepared-collision disposition

Stated explicitly, because leaving it implicit is what terminated the previous
attempt.

```text
§2 prerequisites hold
commands complete successfully
oid(kind, bytes) == requested oid
    -> RESOLVED

even if the returned bytes are a colliding alternate to the artifact
the citer had in mind.
```

This is **not** an exception, a degraded `RESOLVED`, or grounds for
`LOOKUP_UNOBTAINABLE`. It is the ordinary operation of §3. The resolver observes
**identity equality**, not citer intent; a colliding alternate satisfies identity
equality, and §4 already declines to claim intent. No implementation may read this
case as anything other than `RESOLVED`.

**`RESOLVED` here is unconditional.** §3.1 requires a total, non-collision-detecting
digest precisely so that this case has exactly one outcome; there is no conforming
implementation that emits anything else for it. No implementation may decide this case by inspecting the
*content* of the bytes — recognising a pattern and selecting a state from it is
classification by mechanism, which §3 does not authorise and which W8 exists to
catch.

---

## 6. `LOOKUP_UNOBTAINABLE` non-claims

`LOOKUP_UNOBTAINABLE` means exactly: *this layer did not obtain an admissible
complete response.* It does **not** mean any of:

```text
the object is absent locally
the object is absent from the repository
permission was denied
the repository is corrupt
a fetch is required
the locator is wrong
git malfunctioned
```

Each of those is a **different** proposition requiring its own positive evidence.
None of them may be inferred from this state, reported alongside it as if
established, or selected by matching the text of a diagnostic message.

---

## 7. Consumer mapping

```text
RESOLVED
    -> continue to referential validation

LOOKUP_UNOBTAINABLE
    -> ERROR — judgment unavailable
    -> NO re-pin, NO locator edit

IDENTITY_VIOLATION
    -> ERROR — the evidence source is untrustworthy
    -> NEVER recorded as a locator defect
```

Both error states are `ERROR`, not `FAIL`: they report that the verifier could not
form a judgment, not that the subject failed one. Neither authorises a repair of
the citation, because in neither case has the citation been shown to be wrong.

---

## 8. Layer boundary

A resolver result is about **obtaining bytes**, never about their suitability.

```text
RESOLVED(commit, ...) where the locator expected a blob
    -> the RESOLVER succeeded
    -> the LOCATOR layer rejects the citation
```

The wrong object kind is not a resolver failure and must not be folded into any
resolver state. Collapsing it would make the resolver's report less discriminating
than the distinction the locator layer has to enforce.

---

## 9. Required implementation witnesses

An implementation claiming this contract must be falsifiable by each of the
following.

**These witnesses are instances of §3, never an independent source of states.**
Each one fixes an input; the state beside it is the one §3 already assigns to that
input, reproduced here only so the experiment is named in advance. If any witness
below can be read as assigning a state §3 does not, that is a defect in the
witness, and §3 governs.

```text
W1  §2 holds; healthy blob
        -> RESOLVED

W2  the object cannot be produced at all
        -> LOOKUP_UNOBTAINABLE

W3  §2.1 holds; the kind operation succeeds, the body operation emits
    partial output and then fails
        -> LOOKUP_UNOBTAINABLE
        -> the partial output is never hashed

W4  §2 holds; a complete successful response whose bytes recompute to a
    different id
        -> IDENTITY_VIOLATION

W5  §2 holds; a successful resolution of a non-blob object
        -> RESOLVED, and the locator layer rejects it (§8)

W6  the wording of any diagnostic text is varied
        -> the selected state does not change

W7  a §2.1 provenance prerequisite is ABSENT
        -> LOOKUP_UNOBTAINABLE for BOTH the hash-matching and the
           hash-mismatching case

W8  §2 holds; a PREPARED COLLISION: a complete successful response whose
    bytes are NOT the artifact the citer meant, and which recompute to the
    requested id
        -> RESOLVED, with no citer-intent claim attached

        REQUIRES GENUINE COLLISION MATERIAL. There is no weaker model: any
        input that is merely a healthy matching response is W1, and cannot
        falsify §5.

        UNTIL EXERCISED, W8 IS OWED — NEVER RECORDED AS PASSED.

W9  §2.1 holds; the kind and the complete bytes ARE obtained, but the
    object-format read FAILS or returns a format outside the enumerated set
        -> LOOKUP_UNOBTAINABLE
        -> the bytes are never hashed under a guessed or defaulted format

        This is the §2.3 witness. It must be exercised in BOTH arms — read
        failure, and unrecognised value — because they are different
        conditions and an implementation can handle one while defaulting
        the other to sha1.
```

W4, W5 and W8 are conditioned on `§2 holds` because a complete successful response
establishes **§2.2 only**. Without that condition each of them would overlap W7 on
the provenance-absent input and demand the opposite state — which is exactly how a
witness list turns into a second, conflicting statement of the partition.

W9 is conditioned on `§2.1 holds` rather than `§2 holds` for the same reason in
the other direction: its whole point is an input where §2.1 and §2.2 are satisfied
and §2.3 is not. Conditioning it on `§2 holds` would make it unreachable.

W3, W7 and W9 are the three that a plausible implementation is most likely to
fail, and W7 and W9 must each be exercised in **both** of their arms to be
meaningful.

---

## 10. Scope and future extension

**In scope:** the state partition, its prerequisites, the claims attached to each
state, the consumer mapping, and the witnesses above.

**Out of scope:** the resolver implementation, its defect fixes, the locator layer,
the ledger gate, and any change to the frozen graph registry.

**`NOT_PRESENT_LOCALLY` is deliberately NOT frozen.** Adding it requires a
**positive absence oracle** — a mechanical witness that establishes absence rather
than inferring it from a failure to produce. No such oracle is specified here, and
until one is, absence is not distinguishable from the other causes listed in §6.
