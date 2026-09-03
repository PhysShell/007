# Closure source provenance — binding contract V1

**Status: proposed normative contract. NOT an implementation.**

This document freezes how an immutable closure snapshot binds to the GitHub
observations that were actually obtained during evaluation, before any
acquisition code exists. Issue #147 defines the closure states and the predicate;
the pure classifier (merged, `crates/o7-closure-classifier`) implements them.
Neither says how a predicate stays auditable once the GitHub objects behind it
change.

## 1. Scope

**In scope.** The contract for source locators, retained source snapshots,
decision basis, canonicalization, digests, query/absence provenance, pagination
completeness, falsification-scan provenance, derived-fact provenance, and the
security boundary on what may be retained.

**Not in scope, and forbidden in the same change.** Any GitHub API client,
pagination implementation, classifier schema change, acquisition adapter,
`actions/attest` workflow, in-toto Statement, Sigstore/OIDC, artifact upload,
attestation verifier, reviewer requester, scheduler, or merge action. In-toto
does not participate in defining these semantics; it will later authenticate
what this contract defines.

## 2. The problem

The classifier records, per observation:

```text
source.kind
source.stable_id
state
acquisition status
failure reason
```

`stable_id` identifies an **object**. It does not identify the **version of that
object's content** that the classifier saw. An issue comment with id
`5305700001` may today read

```text
defect X reproduces as follows ...
```

and tomorrow, at the same id, read

```text
edited: never mind
```

A re-fetch by id returns whatever is there now. It cannot show what the
classifier actually observed. The predicate would then reference evidence it can
no longer produce.

This is not confined to comment bodies. A `review/codex = OWED` with
`acquisition = AVAILABLE` is produced because

```text
review.commit_id = Y     expected_sha = X     Y != X
```

but if `Y` is not durably recorded, the predicate cannot explain its own state
without re-fetching a mutable object. Same class, no Markdown involved.

## 3. The normative law

> Any value that influenced **subject identity**, **admissibility**, an
> observation **state**, the **headline**, or a **falsification state** must
> either (A) be present in the immutable snapshot, or (B) be unambiguously
> recoverable from retained immutable bytes that are cryptographically bound to
> the predicate.

A reference to a mutable API object is **not** option B.

The false equivalence this contract exists to forbid:

```text
stable GitHub object id   !=   immutable evidence version
```

## 4. Three concepts that must not merge

They collapsed into one `source` struct once already. V1 keeps them distinct.

| concept | answers | mutable? |
|---|---|---|
| **locator** | where the object or query lives in GitHub | points at mutable state |
| **immutable source snapshot** | what the acquisition layer actually observed and retained | frozen bytes |
| **decision basis** | which normalized values the classifier actually consumed | frozen values |

```text
locator:
  kind: github-issue-comment
  repository: PhysShell/007
  stable_id: "5305700001"

immutable snapshot:
  user id/login, body, created_at, updated_at, ...

decision basis:
  falsification verification: REPRODUCED
```

The locator enables re-fetch. The snapshot says what was there then. The
decision basis says what actually entered the classifier. Three different jobs;
convenience is not a reason to fuse them.

## 5. Ruling — a digest alone is not provenance

Rejected as a complete solution:

```text
stable_id + sha256(body)
```

A digest detects drift:

```text
old bytes -> digest D
new bytes -> digest E
D != E
```

but once the object has changed, a digest cannot reconstruct what was there. It
authenticates an absence. V1 therefore requires:

```text
retained immutable snapshot bytes
        +
cryptographic digest of those bytes
```

or an equivalent content-addressed representation. A digest whose bytes were
discarded is a fingerprint with no evidence attached.

## 6. Ruling — do not hash the raw HTTP response

Semantics must not be bound to transport formatting. The following are **not**
authorities and must not be hashed as evidence:

```text
raw HTTP response bytes      HTTP headers        ETag
JSON key order GitHub chose  compression         Transfer-Encoding
whitespace in transport JSON
```

The pipeline is:

```text
GitHub response
      -> allowlisted typed projection
      -> canonical serialization
      -> SHA-256
```

not `curl bytes -> sha256`. Otherwise a harmless change in GitHub's JSON
formatting registers as evidence mutation, and the system authenticates
whitespace instead of facts.

## 7. Canonicalization and digest

```text
canonical form  RFC 8785 JSON Canonicalization Scheme (JCS)
digest          SHA-256 over the canonical UTF-8 bytes
digest syntax   sha256:[lowercase-hex]
```

Every canonical object carries its own `schemaVersion` and `sourceKind`, so the
digest is domain-separated by its own content rather than by an external
convention:

```json
{
  "schemaVersion": 1,
  "sourceKind": "github-issue-comment",
  "stableId": "5305700001",
  "user": { "id": "136622811", "login": "coderabbitai[bot]", "type": "Bot" },
  "authorAssociation": "CONTRIBUTOR",
  "body": "exact decoded body ...",
  "createdAt": "...",
  "updatedAt": "..."
}
```

The example is a **complete** §8.5 projection, not an abbreviated one. Where an
example and an allowlist disagree, the allowlist is authoritative — but an
example that silently drops an allowlisted field is how a projection ends up
weaker than its own contract.

`user.type` is shown as `Bot` deliberately. The login already ends in `[bot]`,
and that is precisely why the typed field must be carried: a projection that
keeps only the login invites admissibility to be decided by string-matching a
suffix any account can adopt.

Numeric ids are serialized as **strings**, so the canonical form does not depend
on any JSON number implementation limit.

String values are taken exactly as decoded:

```text
UTF-8, no trim, no newline normalization, no Markdown rendering,
no HTML rendering, no Unicode case folding, no login normalization,
no body normalization
```

JSON escaping is transport, not semantics: a `"\n"` escape and a literal decoded
newline are the same JSON string value before canonical serialization.

## 8. Snapshots are allowlisted, not raw dumps

Retain the complete closure-relevant entity — and not the rest of the GitHub API
garden. Following the Step 0B precedent, these are deliberately excluded:
headers, auth, permissions, unrelated account metadata, reactions, avatar URLs,
ambient API metadata.

**The projection schema is closed.** For each `sourceKind` below, the fields are
given as an exact **REQUIRED** set and an exact **OPTIONAL-IF-PRESENT** set.
There is no third category. An adapter that emits a field outside both sets, or
omits a REQUIRED one, is non-conformant — not "extended".

```text
adding a field later  =  new schemaVersion, and a stated reason
                      != the adapter's judgement at fetch time
```

Phrases like *where closure-relevant* and *plus any further closure-relevant
field* were removed from this section deliberately. They leave the projection to
be settled by whoever writes the adapter, which is the same open door §12 closes
on the evidence bundle. If a field turns out to be decisive, that is a contract
change with a version number, not a silent widening.

**Null and absent are the same input, and canonicalize to omission.** GitHub
sends `"in_reply_to_id": null` for a top-level review comment; another response
may omit the key. Both mean *no value*, and both MUST produce a canonical object
with the key absent. Otherwise two adapters observing the same fact compute two
digests, and the digest stops identifying content.

An OPTIONAL-IF-PRESENT field is therefore never `null` in a canonical object: it
is present with a value, or not present.

### 8.1 Pull request head / subject read

`sourceKind: github-pull-request-head`

```text
REQUIRED             schemaVersion  sourceKind
                     repository  pullRequest
                     headSha  headRef  headRepoFullName
OPTIONAL-IF-PRESENT  updatedAt
```

Deliberately **not** in this projection: `state`, `merged`, `draft`,
`mergeable_state`, `base`, `created_at`, `html_url`, `node_id`. The head read
answers *what is the subject, and did it move*. If a later policy makes a
lifecycle field decisive, it gets a new `sourceKind` or a `schemaVersion` bump —
it does not join this one because it happened to be in the same JSON response.

**Two head reads are two acquisition events, not two pointers.** Retaining one
snapshot and referring to it twice does not record that two reads were performed.
V1 requires a durable event per read, and the event is **tagged by its
acquisition status** — a read that did not happen has no bytes to point at.

The event is `sourceKind: github-head-read-event`, and it is a retained object in
its own right, so §7 domain-separates it by its own content like any other:

```text
HeadReadEvent, acquisition = AVAILABLE
  schemaVersion
  sourceKind      github-head-read-event
  role            HEAD_BEFORE | HEAD_AFTER
  acquisition     AVAILABLE
  snapshotDigest  REQUIRED
  observedAt

HeadReadEvent, acquisition = FAILED
  schemaVersion
  sourceKind      github-head-read-event
  role            HEAD_BEFORE | HEAD_AFTER
  acquisition     FAILED
  reasonCode      REQUIRED
  observedAt
  snapshotDigest  MUST BE ABSENT
```

**Why the declaration is written here rather than inferred.** Earlier revisions
of this section described the event's two shapes without ever naming its
`sourceKind`, and an implementation that needed one supplied it — so the name a
consumer dispatched on existed because somebody wrote it down in a crate, not
because this document defined it. That is the same defect as a contract-declared
kind an implementation forgets, pointing the other way: neither is caught by any
check whose denominator comes from the side that made the mistake. The two
members above were being required by implementations on §7's authority and are
now stated, so a reader does not have to derive them.

**`observedAt` on either shape is an RFC 3339 `date-time` in UTC: a literal `Z`
designator, no fractional seconds, no numeric offset.** The same value domain
redaction policy V1 §9 freezes for the assessment's `observedAt`, and stated in
both places because both are declarations of the member rather than one
declaration and one reference.

It matters more here than there. §8.1's whole purpose is to bracket an
evaluation between two reads, and `observedAt` is what says which read came
first. A value nothing constrains cannot order two events, so an implementation
that accepts any string has an event pair whose bracket is decorative. The
single spelling carries the same digest-identity argument §8 settles for `null`
versus absent: two spellings of one instant are two digests for one fact, and a
`HEAD_BEFORE` event that exists twice under two digests is two claims about one
read.

A conforming value matches exactly `YYYY-MM-DDThh:mm:ssZ`, and the date and time
it names must exist.

Requiring `snapshotDigest` on every event, as an earlier revision did, forces the
adapter to invent one for a read that produced nothing — and the only digests
available to invent are a stale one or a fabricated one. Both make a failed read
look like a successful read of unchanged bytes, which is the exact confusion this
event was introduced to prevent.

`NOT_PRODUCED` and producer rate-limiting are **inadmissible** on a head read.
The subject head is not produced by any external party; nobody can decline to
emit it. An API rate limit is an acquisition failure and is recorded as `FAILED`
with `reasonCode: RATE_LIMITED` — §15's distinction applies here in only one of
its two directions.

**`reasonCode` is a closed vocabulary, and this is a correction.** The member was
declared `reason` and given no domain, so it accepted any string:

```text
RATE_LIMITED  REQUEST_FAILED  NOT_FOUND  UNAUTHORIZED
```

The four are separated because they are four different facts about the subject,
and collapsing them loses the distinction that matters most: `NOT_FOUND` says
the subject may not exist, `UNAUTHORIZED` says nothing about the subject at all,
and reading the second as the first is an absent signal reported as a negative
result. `RATE_LIMITED` is named by the paragraph above; the other three are the
acquisition outcomes that paragraph's `FAILED` covers.

