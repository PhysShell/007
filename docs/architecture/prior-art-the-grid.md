# the_grid as prior art for controller/lifecycle, read against A1-F

Status: **prior-art record, non-normative** · Scope: **one external system read
against the A1 authority-contract freeze candidate** · Contract effect: **none**.

This note changes no contract. It does not modify, unfreeze, or add a
requirement to `docs/q-deck/a1-authority-contracts.md`, and it does not create
work. It records a comparison against an external artifact so that a later
controller/lifecycle slice can reuse what is genuinely reusable, and so the
comparison itself does not have to be re-derived from memory.

Per rule 4 of `docs/evidence-and-decision-discipline.md`, every claim below
binds an artifact **revision**, and *artifact says* / *inference* / *decision*
are kept apart. Both sides of this comparison are moving objects, so both are
pinned and both carry an explicit staleness condition (§2.3).

## 1. Bindings

### 1.1 External artifact under comparison

```text
repository   memento-engineering/the_grid
revision     46d6d260de07c29624564ba153c99e959518a9f3   (main, read 2026-08-07)
artifact     docs/adr/ADR-0009-the-allocation-tree.md
             blob    ae2487dc214babc5347f906d6713c7082a27eece
             sha256  afe37a2413b94891d2f7943b81534f4da7c9a04adefe119d27122ae64ac063b1
             241 lines, 16452 bytes
```

No commit of *this* repository captures that artifact, so it carries its own
anchors — the same construction `docs/evidence-and-decision-discipline.md` uses
for the Vargas artifact, and the same full-object-ID discipline as
`docs/architecture/prior-art-fusio.md`. The `sha256` is over the exact artifact
bytes; the blob ID is the git-native equivalent. Either resolves the artifact
without trusting the branch name, which moves.

### 1.2 Supporting external artifacts

Two items in the *do not adopt* list (§4) are **not** decisions of ADR-0009 and
must not be attributed to it. They are pinned separately at the same revision:

```text
docs/adr/ADR-0002-package-topology-and-domain-projections.md
             blob    b7af79af488ff3c9d9d7629847e2668b093cc270
             sha256  16cd491ce211eff676f45b22a5b98ad9bf3d161e264eb538dcd93c26d817f117
             — establishes beads as the substrate; ADR-0009 does not mention it

docs/adr/ADR-0007-tree-engine-and-genesis-supersession.md
             blob    eff214bb82839ada65442cc27d7837d3f28e4a94
             sha256  38cc066ccb2392eae62a65d01d8141c7c42817923eac1765554e11897b150168
             — the tree engine ADR-0009 extends and builds upon
```

The implementation language is Dart (`pubspec.yaml` at the pinned revision).

### 1.3 007 artifact under review

```text
artifact     docs/q-deck/a1-authority-contracts.md
branch       claude/a1-contract-freeze-dkrnnq
revision     3c3097482c86d1fc65ccf793e6b3b2ce1e60ef48
merged       NO — not reachable from origin/main at the time of this record
status       as stated in its own header: PROPOSED FREEZE / REVIEW REQUIRED /
             NON-AUTHORITATIVE — a freeze *candidate*, to which "no A1
             implementation may bind [...] until it is reviewed and merged"
```

The stronger fact is the document's own status, not merely its merge state:
A1-F is not an accepted freeze. It reached `3c30974` through three corrective
rounds after the commit that named itself a freeze (`144ebf6`, 1127 lines →
`3c30974`, 2154 lines). A comparison stated against "the current freeze" without
a head names nothing.

### 1.4 Staleness condition

Either side moving invalidates this record:

- the_grid at a revision other than `46d6d26` — §3 must be re-derived;
- A1-F at a head other than `3c30974`, **including acceptance** — §4 and §5 must
  be re-reviewed before being relied on.

This is the same discipline `docs/research/specula-trace-conformance.md` states
for its own A1-F audit: a conclusion about a document that is still moving is
`STALE`, not `ESTABLISHED`, once the document moves.

## 2. Artifact says

Quoted verbatim from ADR-0009 at the pinned blob. These are *the artifact's*
claims about *its own* system. They are not claims about 007.

| Notion | Artifact @ `ae2487d` |
| --- | --- |
| Addressable, stable identity | Decision 2: "each Allocation has **stable, addressable identity independent of its `Branch` lifecycle** — so a *surviving effect* is **re-adopted** into a freshly-built tree" |
| Single write locus | Decision 3: 'The "sandbox" **dissolves**; invariants hold by *layering + a single write-locus*, not a wall' |
| Lifecycle verbs | Decision 4: "Lifecycle: **`startOrAdopt` / `update` / `dispose` / `detach`** — all TYPE properties" |
| Proof before adoption | "`startOrAdopt(ctx)` — spawn fresh **or** prove-and-adopt a survivor at the **address**"; the engine owns "the stable address + **no-adopt-on-faith** (the type must *return proof* of freshness; can't prove → create/respawn)" |
| Adoption obligation is testable | "**No-adopt-on-faith** is a **contract obligation + a mutation-test** (an adopt path without a freshness assertion fails), not an engine mechanism" |
| Detach ≠ terminate | "`dispose()` = KILL"; "`detach()` = LEAVE RUNNING + persist the handle — a *distinct verb*, **not** an overloaded `dispose`", a per-type opt-in whose clearest win is "**graceful controller restart**"; "The **reaper catches a detached effect nobody re-adopts**" |
| Host/effect split | Decision 5: "Host = thin *sync* driver" which on mount "kicks `startOrAdopt`" and on unmount kicks "`dispose` (kill)" or "`detach`"; the effect owner "**holds NO writer** — it reports, the Host persists" |

