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

The gate is evaluated **per field**, and the outcome below is the summary of
those per-field results. Field-level evaluation is what §7 and §10 need; a
whole-object verdict cannot say which inputs survived.

```text
RETAIN          every field the projection would retain was successfully
                assessed, and none carries a blocking finding

BLOCK_SECRET    at least one assessed field carries a blocking finding

CANNOT_ASSESS   no blocking finding, and at least one field the projection
                would retain was not successfully assessed
```

### 5.1 Precedence, because the cases overlap

A detector can find a secret in `/body` and then die before reaching
`/user/login`. Both descriptions then apply, so the choice must be frozen rather
than left to whoever writes the adapter:

```text
any blocking finding                       -> BLOCK_SECRET
else any retained field unassessed         -> CANNOT_ASSESS
else                                       -> RETAIN
```

`BLOCK_SECRET` therefore does **not** imply that everything was inspected. So
coverage is recorded separately and always:

```text
coverageComplete   true only when every field the projection would retain
                   was successfully assessed
```

`BLOCK_SECRET` with `coverageComplete: false` is a normal, expressible state: a
secret was found and the rest was never looked at. Collapsing that into either
outcome alone would lose one of the two facts.

### 5.2 The denominator is normative, not declared

"Every field the projection would retain" is fixed by §5.3 below, derived from
the closed projections of provenance V1 §8. It is **not** whatever an
acquisition record claims it is.

This matters more than it looks. If the assessed set and the required set both
come from the same producer, coverage is self-certified:

```text
record:   "I assessed everything"
checker:  "well, if you say so"
```

A consumer verifying an assessment MUST compute the required set from §5.3 by
`sourceKind`. A record MAY also carry the set it believed was required, but only
as a declaration to be checked against §5.3 for exact equality — never as the
authority.

### 5.3 Required field set per source kind

JSON pointers into the **decoded source object**, so these are GitHub's field
names, not the canonical projection's:

```text
github-issue-comment
  /id  /user/id  /user/login  /user/type  /author_association
  /body  /created_at  /updated_at

github-submitted-review
  /id  /user/id  /user/login  /user/type  /author_association
  /state  /body  /submitted_at  /commit_id

github-review-comment
  /id  /pull_request_review_id
  /user/id  /user/login  /user/type  /author_association
  /body  /commit_id  /original_commit_id  /path
  /created_at  /updated_at
  present-only: /in_reply_to_id  /line  /original_line  /side  /start_line

github-pull-request-head
  /number  /head/sha  /head/ref  /head/repo/full_name
  present-only: /updated_at

github-actions-check
  /id  /name  /head_sha  /status
  present-only: /conclusion  /started_at  /completed_at
```

A **present-only** field joins the required set exactly when it is present in
the decoded source. Absent means nothing to assess; present means it is retained
and must therefore be assessed like any other.

`github-query-snapshot` is outside the gate. It is constructed rather than
fetched, and retains only enumeration facts and digests of objects that passed
the gate on their own.

Structurally constrained fields — ids, timestamps — are in the set on purpose.
Assessing them is cheap, and an exception list is how a coverage rule rots: the
first carve-out is always obviously safe, and it is never the last.

### 5.4 Why the three do not merge

`CANNOT_ASSESS` and `RETAIN` must never merge. "The detector found nothing" and
"the detector did not run" produce the same empty finding list and mean opposite
things. This is the project's oldest failure mode:

```text
failure -> empty set -> green
```

An assessment that did not successfully complete over the exact bytes is **not**
evidence that retention is safe.

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
PERMITTED   a separately typed reduced source record (§7),
            holding exactly the fields that individually passed
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
know whether the content is safe, and unknown is not safe. The reduced source
record of §7 is permitted on the same terms, and carries the `CANNOT_ASSESS`
outcome rather than pretending a finding exists.

The two outcomes are **not** merged into one "not retained" bucket. They record
different facts — one is a positive detection, the other an absent measurement —
and collapsing them destroys the distinction exactly as `NOT_PRODUCED` and
`RATE_LIMITED` would be destroyed by collapsing them into `OWED`.

## 7. The reduced source record

When a source does not pass the gate as a whole, the fields that **individually**
passed MAY still be retained — never as, and never mistakable for, the
projection §8 of provenance V1 defines.

