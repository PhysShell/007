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

## The specimens

### A — one `stable_id`, three observed versions

`mutable-comment-v1.json`

One issue comment id observed three times: original body, a whitespace-only
edit (one trailing space), then a replaced body. Three distinct canonical
digests. This is the §2 problem stated as bytes: re-fetching id `9000000001`
after the third observation returns "edited: never mind", and without retained
snapshots the earlier text is unrecoverable.

The whitespace version is the §9 witness — byte-exact retention means a
one-space edit *does* move the digest, and the contract accepts that rather
than normalizing it away.

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

### C and D — an empty result is not a fact until the enumeration is

`complete-empty-query-v1.json`, `incomplete-query-v1.json`

A matched pair. `matchedStableIds` is `[]` in both — byte-identical. They
differ only in whether the enumeration finished:

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

`head_before` and `head_after` are both present and carry the *same* digest —
the head did not move. §8.1 requires both reads regardless; a specimen where
they agree is the one that shows the pair is not stored only when it is
interesting.

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

- **The query-snapshot key names are illustrative.** Contract §13 fixes the
  required *content* of a query snapshot (surface, binding, pagination
  traversed, completeness, matched objects); it does not fix `pagesRequested`,
  `pagesObtained`, `nextPagePresent`, `enumeration`, `incompleteReason`. These
  specimens had to pick names to be concrete. Picking them here does not
  legislate them — if the acquisition slice fixes different names, §13 is where
  that gets decided, and these constants change with it.
- **No pagination witness exists in Step 0B and none is invented here.** C and D
  are synthetic and say so; they are not evidence that this ever occurred.
- **No verification witness.** §19 leaves the binding between
  `Verification::Reproduced` and its evidence OWED, so specimen A retains only
  what the comment *said*, never a claim that anything was checked.
- **No reaction surface specimen**, matching the frozen Step 0B position. The
  `reactions` field appears in specimen B strictly as noise to be discarded.
- **No secret-in-body specimen.** §20 names the boundary and refuses silent
  masking; authoring a specimen would require choosing a redaction policy, which
  §23 leaves OWED.