**Scope note on the last row.** In ADR-0009 the mount/unmount correspondence is
the Host↔Allocation contract of *that* engine, stated for a tree that "rebuilds
deterministically from work source + cursor". The artifact does not state it as a
universal kernel rule, and §4 does not reject it as one — it declines to import
it into 007's kernel as one.

## 3. 007 comparison

*Inference, drawn on top of §2 and the pinned A1-F.*

A1-F governs a different boundary: agent-produced report → controller acceptance
→ durable typed artifact (`CandidateReceipt`, `ReviewVerdict`, `HumanDecision`).
It does not own live-execution reconciliation, and says so — it binds the durable
dispatch boundary and fail-closed post-dispatch ambiguity to the R1 command
vertical (`docs/q-deck/r1-command.md` §11.1–§11.2, cited in A1-F §0 "What this
contract binds to and may not redefine", and again at FD-9), and its §6 excludes
external reconciliation from scope.

On one point the two systems already agree, reached independently: A1-F fixes
that "GitHub PR comments are a human-readable **projection** of these objects. No
agent consumes a mutable PR comment as authoritative input" — the same
projection ≠ authority separation ADR-0009 applies to live effect state.

Where the ADR-0009 notions land is therefore **below** A1-F's boundary, in the
controller/lifecycle layer that does not yet exist. Nothing in §2 contradicts an
A1-F contract, and nothing in §2 identifies a missing A1-F invariant.

## 4. Disposition

**NO A1-F CONTRACT CHANGE.** No finding here is a freeze blocker.

**DEFERRED / NOT PART OF A1 / FIRST ELIGIBLE CONSUMER: controller-lifecycle /
REOPEN ONLY AGAINST A CONCRETE CONSUMER.** The following notions are recorded
as prior art worth re-reading when that consumer exists — not as a chosen
design, not as a work item, and not as a commitment to this ADR's model:

- desired-state → live-execution reconciliation as a distinct concern;
- stable execution identity that outlives the structure that created it;
- proof-before-adopt recovery — an identifier found after restart is not by
  itself evidence that it is the same execution;
- explicit detach-vs-terminate semantics, rather than one verb carrying a flag;
- live-state projection kept distinct from durable reduced state, such that a
  durable `RUNNING` record plus a dead liveness proof is a contradiction rather
  than a green light;
- a foreign work source read/mutated conditionally, without becoming the store
  for internal cursors, leases, or execution internals.

**DO NOT ADOPT.** Recorded so a later reader does not re-derive the rejection:

| Not adopted | Established by | Reason |
| --- | --- | --- |
| `genesis_tree` / tree-engine substrate | ADR-0007 @ `eff214b`, extended by ADR-0009 | 007 has a canonical event log and reducer; a second substrate is not a gap |
| The Dart implementation | `pubspec.yaml` @ `46d6d26` | language and runtime mismatch; the ADR is read for its model only |
| Beads as a 007 authority | **ADR-0002 @ `b7af79a`** — *not* ADR-0009 | 007's durable truth is its own event log; a foreign store is a work source, never an authority |
| mount = spawn / unmount = kill as a universal 007-kernel rule | ADR-0009 Decision 5 @ `ae2487d`, where it is a Host↔Allocation contract | declined as a kernel-wide rule; see the scope note in §2 |

## 5. Provenance — what this record does not claim

This record establishes a **prior-art comparison only**.

It does **not** claim that any existing 007 rule was derived from the_grid. In
particular, commit `0d0c9ce` (`docs/evidence-and-decision-discipline.md`, on
`origin/main`) establishes only that four rules were maintainer-ratified and
landed in 007. That document binds a different external artifact — Vargas
(2026), *The Semantic Cognition Matrix*, `sha256:390174db8a86…` — and states of
it that the rules "were derived independently and are stronger" and that "the
citation adds no rule and changes none". No artifact establishes a causal path
from the_grid to `0d0c9ce`, and none is asserted here.

This paragraph is deliberate, not defensive. Rule 4 names the failure it guards
against: an inference gets cited by a later agent as source truth. Two systems
converging on a similar idea is the cheapest possible material for that failure
— absent an explicit denial, the next reader draws the arrow.

## 6. Deferral discipline check

Applying the check recorded at `e938b2d` (`docs/deja-vu-memory-evaluation.md`) —
stated name-free, since grepping for the token you were last shown is how that
lesson was learned:

> Does any downstream artifact — diagram, phase, layout, schema example,
> command, fixture — still let a reader carry out the deferred decision by
> following it?

For this note: no. §4 names the deferred notions as notions only. It proposes no
007 type, module, crate, field, or schema; draws no architecture diagram in
which a box could be built; and defines no phase or command. The external verbs
in §2 appear as quotations attributed to an external artifact, in the
artifact-says section, and nowhere as a proposed 007 surface. A contributor
following this document has nothing to execute — which is the intended property
of a deferral.