An earlier revision called this a "blocked-source metadata record" and let the
producer nominate which fields were safe. Both were wrong. The name implied only
incidental metadata, while under the rule below a successfully-assessed clean
`/body` is retainable; and "safe" decided by the same party that wrote the record
is not a check, it is a claim.

```text
sourceKind           github-reduced-source-record
REQUIRED             schemaVersion  sourceKind
                     locatorKind          the provenance V1 kind that was refused
                     stableId
                     redactionPolicyVersion
                     outcome              BLOCK_SECRET | CANNOT_ASSESS
                     coverageComplete
                     retainedFields       object keyed by JSON pointer
                     blockedFields        array of JSON pointers
```

### 7.1 Which fields may be retained — computed, not nominated

Let `required` be the §5.3 set for `locatorKind`, `assessed` the fields the
detector successfully assessed, and `flagged` the fields named by blocking
findings. Then:

```text
blockedFields   = flagged  ∪  (required \ assessed)
retainedFields  = required \ blockedFields
```

Both are **determined**, not chosen. Consequences, stated so they cannot be
quietly avoided:

- A field named by a finding is blocked. Always.
- A field the detector never successfully assessed is blocked. Always — an
  unassessed field is not "probably a timestamp", it is unexamined.
- `retainedFields` and `blockedFields` **exhaustively partition** `required`:
  every field appears in exactly one, and nothing appears in neither.
- Retention is not discretionary in the other direction either. A field that
  survives the computation is retained, so the record cannot be thinned by
  judgement after the fact.
- Keys are full JSON pointers. `/user/login` is a nested field and comparing it
  by a trimmed leaf name is not the same comparison.

### 7.2 What the record is, and is not

- Its `sourceKind` is **distinct** from every provenance V1 §8 kind and MUST NOT
  reuse the refused kind or that kind's `schemaVersion`. A partial record wearing
  a complete record's identity is how a projection silently becomes weaker than
  its contract.
- `blockedFields` MUST be non-empty. A record that blocked nothing is not a
  reduced record — it is a complete projection, and should be one.
- It is canonicalized, digested and retained like any other object, and bound to
  its authorising assessment per §9.2. It is real evidence of a reduced
  observation, not a placeholder.
- **It satisfies §11 of provenance V1 only for facts derived solely from the
  fields it actually contains.** A decision that read a blocked field is not
  rescued by it.

That last rule is the load-bearing one. A wrong-SHA `OWED` derives from
`review.commitId` and the subject head; if the review *body* is blocked while
`/commit_id` was assessed clean and retained, that decision remains fully
explainable from retained immutable bytes.

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

Facts derived **solely** from fields that survived §7.1 are unaffected. The test
is mechanical and must be **computed, never declared**:

```text
for each input pointer of the derived fact
    resolve it against the retained records for that locator
        complete projection, or reduced source record
    absent  ->  the fact is inadmissible
```

A record that merely asserts `everyInputFieldRetained: true` proves nothing: the
same party wrote the assertion and the record it describes. A consumer resolves
the pointers itself, and treats such a field as a claim to be checked rather than
an answer to be trusted.

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
                     assessedFields        fields SUCCESSFULLY assessed
                     coverageComplete
                     outcome               RETAIN | BLOCK_SECRET | CANNOT_ASSESS
                     observedAt
CONDITIONAL          findings              REQUIRED, non-empty, on BLOCK_SECRET
                     reasonCode            REQUIRED on CANNOT_ASSESS
                     reasonDetail          OPTIONAL, and constrained by §9.3
```

- `representation` names the form the detector actually saw. V1 defines exactly
  one legal value, `decoded-source-field-values`, matching §4. It is a field
  rather than an assumption so a future representation cannot arrive silently.
- `assessedFields` means **successfully assessed**: the detector produced a
  result for that field. A field it started and abandoned is not assessed, and
  listing it would convert a crash into coverage.
- `findings` carry an opaque finding identifier and the field pointer, and every
  field they name must appear in `blockedFields` per §7.1.
- A `RETAIN` outcome MUST NOT appear without `coverageComplete: true`.

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

### 9.2 The assessment is retained evidence, and it is bound

An assessment that authorises permanent retention and then evaporates is worse
than no record: a month later one can prove the bytes were kept and not which
decision permitted keeping them.

```text
RetentionAssessment
        ↓ canonicalize (provenance V1 §7)
        ↓ SHA-256
   assessmentDigest      and the canonical bytes are RETAINED
