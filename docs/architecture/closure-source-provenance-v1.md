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
changes no classifier schema. `snapshotDigest` is the one field §21 hands to the
next slice to add.

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
                     allReturnedSnapshotDigests
                     matchedSnapshotDigests
OPTIONAL-IF-PRESENT  incompleteReason  binding.sha
```

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
```

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

**Next: classifier provenance binding** (a separate slice, after this contract
merges). The merged classifier must learn to carry:

```text
subject read provenance
decision basis
source/query snapshot digest bindings
falsification scan state
```

Only then does the acquisition adapter have a stable consumer contract.

**Then: acquisition adapter.** It must fetch every required surface, paginate
completely, construct normalized source and query snapshots, **retain the
snapshot bytes**, compute the canonical digest, construct the decision basis, and
pass only typed values to the classifier.

**Then: attestation envelope.** It receives an already-stable predicate plus
content-addressed provenance snapshots and merely authenticates them.

## 22. Acceptance criteria

This contract is frozen only if each question below is answerable mechanically
from this document. "The implementation will sort it out" means it is not.

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
| How is the matcher re-executed later? | §13.1 — id, version **and** parameters |
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
| What remains OWED? | §23 |

## 23. Residuals — OWED, not decided here

- **Verification witness binding** (§19). No form is specified. A source digest
  must not be substituted for one.
- **Redaction policy** for secrets pasted into untrusted bodies (§20). Naming
  the boundary is not solving it.

  These two are OWED, and that is compatible with freezing this contract. It is
  **not** compatible with starting the acquisition adapter. §9 requires bodies
  retained byte-exact and §11 requires the bytes kept; an adapter built before a
  redaction decision would therefore implement careful immutable storage for a
  credential somebody pasted into a comment, and content addressing makes that
  hard to take back. The redaction decision is a precondition for acquisition,
  not for the contract.
- **Semantic normalization** of bodies (§9). V1 is byte-exact; any
  whitespace-insensitive comparison is a later, separately versioned decision.
- **Matcher implementation registry** (§13.1). The contract obliges
  `matcher.id` + `matcher.version` to resolve to exactly one predicate, and says
  nothing about *how* that resolution happens — a registry file, a crate path
  plus a version, a digest over the implementation. Until it is decided, a
  matcher named only in prose is a locator pointing at something mutable, which
  is what §3 objects to everywhere else. The specimens here name such a matcher
  and say so rather than implying the binding already exists.
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
- **§8.1 — two head reads are two `HeadReadEvent`s**, each with `role`,
  `snapshotDigest`, `acquisition` and `observedAt`. Two pointers at one snapshot
  record one read. A FAILED HEAD_AFTER yields `CANNOT_CHECK`, never a silent
  absence of `STALE`.
- **§13 — authoritative absence requires the retained candidate set and a named
  matcher.** `allReturnedSnapshotDigests` and `matcher.id`/`matcher.version` are
  REQUIRED; `NotProduced` is legal only when a COMPLETE enumeration *and* the
  identified matcher over the retained candidates yield an empty matched subset.
  Matched sets are digests, not ids.

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