**Why a code rather than a sentence, and why this document rather than a
consumer.** Redaction policy V1 §9.4 settled this exact question for the
assessment — it removed `reasonDetail` and closed the schema instead, because
"a closed field cannot carry a secret out because its range does not depend on
the content inspected". A failed head read is written by an acquisition layer
holding an HTTP response and an authorization header, and a free-text
diagnostic is the most natural place in this entire contract for a credential to
land. That member was not reconsidered when §9.4 was decided. This amendment
reconsiders it, on §9.4's own argument, for the retained object where the
argument applies just as exactly.

A consumer could not have made this correction. Refusing free text where a
contract permits it is a consumer inventing a norm, which is the direction §8.1
already refused once for `observedAt`; the document has to move first, and this
is that move.

Two events MAY carry the same `snapshotDigest` — that is precisely how "the head
did not move" is recorded — but there are still two declared reads. Exactly two
events per evaluation; fewer is non-conformant.

If HEAD_AFTER is `FAILED`, staleness is **unknown**, and unknown is not "not
stale":

```text
HEAD_AFTER failed  ->  CANNOT_CHECK
                   ->  never a silent absence of STALE
```

Recording only `subjectStale: true` makes `STALE` unexplainable from the
artifact; recording only one snapshot digest twice makes a missing second read
unexplainable in the same way, one level down.

#### 8.1.1 OPEN QUESTION — is the frozen witness adequate to order two reads?

**Non-normative. This subsection states no obligation, changes no rule, and
nothing may be implemented on the strength of it.** It records an unresolved
question about §8.1's evidence so that it is not rediscovered as a defect and
not settled by an implementation picking a reading.

§8.1 says the two reads bracket an evaluation and that `observedAt` is what
says which read came first, and it freezes `observedAt` to whole seconds. A
pair whose `HEAD_AFTER` is observed strictly before its `HEAD_BEFORE`
contradicts the first of those and is refused — that much follows from the
frozen text and is implemented.

Equal timestamps do not follow from it, in either direction:

- two genuine reads bracketing a fast evaluation serialize to the same second
  at this precision, so refusing the pair would refuse conformant evidence;
- two equal timestamps cannot themselves establish which read came first, so
  admitting the pair accepts a bracket the witness does not demonstrate.

Both are true at once. The question is therefore whether a whole-second
`observedAt` is an adequate witness for the ordering §8.1 relies on, and the
answers available to it are ones only this document can give — sub-second
precision, an explicit ordering obligation, an explicit statement that equal
timestamps are admissible, or a different witness for the order. Until one is
chosen, the equal case is treated exactly as it was before the question was
asked.

### 8.2 Check run

`sourceKind: github-actions-check`

```text
REQUIRED             schemaVersion  sourceKind  stableId
                     name  headSha  status
OPTIONAL-IF-PRESENT  conclusion  startedAt  completedAt
```

`conclusion` is optional because a queued or in-progress run has none; per §8 it
is then absent, not `null`.

This is **exactly** the field set the frozen Step 0B check-run objects carry —
`id, name, head_sha, status, conclusion, started_at, completed_at` — so the
earlier promise that the V1 projection would never be weaker than the frozen
evidence is now discharged by enumeration rather than by an open-ended clause.

### 8.3 Submitted review

`sourceKind: github-submitted-review`

```text
REQUIRED             schemaVersion  sourceKind  stableId
                     user.id  user.login  user.type
                     authorAssociation  state  body
                     submittedAt  commitId
OPTIONAL-IF-PRESENT  (none)
```

`user.type` is REQUIRED. The frozen Step 0B corpus retains it, and it is the
field that distinguishes `Bot` from `User` independently of a login that merely
looks like a bot. Admissibility must not rest on string-matching `[bot]`.

Excluded: `html_url`, `pull_request_url`, `node_id`. They are locators (§4) and
per §10 no locator is a content identity.

### 8.4 Review comment

`sourceKind: github-review-comment`

```text
REQUIRED             schemaVersion  sourceKind  stableId
                     pullRequestReviewId
                     user.id  user.login  user.type
                     authorAssociation  body
                     commitId  originalCommitId  path
                     createdAt  updatedAt
OPTIONAL-IF-PRESENT  inReplyToId  line  originalLine  side  startLine
```

`pullRequestReviewId` is REQUIRED because §18's example derivation
(`review_comment.pull_request_review_id == review.id`) is unreproducible without
it.

### 8.5 Issue comment

`sourceKind: github-issue-comment`

```text
REQUIRED             schemaVersion  sourceKind  stableId
                     user.id  user.login  user.type
                     authorAssociation  body
                     createdAt  updatedAt
OPTIONAL-IF-PRESENT  (none)
```

Excluded: `html_url`, `issue_url`, `node_id`, `url`, `user.avatar_url`,
`user.site_admin`, `reactions`, `performed_via_github_app`.

Reactions are **not** added here. The frozen Step 0B README already records
honestly that no reaction specimen exists.

## 9. Mutable bodies are retained exactly as observed

```text
snapshot.body = the exact JSON-decoded string obtained from GitHub
```

No `trim()`, no Markdown-to-HTML, no HTML-to-text, no whitespace collapsing, no
code-fence stripping, no quote-marker normalization.

If an edit changes one space, the snapshot digest changes. That is correct: the
evidence bytes changed. If a later policy decides some whitespace is
semantically irrelevant, that is a **semantic normalization version** and gets
its own contract — it does not hide inside provenance V1.

**A witness for this rule must vary the body alone.** Two projections that differ
in the body *and* in `updatedAt` produce different digests either way, so they
cannot show that the body was retained byte-exact — an adapter calling `trim()`
would pass that comparison. The conformance witness is a pair whose canonical
objects are identical in every field except one trailing byte of `body`,
`updatedAt` included. Only then does an equal digest prove the trim happened.

## 10. Ruling — `updated_at` is not content identity

```text
stable_id + updated_at
```

does not substitute for a snapshot digest. `updated_at` is useful provenance;
authority is *snapshot bytes + digest*. The same applies to `html_url`,
`node_id` and `ETag`: none is an immutable content identity.

## 11. Content-addressed evidence bundle

Snapshots are not inlined into every observation. The preferred V1 shape:

```text
closure-predicate.json     states, headline, bindings
closure-sources.json       immutable normalized snapshots, addressed by digest
```

The predicate binds by digest:

```json
{
  "source": {
    "kind": "issue-comment",
    "stable_id": "5305700001",
    "snapshotDigest": "sha256:abc..."
  }
}
```

This fragment is **predicate** shape, so it uses the merged classifier's
vocabulary: `SourceOut` serializes `kind` and `stable_id` unrenamed, and
`SourceKind::IssueComment` serializes as `issue-comment`. The
`github-issue-comment` spelling used in §4 and §7 belongs to the locator and the
canonical snapshot, which are V1 concepts with their own vocabulary. The two
must not be interchanged in writing: a predicate example carrying snapshot
vocabulary reads as an unannounced classifier schema change, and this slice
changes no classifier schema. `snapshotDigest` is one of the fields §21 hands to
the classifier provenance binding slice; §21 has the full list.

**Critical rule:**

```text
snapshotDigest MUST resolve to a retained snapshot
```

The following is forbidden:

```text
predicate references digest
snapshot bytes discarded
```

That is a fingerprint without evidence — exactly what §5 rejects. File names are
not authority; the binding `digest -> canonical snapshot bytes` is.

## 12. The bundle is not a second classifier schema

`closure-sources.json` holds provenance only. It must not contain:

```text
expectedState        headline
"this means FINDING" "review/codex = PASS"
```

Otherwise the evidence bundle becomes another answer key, and a later test
confirms its own crib sheet. Permitted: source identity, query identity,
observed source fields, pagination/completeness metadata, canonical schema
version, digest. Classifier decisions stay in the predicate.

## 13. Authoritative absence needs its own provenance

For

```text
NotProduced -> OWED
```

there is no object snapshot, because no object was found. But the absence is a
claim about the **result of a query**, not about an object. And "no object was
found" is really two claims that fail independently:

```text
the endpoint returned nothing            (enumeration)
nothing the endpoint returned qualified  (selection)
```

Recording only the matched set proves neither. An endpoint that returned a
qualifying review, plus a matcher that failed to recognise it, is indistinguishable
from an empty repository — and the failure is invisible because the surviving
artifact contains exactly one thing: an empty list.

V1 therefore requires a **query snapshot** with the candidate set retained and
the selection rule named. `sourceKind: github-query-snapshot`:

```text
REQUIRED             schemaVersion  sourceKind
                     surface  requiredObservationId
                     binding.repository  binding.pullRequest
                     pagination.perPage
                     pagination.pagesRequested  pagination.pagesObtained
                     pagination.nextPagePresent
                     enumeration
                     matcher.id  matcher.version  matcher.parameters
                     matcher.implementationDigest   (schemaVersion 2 only)
                     allReturnedSnapshotDigests
                     matchedSnapshotDigests
OPTIONAL-IF-PRESENT  incompleteReason  binding.sha
```

**`schemaVersion` 2 adds `matcher.implementationDigest`.** The two shapes are
closed key sets and neither may borrow from the other: a version-1 snapshot
carrying the field and a version-2 snapshot missing it are both malformed.
Version-1 snapshots remain valid records of what the contract required when they
were written; replaying one yields CANNOT_CHECK on the implementation axis — an
axis with no evidence, never a pass. The reason the field exists is §13.1's
second half, below.

`allReturnedSnapshotDigests` is the **complete candidate set** — every object the
enumeration returned, each retained as a source snapshot under §11, including the
ones that did not qualify. `matchedSnapshotDigests` is a subset of it.

The rule:

```text
NotProduced is legal ONLY when
    enumeration = COMPLETE
  AND a deterministic, identified matcher
  AND that matcher, applied to the RETAINED candidate set,
      yields an empty matched subset
```

Digests, not ids: a bare `stable_id` in a matched set is a reference to a mutable
object, which §3 forbids as evidence.

`enumeration` is a **closed set of two**:

```text
enumeration states   COMPLETE  INCOMPLETE
```

Closed, because the rule above turns on the value: a reader that accepts an
unrecognised state has to decide what it means, and every available default is
wrong. Treating it as complete manufactures authority the adapter never claimed;
treating it as incomplete silently discards evidence. Refusing it is the only
answer that does not invent a fact.

`FAILED` is deliberately not one of them. It is §16's vocabulary for a
falsification surface scan, which is a different record about a different
question, and specimen D is the frozen witness that a failed page fetch is
recorded *here* as `INCOMPLETE` with an `incompleteReason`. A shape that borrows
a neighbouring record's states is not closed either.

An `INCOMPLETE` snapshot is a **well-formed** record. §14 forbids it from
supporting authoritative absence; it does not forbid it from existing, and
specimen D exists precisely so the non-authoritative empty result stays
recordable and distinguishable from the authoritative one. Conformance and
admissibility are separate questions and a consumer that collapses them destroys
the distinction this section was written to create.

Both `allReturnedSnapshotDigests` and `matchedSnapshotDigests` MUST be present
even when both are empty. An empty candidate set is a fact about the enumeration;
an absent one is a fact about the adapter.

### 13.1 The matcher must be re-executable, not merely named

Retaining the objects a function ran over does not reproduce the function. A
selection rule takes two inputs — the candidate snapshot **and** its parameters —
and an identity pair that names only the rule leaves the second input to memory:

