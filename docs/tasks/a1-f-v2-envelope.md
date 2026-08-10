# A1-F v2 — Envelope v2

**Status: DRAFTING (revision E-R0) — AWAITING INDEPENDENT REVIEW.**

First revision. It establishes the realization ledger and its gate, records one
substantive decision (**E-1**, the envelope size bound), and corrects one
convergence input that current authority has already overtaken. No graph change:
the frozen registry is a hard input here, not a subject.

## 0. Inputs and authority

```yaml
base:
  commit: 6301881                      # merge of PR #128 into main
  role:   the branch point for this phase

hard_input:
  document: docs/tasks/a1-f-v2-phase-g.md   # as merged on main
  decision: FD-v2-GRAPH — APPROVED / CLOSED
  role:     EXTERNAL IMMUTABLE INPUT. Not a neighbouring file to be adjusted.
  extract:  docs/tasks/a1-f-v2-graph.json   # generated from it; do not hand-edit

current_v1_authority:
  document: docs/q-deck/a1-authority-contracts.md
  blob:     3b26849cc39a3391aaed46cca56be3b6715afabb
  rounds:   R1..R5.2, then S1 (FD-1.4 only)

design_input:
  ref:    refs/tags/a1-f-v2-phase-g-design-input-v1
  target: 37502e3ce5c397a7437445aafb88c13d84ba4ac0
  role:   EVIDENCE. Never authority.
```

The design input is now reachable through a preservation ref rather than a
deletable branch, so every citation in this phase is reproducible from a clone
of `main` plus that tag. That was not true while Phase G was being written, and
it is the one provenance defect the post-closure review caught.

## 1. Mandate, and what is not local drafting

```text
SCOPE
  Envelope v2 wire / schema / digest / version decisions
  + the realization ledger, from commit 1

FORBIDDEN as a local drafting choice — each REOPENS or SUPERSEDES Phase G:
  a new semantic node
  a new semantic edge
  removal of a frozen edge
  a twenty-second event kind

DISPOSITIONS
  close only rows a concrete Envelope-v2 decision consumes.
  never bulk-close the 47 + 141.
```

The forbidden list is not advice. Each of its four items is a check the gate of
§2 performs mechanically, so a drafting session cannot reach any of them by
accident — only by deciding to, on the record.

## 2. The realization ledger, and the gate that makes it binding

Phase G §4.2.6 made one machine-checked wire realization ledger a **required
acceptance artefact** of this phase rather than an optional aid, because the
joint between graph authority and wire schema was the last place a relation
could still be invented by a human reading a document. Two files and one
checker:

```text
docs/tasks/a1-f-v2-graph.json              the frozen right-hand side.
                                           GENERATED from the merged Phase G
                                           document; every count re-derived and
                                           asserted, nothing hand-typed.

docs/tasks/a1-f-v2-realization-ledger.json the left-hand side. One row per
                                           semantic edge CARRIER.

tools/a1_v2_ledger_gate.py                 the five steps, in the frozen order.
```

Run: `python3 tools/a1_v2_ledger_gate.py`

**The gate's order is the frozen one**, and the ordering is load-bearing — the
universe check runs before the presence map, because a presence map is exact
only over a set it is quantified across:

```text
1. event-kind universe equality      v2's declared set == the frozen 21
2. payload presence map              exactly 11 one-carrier / 10 zero-carrier
3. recursive ArtifactRef extraction  v2 schema fields == artifact_ref carriers
4. forward carrier coverage          every carrier maps to an admitted edge
5. reverse carrier coverage          every admitted edge has >= 1 carrier
```

**It fails closed on absence, which is the whole point.** With nothing declared
yet, the gate does not pass vacuously — it reports every step OWED and exits 1:

```text
  1. event-kind universe equality       OWED   v2 declares no event kinds; frozen set has 21
  2. payload presence map               OWED   no carriers declared; map requires 11 one / 10 zero
  3. recursive ArtifactRef extraction   OWED   no v2 schema fields extracted and no artifact_ref carriers
  4. forward carrier coverage           OWED   no carriers declared
  5. reverse carrier coverage           OWED   69/69 admitted edges have no wire carrier
RESULT: FAIL — Envelope v2 is not realized
```

**A gate that only ever fails is worthless**, so it was tested for
discrimination, not just for strictness. Four synthetic ledgers, results
recorded:

| Synthetic ledger | Expected | Gate result |
|---|---|---|
| complete, faithful realization of all 69 edges | pass | **exit 0**, all five PASS |
| the same, plus a twenty-second event kind | fail at step 1 | **step 1 FAIL**, others pass |
| the same, plus a carrier for `CoderReport → ReviewVerdict` | fail at step 4 | **step 4 FAIL**, others pass |
| the same, plus `event_payload_digest` on a payload-free kind | fail at step 2 | **steps 2 and 4 FAIL** |

The fourth case is the one worth keeping. It is exactly the scenario G-R12
constructed to argue the presence map into existence — a wire schema that grows
`event_payload_digest` on one of the ten payload-free kinds while the drafter
simply omits a ledger row. Forward coverage alone would not see it, because
there would be no offending row to inspect; only a check that asserts *absence*
catches it. The gate catches it.

`_note` fields in the ledger record one rule the format cannot enforce: the
declared `event_kinds` must come from the v2 schema, never be copied from the
frozen graph. A right-hand side copied onto the left proves nothing, and step 1
would then pass by construction.

## 3. E-1 — the envelope size bound

**Decision: the maximum is DERIVED from the envelope field set, never chosen.**

### 3.1 The seam

`E-V0-4` reached this phase as the one *contract*-level finding from the PR #124
probe, and Phase G §7 explicitly left "the envelope-size bound" undecided. The
open half:

```text
FD-1.8   ref to an envelope-bearing artifact:
             size = stored envelope bytes + stored payload bytes, together
FD-1.4   bounds typed A1 JSON objects, InteractionManifestV1, and opaque
             evidence blobs. It bounds no ENVELOPE.
```

So `max(ref.size)` is not derivable for eleven of the kinds. That is not
cosmetic. FD-1.5 charges `ref.size` against the direct and closure budgets
**before reading**, and a resolver must therefore reject an over-large declared
size — but for envelope-bearing artifacts there is no size to reject against.
Two conforming implementations can accept different bytes:

```text
implementation A   accepts any declared size up to the closure budget,
                   so an envelope-bearing ref may claim 128 MiB
implementation B   invents a ceiling — the probe used
                   1 MiB + min(64 MiB, 1 MiB) — and rejects above it
```

Same wire bytes, different verdicts. FD-1.2 froze digest framing precisely so
that two implementations cannot disagree about identity; leaving admissibility
undetermined re-opens the same class of divergence one layer up.

### 3.2 The candidates, and why two of them are already spent

The convergence ledger recorded three candidate shapes without choosing.

**Split the ref size — REJECTED.** Defining `size` over the payload alone and
carrying the envelope's size separately reintroduces exactly the defect R5.2
repaired: FD-1.8 exists because a ref that sizes only one half "charges half the
cost of what the resolver reads". Splitting it back out restores correct
accounting only if both numbers are always carried and summed — which is this
decision with extra steps, plus a new failure mode in which the two numbers
disagree.

**Reclassify typed non-envelope objects — ALREADY DONE, and it addressed the
other half.** See §4: S1 has already applied it to `InteractionManifestV1`. It
does not bound envelopes and never could.

**Bound the envelope explicitly — ACCEPTED, in a specific form.** The naive
reading of "bound the envelope" is to pick a number, and a picked number is the
thing this track refuses. The envelope does not need one:

```text
FROZEN for v2

max_envelope_bytes is DERIVED from the v2 envelope field set together with the
scalar bounds already frozen in FD-1.4 — Text <= 65536, opaque id <= 256,
Digest256 fixed-width, artifact_refs <= 256 entries — and is PUBLISHED with the
derivation beside the field set, never as a bare constant.

max_ref_size(kind) = max_envelope_bytes + max_payload_bytes(kind)

A resolver rejects ref.size above that maximum BEFORE reading either half, so
FD-1.5's declared-size-before-read discipline extends to envelope-bearing
artifacts as it already does to every other kind.
```

The consequence worth stating: `max_envelope_bytes` becomes a *computed
property of the schema*, so changing a field's bound changes the maximum by
arithmetic rather than by amendment, and the two can never drift. A bare
constant would need FD-1.4 edited every time an envelope field moved, and the
edit that gets forgotten is the one that makes two implementations disagree.

### 3.3 What E-1 does not settle

The **number** is not decided here, and cannot be: it is a function of the v2
envelope field set, which this phase has not drafted. E-1 decides the *rule* and
the obligation to publish the derivation. The value lands with the field set.

