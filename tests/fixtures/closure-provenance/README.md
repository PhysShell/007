# Closure provenance contract specimens (V1)

These are **synthetic contract specimens** for
`docs/architecture/closure-source-provenance-v1.md`.

**These do not extend the Step 0B historical corpus. They witness the new
provenance-binding contract only.**

## What these are, and are not

| | |
|---|---|
| origin | authored for this contract; **no historical origin** |
| relation to `tests/fixtures/github/` | none — that corpus is frozen historical evidence and is untouched here |
| relation to PR #145 | none; nothing here reconstructs anything observed there |
| ids, SHAs, logins, timestamps | invented, deliberately outside any plausible real range |
| consumer | **none in this change.** No test, no crate, no CI job reads these files |
| classifier outcomes | **absent by design** — see below |

Every file carries `"synthetic": true`. Nothing here should ever be cited as a
record of something that happened.

### No expected classifier outputs

Following the Step 0B rule, *specimen purpose is not expected classifier
output*. No file states `PASS`, `FINDING`, `OWED`, `CANNOT_CHECK` or `STALE`,
and none states a headline. A specimen records what the acquisition layer
observed and which contract clause it exercises. What that implies about a
closure state is the contract's job (§13–§17), not the specimen's — otherwise a
later test confirms its own crib sheet, which is exactly what §12 forbids.

The `witnesses` array in each file lists section numbers of the contract
document, unprefixed (`"8.1"` means §8.1).

### Review correction rounds

**Round 1.** Specimens F and G were added, and A, C, D and E revised, after
review found four ways a specimen or a projection could be satisfied without the
property it was written to establish. Every canonical object here conforms to
§8's **closed** projection schema — which is why `user.type` appears throughout
and every digest constant was recomputed. The defects are named in place: see A's
`notAWitnessFor`, F's `why`, G's `why`, and E's `subjectRead.note`.

**Round 2.** Specimen H was added and the three query snapshots gained
`matcher.parameters`, so the query digests for C, D and G were recomputed; the
candidate review digests are unchanged, since a candidate snapshot does not
contain the matcher. F was already correct and is untouched. The finding behind
this round is worth stating plainly, because it generalises: round 1 required
retaining the objects a selection rule ran over, and round 2 found that the
rule's *other argument* was never retained. Provenance could resolve every
candidate digest and still not reproduce the decision.

## The specimens

### A — one `stable_id`, three observed versions

`mutable-comment-v1.json`

One issue comment id observed three times: original body, a whitespace-only
edit (one trailing space), then a replaced body. Three distinct canonical
digests. This is the §2 problem stated as bytes: re-fetching id `9000000001`
after the third observation returns "edited: never mind", and without retained
snapshots the earlier text is unrecoverable.

A is **not** the §9 witness, and an earlier revision of this file claimed it
was. A-v2 moves `body` and `updatedAt` together, as a real GitHub edit does — so
its digest differs from A-v1 for two reasons at once, and an adapter calling
`trim()` on bodies would have produced a different digest there too. The
comparison was green while testing nothing about byte-exactness. Specimen F
holds `updatedAt` equal so the trim is the only thing that can move the digest.

### B — same snapshot, different irrelevant API noise, same digest

`mutable-comment-v1.json`, `specimenB`

Two raw API observations of the *same* comment version, differing only in
fields outside the §8.5 allowlist and in JSON key order: `node_id`, `url`,
`html_url`, `issue_url`, `avatar_url`, `site_admin`, `performed_via_github_app`,
and a `reactions.total_count` that moved from 0 to 3. Both project to one
canonical object and therefore to one digest.

This is the §6 witness. Had the contract hashed raw HTTP bytes, somebody adding
a thumbs-up would have registered as evidence mutation.

A and B share a file because they share a subject: one comment id, one
allowlisted projection, examined for what does and does not move the digest.

### F — the byte-exact witness that actually discriminates

