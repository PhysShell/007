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
names, not the canonical projection's. Each kind has an **always** set and a
**present-only** set:

```text
github-issue-comment
  always        /id  /user/id  /user/login  /user/type
                /author_association  /body  /created_at  /updated_at
  present-only  (none)

github-submitted-review
  always        /id  /user/id  /user/login  /user/type
                /author_association  /state  /body  /submitted_at  /commit_id
  present-only  (none)

github-review-comment
  always        /id  /pull_request_review_id
                /user/id  /user/login  /user/type  /author_association
                /body  /commit_id  /original_commit_id  /path
                /created_at  /updated_at
  present-only  /in_reply_to_id  /line  /original_line  /side  /start_line

github-pull-request-head
  always        /number  /head/sha  /head/ref  /head/repo/full_name
  present-only  /updated_at

github-actions-check
  always        /id  /name  /head_sha  /status
  present-only  /conclusion  /started_at  /completed_at
```

A **present-only** field joins the required set exactly when it is present in
the decoded source. Absent means nothing to assess; present means it is retained
and must therefore be assessed like any other. Per provenance V1 §8, `null` and
absent are the same input, so a `null` present-only field is absent here too.

`github-query-snapshot` is outside the gate. It is constructed rather than
fetched, and retains only enumeration facts and digests of objects that passed
the gate on their own.

Structurally constrained fields — ids, timestamps — are in the set on purpose.
Assessing them is cheap, and an exception list is how a coverage rule rots: the
first carve-out is always obviously safe, and it is never the last.

### 5.4 Coverage failures name themselves

Whenever coverage is incomplete, the record says why — and it says so
**independently of the gate outcome**:

```text
coverageComplete: false   ->   coverageFailureCode REQUIRED
```

An earlier revision required a reason only on `CANNOT_ASSESS`, which left
`BLOCK_SECRET` with `coverageComplete: false` recording *that* the assessment
was partial and never *why*. The vocabulary is closed and is the same one §9.3
uses:

```text
DETECTOR_UNAVAILABLE  DETECTOR_FAILED  INCOMPLETE_COVERAGE  INVALID_RESULT
```

### 5.5 Set-like fields are ordered arrays

`assessedFields`, `blockedFields` and `findings` are logically sets and
physically JSON arrays. Provenance V1 §13.2 already had to settle this once for
query snapshots, and the reason is unchanged: JCS orders object keys and does
**not** sort arrays, so order is inside the digest whether or not anyone chose
it.

```text
assessedFields   unique, ascending lexical JSON-pointer order
blockedFields    unique, ascending lexical JSON-pointer order
findings         unique on (field, findingId),
                 ascending lexical order by (field, findingId)
```

Without this, `["/body", "/id"]` and `["/id", "/body"]` are one fact and two
digests, and `["/body", "/body", "/id"]` is a third. Detector emission order is
deliberately **not** preserved: it is not evidence this contract uses, and
keeping it would make the digest depend on scheduling.

### 5.6 Why the three do not merge

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
                     locator              §7.3, identity only
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

### 7.2 What each retained pointer holds

Choosing the right key set and then storing the wrong bytes under it is a
provenance failure that looks like a success, so the value is frozen too:

```text
retainedFields[p]  =  exactly the value the COMPLETE §8 projection
                      would have carried for p
```

This is deliberately defined by reference rather than as a second mapping. The
reduced record is a canonical object under provenance V1 §7 and obeys its rules
— numeric ids serialized as strings, string values taken exactly as decoded,
no trim, no normalization. So a reduced record is a projection of a **subset** of
the fields, not a differently-shaped artifact that happens to share a vocabulary.

The practical consequence, and the reason it needed saying: the frozen Step 0B
fixtures store `"id": 4944100001` as a JSON number because that is the **raw API**
shape, while a canonical projection carries `"9100000201"` as a string. Both were
defensible readings of the earlier text. Only one is now legal.

A consumer verifies these values against the decoded source it holds, for
**every** retained pointer. Checking only `/body` leaves the rest as a place
where a correct pointer can name incorrect bytes.

### 7.3 The locator is identity, not surviving evidence

