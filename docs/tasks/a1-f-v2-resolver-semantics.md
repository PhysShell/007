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

**Each requirement below is scoped by §4.3.1's criterion**, and the scoping is
not decoration: three separate findings — 8, 13 and 16 — were the same defect,
a prerequisite phrased broadly enough to swallow `refs/replace`, which §4.3.1
classifies as a *detectable* guard. Broad phrasing of a requirement is how that
keeps happening, so a requirement may exclude only interference the
postcondition cannot see. Anything it can see belongs to §4.3.1 as a guard, and
saying so inside each clause is what stops the next rewrite from reintroducing
it.

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

guard (not a    GIT_NO_REPLACE_OBJECTS=1, whose failure the identity
 prerequisite)   postcondition detects — see §4.3.1
                     2d372a7  guard and postcondition added together
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
and does substitute the bytes returned for a name. This section designates exactly that
case a **guard** failure, classified by the identity postcondition, and §7's
witness 4 requires it to yield `IDENTITY_VIOLATION`. A blanket clause made those
unsatisfiable — the same contradiction as finding 4, reintroduced by a broader
rule while repairing something else.

The criterion that separates them is **what the postcondition can detect**:

```text
PREREQUISITE   the interference can leave the object id MATCHING, so
               recomputing it cannot reveal the interference
               - counterfeit executable   (genuine bytes, correct hash)
               - lazy fetch               (correct bytes, from the network)
               - smudge/textconv filters  (pass-through leaves bytes identical)

GUARD          the interference changes the bytes, so recomputing the object
               id reveals it — see §4.3.2 for which hash property that needs,
               and for which state actually needs it
               - refs/replace substitution (substitute bytes, different hash)
```

A prerequisite is required precisely where the postcondition is blind. Where the
postcondition can see, a guard is defence in depth and its failure is a finding
rather than a disqualification — which is what witness 4 exercises, and what
corpus case 3t already asserts.

This is not a new position, and the artifact is named so it can be re-checked
rather than taken from this document's summary of it:

```text
artifact   tools/tests/test_a1_v2_ledger_gate.py, case 3t (~L880-L900)
introduced 2d372a7  ("EB-R1 review fix 4 — check the bytes against the name
                     asked for")
asserts    with GIT_NO_REPLACE_OBJECTS deliberately removed from GIT_ENV,
           a substituted object still raises OBJECT IDENTITY VIOLATED
```

What the witness **asserts** is that one abort still happens without that one
guard. The guard/prerequisite distinction in this section is an **inference**
drawn from it — the witness is evidence for the principle, not a statement of
it, and a reader who disagrees with the inference should be able to go and read
the assertion. The first draft of this contract contradicted the asserted
behaviour outright, which had already shipped and been independently reviewed
twice.

These are already-merged properties, not new work. Naming them here binds this
contract to them, so that a future implementation cannot satisfy the letter of
§4.2 while answering a weaker question than §3 asks. **This does not choose the
git command** — that stays out of scope per §10. It forbids the byte-derived
states from floating free of the guarantees that make them mean anything.

The merged implementation already holds every prerequisite, so the counterfeit
scenario above is not reachable there today. The defect is in what this
document would have *licensed*: an implementation could satisfy the frozen rule,
drop a prerequisite, and issue the system's strongest accusation on the strength
of output from an arbitrary program. A preregistration is judged by what it
permits, not by what the current code happens to do.

**The cause of a non-zero exit is not classified.** No branch of this rule reads
`stderr`, matches on a message, or consults an exit code beyond
"did we obtain a trustworthy kind and bytes". Enumerating known failure
messages is the instance-by-instance repair this effort has spent six rounds
declining, and a resolver that classifies by diagnostic string inherits every
future change to git's wording as a silent semantic change.

`RESOLVED` carries `kind` because the resolver observed it. It does **not**
judge whether that kind is the one the citation required.


**What this criterion does and does not need.** "The bytes change, so
recomputing the object id reveals it" is not a mechanical fact — but the
property it needs is **second-preimage resistance for the cited oid**, stated in
§4.3.2, and not collision resistance. A substitution aimed at an oid that
already exists must produce different bytes with that same id, which is the
second-preimage problem.

The premise therefore bears on `RESOLVED` only. `IDENTITY_VIOLATION` needs
nothing here: it fires on an *observed* mismatch, and an observation does not
depend on the strength of the hash.

```text
verified about this repository and toolchain:
    object format          sha1        (git rev-parse --show-object-format)
    postcondition digest   Python hashlib.sha1 — PLAIN SHA-1, no collision
                           detection  (a1_v2_extract_graph.py:242)
    git's own backend      not established here; git may hash with a
                           collision-detecting variant, and if it does, the
                           postcondition is strictly weaker than the tool it
                           is checking
```

That asymmetry is recorded because it was found by checking rather than by
assuming, and because it is the kind of thing that reads as settled once nobody
writes it down. Whether the postcondition should hash the way git hashes belongs
to the implementation branch; this document contains no code.



### 4.3.2 What RESOLVED actually rests on: second preimage, not collision

A declared premise is weaker than this repository's admissibility rule requires,
which asks for an explicit **checked** one — and no cryptographic assumption is
checkable inside a repository. `git rev-parse --show-object-format` identifies
the algorithm; it establishes nothing about the algorithm's strength.

Two byte-derived states lean on **different properties of the same check**, and
only one of them needs a premise at all:

```text
SOUNDNESS      if the postcondition reports a mismatch, a mismatch is real.
               Needs NO premise: a detection is a positive observation.
               -> IDENTITY_VIOLATION rests on this

COMPLETENESS   if the postcondition reports no mismatch, there is none.
               Needs a premise about the hash.
               -> RESOLVED rests on this
```

**The premise was misnamed for several revisions, and naming it correctly
resolves the dilemma it created.** An earlier draft called it collision
resistance, then tried to discharge it by observing that §2.5.1's citation
scheme assumes the same thing. That is not a discharge — another scheme
depending on an assumption does not make the assumption true — and it left this
document recording both that the algorithm is SHA-1 and that SHA-1 has a known
attack, so a healthy present blob either failed witness 1 for want of an unmet
premise or passed by treating an unchecked premise as true.

`RESOLVED` does not need collision resistance. It needs the weaker property:

```text
SECOND-PREIMAGE RESISTANCE, scoped to the cited oid
    the attacker is given an EXISTING object and its id, and must produce
    DIFFERENT bytes with that same id

COLLISION RESISTANCE  (not what RESOLVED rests on)
    the attacker chooses BOTH byte sequences freely, in advance
```

The distinction is decisive for SHA-1 in particular, because the two properties
do not have the same status:

```text
collision resistance        BROKEN in practice
                            identical-prefix (2017), chosen-prefix (2020)

second-preimage resistance  no practical attack known against full SHA-1
```

A resolver is handed an oid that already exists and asked whether these bytes
are that object. Forging that answer requires a **second preimage** — the
property that still holds. Witness 1 is therefore satisfiable, and satisfiable
on the property that is not broken rather than by quietly assuming the one that
is.

**Where collision resistance bites, and why relocating it is not enough.** An
adversary who *authored* an artifact before it was pinned can prepare a colliding
pair, cause the locator to cite the shared oid while the benign blob is present,
and later swap in the malicious twin. Every §4.3 prerequisite holds, the lookup
exits zero, the object id matches — and no second preimage was ever needed,
because the collision was prepared before the citation existed.

An earlier revision answered this by assigning the attack to Phase G. That was
wrong as a *defence*: saying an attack belongs to another layer does not license
a claim at this one. What it does establish is which claim this layer may make.

**So `RESOLVED` is narrowed to what the resolver can observe:**

```text
RESOLVED CLAIMS
    these bytes were obtained locally, under every §4.3 requirement, and
    their object id equals the cited oid

RESOLVED DOES NOT CLAIM
    that these are the bytes the citer intended

    whether the cited oid denotes a unique artifact is a property of the
    CITATION, fixed when the oid was chosen. A resolver handed an oid
    cannot recover the intent behind it, and no amount of care at this
    boundary can.
```

That narrowing costs the consumer nothing it actually had. §8.5 asks whether
exact bytes for a cited oid can be obtained here without network access; it does
not ask the resolver to audit the citation's authorship, which the resolver has
no means to do.

The two hash properties then land where they belong, and neither is a premise
this layer can discharge:

```text
second-preimage resistance   makes RESOLVED useful for a citation already
                             trusted — no practical attack against SHA-1

collision resistance         determines whether the CITATION denotes
                             uniquely at all. Settled when the oid was
                             chosen, and SHA-1 does not provide it.
                             Inherited by every consumer of the pin,
                             including PINNED_BLOB, and not observable
                             from inside a resolver.
```

**What §2.5.1 says, and what this document infers, kept apart:**

```text
WHAT THE SOURCE SAYS
  docs/tasks/a1-f-v2-envelope.md §2.5.1, introduced 51b2d6a:
      blob:  <immutable git blob oid>     # the ONLY identity
  It does NOT mention any hash property, and says nothing about PINNED_BLOB.

WHAT THIS DOCUMENT INFERS
  identifying an existing artifact solely by its hash presupposes that a
  second preimage cannot be produced for it. That inference is this
  document's; it is not a ratification recorded anywhere else, and this is
  the first place the premise is written down.
```

```text
PREMISE for RESOLVED (named here, not ratified elsewhere, not
                      dischargeable inside a repository)
    SECOND-PREIMAGE resistance of the object-id algorithm for the cited oid
    -> SHA-1: no practical attack known

NOT the premise, deliberately
    collision resistance   -> SHA-1: broken in practice
    applies to AUTHORSHIP and PINNING (Phase G), where both sequences can
    be chosen in advance — not to a resolver handed an existing oid

  if the RESOLVED premise fails:  `blob` is not an identity for an existing
                                  object, and this resolver cannot be
                                  repaired into soundness
```

**The postcondition is the only verification in the path**, which is why its
soundness carries so much and why its completeness is worth exactly what the
hash is worth. Demonstrated, and not previously written down anywhere in this
effort:

```text
git cat-file DOES NOT VERIFY that the object stored at an oid's path hashes
to that oid. With A's object file overwritten by a valid object for B:

    cat-file blob <A>  ->  "SUBSTITUTED CONTENT ENTIRELY"
    git fsck           ->  error: hash-path mismatch
```

So the identity postcondition is not defence in depth over a check git already
performs; it is the check. The replacement guard remains a **guard** under
§4.3.1: its failure produces bytes that do not hash to the name, and the
postcondition detects them.

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

`IDENTITY_VIOLATION` preserves the existing family-level postcondition
(recompute the object id; whatever substitutes the bytes, the substitute does
not hash to the name). This state rests on the postcondition's SOUNDNESS and
carries no hash premise at all — §4.3.2.
This preregistration does not weaken it and does not propose to.

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
   this witness reachable at all.
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