`trailing-space-discriminator-v1.json`

One observation, two candidate projections of it. Every canonical field is
identical — `stableId`, `user`, `authorAssociation`, `createdAt`, **and
`updatedAt`** — and the only difference is the final byte of `body`:

| | body ends | digest |
|---|---|---|
| `F-faithful` | `early. ` | `sha256:158067f6…` |
| `F-trimmed` | `early.` | `sha256:afde4f22…` |

`F-trimmed` is what a `trim()`-ing adapter emits and is marked
`"conformsToContract": false`. Because nothing else varies, the differing digest
is caused by the trim and by nothing else. That is the property A-v2 could not
establish.

### C and D — an empty result is not a fact until the enumeration is

`complete-empty-query-v1.json`, `incomplete-query-v1.json`

A matched pair. `allReturnedSnapshotDigests` and `matchedSnapshotDigests` are
`[]` in both — byte-identical. They differ only in whether the enumeration
finished:

| | C | D |
|---|---|---|
| pages requested | `["1"]` | `["1","2"]` |
| pages obtained | `["1"]` | `["1"]` |
| next page present | `false` | `true` |
| enumeration | `COMPLETE` | `INCOMPLETE` |

Their digests differ. That is the whole point: §13 and §14 exist because an
empty `Vec` cannot distinguish "nobody produced this" from "the fetch broke",
and a system that cannot distinguish them will eventually report the second as
the first.

### G — the empty matched subset that the candidate set contradicts

`matcher-candidate-set-v1.json`

A COMPLETE enumeration that returned two reviews — one by an unrelated bot, one
by the expected author — recorded next to an empty `matchedSnapshotDigests`.

C and G are the discriminating pair for §13's second half. Under a query
snapshot that keeps only the matched set, they are the same artifact: an empty
list, a completed enumeration, nothing else. Retaining
`allReturnedSnapshotDigests` and naming the matcher is what separates them,
because the empty subset in G does not survive re-running the named rule over
the two retained candidates.

The matcher's `parameters` are carried in the canonical query snapshot, not in
this README. Naming the rule without its arguments would leave an auditor able to
resolve every candidate digest and still unable to say which author was expected
— the input reproduced and the decision not. `expectedAuthorLogin` is therefore
part of what the query digest covers.

`allReturnedSnapshotDigests` is in observation order and `matchedSnapshotDigests`
is a subsequence of it (§13.2). JCS does not sort arrays, so this order is inside
the digest; leaving it to the adapter would let two conforming implementations
produce two query digests from one observation.

An absence claim that cannot be re-derived from retained bytes is an assertion,
and a broken matcher is the cheapest way for a real object to become "nothing
found".

### H — the head read that did not happen

`failed-head-after-v1.json`

`HEAD_BEFORE` succeeded and carries a `snapshotDigest`; `HEAD_AFTER` failed on an
API rate limit and carries a `reason` and **no digest**.

Under the previous rule — every head-read event carries a `snapshotDigest` — this
observation had no honest encoding. The adapter would have had to supply the
HEAD_BEFORE digest or fabricate one, and either makes a read that produced
nothing look like a read that produced unchanged bytes. E and H are the pair:
in E, two AVAILABLE events sharing one digest say *read twice, unchanged*; in H,
the second event has no digest at all and says *read attempted, no bytes*. They
are no longer expressible as the same artifact.

What the record deliberately does not contain is whether the head moved during
those sixteen minutes. It contains that the question was asked and not answered,
which is a different fact.

### E — a wrong-SHA review that explains itself

`wrong-sha-review-v1.json`

A submitted review with `commitId = 9d4c1f0b…` against a subject head of
`1f2e3d4c…`. Both are retained: the review's `commitId` in the review snapshot,
the head in a pull-request-head snapshot, and a `decisionBasis` block in which
each value names the digest of the snapshot it came from. The derived fact
`reviewSubjectMatchesHead` names both inputs, per §18.