```text
"reviews by the expected author"   expected author = ?
```

An auditor who can resolve every candidate digest and still cannot say which
author was expected has reproduced the input and not the decision.

```text
matcher.id          names a deterministic, total, pure predicate
                    f(candidate canonical snapshot, parameters) -> bool
matcher.version     changes whenever f's behaviour changes for ANY input
matcher.parameters  every value f reads that is not the candidate snapshot
matcher.implementationDigest
                    which implementation actually ran (schemaVersion 2)
```

`matcher.version` is a **semantic name**: it says which rule was intended.
`matcher.implementationDigest` is the **replay binding**: it says which code
carried that intention out. The two are different obligations and the second
does not follow from the first, because `version` is a string an implementer
chooses and `ANY input` is not provable by any finite check that implementer can
run. The triple

```text
(matcher.id, matcher.version, matcher.implementationDigest)
```

is what a replay is entitled to rely on. An implementation whose digest differs
from the one a snapshot recorded is a *different* implementation regardless of
what version it claims, and MUST be refused rather than reconciled.

The digest MUST be over the implementation itself — bytes that the running code
is built from — and MUST NOT be over a sample of the implementation's behaviour.
A digest over results on a finite vector set binds a finite observation of `f`
and not `f`, so a behaviour change on any input outside that set passes it
unchanged. Behavioural vectors remain useful as regression witnesses and are not
a substitute for this.

The expected value MUST be recoverable from something other than the tree that
holds the implementation. A constant sitting beside the implementation is edited
by the same act that edits the implementation, so it establishes that two current
fields agree and not that a version is what it was. That is why the digest is
carried in the **snapshot** — an artifact written at acquisition time, whose own
canonical digest is covered by whatever retained it — rather than only in a
registry. This does not make an implementation unforgeable by whoever controls
both the code and the corpus; it makes drift a change to a durable record instead
of a change to a neighbouring line, and it makes an artifact already emitted
unreplayable under changed code.

The rules that make re-execution possible:

- `f` MUST depend on nothing beyond its two inputs. No clock, no network, no
  ambient configuration, no repository working state, no environment.
- `matcher.parameters` MUST be immutable literal values, canonicalized like any
  other object under §7. A parameter that is a *reference* to a mutable GitHub
  object is forbidden by §3 for the same reason a matched set of `stable_id`s is:
  it does not carry what it pointed at.
- `matcher.parameters` MUST be present even when the rule takes none, as `{}`.
  An absent parameter block cannot be distinguished from a forgotten one.
- V1 matchers are **per-candidate**. `f` decides each candidate independently, so
  `matchedSnapshotDigests` is exactly the subsequence for which `f` is true.
  Anything needing cross-candidate context — latest-wins, dedup by author,
  first-match-only — is a different matcher class and needs its own contract
  rather than an adapter's judgement.

The conformance obligation:

```text
given  matcher.id + matcher.version + matcher.parameters
  and  allReturnedSnapshotDigests resolved to their retained snapshots
then   matchedSnapshotDigests MUST be exactly recomputable
```

If it is not recomputable, the absence claim is an assertion with provenance
decoration. That is the whole difference this section exists to protect.

Two things follow that an implementation can get wrong while still looking like
it satisfies the paragraph above.

**Replay runs over the complete declared set, or it does not run.** The candidate
sequence is `allReturnedSnapshotDigests` exactly, in observation order. A replay
given a prefix, a subset, or a reordering MUST be refused, not performed over
what is present. Resolving only part of a bundle — a retained blob that will not
load — and recomputing over the part that loaded answers a different question
than the snapshot recorded, and the two answers agree most often in exactly the
case that matters: an empty claim reproduces against an empty slice. *Partial
success is not success.*

**A candidate that does not conform to its §8 schema is not admissible input.** A
matcher is defined over *canonical source snapshots*; an object that declares a
`sourceKind` and then omits a field that kind requires is not one, and MUST be
refused rather than passed to the matcher. Scoring it produces `false` — a
snapshot that could not be read, recorded as a candidate that did not qualify.
Note that the digest binding does not catch this: a truncated projection hashes
to its own digest correctly, so every candidate can verify while the set as a
whole is unreadable. *An absent signal is not a negative result.*

"Conform to its §8 schema" means the **whole** closed shape for that
`sourceKind`, not the subset a particular matcher happens to read. A review
missing `commitId` is not a canonical source snapshot even though no registered
matcher reads `commitId`, and admitting it because the selection rule would not
have looked at that field lets an empty matched set be assembled out of objects
that are not evidence. Both directions of the closed key set are checked, since a
member outside the declared set is a §8 violation exactly as a missing one is,
and `null` never stands in for an absent optional member.

**§7's universal members are checked on every candidate, whatever kind it names.**
An object with no `sourceKind`, or a non-string one, is not a canonical object at
all, so it is not "a candidate of a different surface" — the delivery-surface
path is for objects that legitimately declare *another* kind, not for objects
that declare none. The same holds for `schemaVersion`. Treating an undeclared
object as a foreign one lets a truncated snapshot that still carries the expected
login be scored as a candidate that did not qualify.

**Conformance is judged against the kind the candidate declares, not the kind the
running matcher scores.** A candidate of another surface is still an ordinary
non-match — that is the delivery-surface law — but it has to be a well-formed
object *of that surface* first. Validating only the matcher's own kind leaves a
malformed foreign object scored `false` and joining an absence claim, which is
the same defect as a malformed same-kind one wearing a different `sourceKind`.
A `sourceKind` §8 does not define is likewise unreadable: a canonical source
snapshot comes from an enumerated surface, so an object claiming another one is
refused rather than scored.

**An unregistered `schemaVersion` is unreadable evidence, not a non-match.** §8
gives a changed projection a new version, so a candidate declaring a version the
consumer does not know is an object whose shape is unknown. Applying the shape it
does know, because the object happens to satisfy that key set, scores evidence
the consumer was never taught to read. Checking that `schemaVersion` is an
integer establishes only that the field is well-typed; admissibility turns on its
value, since that value is what says which key set the object was built to.

**The claim is read from the snapshot, never supplied alongside it.** A verifier
handed `matchedSnapshotDigests` as a separate argument is a verifier whose caller
chooses what it is checking against, and passing the recomputed value in place of
the artifact's own reports agreement for a snapshot that contradicts itself.
`matchedSnapshotDigests`, `allReturnedSnapshotDigests` and
`matcher.implementationDigest` are all fields of the recorded matcher for the
same reason: every one of them is the thing being checked, and none may arrive
from the party being checked.

**The query snapshot joins the content-addressed chain.** §11 retains snapshots
BY digest, so the authority is the mapping `digest -> retained bytes`, never the
bytes on their own. Candidates were already bound that way — each candidate's
digest is recomputed and checked against what the query snapshot declared — while
the query snapshot itself was checked against nothing, so the chain terminated on
an unbound object one step above the part that was careful. A consumer MUST
therefore recompute the canonical digest of the **whole** query snapshot and
compare it against a digest supplied from outside, before reading any recorded
value out of it. Checking a subset leaves every unchecked member free to differ
from the artifact the digest names, including the members replay is checked
against.

What that establishes, and what it does not, stated together because the second
half is the part that gets dropped:

```text
bytes + expected digest, mismatched   ->  REFUSE
bytes + expected digest, matching     ->  these are the bytes that digest names
```

A forged snapshot presented together with the digest of that same forgery is
internally consistent and passes. This is **content binding relative to an
expectation, not authentication**. The expectation's *authority* comes from the
layer that retained it — which digest is in the decision basis is a provenance
question, not a matcher one; its *production* from acquisition, which computes
the digest of the bytes it retained; and its *authenticity* from attestation.
The mechanical comparison MUST NOT be deferred to the producer: a producer that
is the sole attester of its own bytes hashing to its own digest has verified
nothing. Naming follows the same discipline — no type or field in this chain may
carry an adjective like "trusted" or "authenticated" that a later reader could
mistake for a property the mechanism has.

**A digest-bound query snapshot is not a checked one.** The rule above binds the
snapshot's *bytes*; it establishes nothing about their *shape*, because a
malformed snapshot hashes to its own digest exactly as a well-formed one does.
A consumer MUST therefore validate the whole closed §13 shape of the version the
snapshot declares — `sourceKind`, a registered `schemaVersion`, every REQUIRED
member present and correctly typed, the nested `binding`, `pagination` and
`matcher` shapes closed in turn, optional members never `null`, and no member
outside the set — **before** reading any recorded value out of it.

This is the same obligation §8 places on candidates, applied to the object that
*declares* the candidates, and it was missed for the reason all nine of its
predecessors were missed:

```text
checking several significant members of an object is not checking the object,
  when the contract defines admissibility by the whole closed form
```

Reading the seven members a matcher parser needs left the other ten free to be
anything, which is not a cosmetic gap: without `sourceKind` any canonical object
carrying a matcher block reads as a query snapshot, and without `enumeration` an
absence claim can be assembled from a snapshot that never said its enumeration
finished. A closed shape has two sides — a missing REQUIRED member and a member
outside the set are the same violation — and a validator that closes one side
closes neither. Both are checked here, and the earlier decision to carry the
superset side forward as a cosmetic residual was wrong on exactly that ground.

**Conformance is not admissibility, and this layer decides only the first.** That
`enumeration` is present and carries one of the two states §13 defines is a fact
about the object's shape. Whether a given state is sufficient input for a given
decision — §13's `NotProduced` is legal ONLY when `enumeration = COMPLETE` — is a
fact about that decision, and belongs to whoever makes it. A consumer that
refuses `INCOMPLETE` at construction has not enforced the rule; it has made
specimen D unrepresentable and destroyed the distinction §13 exists to create. A
consumer that treats construction as evidence of admissibility has made the
opposite error. The two questions are answered in different layers on purpose.

**A type that carries recorded values must not let a caller assign to them.**
Reading the claim from the snapshot instead of taking it as an argument closes
the bypass only if the parsed value is then immutable. Otherwise a caller parses
a snapshot whose claim is false, overwrites the field with the recomputed list,
and the verifier agrees — the same defect reopened through assignment rather than
through a parameter. The rule is enforced by construction: one constructor, which
reads an artifact, and no public fields.

The second rule is a precondition of the pipeline, not a branch of the predicate.
Placing it inside `f` would make the matcher's own bytes carry schema knowledge
and would make a schema correction a matcher behaviour change under §13.1 —
requiring a new `version` for a fix that has nothing to do with the selection
rule. Which fields are checked is a property of the matcher (it is exactly what
that matcher reads) and is declared alongside it; the checking happens before
`f` is called.

### 13.2 The digest arrays are ordered sequences

They are described as a candidate set and a matched subset, but they serialize as
JSON arrays, and JCS does **not** sort arrays. Array order is therefore part of
the digest, and an undefined order means two conforming adapters compute two
different query digests from one observation.

```text
allReturnedSnapshotDigests   observation order:
                             pages in the order they were obtained, and
                             within a page, the order the API returned objects
matchedSnapshotDigests       the relative order of allReturnedSnapshotDigests
duplicates                   RETAINED, never silently deduplicated
```

`matchedSnapshotDigests` is a **subsequence** of `allReturnedSnapshotDigests`,
not merely a subset: same members, same relative order.