The record must remain findable even when nothing survived — R4 retains no field
at all and still has to say which object was refused. That identity is carried in
a `locator`, and provenance V1 §4 already separates locator from immutable source
snapshot, so this is that distinction applied one level down.

```text
github-issue-comment      repository  pullRequest  stableId
github-submitted-review   repository  pullRequest  stableId
github-review-comment     repository  pullRequest  stableId
github-actions-check      repository  stableId
github-pull-request-head  repository  pullRequest
```

`github-pull-request-head` has no `stableId` on purpose: provenance V1 §8.1
identifies the subject read by repository and pull request number, and inventing
a synthetic id for it here would contradict the merged schema. An earlier
revision required `stableId` for every kind and only avoided the contradiction
because no head specimen existed.

**The normative rules:**

```text
a locator value is NOT surviving source evidence
and MUST NOT satisfy a decision-basis pointer

locatorKind MUST equal the source kind that was gated

locator MUST equal the acquisition locator of that source
```

The second and third exist because shape alone is not identity. A record whose
locator has the right keys and the wrong values, or the right values under the
wrong `locatorKind`, is a well-formed pointer at the wrong object — which is
worse than a missing one, because it resolves.

Without it, `/id` can appear in `blockedFields` while the same source-derived id
sits permanently in the record as `stableId` — the field-retention gate bypassed
by an alias. A locator is what lets you go and look again; it is never what you
show instead of having looked.

### 7.4 What the record is, and is not

- Its `sourceKind` is **distinct** from every provenance V1 §8 kind and MUST NOT
  reuse the refused kind or that kind's `schemaVersion`. A partial record wearing
  a complete record's identity is how a projection silently becomes weaker than
  its contract.
- `blockedFields` MUST be non-empty. A record that blocked nothing is not a
  reduced record — it is a complete projection, and should be one.
- It is canonicalized, digested and retained like any other object, and bound to
  its authorising assessment per §9.2.
- **It satisfies §11 of provenance V1 only for facts derived solely from the
  fields it actually contains in `retainedFields`.** A decision that read a
  blocked field is not rescued by it, and neither is one that reads the locator.

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

Resolution reaches `retainedFields` **only**. Per §7.3 the locator is identity,
not surviving evidence, so a decision basis naming `/id` is not satisfied by the
record's `stableId` even though the two describe the same number.

## 9. Detector provenance

"Safe to retain" is itself an observation and must be as auditable as any other,
or it becomes ambient magic that no later reader can question.

**The assessment schema is closed, at every level.** Exactly the fields below,
no others, and the same for every nested object — §9.5. A producer that adds one
is non-conformant, not "extended", and §9.4 explains why that closure is the
whole security argument rather than a tidiness rule.

It carries its own `sourceKind` because provenance V1 §7 requires every canonical
object to be domain-separated by its own content, and §9.2 sends this object
through exactly that canonicalization. An earlier revision omitted it, which put
this contract in direct conflict with the merged one.

```text
RetentionAssessment
REQUIRED             schemaVersion
                     sourceKind            closure-retention-assessment
                     redactionPolicyVersion
                     detector.id
                     detector.version
                     detector.configDigest
                     representation        decoded-source-field-values
                     assessedFields        fields SUCCESSFULLY assessed, §5.5 order
                     coverageComplete
                     outcome               RETAIN | BLOCK_SECRET | CANNOT_ASSESS
                     observedAt
CONDITIONAL          findings              present IFF outcome is BLOCK_SECRET
                     coverageFailureCode   present IFF coverageComplete is false
```

The conditionals are **iff**, not "at least". `findings: []` and an absent
`findings` would be two encodings of one fact, and provenance V1 §8 already
settled that argument for `null` versus absent: one fact, one canonical form, or
the digest stops identifying content.

- `representation` names the form the detector actually saw. V1 defines exactly
  one legal value, matching §4. It is a field rather than an assumption so a
  future representation cannot arrive silently.
- `assessedFields` means **successfully assessed**: the detector produced a
  result for that field. A field it started and abandoned is not assessed, and
  listing it would convert a crash into coverage.
