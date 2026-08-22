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
acquisition status** — a read that did not happen has no bytes to point at:

```text
HeadReadEvent, acquisition = AVAILABLE
  role            HEAD_BEFORE | HEAD_AFTER
  acquisition     AVAILABLE
  snapshotDigest  REQUIRED
  observedAt

HeadReadEvent, acquisition = FAILED
  role            HEAD_BEFORE | HEAD_AFTER
  acquisition     FAILED
  reason          REQUIRED
  observedAt
  snapshotDigest  MUST BE ABSENT
```

Requiring `snapshotDigest` on every event, as an earlier revision did, forces the
adapter to invent one for a read that produced nothing — and the only digests
available to invent are a stale one or a fabricated one. Both make a failed read
look like a successful read of unchanged bytes, which is the exact confusion this
event was introduced to prevent.

`NOT_PRODUCED` and producer rate-limiting are **inadmissible** on a head read.
The subject head is not produced by any external party; nobody can decline to
emit it. An API rate limit is an acquisition failure and is recorded as `FAILED`
with the reason naming the limit — §15's distinction applies here in only one of
its two directions.

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
subject       head_before, head_after
falsification subject_sha (if any), verification status
```

Without this, source bytes can be retained perfectly while an adapter bug stays
invisible.

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
| What does a snapshot written before that field prove about the implementation? | §13 — nothing; CANNOT_CHECK, not a pass |
| What stops a version's predicate from changing under it? | §23 — a digest over the implementation's bytes, **not** over its results |
| May a matcher read anything else? | §13.1 — no; two inputs only |
| What order do the digest arrays use? | §13.2 — observation order, duplicates kept |
| What does a failed head read record? | §8.1 — `reason`, and no `snapshotDigest` |
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
  2. An author who edits the implementation, the registry constant and the
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
