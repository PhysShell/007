# Closure redaction contract specimens (V1)

Synthetic specimens for `docs/architecture/closure-redaction-policy-v1.md`,
which itself depends on the merged `closure-source-provenance-v1.md`.

**These do not extend the Step 0B historical corpus, and they do not extend the
closure-provenance specimens. They witness the redaction gate only.**

## What these are, and are not

| | |
|---|---|
| origin | authored for this contract; **no historical origin** |
| relation to `tests/fixtures/github/` | none — that corpus is frozen historical evidence, untouched |
| relation to `tests/fixtures/closure-provenance/` | none — merged in #152, untouched |
| ids, logins, timestamps, tokens | invented; the tokens are non-functional strings |
| classifier outcomes | **absent by design** — see below |

Every file carries `"synthetic": true`.

### No live credential material

The only credential-shaped string here is

```text
SYNTHETIC_FAKE_TOKEN_AAAA0000BBBB1111CCCC2222
```

It is not a credential, is not shaped like any real provider's token, and was
chosen so that no scanner anywhere has a rule for it. Nothing in this directory
was ever valid, and nothing here was revoked — there was never anything to
revoke.

### No classifier verdicts

A redaction specimen states its **gate outcome**, because the gate is the thing
being preregistered here. It states no closure state: no `PASS`, `FINDING`,
`OWED`, `CANNOT_CHECK` or `STALE`, and no headline. Contract §10 derives the
closure consequence; a fixture that also encoded it would be a second classifier
with an answer key, which is what provenance V1 §12 forbids.

## The synthetic detector

The specimens name a detector that **does not exist and is not proposed**:

```text
id            synthetic-fixture-detector
version       1
configDigest  sha256:f00372ef104d490574feb48d7bc487fed22a1a262aab4277996f57a674c20e43
policy        SYN-TOKEN-1 — a line whose first characters are
              SYNTHETIC_FAKE_TOKEN_ blocks the field
```

The rule is deliberately trivial and **line-anchored**, so that exactly one byte
can decide an outcome. That is a fixture requirement, not a suggestion about how
real detection should work — choosing a real scanner is explicitly out of scope
for this slice, and §11 of the contract records the binding of a detector
identity to an implementation as OWED.

`configDigest` is a real JCS + SHA-256 digest over the config object above,
computed the same way as every other digest in this repository.

## The specimens

| | gate | discriminates |
|---|---|---|
| R1 `safe-body-v1.json` | `RETAIN` | full coverage, no finding → normal retention |
| R2 `explicit-secret-v1.json` | `BLOCK_SECRET` | no snapshot, no digest of the original, no masked form |
| R3 `whitespace-sensitive-secret-v1.json` | both | one byte flips the **verdict** |
| R4 `detector-failure-v1.json` | `CANNOT_ASSESS` | did not run ≠ found nothing |
| R5 `detector-inconclusive-v1.json` | `CANNOT_ASSESS` | partial coverage ≠ partial pass |
| R6 `derived-fact-blocked-v1.json` | `BLOCK_SECRET` | a fact that read the body cannot be emitted |
| R7 `safe-metadata-retained-v1.json` | `BLOCK_SECRET` | a fact that read only `commitId` survives |
| R8 `token-shaped-safe-v1.json` | `RETAIN` | the record is the detector's result, not the reader's hunch |

### R3 — the byte-exact discriminator

Two decoded bodies differing by **one U+0020**, at the start of the token line.
The line-anchored rule blocks the first and not the second.

This is stronger than the `updatedAt`-equal pair in the provenance specimens.
There, one byte moved a digest; here one byte moves the **gate outcome**. If any
`trim`, per-line whitespace normalization or similar ran between decoding and
assessment, the two inputs would collapse into one and both would block — so the
specimen fails loudly rather than passing for the wrong reason.

### R4 and R5 — two ways not to know

Both are `CANNOT_ASSESS`, and they are separate files because they fail
differently: R4's detector died, R5's detector finished but had looked at
`/body` only while the projection also retains `/user/login` and
`/author_association`.

R1 and R4 carry **the same empty finding list**. Only `outcome` and `reason`
separate "inspected, found nothing" from "never inspected". That pair is the
whole reason the gate has three outcomes instead of a boolean.

### R6 and R7 — the derived-fact pair

Both block a body. R6's candidate fact (`reproductionClaimed`) was derived from
the body and dies with it; R7's (`reviewCommitIdObserved`) was derived from
`commitId`, which the blocked-source metadata record retains, and survives.

The line is not how useful the fact is. It is whether **every input still exists
as retained immutable bytes** — provenance V1 §18 applied to a source that is
legally forbidden to exist.

### `assessedBody`

Each specimen shows the exact decoded string the detector saw. It is a
fixture-visible input, never retained evidence:

```text
gate = RETAIN          assessedBody is byte-identical to the retained
                       canonical body — which is what proves nothing
                       normalized it between assessment and projection

gate = BLOCK_SECRET    assessedBody appears in no canonical object, and no
   or CANNOT_ASSESS    digest in this directory covers it
```

## Recorded honestly, not resolved

- **The detector is a fixture device.** `synthetic-fixture-detector` exists only
  in these files. Nothing executes it, so R3's discrimination is verifiable by
  reading, not by running. Contract §11 records the binding of detector
  identity to an implementation as OWED and names what it blocks.
- **No real GitHub content was scanned** to build these, and none should be. The
  specimens are authored, not observed.
- **No re-assessment semantics.** What happens to already-retained snapshots when
  a detector version changes is out of scope and recorded as a residual.
- **The integrity test is not a scanner.** It holds the preregistration —
  outcomes, coverage, provenance completeness, and the retention prohibitions.
  It deliberately implements no secret detection of its own; R8 exists so that a
  test which quietly grew one would fail.
