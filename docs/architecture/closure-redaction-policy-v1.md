# Closure redaction and secret-retention policy V1

**Status: proposed normative contract. NOT an implementation.**

This document decides what a future acquisition layer must do when a
closure-relevant GitHub source contains a credential, a secret, or anything else
that must not be turned into durable content-addressed evidence.

It **depends on** `docs/architecture/closure-source-provenance-v1.md` (merged in
PR #152, on `main` as `4d60db64…`) and does not modify it. That contract is the
input to this one. Where the two are read together, provenance V1 remains
authoritative for everything it already decides; this document adds a gate in
front of retention and says what happens on each of its outcomes.

## 1. The conflict this exists to resolve

Three already-frozen rules point in different directions, deliberately:

```text
§9   a source body is retained byte-exact, with no normalization
§11  a digest is not provenance unless the bytes are retained
§20  a pasted credential must not be quietly made permanent,
     and mask(body) -> digest is refused because the digest would
     then bind something never observed
```

Taken together they leave exactly one unanswered case: a body that §9 and §11
require to be retained, and §20 forbids retaining. Provenance V1 named the
boundary and explicitly declined to solve it. This document solves it.

The anti-pattern the contract must forbid by name:

```text
original body containing a secret
        ↓
    mask(secret)
        ↓
  digest(masked body)
        ↓
presented as provenance of what GitHub returned
```

That is not redaction of evidence. It is evidence of something that never
existed. **A security problem must not be solved by falsifying the record.** The
honest alternative is to retain less and say so, which is what follows.

## 2. Scope

**In scope.** The retention gate and its outcomes; where the gate sits relative
to decoding, projection, canonicalization and retention; what may and may not be
retained on each outcome; detector provenance; the status of facts derived from
content that cannot be retained; the consequence for an observation's closure
state, derived from the frozen vocabulary rather than asserted.

**Not in scope, and forbidden in the same change.** Any secret-scanner
implementation; the selection of any specific scanner; any GitHub API client,
acquisition adapter, pagination or matcher implementation; encryption at rest;
any key or secret store; any new dependency; any network access. This contract
states requirements a future detector must satisfy **without choosing one**.

## 3. Why this slice precedes acquisition

From provenance V1 §23: the redaction decision blocks the acquisition adapter,
because §9 retains bodies byte-exact and §11 retains the bytes. An adapter built
first would implement careful immutable storage for whatever somebody pasted
into a comment, and content addressing makes that hard to take back.

This document does **not** unblock acquisition on its own. Matcher
implementation binding (provenance V1 §13.1) remains an unmet precondition. §12
below restates the sequence.

## 4. The gate, and where it sits

```text
GitHub semantic response
        ↓
exact JSON-decoded values of the closure-relevant source fields
        ↓
RETENTION ASSESSMENT over exactly those decoded values      <-- the gate
        ↓
   ┌────────────────────────┴────────────────────────┐
   │ RETAIN                                          │ BLOCK_SECRET
   │                                                 │ CANNOT_ASSESS
   ↓                                                 ↓
closed projection (§8)                          no normal source snapshot
   ↓                                             (§7 and §8)
JCS + SHA-256 (§7)
   ↓
immutable retention (§11)
```

Two orderings are forbidden, and they fail in opposite directions:

```text
retain first, assess later      the secret is already durable and
                                content-addressed before anyone looks

normalize/mask first, then      whatever is assessed and retained is no
assess                          longer what GitHub returned, and §9 is void
```

The gate therefore sits **after** exact JSON decoding and **before** any
projection, canonicalization or retention. Between the decoded value and the
assessment there MUST be no `trim`, no newline normalization, no Unicode
normalization, no case folding, no Markdown or HTML rendering, no truncation —
the same list §7 and §9 already forbid on the retention path, applied one step
earlier.

**The assessed representation is the decoded source field values, and the
assessment record must say so.** JSON escaping is transport (§7): a `"\n"`
escape and a literal newline are the same value, and the assessment sees the
value.

## 5. Gate outcomes

Exactly three, and they are not collapsible:

```text
RETAIN          the detector successfully assessed every field that would be
                retained, and emitted no blocking finding under its policy

BLOCK_SECRET    the detector successfully assessed and emitted at least one
                blocking finding

CANNOT_ASSESS   the detector did not successfully assess every field that
                would be retained — it failed, was unavailable, errored, or
                saw only part of the input
```

**`CANNOT_ASSESS` and `RETAIN` must never merge.** "The detector found nothing"
and "the detector did not run" produce the same empty finding list and mean
opposite things. This is the project's oldest failure mode:

```text
failure -> empty set -> green
```

An assessment that did not successfully complete over the exact bytes is **not**
evidence that retention is safe. Absence of a finding is only meaningful as the
result of a completed assessment.

`RETAIN` additionally requires **coverage**: the assessed field set must include
every field the projection would retain. A detector that read `/body` while the
projection also retains `/user/login` assessed part of the input, and partial
assessment is `CANNOT_ASSESS`, not `RETAIN`.

## 6. What each outcome permits

### 6.1 RETAIN

Normal provenance V1 behaviour, unchanged: closed projection (§8), JCS +
SHA-256 (§7), retained bytes (§11).

### 6.2 BLOCK_SECRET

```text
FORBIDDEN   a normal source snapshot of any §8 sourceKind
FORBIDDEN   digest(original bytes) with the bytes discarded
FORBIDDEN   mask(original) and any digest over the masked form
FORBIDDEN   any semantic fact derived from the blocked field values
PERMITTED   a separately typed blocked-source metadata record (§7)
```

Each prohibition, with its reason:

- **A normal source snapshot** would require the bytes, which is the thing being
  refused.
- **`digest(original) + discard bytes`** is refused by provenance V1 §5 and §11:
  a digest whose bytes were discarded is a fingerprint with no evidence
  attached. It is additionally refused here on its own merits — a digest of a
  low-entropy secret is an offline guessing target, so the "harmless" residue is
  not harmless.
- **`mask(original)` and any digest over it** is refused by §20 and by §1 above.
  A masked digest authenticates a string that was never observed.
- **Derived semantic facts** are covered in §8.

V1 defines **no redacted representation** of blocked content. Nothing on the
acquisition path needs one, and defining one invites exactly the confusion §1
forbids. If a later slice wants an operational redacted artifact, it is a
separate `sourceKind` under its own contract, and it is never a decision basis
input. Recorded as a residual in §11, not smuggled in here.

### 6.3 CANNOT_ASSESS

Same prohibitions as `BLOCK_SECRET`, for a different reason: the system does not
know whether the content is safe, and unknown is not safe. The blocked-source
metadata record of §7 is permitted on the same terms, and carries the
`CANNOT_ASSESS` outcome rather than pretending a finding exists.

The two outcomes are **not** merged into one "not retained" bucket. They record
different facts — one is a positive detection, the other an absent measurement —
and collapsing them destroys the distinction exactly as `NOT_PRODUCED` and
`RATE_LIMITED` would be destroyed by collapsing them into `OWED`.

## 7. The blocked-source metadata record

When a source is blocked, fields that are themselves safe MAY still be retained
— but never as, and never mistakable for, the projection §8 defines.

```text
sourceKind           github-blocked-source-metadata
REQUIRED             schemaVersion  sourceKind
                     locatorKind          the §8 sourceKind that was refused
                     stableId
                     redactionPolicyVersion
                     outcome              BLOCK_SECRET | CANNOT_ASSESS
                     retainedFields       object: the safe field values kept
                     blockedFields        array: JSON pointers NOT retained
OPTIONAL-IF-PRESENT  (none)
```

Rules:

- Its `sourceKind` is **distinct** from every §8 kind. It MUST NOT reuse the
  refused kind or that kind's `schemaVersion`. A partial record wearing a
  complete record's identity is how a projection silently becomes weaker than
  its contract.
- `blockedFields` MUST be non-empty. A record that blocked nothing is not a
  blocked-source record.
- `retainedFields` MUST NOT contain any field named in `blockedFields`, in whole
  or in part — no excerpt, no prefix, no length, no digest of it.
- It is canonicalized and digested like any other retained object (§7 of
  provenance V1) and its bytes are retained. It is real evidence of a reduced
  observation, not a placeholder.
- **It satisfies §11 only for facts derived solely from the fields it actually
  contains.** A decision that depended on a blocked field is not rescued by it.

That last rule is the useful one. A wrong-SHA `OWED` derives from
`review.commitId` and the subject head; if the review *body* is blocked but
`commitId` is retained, that decision remains fully explainable from retained
immutable bytes and stays admissible. A decision that read the body does not.

## 8. Derived facts over blocked content

Provenance V1 §18 requires every derived fact influencing the classifier to list
the source snapshot digests it was derived from. That rule decides this case on
its own:

```text
a semantic fact derived from a blocked field
        ↓
must name the source snapshot digest it came from
        ↓
that snapshot may not legally exist
        ↓
the fact cannot meet §18 and MUST NOT be emitted
```

So a body reading *"I reproduced defect X; token=…"* yields **no** retained
`claim = reproduced` when the body is blocked. The claim is not "probably fine
because we saw it once". Emitting it would create the precise shape this project
exists to prevent:

```text
derived decision  +  deliberately destroyed source  =  provenance hole
```

Facts derived **solely** from retained safe metadata are unaffected, and §7
already states the test: every input field of the derived fact must appear in a
retained record.

## 9. Detector provenance

"Safe to retain" is itself an observation and must be as auditable as any other,
or it becomes ambient magic that no later reader can question.

```text
RetentionAssessment
REQUIRED             schemaVersion
                     redactionPolicyVersion
                     detector.id
                     detector.version
                     detector.configDigest
                     representation        what was assessed
                     assessedFields        JSON pointers into the decoded source
                     outcome               RETAIN | BLOCK_SECRET | CANNOT_ASSESS
                     observedAt
CONDITIONAL          findings              REQUIRED, non-empty, on BLOCK_SECRET
                     reason                REQUIRED on CANNOT_ASSESS
```

- `representation` names the form the detector actually saw. V1 defines exactly
  one legal value, `decoded-source-field-values`, matching §4. It exists as a
  field rather than as an assumption so that a future representation cannot be
  introduced silently.
- `findings` carry an opaque finding identifier and the field pointer. They MUST
  NOT carry the matched substring, an excerpt, a prefix, a length, or a digest
  of the matched bytes. A findings list that quotes the secret is a leak wearing
  a security record's clothing.
- `reason` on `CANNOT_ASSESS` MUST be non-empty. An unexplained inability to
  check is indistinguishable from not having tried.
- A `RETAIN` outcome MUST NOT appear without a completed assessment, and MUST
  NOT appear when `assessedFields` omits a field the projection retains.

### 9.1 What "no blocking finding" does and does not mean

```text
WRONG   no secret was detected
        ==
        this content contains no secret

RIGHT   detector D, version V, configuration C successfully inspected
        representation R of fields F and emitted no blocking finding
        under policy P
```

The second is a bounded observation about a named tool. The first is a claim
about the world that no scanner can support. The contract records the second,
and the required fields exist so that the record cannot be mistaken for the
first.

## 10. Consequence for closure state — derived, not decreed

The tempting shorthand is `SECRET_DETECTED -> CANNOT_CHECK`. Checked against the
frozen vocabulary, that is **right about the state and wrong about the record**,
so it is not adopted as written.

Derivation, using only #147's vocabulary and provenance V1:

```text
the producer produced the object          -> not NOT_PRODUCED, so not OWED
                                             on the producer axis (§15)
the fetch succeeded; we hold the bytes    -> acquisition is AVAILABLE
                                             recording FAILED would misreport
                                             what happened
the body cannot be retained               -> §3: any state that depended on it
                                             is unexplainable from the artifact,
                                             so not PASS and not FINDING
nothing here concerns head movement       -> not STALE
                                          => CANNOT_CHECK
```

So the **state** is `CANNOT_CHECK`. But the naive arrow collapses the outcome
into the acquisition axis, which would destroy a true observation: the fetch
succeeded. Retention admissibility is therefore a **third axis**, orthogonal to
acquisition status:

```text
acquisition     what happened when we tried to obtain the object
retention gate  whether what we obtained may become retained evidence
state           derived from both, never replacing either
```

```text
acquisition = AVAILABLE  +  gate = BLOCK_SECRET     -> CANNOT_CHECK
acquisition = AVAILABLE  +  gate = CANNOT_ASSESS    -> CANNOT_CHECK
acquisition = FAILED                                 -> CANNOT_CHECK (§15)
                                                        gate never reached
```

All three yield the same headline and are three different facts. Per #147's
headline rule, the derived presentation must never destroy the per-observation
vector, so all three remain separately recorded.

**The classifier has no such axis today.** This is a requirement handed to the
classifier provenance binding slice, recorded in §11 with what it blocks. No
classifier change is made here.

## 11. Residuals — OWED, each naming what it blocks

- **Detector implementation binding.** `detector.id` + `detector.version` +
  `detector.configDigest` must resolve to exactly one detector behaviour, and
  this document does not say how — a registry, a pinned artifact, a digest over
  the implementation. *Blocks the first consumer that runs a detector, i.e. the
  acquisition adapter.* This is deliberately the same shape as provenance V1
  §13.1's matcher binding, and for the same reason: a named algorithm without
  immutable resolution is a locator pointing at mutable semantics.
- **Classifier retention axis.** The classifier cannot currently express
  `acquisition = AVAILABLE, gate = BLOCK_SECRET`. *Blocks the classifier
  provenance binding slice*, which must add it; without it an adapter would have
  to forge `FAILED` to express a blocked retention, misreporting a fetch that
  succeeded.
- **Operational redacted artifact.** Not defined in V1 (§6.2). *Blocks nothing.*
  If ever added it is a separate `sourceKind` and never a decision basis input.
- **Re-assessment on policy change.** What happens to already-retained snapshots
  when the detector or its configuration changes — nothing is retroactively
  re-gated by this document. *Blocks nothing on the current path*; it becomes
  live the first time a detector version is bumped against an existing store.
- **Secret store / encryption at rest.** V1 retains nothing sensitive, so no key
  management is required. *Blocks nothing*, and is listed only to record that
  "put it in a vault" was considered and rejected as larger than the problem.

## 12. Sequence after this slice

Unchanged from provenance V1 §21 except that this box is now filled:

```text
provenance contract freeze                      merged, 4d60db64…
        ↓
redaction decision                              THIS SLICE
        ↓
matcher implementation binding (§13.1)          still an unmet precondition
        ↓
classifier provenance binding                   + the retention axis of §10
        ↓
acquisition adapter                             emits Claimed, never Reproduced
        ↓
attestation envelope

verification-witness binding                    gates any producer of Reproduced
```

Freezing this document does **not** authorise starting the acquisition adapter.
Two preconditions remain: matcher implementation binding, and the classifier
retention axis.

## 13. Acceptance criteria

| question | section |
|---|---|
| Where does the safety gate sit? | §4 — after decoding, before projection |
| What exactly does the detector assess? | §4, §9 `representation` |
| May anything be normalized before assessment? | §4 — no |
| What are the gate outcomes? | §5 |
| Can "detector failed" become "no secret"? | §5 — no |
| What if only some fields were assessed? | §5 — `CANNOT_ASSESS` |
| May `digest(original)` survive without bytes? | §6.2 — no |
| May `mask` then `digest` be provenance? | §6.2, §1 — no |
| May any metadata survive a block? | §7 — yes, separately typed |
| Can that metadata satisfy §11 for a body-derived fact? | §7 — no |
| May a claim extracted from a blocked body be emitted? | §8 — no |
| What must a detector record about itself? | §9 |
| What does "no blocking finding" mean? | §9.1 |
| What closure state results? | §10 — `CANNOT_CHECK`, derived |
| Is the gate part of the acquisition status? | §10 — no, a third axis |
| What is still OWED, and what does it block? | §11 |
| Does this unblock acquisition? | §12 — no |