- `findings` carry `field` and `findingId`. Every field a finding names MUST be
  in the §5.3 required set **and** in `assessedFields`, and it then appears in
  `blockedFields` per §7.1.

  The first two are not pedantry about well-formedness. A detector can only find
  something in a field it actually looked at, so a finding naming an unassessed
  or non-required pointer is malformed output — and accepting it is a way to
  block nothing while reporting `BLOCK_SECRET`:

```text
finding.field = /made-up-field    not in the required set, so blocked nothing
/body          assessed, unflagged, therefore RETAINED
outcome        BLOCK_SECRET, because findings is non-empty
```

  Every other rule still holds in that record: the partition is exhaustive, the
  locator matches, coverage is honest, the rule id is configured. The secret is
  retained anyway. A checker must not scan bodies — that is §9.1's boundary — but
  it must reject a finding that could not have been produced.
- **`findingId` is a rule identifier from the bound detector configuration** —
  one of the rule ids covered by `detector.configDigest`. It is not an arbitrary
  opaque string. That is what makes it a closed value in the sense of §9.4: its
  value set is fixed before anything is inspected, so it cannot depend on what
  was found.
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
           sourceKind         closure-retention-binding
           recordDigest       the retained projection or reduced record
           assessmentDigest   the assessment that authorised it
```

Every retained record produced through this gate — complete projection or reduced
record — MUST have a `RetentionBinding`. A retained record with no reachable
authorising assessment is inadmissible: it is bytes somebody kept, not evidence
somebody was permitted to keep.

### 9.3 Closed vocabularies

```text
outcome              RETAIN  BLOCK_SECRET  CANNOT_ASSESS
coverageFailureCode  DETECTOR_UNAVAILABLE  DETECTOR_FAILED
                     INCOMPLETE_COVERAGE   INVALID_RESULT
representation       decoded-source-field-values
findingId            a rule id covered by detector.configDigest
```

Findings additionally MUST NOT carry the matched substring, an excerpt, a prefix
or suffix, a length, a character count, or a digest of the matched bytes.

### 9.4 V1 has no free text, and that is the actual defence

Two earlier revisions tried to keep a free-text failure field safe by forbidding
it from containing runs of the assessed content — first over the whole
assessment, then over free text only. Both were the wrong shape of answer.

A substring rule cannot work. It admits every secret shorter than its threshold —
a six-digit OTP, a PIN, a short passphrase, the interesting half of an API key —
and it is checking a symptom of leakage rather than the channel. Worse, a
free-text field plus an open schema means a producer can simply add
`"debug": "<the secret>"` and satisfy every rule that was written about the
fields somebody thought of.

So V1 removes `reasonDetail` and closes the schema instead:

```text
every field of an assessment is a closed vocabulary value,
a structural identifier, a boolean, or a JSON pointer

no field of an assessment is free text
```

That is a property of the value **sets**, not of the values, and it is checkable
without guessing what a secret looks like. A closed field cannot carry a secret
out because its range does not depend on the content inspected.

Nothing is lost. `coverageFailureCode` says why coverage failed, `assessedFields`
says exactly what was covered, and `findings` say which fields were flagged and
by which configured rule. A prose sentence adds nothing a consumer of this
contract needs, and adds one place for the entire secret to appear.

Introducing a free-text field later is therefore a contract change with a
security argument attached, not an implementation convenience.

### 9.5 Closed means closed at every level

A schema closed only at its top level is not closed. The nested objects below are
**exact** key sets, and an object carrying anything else is non-conformant:

```text
RetentionAssessment          the §9 field list
  detector                   id  version  configDigest
  findings[]                 field  findingId

RetentionBinding             schemaVersion  sourceKind
                             recordDigest  assessmentDigest

github-reduced-source-record the §7 field list
  locator                    the §7.3 shape for locatorKind
```

The reason is the one §9.4 already gives, one level down. Closing the top level
and leaving `detector` open means

```json
"detector": { "id": "...", "version": "1", "configDigest": "...",
              "debug": "<the secret>" }