E-1 also touches no node, no edge and no event kind, so the gate of §2 is
unaffected by it.

## 4. A correction to a convergence input: S1 already closed half of `E-V0-4`

The convergence ledger describes `E-V0-4` as two seams, both open. One of them
is no longer open against current authority, and this phase would otherwise
inherit a repair order for work already done.

`E-V0-4`'s second half was that FD-1.4 listed `manifest` among the 64 MiB
evidence blobs while also bounding "any typed A1 payload" at 1 MiB, leaving
`InteractionManifestV1` bounded twice. **S1 repaired it in v1**, by the third
candidate shape: current FD-1.4 reads

```text
typed A1 JSON object, except the one below       <=      1 MiB
InteractionManifestV1                            <=     64 MiB
opaque evidence blob (diff, raw provider bytes,
  gate log, patch)                               <=     64 MiB
```

`manifest` is gone from the evidence-blob list and the manifest has its own
line, on the grain argument that there is one manifest per *execution* rather
than per dispatch. Phase G §6 already records the 64 MiB figure as the inherited
v2 drafting baseline.

So `E-V0-4` enters this phase as **one** open seam, not two, and the remaining
half is §3's. Recorded here rather than silently narrowing the finding: a
convergence input that quietly shrinks is indistinguishable from one nobody
checked.

## 5. Dispositions consumed by this revision

Under the standing rule — a row closes only alongside the concrete decision that
consumes it, never in bulk because an adjacent phase finished.

**Fully consumed by E-1** (recorded in the convergence ledger §3.2):

```text
FD-1.4  per-object bounds       E-1 adds a derived envelope maximum; the frozen
                                scalar bounds are its inputs and are retained
FD-1.8  artifact refs           E-1 preserves the summing rule verbatim and
                                supplies the maximum it was missing
```

**Consumed in part — semantic coordinate only.** Their proof coordinate depends
on v2 wire types that do not exist yet, so it stays empty rather than guessed:

```text
V1-N116  ref sizing only the envelope           semantic: KEEP (E-1 preserves)
V1-N117  ref digesting the payload not envelope semantic: KEEP (E-1 preserves)
V1-N118  closure sized by envelopes alone       semantic: KEEP (E-1 preserves,
                                                and now bounds the sum)
```

Everything else in the 47 + 141 remains OPEN. Three rows carrying half a
disposition is the honest state of a first revision; a first revision that had
closed a category would be evidence of a spreadsheet, not of a decision.

## 6. What this revision does not decide

- the v2 envelope field set, framing order, or any digest domain;
- the *value* of `max_envelope_bytes` (§3.3);
- `campaign_protocol_version`, or `message_kind_version` for any kind;
- `E-V0-1`, `E-V0-2`, `E-V0-3` — implementation-level findings, still owed;
- any disposition beyond the five of §5.

## 7. For the independent reviewer (revision E-R0)

1. **The extract.** `a1-f-v2-graph.json` is generated from the merged Phase G
   document, and everything downstream trusts it. Re-derive it independently:
   69 edges / 56 `Intra` / 13 `Causal`, 21 event kinds, 11 payload variants, 47
   typed nodes, 20 terminal kinds, 11 payload-bearing / 10 payload-free. A
   wrong extract silently relaxes every check built on it.
2. **The gate's discrimination table.** Four synthetic ledgers, four expected
   verdicts. Re-run them. In particular check whether case A *should* pass: it
   declares a carrier for all 69 edges with synthetic paths, and if a fully
   synthetic ledger passes every step, the gate may be checking shape rather
   than substance.
3. **Step 3's direction.** It compares declared schema fields against
   `artifact_ref` carriers. That catches a field with no carrier and a carrier
   with no field — but both sides are written by the same drafter. What
   independently establishes the field list?
4. **E-1's rejection of the split.** It rests on R5.2's reasoning about
   half-charged refs. Is there a form of the split that keeps FD-1.5's
   accounting honest and that §3.2 dismissed too quickly?
5. **E-1's derivation claim.** `max_envelope_bytes` is asserted to be computable
   from the field set plus frozen scalar bounds. Check that this is true of a
   *realistic* v2 envelope — a variable-length repeated field whose element
   count is bounded but whose element size is not would break it.
6. **§4's staleness correction.** Verify against blob `3b26849c` that S1 really
   removed `manifest` from the evidence-blob list, and that no third seam hides
   in the same FD-1.4 block.