```

Binding is a **separate retained object**, because provenance V1 §8 projections
are closed and adding a field to them retroactively is exactly the widening §8
forbids:

```text
RetentionBinding
REQUIRED   schemaVersion
           recordDigest       the retained projection or reduced record
           assessmentDigest   the assessment that authorised it
```

Every retained record produced through this gate — complete projection or reduced
record — MUST have a `RetentionBinding`. A retained record with no reachable
authorising assessment is inadmissible: it is bytes somebody kept, not evidence
somebody was permitted to keep.

### 9.3 The assessment must be safe by construction

The assessment record is retained forever. It must therefore not become the
place the secret lives.

```text
FORBIDDEN in any field of the assessment
    the matched substring, an excerpt, a prefix or suffix,
    a length, a character count, a digest of the matched bytes,
    raw detector stdout, stderr, or an exception message,
    any quoted fragment of an assessed field value
```

Prohibiting this in `findings` alone is not enough. People are remarkably good at
moving a leak one field sideways, and the obvious sideways field is the free-text
failure reason:

```text
reason: "parser failed near \"<the secret>\""
```

So `CANNOT_ASSESS` carries a **closed** `reasonCode`:

```text
DETECTOR_UNAVAILABLE   the detector could not be invoked at all
DETECTOR_FAILED        it was invoked and did not produce a usable result
INCOMPLETE_COVERAGE    it produced results for only part of the required set
INVALID_RESULT         it produced a result that failed validation
```

`reasonDetail` is optional, and when present MUST be producer-authored text —
never a pass-through of detector output. The mechanical form of that rule: no
string anywhere in an assessment may contain a run of 8 or more consecutive
characters taken from any assessed field value.

## 10. Consequence for closure state — via the decision basis

An earlier revision of this section drew the arrow

```text
acquisition = AVAILABLE  +  gate = BLOCK_SECRET   ->  CANNOT_CHECK
```

and that contradicted §7 and its own specimen R7. §7 says a wrong-SHA decision
resting on a retained `/commit_id` stays fully explainable when the body is
blocked; the arrow above says the observation is unexplainable regardless. Both
cannot be true.

The arrow was wrong, and instructively so. Refusing to write `acquisition =
FAILED` for a successful fetch was correct; asserting `CANNOT_CHECK` for a
decision whose every input survives is the same error one step later — reporting
a loss of evidence that did not occur.

**The gate does not determine the state. It determines which inputs remain
provenanced, and the state follows from that, per decision.**

```text
gate outcome
      ↓
admissible decision basis     which fields still resolve to retained bytes
      ↓
per-decision evaluation       does THIS decision's every input resolve?
      ↓
state
```

The rule:

```text
every input of this decision resolves to a retained record
    -> the frozen classifier semantics apply, unchanged
    -> a wrong-SHA review may still be OWED, a check may still be PASS

any input of this decision does not resolve
    -> that observation is CANNOT_CHECK