```

satisfies every rule anyone wrote. Untrusted content behaves like water: it finds
whichever gap is left. The blacklist of obviously-bad finding keys — `excerpt`,
`match`, `length` and friends — is kept as a redundant guard, but the exact key
set is what actually holds.

**Versions belong to the retained objects, not to their components.** The three
objects that are independently canonicalized, digested and retained —
`RetentionAssessment`, `RetentionBinding` and `github-reduced-source-record` —
each carry `schemaVersion: 1`, and every one of them that carries
`redactionPolicyVersion` carries the same value as the assessment that authorised
it. A version field present but unrelated to its neighbours is decoration on
bytes about to be hashed.

`detector`, a `finding` and a `locator` do **not** carry `schemaVersion`. They
are components of a retained object rather than retained objects themselves:
nothing digests them separately, so there is nothing for a version of their own
to identify, and their exact key sets above deliberately exclude it.

An earlier revision said "every object listed above", which included the nested
three and made the contract unsatisfiable — adding the field broke the exact-key
rule, omitting it broke the version rule. The checker had it right and the prose
did not, which is the reverse of the usual direction and worth recording as such.

### 9.6 The retained assessment is the authority on its own outcome

`outcome` lives in the assessment, and the assessment is what is retained,
digested and bound. Nothing outside it may carry a competing answer that the
retention path actually follows.

```text
assessment.outcome    the authoritative retained fact,
                      and it MUST equal the §5.1 computation over
                      this assessment's own findings and coverage