No sorting. Sorting would discard the enumeration order, which is itself observed
evidence — and a duplicate is evidence too. GitHub can return the same object on
two pages when data shifts mid-pagination; recording that twice says the
enumeration saw it twice, while quietly collapsing it says the adapter decided
what the enumeration meant.

Without all of this, `NotProduced` means only "this `Vec` is currently empty". We
already know how that engineering ends.

## 14. Pagination — rule frozen now, implementation later

No pagination implementation in this slice. The norm is frozen before the
acquisition adapter exists, because otherwise the adapter picks one by accident.

```text
first page != complete query

NotProduced is legal ONLY after COMPLETE enumeration
```

If a next page exists, pagination terminated early, a page fetch failed, or the
pagination state is unknown, the acquisition layer **may not** claim
authoritative absence. That is:

```text
CANNOT_CHECK        not OWED, not PASS, not NotProduced
```

Step 0B still has no pagination specimen and one is not created here. Contract
vectors for this rule are synthetic and labelled as such.

## 15. Two distinct rate limits

They must not share one ambiguous `RateLimited` without naming the layer.

```text
PRODUCER rate-limited
    an external reviewer could not produce a verdict
    -> OWED            (already #147's rule)

ACQUISITION rate-limited
    the GitHub API did not give us a trustworthy answer about
    observations that may well exist
    -> CANNOT_CHECK    (acquisition failure)
```

## 16. Falsification scans need provenance even at zero claims

The classifier currently accepts `falsifications: Vec<FalsificationFact>`. An
empty vector does **not** say "the surface was fully examined and there are no
claims". It equally permits: never fetched, fetch broke, only page 1 read,
parser died, surface unavailable.

V1 therefore requires a **falsification surface scan** record that exists even
when zero claims were found:

```text
FalsificationSurfaceScan
  surface
  query binding
  acquisition / completeness
  source or query snapshot digest
```

```text
COMPLETE scan + zero claims      -> zero falsification records is meaningful
INCOMPLETE or FAILED scan        -> must NOT become zero falsifications
                                 -> contributes CANNOT_CHECK
```

This is the oldest demon in this project — `failure -> empty set -> green` — and
it would otherwise walk back in through the acquisition adapter's front door.
No classifier change is made here; the norm is frozen so the adapter cannot
choose its own.

## 17. Decision basis is separately auditable

The snapshot answers *what did GitHub return*. It does not answer *what did the
adapter hand the classifier*. Both are needed:

```text
source snapshot
    -> adapter rules ->
decision basis
    -> classifier ->
predicate state
```

Minimum decision basis per observation:

```text
check         observed head_sha, observed conclusion
review        observed commit_id, derived carries_finding
absence       expected query snapshot digest
subject       head_before, head_after
falsification subject_sha (if any), verification status
```

Without this, source bytes can be retained perfectly while an adapter bug stays
invisible.

**The `absence` row was added after the other four, and the reason it was
missing is worth stating.** An authoritative absence is the one decision whose
subject is that no object was found, so it has no observed field to require:
demanding `commit_id` and a derived `carries_finding` would be demanding
evidence *of the very object the decision says is not there*. A consumer with
only the four rows above therefore had no honest profile to evaluate an absence
claim under, and the gap was invisible while nothing checked minimum basis at
all — an absent requirement and a satisfied one look identical until something
starts asking.

The row requires one thing, and deliberately only one:

```text
absence  ->  the basis names the query snapshot the claim rests on
```

It does **not** restate that the snapshot must be `COMPLETE`, that its matcher
must be bound to its implementation, that replay must reproduce the recorded
selection, that `requiredObservationId` must equal the basis's observation, or
that the matched set must be empty. §13 and §14 already impose all five, and a
minimum-basis row that repeated them would be a second copy of a frozen rule —
the failure §5.2 of the redaction policy names for denominators, in the place
where this document states obligations.

The division is exact. §13/§14 say what the query snapshot must BE; this row
says what the basis must PROVIDE before those questions have a subject at all.
A basis naming no snapshot does not fail the §13 checks — it never reaches
them, which is precisely how a decision with no evidence reads as a decision
with no problems.

**The decision basis is also where an expected digest lives, and the direction
is load-bearing.** §13.1 establishes that a consumer checks retained bytes
against a digest supplied from outside them, and records that the *authority* of
that digest is a provenance question rather than a matcher one. This is that
answer: the digest is named by the frozen decision basis, and the retained-
evidence store is asked only to resolve it. The store never supplies the value
it will then be checked against.

```text
store.query_snapshot() -> (snapshot, digest)     FORBIDDEN
```

That shape is self-consistency wearing the costume of verification — a thing and
its certificate written by the same act — and it passes every local check. A
retained-evidence interface may still legitimately return artifacts that contain
further references — a retention binding names the assessment that authorised it,
a head-read event names the snapshot it read — so a rule forbidding a digest to
appear in a returned value would forbid the evidence chain itself. An earlier
revision stated exactly that rule, and **it is withdrawn as wrong** rather than as
poorly enforced. The correct rule is about **authority**, not about where a digest
may appear:

```text
A retained store is never an authority merely because it returned a value.
Every value the store returns is an untrusted claim.

A digest or reference returned INSIDE such a value may be consumed only when
  1. its subject relation is checked against the independently requested subject
  2. every referenced artifact required for admissibility is resolved
  3. the resolved bytes are re-digested against the reference
  4. the required type, schema and relationship checks succeed

The store MAY resolve an independently supplied digest.
It MAY return artifacts containing further digest references.
It MAY NEVER make those references authoritative merely by returning them.
```

Each clause is there because omitting it was reachable, and three of the four
were reached in one implementation at once. Without (1), a binding answering a
request about record A while naming record B is accepted — a well-formed pointer
at the wrong subject, which resolves. Without (2), a binding may name an
assessment nobody retained, so the permission is a rumour about a document.
Without (4), a scan may be evidenced by a snapshot that is real, complete and
correctly digested and is **about a different query**; "the evidence is genuine,
just of another question" is a distinct escape from "the evidence is missing",
and only a relation check closes it.

**A structural test over an API surface is not evidence of this law.** Asserting
the exact set of methods on the interface is worth doing — it makes a change to
the trust surface deliberate rather than silent — but it establishes only that
the surface did not change unnoticed. The law is behavioural and is carried by
witnesses that exercise each relation. Documenting a surface guard as though it
enforced the law is how the withdrawn rule survived review: the guard was green,
so the property was believed.

### 17.1 Content binding is necessary and not sufficient

The four-clause authority rule above says when a reference returned by a store
may be **consumed**. This says what consuming it has still not established.

```text
resolved and re-digested   ->  this artifact exists, and these are its bytes
                           ->  NOT that it concerns this subject
                           ->  NOT that it has the role this decision assigns it
                           ->  NOT that its state supports this claim
                           ->  NOT that it authorises this other artifact
```

So a retained artifact may influence a decision only after **both** are
established:

```text
1. artifact validity   bytes, digest, type, closed schema
2. relation validity   the artifact's own fields establish the exact subject,
                       role, state, partition and relation under which this
                       decision consumes it
```

Three consequences, each of which was a live escape before it was written down:

- **The subject must arrive from outside the artifacts being checked.** Two head
  reads of another pull request are real, correctly digested, correctly roled,
  and agree with each other perfectly. Deriving the target from the same retained
  events under examination is the party in question supplying the identity it is
  examined against, so §8.1's staleness question takes
  `{ repository, pullRequest, expectedSha }` from the caller.

- **The role is part of the relation.** A record consumed as *the query snapshot
  an absence claim rests on* and a record consumed as *a gated source read
  through a decision pointer* are two different jobs. A submitted review standing
  in for the first resolves, re-digests, and answers a question it cannot answer;
  §13 is explicit that only a query snapshot carries the enumeration and matcher
  an absence rests on.

- **A caller's account of a state is not the state.** A falsification scan
  declaring itself COMPLETE, evidenced by a snapshot of exactly the right surface
  and query whose own `enumeration` reads `INCOMPLETE`, passes every content
  check and returns zero claims as a fact about the surface.

### 17.2 Validation is a door, not a habit

§17.1 says what must be established before an artifact influences a decision.
This says where. The distinction is not pedantry: three correction rounds stated
§17.1 correctly and implemented it as separate checks in separate paths, and each
round found the next path it had not been carried to.

```text
RetainedEvidence::resolve(..)
        |
        v   digest identity
        v   known artifact kind
        v   exact closed form — required, optional-if-present, no unknown
        v      members, nested closure too, REGISTERED schemaVersion
        v   gate classification
        |
   validated artifact
        |
        +--> retention authority      +--> scan semantics
        +--> pointer semantics        +--> head-read semantics
        +--> query replay             +--> subject and relation checks
```

**A raw resolved object is not an admissible argument to any semantic path.**
Not "should not be" — must not be expressible as one. A consumer added next year
cannot reach pointer, scan, head or relation semantics without coming through
the door, because the type the door returns is the only thing those paths accept
and nothing else can construct it.

Three consequences that are easy to miss, each of which was a defect before it
was written down:

- **Order is part of the claim.** A malformed reduced record must be refused as
  a malformed artifact and never as a blocked pointer. `PointerBlocked` is a
  statement about the retention semantics of that record, so producing it means
  the partition was consulted — the object was treated *as* a reduced record —
  before anything established that it is one.

- **The gate side is a property of the kind, not of the call site.** §5.3's
  gated set includes `github-pull-request-head`, so the subject read acquires
  §9.2 authority by being a gated kind and not by a binding lookup written into
  the head path. A check placed at one call site is a check the next call site
  will not have.

- **The version is part of admissibility.** §8 and §13 both give a changed shape
  a new version, so `schemaVersion` is matched against the REGISTERED values of
  that kind rather than merely typed as an integer. A version this reader does
  not know describes a key set nobody agreed to, and checking it against a
  neighbouring version's table is checking the wrong contract.

**A refusal vocabulary shrinks when a door is added, and that is the point.**
Assessment-specific spellings of "these bytes are not the ones the digest names"
and "this is not a conforming object" were removed as unreachable: every artifact
enters one way, so those are one fact each, not one per kind. A variant that
survives only because one artifact still has a private validation path is the
last trace of the design being removed.

### 17.3 A validated artifact is not yet evidence

§17.2 made clause 1 of §17.1 a construction. Clause 2 stayed a set of procedures,
and the round after §17.2 found what that costs: a query snapshot recording
`INCOMPLETE` is a **well-formed** §13 artifact — this document says so outright —
and the falsification-scan path inspected `/enumeration` while the absence path
did not. One artifact, two consumers, two answers. Not because the two decisions
need different things, but because one consumer remembered a clause.

So the same move applies one level up:

```text
        validated artifact           §17.2
              |
              v   qualify for the role
              v     required state           enumeration, acquisition, outcome
              v     subject relation         this record, this query, this head
              v     replay agreement         the artifact's own claim recomputes
              v     authority                §9.2's retained binding chain
              |
     role-qualified evidence
              |
              v
   an admissible decision, a meaningful zero, a staleness verdict
```

