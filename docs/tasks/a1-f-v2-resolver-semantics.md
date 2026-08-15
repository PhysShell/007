# A1-F v2 — Resolver observation contract (RS-P0)

**Status:** preregistration. Normative. Docs-only: this change adds no production
code, no tests and no fixtures.

**What this file is.** The frozen contract for what the object resolver may
*conclude* from a lookup, and what each conclusion obliges its consumer to do. It
is written before an implementation exists so that the implementation can be
judged against a target it did not choose.

**Precedence.** `docs/tasks/a1-resolver-semantics-evidence.md` records the
observations this contract rests on. It is **non-normative**. Where that file and
this one disagree, this one governs and the disagreement is a defect in the
evidence record.

**Relation to the terminated attempt.** PR #144 preregistered the same question
and was terminated without merge after an independent reviewer found a semantic
self-contradiction in it at its final head. That attempt is retained as a
**falsification corpus** for this document — a list of defects this contract must
be checkable against — and explicitly **not** as source text. The disposition of
each surviving finding is recorded in the evidence file, which is where a coverage
argument belongs; it is not part of this contract.

---

## 1. Purpose — the primary question

> Can this layer, without network access and inside its accepted trust boundary,
> obtain a **complete** and **trustworthy** response for the cited object id?

Everything below is an answer to that question and to nothing else. The resolver
does not decide whether the cited object is the right object, whether the citation
is well-formed, or whether the artifact means what the citer thought. Those are
other layers' questions (§8).

---

## 2. Observation prerequisites

Two independent conditions must hold before any byte-derived conclusion is
admissible. They are checked in the order given.

### 2.1 PROVENANCE admissible

- the executable is **selected by this layer**, from a fixed list of absolute
  paths — never resolved through `PATH` and never named by the repository;
- the child environment is **constructed**, not inherited: it carries only names
  this layer grants explicitly;
- **no lazy or network acquisition** may satisfy the lookup;
- the read is **raw** — no filter, conversion or textual transformation is
  applied to the returned bytes;
- the repository under inspection **cannot choose** which program runs or which
  transformation path is taken.

### 2.2 RESPONSE complete

- **every** operation that produces part of the answer must succeed;
- the output of a failed command is **debris, never evidence** — it is never
  hashed, never parsed, and never compared against the citation.

A partial response is not a weak observation. It is **no observation**.

---

## 3. Normative state table

This is the **only** statement of the state partition in this document. There is
no second table, and no alternative vocabulary that re-expresses it.

```text
any prerequisite of §2 absent
    -> LOOKUP_UNOBTAINABLE

§2 holds
  AND kind and bytes obtained by fully successful operations
  AND oid(kind, bytes) == requested oid
    -> RESOLVED(kind, bytes)

§2 holds
  AND kind and bytes obtained by fully successful operations
  AND oid(kind, bytes) != requested oid
    -> IDENTITY_VIOLATION
```

The three cases are exhaustive and mutually exclusive over the admissible inputs.

**A missing prerequisite outranks both byte-derived states.** If §2 does not hold,
the outcome is `LOOKUP_UNOBTAINABLE` **even when a mismatch was observed** — an
untrustworthy channel cannot support a report about the object store, in either
direction.

---

## 4. Exact claims per state

```text
RESOLVED CLAIMS
    an admissible local acquisition occurred, under every §2 requirement
    these returned bytes hash to the requested object id

RESOLVED DOES NOT CLAIM
    that the citer intended these particular bytes
    that the object id uniquely denotes one semantic artifact
    that collisions do not exist
```

`RESOLVED` therefore rests on **no cryptographic premise**. Both of its claims are
direct observations and remain true whatever the status of any hash property.

`IDENTITY_VIOLATION` likewise rests on no premise: it reports that a recomputation
disagreed with the citation, which is a fact about the arithmetic performed, not
about what an adversary can achieve.

---