```

So a blocked body does not sweep an entire observation aside. It removes the
fields it blocked, and each decision is then evaluated against what is left.
R6 and R7 are the witness pair: identical gate outcome, identical blocked field,
opposite admissibility, because one decision read the body and the other read
only `/commit_id`.

### 10.1 The three axes

Retention admissibility is still a third axis, orthogonal to acquisition — that
part of the earlier revision survives. What changes is that it is **per field**,
and the state is computed from the axes rather than read off one of them.

```text
acquisition     what happened when we tried to obtain the object
retention       per field, whether it may become retained evidence
state           derived from both, per decision, never replacing either
```

```text
acquisition = AVAILABLE  gate = RETAIN                     normal semantics
acquisition = AVAILABLE  gate = BLOCK_SECRET, inputs kept   normal semantics
acquisition = AVAILABLE  gate = BLOCK_SECRET, input blocked CANNOT_CHECK
acquisition = AVAILABLE  gate = CANNOT_ASSESS, input unkept CANNOT_CHECK
acquisition = FAILED                                        CANNOT_CHECK (§15)
```

Several rows share a headline and are different facts. Per #147's headline rule,
the derived presentation must never destroy the per-observation vector, so all of
them remain separately recorded.

### 10.2 What the classifier still lacks

The classifier can express none of this today: no per-field retention, no notion
of a decision basis whose inputs may be individually missing. That is a
requirement handed to the classifier provenance binding slice, recorded in §11
with what it blocks. No classifier change is made here.

## 11. Residuals — OWED, each naming what it blocks

- **Detector implementation binding.** `detector.id` + `detector.version` +
  `detector.configDigest` must resolve to exactly one detector behaviour, and
  this document does not say how — a registry, a pinned artifact, a digest over
  the implementation. *Blocks the first consumer that runs a detector, i.e. the
  acquisition adapter.* This is deliberately the same shape as provenance V1
  §13.1's matcher binding, and for the same reason: a named algorithm without
  immutable resolution is a locator pointing at mutable semantics.
- **Classifier retention axis.** The classifier can express none of §10: no
  per-field retention, and no decision basis whose inputs may be individually
  missing. *Blocks the classifier provenance binding slice*, which must add both;
  without them an adapter would have to forge `FAILED` to express a blocked
  retention, misreporting a fetch that succeeded, or discard an observation whose
  decision inputs all survived.
- **Where retention bindings live.** §9.2 requires a `RetentionBinding` per
  retained record and does not say where the set of them is carried. *Blocks the
  classifier provenance binding slice*, alongside the axis above — the two land
  in the same schema.
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
| Which wins when a secret is found *and* the run dies? | §5.1 — `BLOCK_SECRET` |
| Does `BLOCK_SECRET` imply everything was inspected? | §5.1 — no; `coverageComplete` |
| Who decides what "every retained field" means? | §5.2, §5.3 — the contract, not the record |
| Can "detector failed" become "no secret"? | §5.4 — no |
| Which fields may a reduced record retain? | §7.1 — computed, not nominated |
| Is an unassessed field retainable? | §7.1 — no |
| Do retained and blocked cover every field? | §7.1 — exhaustive partition |
| Can that record satisfy §11 for a blocked-field fact? | §7.2 — no |
| How is a derived fact's admissibility established? | §8 — pointers resolved, not asserted |
| What must a detector record about itself? | §9 |
| Is the assessment itself retained? | §9.2 — yes, digested and bound |
| What binds a retained record to its assessment? | §9.2 `RetentionBinding` |
| Can the failure reason leak the secret? | §9.3 — closed `reasonCode` |
| What does "no blocking finding" mean? | §9.1 |
| Does the gate outcome determine the state? | §10 — no; the decision basis does |
| May a blocked body leave a decision evaluable? | §10 — yes, if its inputs survive |
| Is the gate part of the acquisition status? | §10.1 — no, a third axis |
| What is still OWED, and what does it block? | §11 |
| Does this unblock acquisition? | §12 — no |

## 14. Correction round 1

Seven defects found by independent review of the first draft and its specimens.
Recorded here rather than folded silently into the text above, because which
version of a contract was believed when is itself evidence.

1. **The coverage denominator was self-certified.** §5.2 and §5.3 move it into
   the contract. The first draft let a record declare what "every retained field"
   meant, and its own positive control R1 then declared a set narrower than the
   projection it retained — so the load-bearing rule passed without being tested.
2. **The assessment was declared evidence and never made durable.** §9.2 gives it
   canonical bytes, a digest, retention, and a `RetentionBinding` to the record it
   authorised — as a separate object, since provenance V1 §8 is closed.
3. **§10 contradicted §7 and specimen R7.** The gate does not determine the
   state; it determines which inputs remain provenanced, and each decision is
   evaluated against what survives. Refusing to write `acquisition = FAILED` for a
   successful fetch was right; asserting `CANNOT_CHECK` for a decision whose every
   input survived was the same error one step later.
4. **The reduced record nominated its own safe fields.** §7.1 computes them:
   findings and unassessed fields are blocked, the rest is retained, and the two
   exhaustively partition the required set. The record was also renamed from
   "blocked-source metadata" — under the per-field rule a clean `/body` is
   retainable, so "metadata" described it wrongly.
5. **Derived-fact admissibility was asserted rather than resolved.** §8 requires
   the pointers to be resolved against retained records.
6. **The failure reason was an unguarded exfiltration channel.** §9.3 closes it
   with a `reasonCode` vocabulary and a mechanical constraint on any detail text.
   Forbidding a quoted secret in `findings` only moves it one field sideways.
7. **Outcome precedence was undefined.** §5.1 freezes it, and `coverageComplete`
   keeps the second fact rather than losing it to the first.