**One qualification per artifact kind, consumed by every role that reads it.**
The absence claim and the falsification scan ask different questions of a query
snapshot — one wants an empty matched subsequence, the other wants a claim count
that agrees with a non-empty one — but they ask them *of the same qualified
artifact*. Splitting the qualification and giving each path the clauses somebody
remembered is the defect, not the fix: a clause one path remembers and the other
does not is a procedure, and the two paths will diverge again the next time one
of them is edited.

**The authority chain is an artifact chain.** §9.2's `RetentionBinding` is a
retained object with a closed shape and a registered version, so it comes through
the door like anything else:

```text
binding bytes the store hands over   ->  a CLAIM
        digest                       ->  D
        resolve(D)                   ->  retained bytes, REQUIRED
        validate(D, retained)        ->  the door
        /recordDigest == the record  ->  the subject relation
```

Digesting the handed-over bytes and handing that digest to the validator would be
a tautology — any bytes are the bytes of their own digest. `resolve(D)` is what
makes it a check, because §9.2 requires the binding to be *separately retained*:
a store that can produce the bytes but cannot produce them under their own digest
has produced a claim nobody kept.

**A reference inside a qualified artifact is not a lesser artifact.** A
qualification reads other artifacts — §13's query snapshot names the candidate
set replay runs over — and those references are a fourth arrow inside it:

```text
validated query snapshot
        |
        v   allReturnedSnapshotDigests
 candidate references
        |
        +-- validated       necessary
        +-- AUTHORISED      and not optional
        |
        v
 matcher replay
```

Resolving a reference and checking it is a conforming artifact is clause 1 of
§17.1 by itself. §5.3 places the query snapshot outside the gate precisely
because it holds only enumeration facts and the digests of objects that passed
the gate **on their own** — so the objects are gated even though the artifact
naming them is not, and a candidate with no reachable `RetentionBinding` is
bytes somebody kept taking part in proving an absence.

The role is named rather than inherited. Both a complete projection and a
reduced source record are gated, so the gate classification cannot separate
them, and §13's matcher is defined over canonical §8 source snapshots: a reduced
record is a legitimate thing to read a decision pointer out of and not a thing a
matcher can score. What each role accepts is therefore part of the role, not a
consequence of the gate.

**A denominator drawn from the implementation confirms only that the
implementation remembers itself.** `closure-retention-binding` is declared in
full by redaction policy §9.2 and §9.5, and was absent from this crate's own idea
of what artifact kinds exist — so no mutation over that idea could have found it.
The mapping is checked in both directions, from the contracts' declared kinds to
the implementation and back, and prose naming a kind does not count as
implementing it.

## 18. Derived facts must not masquerade as source fields

`ReviewEvidence.carries_finding` is **not** a GitHub API field. It is derived,
for example by:

```text
review carries a finding
    because review_comment.pull_request_review_id == review.id
```

The contract requires each value to be marked as either a **source field** or a
**derived acquisition fact**, and every derived fact that influences the
classifier must list the source snapshot digests it was derived from.

No general derivation language, no DSL. The V1 rule is only: *a derived fact
names its inputs.*

**Naming is necessary and not sufficient.** A citation nobody follows is
satisfied equally well by the right answer and by sources that do not imply it,
so a consumer MUST recompute the fact from the digests it names and refuse a
disagreement. The case this catches is not a wrong value: it is a value that is
*correct* while resting on sources that do not establish it, which reads as
fully provenanced from every angle except the one nobody looked from.

Recomputation requires `(derivation.id, derivation.version)` to resolve to
exactly one function — the same obligation §13.1 places on a matcher, for the
same reason, and discharged the same way. A derivation that cannot read what it
needs yields **no answer**, never `false`: a rule that could not run has
established nothing, and reporting that as the negative outcome is the
absent-signal-as-negative-result error at the smallest possible scale.

## 19. A claim and its verification are different provenance chains

```text
source snapshot        proves what the comment or review SAID
verification evidence  proves what happened when the claim was CHECKED
```

`Verification::Reproduced` must not come to mean "the body contained the word
reproduction". This slice does not design a verification harness, and it must
not let a source-comment digest stand in for one.

```text
verification witness binding = OWED DESIGN ITEM
```

**Consequence, stated normatively rather than left to sequencing.** While that
binding is OWED, a GitHub acquisition adapter MAY emit `Claimed` and MUST NOT
emit `Reproduced`. A falsification found on a GitHub surface is by construction
an unverified claim: a comment cannot verify itself, and the adapter observes
nothing but comments. Any producer of `Reproduced` is blocked on §19 —
acquisition is not, because this restriction removes its ability to produce one.

Without that restriction written down, "verification witness is OWED" would be
discharged by an adapter deciding for itself what `Reproduced` means, which is
the failure §19 exists to prevent.

## 20. Credential and secret boundary

No raw HTTP capture. A snapshot projection must never include:

```text
Authorization headers   cookies        environment
GitHub token            OIDC token     request headers
runner environment      HTTP debug dumps
```

Only allowlisted GitHub content fields.

An uncomfortable truth, stated rather than quietly handled: `body` is untrusted
GitHub content and may already contain a secret somebody pasted into a comment.
This must **not** be solved by silent masking —

```text
mask(body) -> digest
```

— because the digest would then bind to something the classifier never observed.
If a redaction policy is needed, it is a separate normative decision. V1 names
the boundary and does not introduce masking on its own authority.

## 21. What the next slices must do

The order below is derived from the residuals in §23, not chosen for
convenience. An OWED item that no slice is blocked on is decoration; each
precondition here names the slice it actually blocks.

```text
this contract
      ↓
redaction decision (§20)
      ↓         blocks acquisition: §9 retains bodies byte-exact and §11
      ↓         retains the bytes, so acquisition without it stores whatever
      ↓         was pasted into a comment, content-addressed
      ↓
matcher implementation binding (§13.1)          [DONE: o7-closure-matcher,
                                                 incl. §13 schemaVersion 2]
      ↓         blocked the first consumer that APPLIES a matcher, which is
      ↓         the acquisition adapter, because it computes
      ↓         matchedSnapshotDigests
      ↓
classifier provenance binding
      ↓
acquisition adapter
      ↓
attestation envelope

verification witness binding (§19)
            blocks any producer of `Reproduced`. Not on the line above,
            because §19 forbids the acquisition adapter from emitting one.
```

**Redaction decision.** A precondition for acquisition, not for this contract.

**Matcher implementation binding.** ~~§13.1 obliges `matcher.id` +
`matcher.version` to resolve to exactly one predicate and does not say how.~~
**DISCHARGED** by `crates/o7-closure-matcher` — see §23 for the mechanism, for
the false start that preceded it, and for what it still does not cover. Specimen
G's contradiction is now executed rather than read
(`crates/o7-closure-matcher/tests/frozen_specimens.rs`).

This paragraph is a status annotation. No normative clause of §13 or §13.1 is
changed by it, and the binding was built to satisfy them as written.

**Classifier provenance binding.** The merged classifier must learn to carry:

```text
subject read provenance
decision basis
source/query snapshot digest bindings
falsification scan state
```

Only then does the acquisition adapter have a stable consumer contract.

**Acquisition adapter.** It must fetch every required surface, paginate
completely, construct normalized source and query snapshots, **retain the
snapshot bytes**, compute the canonical digest, apply the bound matcher,
construct the decision basis, and pass only typed values to the classifier. Per
§19 it emits `Claimed`, never `Reproduced`.

**Attestation envelope.** It receives an already-stable predicate plus
content-addressed provenance snapshots and merely authenticates them.

**Verification witness binding** is not on the sequence above because nothing on
it emits `Reproduced`. It gates the first slice that does.

## 22. Acceptance criteria

This contract is frozen only if each question below is answerable mechanically
from this document. "The implementation will sort it out" means it is not.

A row answered **OWED** is answered: the document says the question is open and
names where. What is forbidden is a row that reads as settled while §23 records
it as open — necessary conditions presented as sufficient ones.

| question | section |
|---|---|
| What identifies a GitHub object? | §4 locator |
| What identifies the observed version? | §5, §7 snapshot + digest |
| Which bytes are hashed? | §7 canonical projection |
| Where are those bytes retained? | §11 bundle |
| How is canonicalization done? | §7 JCS |
| How is the digest written? | §7 `sha256:[hex]` |
| Which fields per V0 surface? | §8.1–8.5 |
| Which fields are deliberately excluded? | §8, §20 |
| May an adapter add a field it judges relevant? | §8 — no; new `schemaVersion` |
| Is `null` the same as absent? | §8 — yes, both canonicalize to omission |
| How are `head_before` / `head_after` represented? | §8.1 — two `HeadReadEvent`s |
| What if the second head read failed? | §8.1 — `CANNOT_CHECK`, not "not stale" |
| What proves a matcher did not simply miss the object? | §13 candidate set + matcher id |
| What inputs must be retained for matcher re-execution? | §13.1 — id, version, parameters, retained candidates |
| How does id + version resolve to exactly one predicate? | §23 — **DISCHARGED**: const registry in `crates/o7-closure-matcher` |
| What binds that pair to the code that actually ran? | §13.1 — `matcher.implementationDigest`, recorded in the snapshot |
| May a replay run over the candidates it managed to resolve? | §13.1 — no; the complete declared sequence or refusal |
| What happens to a candidate that violates its own §8 schema? | §13.1 — refused as inadmissible, never scored as a non-match |
| What happens to a query snapshot that violates its own §13 shape? | §13.1 — refused; a digest binds bytes, not shape |
| Does a matching digest establish that an object is a query snapshot? | §13.1 — no; a malformed snapshot hashes to its own digest |
| Which `enumeration` values are admissible? | §13 — `COMPLETE`, `INCOMPLETE`; a closed set of two |
| Is an `INCOMPLETE` snapshot malformed? | §13 — no; well-formed, and §14 bars it from authoritative absence |
| Who applies `enumeration = COMPLETE`? | §13.1 — the layer that decides, not the layer that parses |
| What does a snapshot written before that field prove about the implementation? | §13 — nothing; CANNOT_CHECK, not a pass |
| What stops a version's predicate from changing under it? | §23 — a digest over the implementation's bytes, **not** over its results |
| May a matcher read anything else? | §13.1 — no; two inputs only |
| What order do the digest arrays use? | §13.2 — observation order, duplicates kept |
| What does a failed head read record? | §8.1 — `reasonCode` from a closed set, and no `snapshotDigest` |
| What would show an adapter trimming a body? | §9 — an equal-`updatedAt` pair |
| How is a wrong-SHA `OWED` explained? | §17 decision basis |
| How is `NotProduced` proven? | §13, §14 |
| What happens on incomplete pagination? | §14 |
| Producer vs API rate limit? | §15 |
| How is a falsification scan proven to have happened? | §16 |
| What do zero falsifications mean? | §16 |
| How is a derived fact tied to sources? | §18 |
| What binds `Reproduced` to a verification witness? | §19 (OWED) |
| What must never be retained from HTTP/auth? | §20 |
| What must the next classifier slice add? | §21 |
| What must the acquisition adapter do? | §21 |
| May the acquisition adapter emit `Reproduced`? | §19 — no; `Claimed` only |
| Which residual blocks which slice? | §23, and §21's order is read off it |
| What remains OWED? | §23 |
| Where does an expected digest come from? | §17 — the decision basis; never the store that holds the bytes |
| Is a resolver trusted to return the right bytes? | §17 — no; the digest is recomputed and a mismatch refused |
| Is naming a derived fact's inputs sufficient? | §18 — no; it is recomputed from them and disagreement refused |
| What does a derivation that cannot read its inputs return? | §18 — no answer; never `false` |
| Does a blocked field defeat an observation? | redaction §10 — only the decisions that read it |
| Who is authoritative about which assessment permitted a retention? | redaction §9.2 — the retained binding, not the basis asserting one |
| May a store return an artifact containing a digest? | §17 — yes; that is what an evidence chain is |
| When may a reference inside a store-returned value be consumed? | §17 — subject relation, resolution, re-digest, shape, all four |
| Is a resolved scan snapshot sufficient evidence for a scan? | §16, §17 — no; it must also answer THAT scan's query |
| Does a replayed selection need a bound matcher implementation? | §13.1 — yes; `CANNOT_CHECK` on that axis is not a pass |
| Must an absence claim's query be about the decision's observation? | §13, §17.1 — yes; `requiredObservationId` equals the basis's, exactly |
| Must a reduced record and its authorising assessment share a policy version? | redaction §9.5 — yes; a version unrelated to its neighbours is decoration |
| When is a derivation's implementation binding checked? | §18 — before the rule runs, not only in a registry test |
| What does a successful head read carry? | §8.1 — a reference to a retained event, never a bare SHA |
| Does an API-surface test establish the authority law? | §17 — no; it establishes only that the surface did not change unnoticed |
| Is a correctly resolved artifact ready to be consumed? | §17 — no; its own fields must establish the relation it is consumed under |
| Does a head read witness THIS subject? | §8.1 — only if checked against an independently supplied subject identity |
| Which slot may a head-read event fill? | §8.1 — the one its own `role` names |
| Is a scan's own COMPLETE its authority? | §13, §16 — no; the evidencing snapshot's `enumeration` is |
| May any retained record fill the expected-query-snapshot role? | §13 — no; only a `github-query-snapshot` |
| What does a check with no reachable failure establish? | §24.8 — nothing; it is removed, not witnessed |
| May a raw resolved object reach any semantic path? | §17.2 — no; the door's type is the only admissible argument |
| Where is closed-form validation performed? | §17.2 — once, before role, authority and every relation check |
| Is `schemaVersion` typed or checked? | §17.2 — checked against the registered versions of that kind |
| What refuses a malformed reduced record? | §17.2 — a malformed-artifact refusal, never a blocked pointer |
| Does the subject read need retention authority? | §5.3, §9.2 — yes; it is a gated kind, and the classification is what applies it |
| Does `github-head-read-event` carry `schemaVersion` and `sourceKind`? | §8.1 — yes, listed outright in both the AVAILABLE and the FAILED block |
| Must an absence claim's query be about the decision's SUBJECT? | §13, §17.1 — yes; the snapshot's `binding` is compared against a subject supplied from outside it, exactly as a scan's is |
| May two artifacts of one surface jointly satisfy §17's rows? | §17, §17.1 — no; the table is a minimum basis PER OBSERVATION, and one observation is one artifact |
| May a reduced record withhold a field the computation retained? | redaction §7.1 — no; `blockedFields = flagged ∪ (required \ assessed)` is an equality, and retention is not discretionary in the other direction either |