## 5. Prepared-collision disposition

Stated explicitly, because leaving it implicit is what terminated the previous
attempt.

```text
§2 prerequisites hold
commands complete successfully
oid(kind, bytes) == requested oid
    -> RESOLVED

even if the returned bytes are a colliding alternate to the artifact
the citer had in mind.
```

This is **not** an exception, a degraded `RESOLVED`, or grounds for
`LOOKUP_UNOBTAINABLE`. It is the ordinary operation of §3. The resolver observes
**identity equality**, not citer intent; a colliding alternate satisfies identity
equality, and §4 already declines to claim intent. No implementation may read this
case as anything other than `RESOLVED`.

---

## 6. `LOOKUP_UNOBTAINABLE` non-claims

`LOOKUP_UNOBTAINABLE` means exactly: *this layer did not obtain an admissible
complete response.* It does **not** mean any of:

```text
the object is absent locally
the object is absent from the repository
permission was denied
the repository is corrupt
a fetch is required
the locator is wrong
git malfunctioned
```

Each of those is a **different** proposition requiring its own positive evidence.
None of them may be inferred from this state, reported alongside it as if
established, or selected by matching the text of a diagnostic message.

---

## 7. Consumer mapping

```text
RESOLVED
    -> continue to referential validation

LOOKUP_UNOBTAINABLE
    -> ERROR — judgment unavailable
    -> NO re-pin, NO locator edit

IDENTITY_VIOLATION
    -> ERROR — the evidence source is untrustworthy
    -> NEVER recorded as a locator defect
```

Both error states are `ERROR`, not `FAIL`: they report that the verifier could not
form a judgment, not that the subject failed one. Neither authorises a repair of
the citation, because in neither case has the citation been shown to be wrong.

---

## 8. Layer boundary

A resolver result is about **obtaining bytes**, never about their suitability.

```text
RESOLVED(commit, ...) where the locator expected a blob
    -> the RESOLVER succeeded
    -> the LOCATOR layer rejects the citation
```

The wrong object kind is not a resolver failure and must not be folded into any
resolver state. Collapsing it would make the resolver's report less discriminating
than the distinction the locator layer has to enforce.

---

## 9. Required implementation witnesses

An implementation claiming this contract must be falsifiable by each of the
following. Each names the state it requires.

```text
W1  healthy blob, all prerequisites met
        -> RESOLVED

W2  the object cannot be produced at all
        -> LOOKUP_UNOBTAINABLE

W3  the kind operation succeeds, the body operation emits partial output
    and then fails
        -> LOOKUP_UNOBTAINABLE
        -> the partial output is never hashed

W4  a complete successful response whose bytes recompute to a different id
        -> IDENTITY_VIOLATION

W5  a successful resolution of a non-blob object
        -> RESOLVED, and the locator layer rejects it (§8)

W6  the wording of any diagnostic text is varied
        -> the selected state does not change

W7  a §2 provenance prerequisite is absent
        -> LOOKUP_UNOBTAINABLE for BOTH the hash-matching and the
           hash-mismatching case

W8  a prepared collision, or an equivalent model of one: a complete
    successful response whose bytes recompute to the requested id
        -> RESOLVED, with no citer-intent claim attached
```

W3 and W7 are the two that a plausible implementation is most likely to fail, and
W7 must be exercised in **both** directions to be meaningful.

---

## 10. Scope and future extension

**In scope:** the state partition, its prerequisites, the claims attached to each
state, the consumer mapping, and the witnesses above.

**Out of scope:** the resolver implementation, its defect fixes, the locator layer,
the ledger gate, and any change to the frozen graph registry.

**`NOT_PRESENT_LOCALLY` is deliberately NOT frozen.** Adding it requires a
**positive absence oracle** — a mechanical witness that establishes absence rather
than inferring it from a failure to produce. No such oracle is specified here, and
until one is, absence is not distinguishable from the other causes listed in §6.