This is the §2 argument that the mutability problem is not about Markdown: no
comment body is involved, and the state is still unexplainable if `commitId` is
not durably recorded.

The two head reads are recorded as two `HeadReadEvent`s, each with `role`,
`snapshotDigest`, `acquisition` and `observedAt`. They carry the *same* digest —
the head did not move — and that is exactly why the events matter: an earlier
revision of this file recorded two pointers at one snapshot, which is
indistinguishable from having performed a single read. Two declared events with
one shared digest says "read twice, unchanged"; two pointers say nothing about
how many reads happened.

## Digest provenance

The digest constants were computed with an implementation that is **not** in
this repository, because there is no canonicalizer in this repository yet and a
self-written one must not produce the constants that would later validate it
(contract §23).

```text
canonicalization   rfc8785 (PyPI), version 0.1.4
digest             hashlib.sha256 over the canonical UTF-8 bytes
interpreter        CPython 3.11.15
computed           2026-08-17
```

To recompute — and to falsify — any constant in these files:

```python
import hashlib, json, rfc8785
obj = json.load(open("tests/fixtures/closure-provenance/complete-empty-query-v1.json"))["canonical"]
print("sha256:" + hashlib.sha256(rfc8785.dumps(obj)).hexdigest())
```

The digest is taken over the `canonical` object exactly as written in the file.
`raw` objects are never hashed; they exist only to show what the projection
discards.

**When a canonicalizer is written in this workspace, it must be checked against
these constants and these constants must not be regenerated from it.** A green
test whose expectations were produced by the code under test proves nothing.

## Recorded honestly, not resolved

- ~~The query-snapshot key names are illustrative.~~ **Retired.** §13 now names
  the query-snapshot fields normatively, so the specimens no longer pick names
  the contract left open.
- **No pagination witness exists in Step 0B and none is invented here.** C and D
  are synthetic and say so; they are not evidence that this ever occurred.
- ~~**The matcher rule is described, not implemented.**~~ **Retired.**
  `crates/o7-closure-matcher` binds `review-by-expected-author-login` version `1`
  — the identity pair these specimens already name — and
  `crates/o7-closure-matcher/tests/frozen_specimens.rs` re-runs it over G's two
  retained candidates. G's empty matched subset no longer survives, C's does, and
  D's does while D stays `INCOMPLETE`. The paragraph below is kept because it
  states the gap that was closed and why it mattered; only its tense is now
  wrong.

  **The matcher rule is described, not implemented.** `matcher.id`,
  `matcher.version` and `matcher.parameters` are contract-level identity, and
  `review-by-expected-author-login v1` exists only as the prose rule stated in
  the specimen files. Nothing in this repository executes it, so G's
  contradiction is verifiable by reading, not by running — and the specimen says
  which of its two candidates satisfies the rule rather than leaving the reader
  to infer it. §23 records the underlying gap as OWED: the contract obliges the
  identity pair to resolve to exactly one predicate and does not yet say how that
  resolution happens, so a matcher named only in prose is still a locator
  pointing at something mutable. §21 places that binding **before** the
  acquisition adapter, since the adapter computes `matchedSnapshotDigests` and
  would otherwise have to choose a matcher implementation itself.
- **No verification witness.** §19 leaves the binding between
  `Verification::Reproduced` and its evidence OWED, so specimen A retains only
  what the comment *said*, never a claim that anything was checked. §19 also now
  states the consequence normatively: a GitHub acquisition adapter emits
  `Claimed` and never `Reproduced`, because a comment cannot verify itself.
- **No reaction surface specimen**, matching the frozen Step 0B position. The
  `reactions` field appears in specimen B strictly as noise to be discarded.
- **No secret-in-body specimen.** §20 names the boundary and refuses silent
  masking; authoring a specimen would require choosing a redaction policy, which
  §23 leaves OWED.