## 23. Residuals — OWED, not decided here

Each residual below names **what it blocks**. An OWED item nothing is blocked on
is a decorative grave for a requirement, and §21's sequence is derived from these
statements rather than written alongside them.

- **Redaction policy** for secrets pasted into untrusted bodies (§20). Naming
  the boundary is not solving it.

  *Blocks the acquisition adapter.* §9 requires bodies retained byte-exact and
  §11 requires the bytes kept, so an adapter built first would implement careful
  immutable storage for a credential somebody pasted into a comment — and
  content addressing makes that hard to take back. A precondition for
  acquisition, not for this contract.

- **Verification witness binding** (§19). No form is specified. A source digest
  must not be substituted for one.

  *Blocks any producer of `Reproduced` — which is not the acquisition adapter.*
  An earlier revision grouped this with redaction as jointly blocking
  acquisition, but gave only the redaction argument and never an argument for
  this one. Correcting the grouping rather than padding the sequence: §19 now
  states normatively that a GitHub acquisition adapter emits `Claimed` and never
  `Reproduced`, because a comment cannot verify itself. That restriction is what
  makes acquisition unblocked here; without it, "OWED" would be discharged by
  the adapter deciding for itself what `Reproduced` means.
- **Semantic normalization** of bodies (§9). V1 is byte-exact; any
  whitespace-insensitive comparison is a later, separately versioned decision.
- ~~**Matcher implementation binding** (§13.1).~~ **DISCHARGED** by
  `crates/o7-closure-matcher`. The resolution mechanism is a flat const registry
  — an identity pair resolves to one `fn`, and resolution fails closed on the id
  and on the version separately. The *immutability* half, which is the half
  §13.1 actually turns on, is a SHA-256 over the exact bytes of the file that
  defines the predicate: each version's predicate lives alone in
  `src/matchers/<id>_v<n>.rs`, the registry embeds that file verbatim at compile
  time, and the same path is what the compiler builds — so the hashed bytes are
  the running code. Editing a version's file breaks that version's binding;
  changing behaviour means adding `_v2.rs` and a new entry.

  That digest is recorded in the durable artifact, not only in the registry:
  `github-query-snapshot` `schemaVersion` 2 carries
  `matcher.implementationDigest` (§13, §13.1), so a replay compares the running
  code against a record written at acquisition time rather than against a
  constant beside the code. Version-1 snapshots yield CANNOT_CHECK on that axis.

  The original residual text is kept below because it stated the requirement
  correctly and the first attempt at satisfying it did not.

  > The contract obliges `matcher.id` + `matcher.version` to resolve to exactly
  > one predicate, and says nothing about *how* that resolution happens — a
  > registry file, a crate path plus a version, a digest over the
  > implementation. Until it is decided, a matcher named only in prose is a
  > locator pointing at something mutable, which is what §3 objects to
  > everywhere else.
  >
  > *Blocks the first consumer that applies a matcher*, which is the acquisition
  > adapter, since it computes `matchedSnapshotDigests`.

  **The false start is part of the record.** The binding first shipped with a
  digest over the predicate's *results on a frozen vector set* and was annotated
  as discharging §13.1. It did not. §13.1 says `version` changes whenever
  behaviour changes for **ANY** input, and a finite vector set cannot discharge
  `ANY`: a change gated on a `state` value no vector used passed the entire
  suite under an unchanged version and an unchanged digest. That escape is
  recorded as an executable commit (RED-2) rather than as a sentence here, and
  the conformance digest is now labelled what it always was — a behavioural
  regression witness, kept because the bytes never state what the rule is
  *supposed* to do.

  **The second false start is also part of the record.** The bytes digest first
  shipped with its expected value as a constant in `src/matchers.rs`, two lines
  from the `include_str!` that supplied the bytes it judged, and was annotated
  "append-only, enforced by the digest rather than by policy". It was not. One
  commit edits both fields, and a matcher that had changed behaviour under an
  unchanged version passed the entire suite — recorded as an executable commit
  (RED-3) rather than as a sentence here. The correction was not a better digest
  but a different **authority**: the expected value now lives in the snapshot.
  The pattern across both false starts is the same one this document names
  everywhere else — an artifact certifying the very thing it is checked against —
  and it survived two rounds by moving up a level each time rather than by being
  wrong in a new way.

  **What remains uncovered, and is not being called discharged.**

  1. A behaviour change that leaves the bytes alone: a dependency's semantics
     shifting beneath them, a compiler change, a target difference. Identical
     bytes are not identical behaviour across a moving substrate. The conformance
     vectors are the witness for that residual and are the reason both digests
     are kept; neither subsumes the other.
  2. **The authority of an expected query digest.** NARROWED, not closed, by
     `crates/o7-closure-provenance`. The expected digest now enters replay from
     the frozen decision basis and the retained-evidence store resolves it
     without ever supplying it, so the specific escape this item was written
     about — a store handing out both an artifact and the certificate for it —
     is closed by an interface that cannot express it (§17).

     What remains is one step further out and is genuinely still open: the
     decision basis is itself an artifact somebody wrote. A basis naming a
     forged digest, over a store holding the matching forgery, is self-consistent
     and admissible exactly as before. The regress has moved rather than ended,
     and it ends where it always was going to — at an attestation subject, which
     is Slice D's, or at a bundle whose provenance is signed rather than
     asserted. Recorded here rather than in §13.1 because it is a residual, not a
     rule, and because the previous annotations in this document were withdrawn
     for claiming exactly this kind of thing one level too early.
  3. An author who edits the implementation, the registry constant and the
     recorded digest in one commit. Nothing inside a single repository prevents
     this, and claiming otherwise is how the previous two annotations went wrong.
     What the mechanism buys is that drift stops being a local edit: the record
     is a fixture whose digest comes from an independent canonicalizer, and an
     artifact **already emitted** — an attestation, a snapshot handed to someone
     — is outside the tree entirely and cannot be re-blessed at all. The binding
     that holds against intent begins when artifacts leave; specimen I stands in
     for that case until they do.
- **Reaction surface** (§8). Still no Step 0B specimen; not added here.
- **Pagination specimen** (§14). The rule is frozen; the historical witness does
  not exist and the contract vectors for it are synthetic.
- **Digest constants** for any contract specimen are OWED wherever no independent
  JCS + SHA-256 implementation is available to compute them. A self-written
  canonicalizer must not produce the constants that would later be used to
  validate that same canonicalizer.

## 24. New normative decisions

These are decided **here**, not in #147, and are presented as additions rather
than as pre-existing rules: §3 the binding law; §5 digest-only rejected; §6 raw
HTTP hashing rejected; §7 JCS + SHA-256 + `sha256:` syntax + string ids; §8 the
per-surface allowlists; §9 byte-exact bodies; §10 `updated_at` is not identity;
§11 the retained content-addressed bundle; §12 the bundle carries no verdicts;
§13 query snapshots for authoritative absence; §14 complete-enumeration
precondition for `NotProduced`; §15 the two rate-limit layers; §16 falsification
scan provenance at zero claims; §17 decision basis; §18 derived facts name their
inputs; §19 verification witness kept separate; §20 the secret boundary.

### 24.1 Added in the review correction round

Four decisions added after the first specimens were reviewed. None loosens a
rule; each closes a way the earlier text could be satisfied without the property
it was written to guarantee.

- **§8 — the projection schema is closed.** Exact REQUIRED and
  OPTIONAL-IF-PRESENT sets per `sourceKind`, no third category, extension only by
  `schemaVersion`. The removed *where closure-relevant* / *plus any further
  closure-relevant field* wording delegated the projection to the adapter.
- **§8 — `null` and absent are one input** and canonicalize to omission, so two
  adapters observing the same fact cannot compute two digests.
- **§8.1 — two head reads are two `HeadReadEvent`s.** Two pointers at one
  snapshot record one read. A FAILED HEAD_AFTER yields `CANNOT_CHECK`, never a
  silent absence of `STALE`. *(This round required `role`, `snapshotDigest`,
  `acquisition` and `observedAt` on every event; §24.2 replaced that with the
  tagged shapes, and §8.1 is authoritative.)*