anything else         an expectation, checked against it, never a substitute
```

Without this the same self-certification returns one layer out: a pipeline can
retain an assessment reading `BLOCK_SECRET` while executing a `RETAIN` path
because some outer field said so, and every structural check still passes. The
retained bytes would then disagree with what was done, which is the one thing
an evidence record exists to prevent.

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
- ~~**Classifier retention axis.**~~ **DISCHARGED** by
  `crates/o7-closure-provenance`. A decision basis lists what one decision read
  as `(source digest, JSON pointer)` pairs, and admissibility is computed per
  decision from whether each pointer resolves to retained bytes. Neither of the
  two forced errors is now reachable: no adapter has to forge `FAILED` for a
  fetch that succeeded, because retention is a separate axis; and no observation
  whose decision inputs all survived is discarded, because a blocked field the
  decision never read does not enter its evaluation. §10's R6/R7 pair is
  executable as `b4_…` and `b5_…` — same gate outcome, same blocked field,
  opposite admissibility.

  Two consequences of the computation that were not obvious until it existed. A
  pointer in **neither** `retainedFields` nor `blockedFields` is refused, not
  admitted: §7.1 partitions the required set exhaustively, so a record
  accounting for a field in neither direction is not evidence that nothing was
  blocked. And a blocked pointer is refused as a *retention loss*, distinct from
  a pointer that was simply never in the projection — collapsing the two would
  lose exactly the distinction §10 is built on.
- ~~**Where retention bindings live.**~~ **DISCHARGED**, and not where this
  document guessed. The first implementation of it was also wrong in two ways
  external review caught: it accepted a binding that answered a request about one
  record while naming another, and it never resolved the assessment the binding
  named. §9.2 says the assessment's canonical bytes are RETAINED; a consumer that
  reads only the digest string has confirmed that a permission was *cited*, not
  that it exists. Both are closed, and the assessment is checked against §9's
  closed shape — some other retained object, correctly digested and honestly
  bound, does not authorise a retention.

  One deliberate limit: the assessment is **not** itself required to carry a
  `RetentionBinding`. It is a control artifact, and requiring one would make every
  permission depend on a permission — a recursion with no base case. §9.2 asked where "the set of them" is carried, which assumed
  the set travels with the decision. It does not: the binding is retained
  alongside the record and looked up by `recordDigest`, and the decision basis
  may only *assert* one, which is then compared against the retained binding and
  refused on disagreement.

  That asymmetry is the point. If the basis carried the bindings, the party
  asserting which assessment authorised a record would be the party whose
  decision that record supports — a permission granted by whoever benefits from
  it. Making the store the authority and the basis a claim to be checked is the
  same correction Slice A applied ten times over: nothing being checked may
  arrive from the party being checked.
- **Derivation implementation binding.** `crates/o7-closure-provenance`
  recomputes a derived fact from the sources it names, which requires
  `(derivation.id, derivation.version)` to resolve to exactly one function; it
  does, via a registry hashing the defining file's bytes. The expected digest
  lives in the same tree as those bytes, so one commit can move both — the
  identical half-measure provenance V1 §23 records for matchers before the
  artifact carried the value. *Blocks nothing today*: no artifact yet records a
  derivation digest, so there is nothing for a recorded value to disagree with.
  It becomes live with the first acquisition adapter that emits a derived fact,
  and the answer is already known — record the digest in the artifact, not
  beside the code.
- **Mechanical coverage of three source kinds.** §5.3 freezes required field sets
  for five kinds. The preregistration specimens exercise three —
  `github-issue-comment`, `github-submitted-review`, `github-review-comment` —
  so `github-pull-request-head` and `github-actions-check` are frozen in prose
  and mirrored by the checker, but not witnessed by a specimen. *Blocks nothing
  today*; the acquisition adapter is the first consumer that would notice, and it
  is already blocked twice over.
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
| What value sits under each retained pointer? | §7.2 — what the complete projection would carry |
| May a locator stand in for a retained field? | §7.3, §8 — no |
| How are the set-like arrays ordered? | §5.5 — unique, ascending, no emission order |
| Why is coverage incomplete? | §5.4 — `coverageFailureCode`, whatever the outcome |
| Can a producer add a field to an assessment? | §9, §9.5 — no, at every level |
| Do the retained objects carry their own `sourceKind`? | §9 — yes, per provenance V1 §7 |
| Who is authoritative on the outcome? | §9.6 — the retained assessment |
| Is the locator bound to the gated source? | §7.3 — kind and values both |
| Are the version fields related to each other? | §9.5 — yes, and checked |
| Is there any free text in an assessment? | §9.4 — no, and that is the defence |
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

## 15. Correction round 2

One defect, found by the specimens written against correction round 1 rather
than by reading it.

1. **§9.3's anti-exfiltration rule was scoped to the whole assessment**, so it
   fired on a collision between two closed identifiers: the detector's own `id`
   and an assessed login sharing an eight-character run. §9.4 scopes it to free
   text, declares `reasonDetail` the only free-text field in V1, and states the
   real reason a closed field is safe — its value set does not depend on the
   content inspected — rather than approximating that with a substring test.

## 16. Correction round 3

Six defects from a second independent review. The previous rounds are not
reversed by these — the model they produced is the one being refined. These sit
where provenance systems usually break last: aliasing between locator and
evidence, the value under a correct pointer, canonical order of set-like arrays,
and a small free-text hole that the whole secret eventually fits through.

1. **The reduced record froze which pointers it retained and not what they
   held.** §7.2 defines `retainedFields[p]` as exactly the value the complete §8
   projection would carry, by reference to the already-frozen projection rather
   than as a second mapping. The ambiguity was real: the frozen Step 0B fixtures
   store `"id"` as a JSON number, a canonical projection carries it as a string,
   and both readings fit the earlier text.
2. **`stableId` bypassed the field-retention gate.** A record could list `/id` in
   `blockedFields` while the same source-derived id sat permanently at the top of
   the record. §7.3 separates the locator, states per-kind locator shape, and
   rules that a locator value is not surviving evidence and cannot satisfy a
   decision-basis pointer. It also drops the `stableId` requirement for
   `github-pull-request-head`, which provenance V1 §8.1 identifies by repository
   and pull request number — a contradiction the earlier text avoided only
   because no head specimen existed.
3. **The new set-like arrays had no frozen order or uniqueness.** §5.5 fixes
   unique, ascending lexical order, and forbids duplicates. Provenance V1 §13.2
   had to settle exactly this for query snapshots; JCS does not sort arrays, so
   one fact could otherwise have three digests.
4. **The anti-exfiltration rule was still a substring heuristic.** It admitted
   every secret shorter than its threshold and left the assessment schema open,
   so `"debug": "<the secret>"` satisfied every rule anyone had written. §9.4
   removes `reasonDetail`, closes the schema, and makes the property structural:
   every field is a closed vocabulary value, a structural identifier, a boolean
   or a JSON pointer. `findingId` becomes a rule id covered by
   `detector.configDigest`, so its range is fixed before anything is inspected.
5. **Precedence was defined and unwitnessed.** §5.1's interesting case — a
   blocking finding *and* incomplete coverage — had no specimen, so the corpus
   could not tell the frozen precedence from its inverse. §5.4 additionally
   requires `coverageFailureCode` whenever coverage is incomplete, so
   `BLOCK_SECRET` with partial coverage records why it was partial rather than
   only that it was.
6. **"Mirrors §5.3" overstated the mechanical coverage.** §5.3 now separates
   always from present-only sets for all five kinds, and §11 records honestly
   that specimens exercise three of them.

## 17. Correction round 4

Six findings from a third independent review. None reopens the redaction model —
the chain from decoded values through per-field assessment to a computed
partition and a decision basis is unchanged. These are binding and schema gaps
on the layer that model created.

1. **`RetentionAssessment` and `RetentionBinding` had no `sourceKind`**, while
   §9.2 sends the assessment through provenance V1 §7 canonicalization — which
   requires every canonical object to be domain-separated by its own content. A
   direct cross-contract contradiction. Both now carry frozen kinds,
   `closure-retention-assessment` and `closure-retention-binding`.
2. **The closed schema was closed only at the top level.** `detector` and each
   `finding` accepted extra keys, so `"detector": { …, "debug": "<the secret>" }`
   satisfied every rule §9.4 had written. §9.5 makes every nested object an exact
   key set. Untrusted content behaves like water; it finds whichever gap is left.
3. **Nothing bound `outcome` to the record that carries it.** An outer field
   could say `RETAIN` while the retained assessment said `BLOCK_SECRET`, and the
   retention path would follow the outer one. §9.6 makes the retained assessment
   authoritative and requires it to equal the §5.1 computation over its own
   findings and coverage. This is the same self-certification as the coverage
   denominator and derived-fact admissibility, one layer further out.
4. **`locatorKind` and the locator values were unbound.** Shape was checked and
   identity was not, so a well-formed locator could point at the wrong object —
   worse than a missing one, because it resolves. §7.3 now requires `locatorKind`
   to equal the gated source kind and the locator to equal that source's
   acquisition locator.
5. **`findings` ordering was frozen in prose and unheld.** §5.5 stated it;
   nothing checked it, and every specimen carried a single finding, so the corpus
   could not distinguish canonical order from emission order. A multi-finding
   witness closes it.
6. **Version fields existed without relationships.** §9.5 binds them: every
   retained object at `schemaVersion: 1`, and `redactionPolicyVersion` shared
   between an assessment and the record it authorised. A version field unrelated
   to its neighbours is decoration on bytes about to be hashed.

### 17.1 One thing deliberately not changed

`decodedSource` in the specimens is a map from JSON pointer to decoded value, not
a nested GitHub object, so a consumer reads it by exact pointer rather than by
RFC 6901 resolution. That is consistent with §4 and §9's `representation`
identifier, `decoded-source-field-values` — the gate assesses **extracted exact
field values**, not a document. Real pointer resolution over a live response is
the acquisition adapter's obligation, and this slice deliberately has no adapter.
Recorded here so the wording is not mistaken for "the decoded source object".

## 18. Correction round 5

Three findings from a fourth independent review. Narrow: no conceptual change,
and two of the three are the contract failing to be held rather than failing to
be stated.

1. **`finding.field` was unbound to what was assessed.** §9 now requires it to be
   in the required set and in `assessedFields`. Without that, a finding naming a
   pointer that does not exist blocks nothing while making the record read
   `BLOCK_SECRET`, and the genuinely dangerous field — assessed, unflagged —
   becomes retainable. Every other rule survives that record intact, which is
   what makes it worth stating rather than assuming.
2. **The reduced record's root schema was closed in prose and not mechanically.**
   §9.5 already listed it; nothing checked it, so an unknown root property could
   carry a blocked value into a durable canonical object without touching
   `retainedFields`. The contract text is unchanged; the checker is where this is
   fixed.
3. **The specimens' synthetic detector configuration drifted from its
   description.** A second rule was added in round 4 and the fixture README still
   described one, with the old digest. Repaired there, and the checker now binds
   the admissible rule ids to `detector.configDigest` rather than keeping a
   global list — so a record cannot claim one configuration and use another's
   rules. The production form of that binding stays OWED in §11.