- **§13 — authoritative absence requires the retained candidate set and a named
  matcher.** `allReturnedSnapshotDigests` and matcher identity became REQUIRED;
  `NotProduced` is legal only when a COMPLETE enumeration *and* the identified
  matcher over the retained candidates yield an empty matched subsequence.
  Matched lists are digests, not ids. *(This round named the matcher by
  `id`/`version` only; §24.2 added `parameters`, and §13.1 is authoritative.)*

The corresponding conformance witness, added in §9, is the equal-`updatedAt`
body pair: a witness that varies the body together with another field cannot
distinguish byte-exact retention from `trim()`.

### 24.2 Added in the second review correction round

- **§7 — the example carries `user.type`.** Correction-1 made the example a
  complete §8.5 projection; correction-2 then added `user.type` to the allowlist
  and left the example behind, so the contract contradicted itself in the same
  way a second time.
- **§8.1 — `HeadReadEvent` is tagged by acquisition status.** `AVAILABLE` carries
  `snapshotDigest`; `FAILED` carries `reason` and MUST NOT carry one. Requiring a
  digest unconditionally forced the adapter to supply a stale or fabricated one
  for a read that produced nothing. `NOT_PRODUCED` and producer rate-limiting are
  inadmissible on a head read; an API rate limit is `FAILED`.
- **§13.1 — `matcher.parameters` is REQUIRED**, with the matcher defined as a
  deterministic pure predicate over exactly two inputs, restricted to
  per-candidate decisions, and with an explicit obligation that the matched
  subsequence be recomputable. Correction-2 required retaining the objects the
  function ran over; it did not require retaining the function's other argument,
  so provenance could reproduce the input and still not the decision.
- **§13.2 — the digest arrays are ordered sequences.** Observation order, matched
  as a subsequence, duplicates retained. JCS does not sort arrays, so without
  this two conforming adapters could compute different query digests from one
  observation.

### 24.3 Added in the third review correction round

A consistency pass. It introduces one norm; the rest aligns statements the
document was already making about itself.

- **§19 — a GitHub acquisition adapter emits `Claimed` and never `Reproduced`.**
  The one new norm, and the reason the sequencing correction below is sound
  rather than convenient: it is what actually unblocks acquisition, instead of
  the residual quietly dropping out of the order.
- **§21 / §23 — preconditions and slice order are derived from each other.**
  Every residual now names what it blocks; the sequence is read off those
  statements. Matcher implementation binding enters the order ahead of
  acquisition, since the adapter computes `matchedSnapshotDigests` and would
  otherwise choose its own matcher — §13.1's prohibition defeated by a missing
  arrow rather than a missing rule. Verification-witness binding leaves the
  linear order and becomes a gate on any producer of `Reproduced`; an earlier
  revision asserted it blocked acquisition while giving only the redaction
  argument for the claim.
- **§22 — the matcher acceptance row splits in two.** Retained inputs are
  answered; how `id` + `version` resolve to one predicate is OWED. One row was
  presenting necessary conditions as sufficient ones while §23 recorded the
  question as open.
- **§24.1 — the stale `HeadReadEvent` summary is corrected.** It still described
  the untagged shape that §24.2 replaced. This is the third instance of the same
  defect class in this document: the normative text is fixed and a downstream
  summary keeps living in the previous version. Each cross-reference now points
  at the authoritative section rather than restating it.

### 24.4 Added in the implementation-binding correction round

This round is unusual: it corrects the document **after** the slice that was
supposed to discharge one of its residuals, because two successive attempts at
that discharge were annotated as complete while each was still short of what
§13.1 requires. Both attempts failed the same way, one level apart.

- **§13 — `github-query-snapshot` `schemaVersion` 2 adds
  `matcher.implementationDigest`.** The two shapes are closed and neither may
  borrow from the other. Version-1 snapshots stay valid and yield CANNOT_CHECK
  on the implementation axis; specimens C, D and G are untouched and are the
  witness for that case.
- **§13.1 — `version` and `implementationDigest` are separate obligations.**
  `version` is a semantic name for the rule intended; `implementationDigest` is
  the replay binding for the code that carried it out. The second does not follow
  from the first, because `version` is a string an implementer chooses and `ANY
  input` is not provable by any finite check that implementer can run.
  Additionally: the digest MUST be over the implementation, not over a sample of
  its behaviour; and the expected value MUST be recoverable from something other
  than the tree holding the implementation.
- **§22 — two rows added.** What binds the identity pair to the code that ran,
  and what a pre-field snapshot proves about it. The existing row answered
  resolution and was, once again, being read as answering immutability.
- **§23 — the residual keeps its full history.** Both false starts are recorded
  with the executable commits that demonstrate them (RED-2, RED-3), and the
  uncovered part is now enumerated as two distinct items rather than one, since
  a moving substrate and a determined author are different gaps with different
  answers. The word "append-only" is withdrawn.

The generalisation worth keeping. Every round of this document has found the
same defect wearing a different hat — an artifact certifying the very thing it
is being checked against — and this round found it twice more in the code that
was written to fix it. The pattern is not carelessness about hashing; it is that
each fix moves the certificate closer to the thing certified and then stops,
because at that distance the two now agree. The question that actually
discriminates is not *what does the digest cover* but *who wrote the expected
value, and when*. A check whose expectation was authored by the same act as its
subject is a consistency check, whatever it is hashing.

### 24.5 Added in the admissible-input correction round

Two defects found in external review of the implementation slice, both of which
the document already implied and neither of which it said outright. They are the
same rule at two layers, and the rule is older than this document — it is the
line in `AGENTS.md` about missing evidence not being a passed check.

- **§13.1 — replay is bound to the complete declared candidate sequence.** A
  prefix, a subset or a reordering is refused rather than replayed over what is
  present. The failure this prevents is specific: a bundle that resolves to
  nothing reproduces an empty absence claim, because an empty recomputation
  agrees with an empty claim. Nothing about that outcome looks wrong from the
  inside.
- **§13.1 — a candidate that violates its own §8 schema is inadmissible.** It is
  refused, never scored. Scoring it converts an unreadable snapshot into a
  candidate that did not qualify, which is how an authoritative absence gets
  built out of broken evidence. The digest binding is no defence here: a
  truncated projection hashes to its own digest correctly.
- **§13.1 — the schema precondition is placed outside the predicate**, with the
  reason recorded, because the placement is load-bearing rather than stylistic:
  inside `f` it would make every schema correction a matcher behaviour change
  under §13.1's own versioning rule.
- **§13.1 — conformance means the whole closed shape**, stated explicitly after
  the first implementation of the rule above checked only the fields the matcher
  reads. The document said "a field that kind requires" and the code said "a
  field this matcher reads", and both were written in the same push, so nothing
  compared them. `crates/o7-closure-matcher/tests/schema_parity.rs` now parses
  §8.2 and §8.3 out of this document and fails if the two ever disagree again —
  the expectation is the contract, not a second copy of the key set.
- **§13.1 — §7's universal members are checked on every candidate**, so an object
  declaring no `sourceKind` is refused rather than routed down the
  delivery-surface path as though it were a different surface.
- **§13.1 — the claim is a field of the recorded matcher**, not an argument to
  the verifier. This is the fourth field to move from "supplied by the caller" to
  "read from the artifact", after the implementation digest, the parameters and
  the candidate sequence. The generalisation is now explicit and worth applying
  ahead of the next reviewer: **nothing being checked may arrive from the party
  being checked.** Each of these was found separately, by three reviewers across
  five rounds, and each time the fix was applied to the one field named rather
  than to the class — which is why there were five rounds.
- **§13.1 — the query snapshot is bound to a digest supplied from outside.** The
  ninth instance, and the first to point outward rather than inward: private
  fields stopped a caller assembling a record, and left them free to mutate a
  retained snapshot and parse the result. The fix completes the symmetry with
  candidates and moves the trust boundary down one level; it does not close the
  regress, and §23 carries what remains. Stated with the layer split so the
  mechanical check is not deferred to the producer that computes the digest.
- **§13.1 — the recorded values are immutable after parsing.** The eighth
  instance, and the one that shows the previous seven were all fixed at the wrong
  level: each moved a value from *argument* to *field* while leaving the field
  assignable, so "read from the artifact" was a convention the API invited
  callers to break. One constructor, no public fields. A test asserts both
  against the source, because the compiler enforcing it today is not evidence
  that a later `pub` would be noticed.
- **§13.1 — conformance is judged against the declared kind.** The seventh
  instance, found by a reviewer reading a test rather than the code: the
  "legitimate foreign-surface candidate" in `admissible_input.rs` was itself
  malformed — a `github-issue-comment` with no `user.login` — and the consumer
  accepted it because it was not the matcher's kind. The test that existed to
  show a correct non-match was the demonstration of the bypass. All five §8
  shapes are now registered and every candidate is validated against its own.
- **§13.1 — an unregistered candidate `schemaVersion` is refused.** The sixth
  instance, and the one that does not fit the sentence above, so it earns its own:
  *validating a field's type is not validating the value admissibility turns on.*
  `schemaVersion` was checked as an integer, which is exactly the check that
  cannot tell a V1 projection from a V2 one.

Both were caught by review of the implementation, not of the contract, which is
worth recording as a fact about where these defects live. §13's conformance
obligation was stated correctly and completely; the two ways to satisfy its
letter while defeating it were both in the layer that executes it. A contract
that is right is not the same as a consumer that is right, and only one of them
can be checked by reading.

### 24.6 Added in the query-snapshot conformance round

One defect, found in external review of the implementation slice. The tenth
instance of the pattern this document has been chasing since §24.1, and the first
to be found in the *fix for the ninth* — the round before this one bound the query
snapshot's bytes to a digest, and stopped there.

- **§13.1 — a digest-bound query snapshot is not a checked one.** §13 lists
  seventeen REQUIRED members and the parser read seven. The other ten were free
  to be absent, because the binding added last round establishes that these bytes
  are the ones the expected digest names and says nothing whatever about their
  shape: a malformed snapshot hashes to its own digest exactly as a well-formed
  one does. Two consequences were reachable and neither is cosmetic. Without
  `sourceKind`, any canonical object carrying a matcher block parses as a query
  snapshot. Without `enumeration`, an absence claim can be assembled from a
  snapshot that never declared its enumeration finished — the one precondition
  §13 exists to impose.

The rule, stated rather than the instance:

```text
checking several significant members of an object is not checking the object,
  when the contract defines admissibility by the whole closed form
```

**A residual was reclassified rather than carried.** "An unknown key in the
matcher block is accepted" was recorded as a cosmetic P2 and deferred to the next
slice. That was wrong, and the way it was wrong is worth keeping: a closed shape
has two sides, and only the superset side had been looked at. The subset side —
a REQUIRED member simply absent — turned out to be a semantic escape rather than
an untidiness. A validator that closes one side of a closed shape closes neither,
so both are closed here and the P2 is discharged rather than inherited.

**The layer split was held deliberately, and it is the reason this round did not
grow.** Slice A now establishes two things about a query snapshot and exactly
two: that the bytes are digest-bound, and that they are a conforming versioned
`github-query-snapshot`. That `enumeration = COMPLETE` plus an empty reproduced
selection makes `NotProduced` legal is classifier admissibility and stays with
the layer that decides. The temptation was to require `COMPLETE` at construction
while the validator was open on the desk; doing so would have made specimen D
unrepresentable and destroyed the very distinction §13 was written to create —
the matcher crate would have become a second classifier while fixing a schema
bug. `incomplete_enumeration_is_well_formed_and_makes_no_claim` is the executable
form of that boundary.

**Where the expectation lives.** §13's member table and its enumeration states
are now parsed out of this document by `tests/schema_parity.rs` and compared
against the registered shapes, including the `(schemaVersion 2 only)` annotation,
which is read rather than hardcoded. This matters more here than it did for §8:
the enumeration value set was the one thing in this fix that the contract had not
previously stated, so writing it only into the code would have made the code the
authority on what §13 permits. Both directions were checked by mutation — editing
the table fails the parity test, and so does editing the document.

### 24.7 Added in the store-authority correction round

The first paired external round on the classifier provenance slice returned six
findings from two reviewers, both terminal. Four were one law at four surfaces;
two were about the evidence rather than the mechanism, and those two are the more
useful ones to have on the record.

- **§17 — the "no digest may leave the store" rule is WITHDRAWN as wrong.** Not
  strengthened. A store returns bindings that name assessments and events that
  name snapshots, so the rule as written forbade the evidence chain it was meant
  to protect. What replaces it is the four-clause authority rule above, which
  permits references to be returned and forbids them to become authoritative by
  being returned.

- **A green witness is not evidence of the property it names.** The withdrawn
  rule had a test. The test searched the interface's source for the substrings
  `-> String` and `-> Digest`; the interface returned `Option<RetentionBinding>`,
  whose two digest fields the caller reads. So the property was already absent
  when the witness was written, the witness was green, and a commit message
  asserted the property as established fact. Three of the six findings were
  instances of exactly what that witness claimed to prevent.

  This is worth stating as a general result rather than as an incident: a witness
  can pass while the property it is named for does not hold, and the failure mode
  is invisible from inside because the witness is doing what it says. The
  countermeasure is not a better structural test. It is knowing which tests are
  behavioural — those carry the law — and which merely guard a surface, and never
  writing the second kind's documentation as though it were the first.

- **A justification comment can describe code that does not exist.** A
  restriction-lint allowance was added over a file with no `unwrap`, `expect` or
  `panic` site, carrying a comment explaining why "every panic path below" was
  sound. It was written by copying the shape of the two files beside it. A false
  comment at a provenance boundary is worse than no comment: the next reader
  believes a check was considered. The rule now has a test — an allowance must
  suppress something — rather than a convention.

**Preregistration did not prevent any of this, and that is not an argument
against it.** The escape set for the first round was frozen before the
implementation, which is what stopped the tests being shaped to the code after
the fact. It did nothing about the set being incomplete: it made the store
untrusted for record bytes and the basis untrusted for its assertions, and left
the store trusted for everything else it returns. The same argument this document
already makes about conformance vectors — no finite set discharges "any input" —
applies to escape sets, and applies to the ones written in good faith by whoever
is about to be wrong.

**Mutation testing found a gap the reviewers did not.** Deleting the
`acquisition == AVAILABLE` check on a head-read event failed no test, because the
only malformed-event case covered was a missing `snapshotDigest`, which the next
check caught anyway. A relation with no witness is a relation nobody is holding,
and the only way to find that is to break the check and watch nothing complain.


### 24.8 Added in the relation-binding correction round

Five findings, one law. The previous round made the store untrusted for the bytes
it returns; this one makes it untrusted for what those bytes are **about**. §17.1
above is the normative statement.

**The findings were not five defects.** Each was the same sentence at a different
surface: *an artifact was checked for what it IS and never for what it is
ABOUT.* A conforming retention assessment behind a record it refuses, a §7.1
partition read off the record instead of recomputed from that assessment, a scan
whose evidence contradicts its own completeness claim, a head-read event in the
wrong slot, and a pair of head reads of another pull request. Correcting them as
five fixes would have produced five checks and no rule.

**Checking several significant members is not checking the object** — already
§13.1's rule for query snapshots — extends to every closed form this document
and the redaction policy define. The assessment checker probed ten pointers and
one enumeration; §9.4's whole argument is that the *closure* is the security
property, because an open schema lets a producer add a member holding the content
the gate refused and satisfy every rule anybody enumerated.

**Two checks were removed for having no reachable failure.** Mutation testing
found them: delete the rule, re-run, watch nothing complain. Both were genuinely
subsumed — the distinctness of the two head-read events by the per-slot role
check, and one of the redaction policy's finding rules by its two partition
rules. Each was replaced by the derivation, written where the check had been.

This is the mirror of the previous round's result and belongs beside it. That
round found *a witness green over a property that does not hold*. This one found
*a check that cannot fail*, which is the same defect approached from the code
rather than the test: neither can distinguish a conformant artifact from a
non-conformant one, and both read as coverage.

**Mutation testing found five rules with no evidence, and one rule missing
entirely.** Three of the five had real content and fixtures that merely tripped
an earlier check first — a green suite over a masked rule is the same false
comfort as a green witness over an absent one. The missing rule was
`findings: []` under `BLOCK_SECRET`: every presence rule satisfied, both
conditionals correct, and the record claiming at once that something was found
and that nothing was.

**One transcription, not two.** The §9 member tables and §5.3 field sets the
consumer ranges over are parsed out of the redaction policy and compared against
it, because §5.2 makes that denominator normative precisely so a consumer cannot
take it from the producer — and a consumer taking it from a stale copy of the
document has reintroduced the same problem with a slower clock. Verified by
drifting a pointer, a member name and a vocabulary value in turn.

**A contract-level asymmetry surfaced from implementing it.** §5.3's pointers are
into the decoded source object and §8's projections carry canonical members, so a
reduced record and a complete projection are keyed in different vocabularies. The
fixtures had been mixing them since the reduced record was first written, and
every check that existed accepted it. Recorded as redaction policy §7.5, with the
asymmetry itself as a residual there.


### 24.9 Added in the choke-point round

The fourth round is the first that was not a list of escapes. The owner's ruling
on the third read, in substance: the seven findings deduplicate to three, two of
them one architectural defect, and the answer is not a fourth patch round.

**What RED-B4 measured, before anything was fixed.** Every artifact kind this
crate can admit, crossed with one generated adversarial family — unknown
top-level member, unknown nested member, each required member removed in turn,
wrong member type, wrong `schemaVersion`, wrong `sourceKind` or role:

```text
github-review-comment                       22 malformed mutants admitted
github-submitted-review                     16
github-query-snapshot   expected-query      15
github-issue-comment                        14
github-pull-request-head  gated source      12
github-actions-check                        10
github-query-snapshot   scan evidence       10
github-head-read-event                       8
github-reduced-source-record                 7
github-pull-request-head  subject read       7
closure-retention-assessment                 1
                                           ---
                                           122
```

The last row is the diagnosis as a number. The previous round applied clause 1
to the assessment and to nothing else, so the assessment admitted one malformed
mutant and every other artifact admitted between seven and twenty-two. The
finding was not "a check was missed" but "whole-object validation was made a
special case of one artifact".

**Generated rather than enumerated, and that is the methodological point.** A
hand-written list of adversarial cases is a list of what somebody thought of,
which is the failure being corrected. A table of kinds crossed with a table of
mutations has no such ceiling: adding a kind adds its whole family, and a kind
never added is visibly absent from one list rather than invisibly absent from
sixty. The size of that surface is itself asserted, because a suite whose
families generate nothing has no escapes either.

**The acceptance criterion was not "the witnesses pass".** It was that removing
the door turns them red. Each artifact kind's dispatch was bypassed in turn and
the suite re-run; every bypass was caught, as was removing the whole closed-form
call, flattening the gate classification to ungated, and accepting any kind in
any role. A choke point nothing proves is a choke point is a new sign over the
old corridor.

**One transcription, still.** The five §8 projections and the §13 query snapshot
are read from the matcher crate's tables — the ones its own parity test already
checks against this document — rather than transcribed a second time. Only the
forms that crate has no reason to define were added, and each is parity-checked
against its own contract. Runtime completeness bought with a fresh copy of a
normative schema would trade one provenance defect for another.

**One contract reading was made explicit, and one was retired by amending the
document instead.** §8.1's `HeadReadEvent` blocks used to name `role`,
`acquisition`, `observedAt` and the member distinguishing the two shapes, and not
to repeat `schemaVersion` and `sourceKind`; a consumer supplied those two on §7's
universal authority, and the parity test supplied them alongside the parsed
members. That was a reading, recorded as one so it could be revisited.

It was revisited. §8.1 now lists both members outright in both blocks and
declares `sourceKind: github-head-read-event`, so nothing is added on the
consumer's side and `contract_parity.rs` is a plain transcription check again.
The reading is gone because the document says it, not because anybody stopped
relying on it.

And §8.1's "MUST BE ABSENT" for a failed read's `snapshotDigest` is enforced by
the closed key set rather than by a
second rule saying so. Both are asserted by the parity test, so a later revision
of §8.1 that contradicts either fails the build instead of outliving it.

**Fixtures had been non-conformant since they were written.** The review-comment
specimen carried none of §8.4's REQUIRED `commitId`, `originalCommitId` or
`path`, and a reduced record for a check run carried a `pullRequest` its §7.3
locator does not have. Every check that existed accepted both. That is the same
defect one level down from the one the round is about, and it is worth recording
that the door found it rather than a reviewer.

### 24.10 Added in the free-text-channel round

One member of §8.1 was declared and never given a domain, and the omission
outlived the decision that should have caught it.

**What §9.4 settled, and where it was not applied.** Redaction policy V1 §9.4
removed `reasonDetail` from the assessment and closed that schema, on an argument
about value SETS rather than values: "a closed field cannot carry a secret out
because its range does not depend on the content inspected". §8.1's failed head
read carries a `reason`, written by an acquisition layer holding an HTTP response
and an authorization header — the most natural place in this contract for a
credential to land — and it was not reconsidered when §9.4 was decided.

`crates/o7-closure-provenance`'s own evidence recorded the gap and declined to
close it, correctly:

> free text in a retained object. §9.4 removed free text from findings
> deliberately, and this member was not reconsidered when that was decided.
> Narrower than the finding case — an acquisition layer writes it, not a detector
> over secret-bearing content — but nothing here bounds what it may carry. NOT
> closed by this round: a closed reason set is a contract change this file does
> not make on its own.

That last sentence is the rule this document depends on. A consumer that refused
free text where this contract permits it would be inventing a norm, which is the
direction §8.1 already refused once for `observedAt`. So the residual stood until
external review reported the channel as reachable, and the document moved.

**The member is now `reasonCode`**, over a closed four-value vocabulary, in the
same shape redaction §9.3 uses for its own closed sets. The rename is deliberate:
`reason` names a sentence and `reasonCode` names a value from a set, and a
consumer reading the old name against the new rule would be the ambiguity this
amendment exists to remove.

**What the amendment does not decide.** It closes the retained artifact's member.
It says nothing about diagnostics an acquisition layer keeps in its own logs,
which are outside this contract entirely, and it does not claim that a closed
code is as useful to a human debugging an outage as a sentence was. That is the
trade §9.4 already made once, with the same reasoning and the same cost.
