# A1-F v2 — Envelope v2

**Status: DRAFTING (revision E-R2) — AWAITING RE-REVIEW.**

E-R0 established the ledger and its gate and recorded **E-1**. E-R1 repaired the
five P1s that followed — including E-1's false derivability claim. E-R2 closes
the trust joint itself: the middle term is now extracted rather than asserted,
per-field capability replaces a bare path list, the meta-target obeys frozen
clause 5, and a `--graph` override can no longer bypass its own preflight. Three
revisions, no graph change in any of them: the frozen registry is a hard input
here, not a subject.

**E-1 is settled as of E-R1** and is not reopened below. Everything since has
been about the machine that decides when the gate may go green.

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
acceptance artefact** of this phase, because the joint between graph authority
and wire schema was the last place a relation could still be invented by a human
reading a document.

### 2.1 Three independent terms, each derived rather than asserted

E-R0 read every left-hand side out of the **ledger**, so a ledger could declare a
world and be congratulated for matching it. E-R1 split the terms apart. E-R2
finishes the job, because a split into three files is worth nothing if two of
them are still hand-written:

```text
FROZEN GRAPH    docs/tasks/a1-f-v2-graph.json
  authority     tools/a1_v2_extract_graph.py --check, source pinned at blob
                450380ff. Message kinds, typed supports and the meta-target
                members are now PARSED from the document, not transcribed.
        |
        v
SCHEMA FACTS    docs/tasks/a1-f-v2-schema-facts.json
  what v2 IS    tools/a1_v2_extract_schema.py --check. It exists now, while
                there is no v2 schema, precisely so the trust model is fixed
                before there is anything to be tempted about.
        |
        v
REALIZATION     docs/tasks/a1-f-v2-realization-ledger.json
  what v2 CLAIMS  one row per carrier, and nothing else
```

E-R1 decided a schema was "drafted" from `bool(extracted_from)` and never looked
at `extractor` — so tomorrow's drafter could write `"extracted_from": "whatever"`
and hand-fill three arrays. That is the same guarantee-by-comment the ledger's
`_note` fields were in E-R0, moved one floor up. Both preflights now run
`--check` against a real extractor, and the corpus proves a hand-filled facts
file is rejected.

### 2.2 What each step actually compares

```text
1. schema.event_kinds        == graph.event_kinds
2. schema.payload_presence   == graph.expected_payload
   AND declared structural carriers == schema.structural_commitments
3. per FIELD: source kind and COMPLETE target domain == its carriers'
4. every (source, target, class) carrier is a frozen edge, exactly
5. every frozen (source, target, class) has >= 1 carrier, exactly
```

**Step 3 is per-field capability, not a path list.** Phase G §4.2.6 clause 1
requires a field to declare the complete set of edges it may realize, and a set
of paths cannot express that. The attack it lets through is not exotic:

```text
WorkOrder.input.candidate_ref        actual target  CandidateStateReceiptRef
WorkOrder.input.materialization_ref  actual target  CandidateMaterializationRef

swap which path carries which in the ledger:
  both frozen triples still exist somewhere      -> steps 4 and 5 PASS
  each field realizes the wrong relation         -> nothing notices
```

Schema facts therefore carry `{path, source_semantic_kind,
allowed_concrete_target_kinds}`, and step 3 compares domains per path. The
corpus case is *two fields swap their targets*.

**Steps 4 and 5 are exact, because clause 5 says so.** Frozen: *"AnyCommittedEnvelope
is declared ONCE as a meta-target expansion, never as eleven separate edges."*
E-R1 violated this in its own corpus — it expanded row 69 into eleven carriers
and asserted that keeping only `CampaignFeedItem → WorkOrder` was a faithful
realization, which silently narrows a union of eleven kinds to one. Now the
carrier's target *is* `AnyCommittedEnvelope`, matched exactly; the expansion
appears only in step 3, where the field's permitted domain must equal all eleven
members. Two corpus cases pin it: *meta-target narrowed to one member* and
*meta-target realized as eleven separate carriers*.

### 2.3 The preflight binds to the path it will read

E-R1's preflight ran `extract --check` against the **default** artifact and the
gate then read `args.graph`, so `--graph /tmp/forged.json` produced a PASS
followed by five steps measured against the forgery. That is a literal bypass of
the protection E-R1 had just added. Each preflight now passes `--out` the same
path the gate reads.

`--skip-preflight` is also gone as a documented flag. The corpus uses an
environment variable instead, on the reasoning that a documented skip is an
invitation: within a year somebody adds it to CI to make the checks faster,
which is the customary way of optimising a check by deleting it.

### 2.4 It fails closed, and the corpus is executable

With nothing drafted, both preflights pass and all five steps are OWED:

```text
  preflight: graph extract              PASS   committed graph == derived from Phase G
  preflight: schema extract             PASS   committed facts == derived from the v2 schema source
  1. event-kind universe equality       OWED   no v2 schema extracted; frozen set has 21
  2. payload presence map               OWED   no v2 schema extracted; map requires 11 one / 10 zero
  3. per-field ArtifactRef capability   OWED   no v2 schema extracted; nothing to extract fields from
  4. forward carrier coverage           OWED   no carriers declared
  5. reverse carrier coverage           OWED   69/69 admitted edges have no wire carrier
RESULT: FAIL — Envelope v2 is not realized
```

`tools/tests/test_a1_v2_ledger_gate.py` — **17 cases, 17 as specified.** Thirteen
mutate one thing and assert which step catches it; four exercise the preflight
itself, which E-R1 added and left undefended.

| Mutation | Caught by |
|---|---|
| faithful realization of all 69 edges | nothing — exit 0 |
| nothing declared at all | all five OWED |
| schema grows a 22nd event kind / drops one | step 1 |
| **schema grows a payload on a payload-free kind, no ledger row** | **step 2** |
| **two fields swap their targets, globals still balance** | **step 3** |
| schema field with no carrier / carrier the extractor never saw | step 3 |
| **meta-target narrowed to one member** | **step 3** |
| **meta-target realized as eleven separate carriers** | **steps 4 and 5** |
| carrier for a relation the registry does not hold | step 4 |
| frozen `Intra` edge realized as `Causal` | steps 4 and 5 |
| an admitted edge left with no carrier | step 5 |
| **forged graph passed via `--graph`** | **preflight, no steps run** |
| **hand-filled schema facts** | **preflight** |
| **Phase G source blob mismatch** | **extractor `--check`** |
| real artifacts | both preflights pass |

Every emphasised row is a defect a previous revision of this gate could not
catch while claiming to.

## 3. E-1 — the envelope size bound

**Decision: `ref.size` keeps summing both halves, and Envelope v2 MUST define a
finite protocol-hard `max_envelope_document_bytes` — derived where the encoding
profile makes it derivable, explicitly chosen and justified where it does not.**

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

**Bound the envelope explicitly — ACCEPTED, in a corrected form.** E-R0 wrote
that the maximum is *derived from the field set and never chosen*, and that
claim is false. Frozen FD-1.3 fixes only that stored bytes are UTF-8 JSON with
an object at top level, rejects a BOM and non-object roots, and states that
**content is never rewritten**. It says nothing about insignificant whitespace,
key order, or the lexical form of numbers — and FD-1.2 deliberately made
identity independent of serialization, so a conforming peer may encode the same
envelope differently and the stored document is kept verbatim.

Bounded field *values* therefore do not bound stored document *bytes*, and it is
document bytes that FD-1.8 sums into `ref.size`. Arithmetic over the field set
yields a bound on content, not on the artifact. E-R0 asserted the stronger thing.

```text
FROZEN for v2

ref.size = stored envelope bytes + stored payload bytes        KEEP (FD-1.8)

Envelope v2 MUST define a finite protocol-hard max_envelope_document_bytes.

  If the chosen v2 encoding profile makes that maximum mechanically derivable
  from schema + encoding grammar, publish the derivation.

  Otherwise the protocol chooses an explicit hard ceiling and records the
  justification for the number.

max_ref_size(kind) = max_envelope_document_bytes + max_payload_bytes(kind)

A resolver rejects ref.size above that maximum BEFORE reading either half, so
FD-1.5's declared-size-before-read discipline extends to envelope-bearing
artifacts as it already does to every other kind.
```

Derivability is available, but only at a price this decision does not pay on the
draft's behalf: it requires constraining the lexical representation — whitespace,
escape expansion, container syntax, number form. That is a wire-encoding
decision for Envelope v2 to take deliberately, not a consequence of bounding
fields. What E-1 fixes is that the maximum must be **finite, protocol-hard, and
published with its justification** whichever branch v2 takes; an implementation
inventing `1 MiB + min(64 MiB, 1 MiB)` remains forbidden either way.

### 3.3 What E-1 does not settle

The **number** is not decided here, and cannot be: it depends on the v2 envelope
field set *and* on the encoding profile, neither of which this phase has
drafted. E-1 decides the obligation and its two admissible discharges. The value
lands with the field set and the encoding decision.

E-1 also does not decide the **digest** half of FD-1.8. That FD fixes two things
about a ref — which digest identifies an envelope-bearing artifact, and what
`size` covers — and E-1 touches only the second.

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
consumes it, never in bulk because an adjacent phase finished. E-R0 reached past
that rule and E-R1 pulls it back.

**Consumed by E-1, in the size coordinate only:**

```text
FD-1.4  per-object bounds     PARTIAL. E-1 obliges a finite protocol-hard
                              envelope maximum; the VALUE is still owed, so the
                              block is not discharged. S1 separately resolved
                              this block's manifest double-bound.

FD-1.8  artifact refs         PARTIAL. The total-size semantics are consumed:
                              the summing rule survives verbatim and E-1
                              supplies the maximum it lacked. The digest /
                              identity half stays OPEN for the Envelope-v2
                              digest decision.

V1-N116 ref sizing only the envelope            semantic KEEP; proof coordinate
V1-N118 closure sized by envelopes alone        empty, pending v2 wire types
```

**Reverted to OPEN by E-R1:**

```text
V1-N117 ref digesting the payload, not the envelope
        E-R0 closed this under E-1. It is a DIGEST-DOMAIN row, and E-1 is a size
        decision — while §6 of the same revision said this phase decides no
        digest domain. A row closed by a decision that does not reach it is
        worse than an open row, because it looks discharged.
```

Everything else in the 47 + 141 remains OPEN. Two rows half-dispositioned and
two FDs partial is the honest state of a first substantive decision.

## 6. What this revision does not decide

- the v2 envelope field set, framing order, or any digest domain;
- the *value* of `max_envelope_document_bytes`, and which of E-1's two branches
  discharges it (§3.3);
- `campaign_protocol_version`, or `message_kind_version` for any kind;
- `E-V0-1`, `E-V0-2`, `E-V0-3` — implementation-level findings, still owed;
- any disposition beyond the five of §5.

## 7. Revision record

### E-R2 — second independent review, three P1s and two P2s

All five accepted. Every one sits in the *trust joint* rather than in a design
decision, and one of them was already encoded in the corpus as an expected PASS
— the most expensive kind of defect, because the test suite was defending it.

**P1-6 — the middle term was still on its honour.** `drafted =
bool(schema.get("extracted_from"))`, with `extractor` never inspected: a drafter
could point `extracted_from` at anything and hand-fill the three arrays. E-R1
had moved guarantee-by-comment from the ledger up one floor rather than removing
it. Closed by committing `tools/a1_v2_extract_schema.py` **now**, while there is
no v2 schema and therefore nothing to be tempted about, so that when the field
set lands the extractor's *implementation* changes and the gate's trust model
does not. A hand-filled facts file is now a corpus case.

**P1-7 — schema facts were too poor to express clause 1.** They held a bare path
list, so step 3 compared sets of paths. Swapping the targets of
`WorkOrder.input.candidate_ref` and `...materialization_ref` leaves both frozen
triples present globally: steps 3, 4 and 5 all pass while each field realizes
the wrong relation. Facts now carry per-field capability — source kind and the
complete permitted target domain — and step 3 compares domains per path. This is
also the realistic form of the "wrong direction" attack §8 of E-R1 asked about:
not a reversed arrow, but a correct edge on the wrong field.

**P1-8 — the meta-target was realized against frozen clause 5.** *"Declared ONCE
as a meta-target expansion, never as eleven separate edges."* E-R1's `faithful()`
expanded row 69 into eleven carriers, and its *expected-PASS* case kept only
`CampaignFeedItem → WorkOrder` — asserting that a `subject_refs` permitting one
of eleven kinds faithfully realizes a union of eleven. Steps 4 and 5 now match
the frozen triple exactly, and the expansion lives in step 3 against the field's
domain. Two cases pin it in both directions.

**P1-9 — `--graph` bypassed its own preflight.** The preflight checked the
default artifact; the gate then read the override. `--graph forged.json` gave
PASS followed by five steps measured against the forgery — a literal bypass of
the protection E-R1 had just introduced. Preflights are now bound with `--out`
to the path that will be read, and `--skip-preflight` is gone as a flag: a
documented skip is an invitation to make CI faster by deleting a check.

**P2 — the extractor was still partly transcription.** Message kinds, typed
supports and meta-target members were hand-kept constants beside a docstring
claiming nothing was hand-typed. The reviewer confirmed the values were correct;
they are now parsed from the document anyway, since a hand-kept copy of a frozen
set is a second registry, and this one decides what a `subject_refs` field may
carry.

**P2 — the preflight had no regression corpus.** Every case ran with
`--skip-preflight`, so P1-3's protection existed and nothing defended it. Four
preflight cases added: forged graph via `--graph`, hand-filled schema facts,
Phase G source blob mismatch, and a control asserting the real artifacts still
pass. 13 step cases + 4 preflight cases, **17/17**.

**Accepted from the review without change**, and recorded because they close
open questions rather than raising them:

```text
max_payload_bytes(kind) = 1 MiB for all eleven message kinds under current
    FD-1.4 — the sole exception is InteractionManifestV1, which is not the
    payload of any message kind. Recomputed if v2 supersedes those bounds.

fail-fast preflight is right: if the right-hand side is unproven, steps 1-5 are
    not extra diagnostics, they are numbers against an unknown ruler.

an explicitly chosen document ceiling is not architecturally worse than a
    derived one. If the number is normatively frozen and every implementation
    checks it, there is no drift; the derivable branch costs a constrained
    lexical JSON representation. The explicit ceiling is the likelier outcome
    absent a strong reason to introduce an encoding profile.
```

### E-R1 — first independent review, five P1s

All five accepted; two were verified against the frozen contract before being
accepted, since both turned on what A1 actually says rather than on taste.

**P1-1 — the gate checked the ledger against itself.** Steps 1, 2 and 3 all read
their left-hand side out of the ledger, so there was no schema-side input at all
and E-R0's "faithful realization" case passed by declaring the world it was
matched against. Worse, the reviewer showed E-R0's fourth synthetic case tested
the *easier* error: G-R12's failure mode is a schema that grows
`event_payload_digest` on a payload-free kind **with the ledger row omitted**,
and a ledger-only check is blind to that by construction. Repaired by §2.1's
three-way split and a committed test that reproduces the original failure mode.

**P1-2 — edge class had fallen out of identity.** `edge_key()` returned
`(source, target)`, so a mis-classed carrier passed. Now `(source, target,
class)`, with a mutation case. The meta-target expansion was also inverted,
admitting `(AnyCommittedEnvelope → member)` and thereby giving a target-position
union out-edges Phase G says it has none of; the extractor now asserts no
meta-target appears as a source.

**P1-3 — the frozen side was held on trust.** `graph.json` claimed to be
generated and no generator was committed, so the right-hand side was a second
editable registry. `tools/a1_v2_extract_graph.py` now derives it from the Phase G
document with the source pinned by blob (`450380ff`), and the gate runs
`--check` as a preflight that aborts the run on mismatch rather than reporting
five steps measured against a forged ruler.

**P1-4 — E-1's central claim was false.** Verified before accepting: FD-1.3 fixes
UTF-8, rejects a BOM and non-object roots, and states content is never
rewritten — and says nothing about insignificant whitespace, key order or number
form, while FD-1.2 deliberately made identity independent of serialization. So
bounded field values do not bound stored document bytes, which is what FD-1.8
sums into `ref.size`. E-R0 asserted derivability as a consequence of arithmetic
over the field set; it is available only under an encoding profile that
constrains lexical representation, which is a separate v2 decision. E-1 is
restated as *derived where derivable, explicitly chosen and justified where
not* — finite and protocol-hard either way.

**P1-5 — the dispositions reached past E-1.** `V1-N117` is a digest-domain row
closed under a size decision, in a revision whose own §6 said it decides no
digest domain; reverted to OPEN. `FD-1.8` is PARTIAL — size half consumed,
digest half open. `FD-1.4` is PARTIAL rather than "fully consumed": the rule is
decided, the value is owed.

**P2 — the discrimination tests were narrative.** Now
`tools/tests/test_a1_v2_ledger_gate.py`, twelve executable cases, 12/12 as
specified.

**Not disputed:** the S1 correction of §4 was independently confirmed against
blob `3b26849c`.

## 8. For the independent reviewer (revision E-R2)

The mechanism is now the object worth reviewing, and it is reviewable
independently of any field set — which is the argument for looking at it before
one grows on top.

1. **The schema extractor is a stub that returns the undeclared form.** Its
   `--check` therefore proves the committed facts are empty, not that they came
   from a schema. The trust model is fixed, but its first real test arrives with
   the first real schema. What should `--check` assert then — re-extraction
   equality, or something stronger, given the extractor and the schema will be
   written by the same person?
2. **Step 3 compares domains per path, with both sides authored by the drafter.**
   The schema side is now extractor-produced, which helps — but only once a
   schema exists. Is there a case where a *correct* extractor and a *correct*
   ledger still admit a wrong realization?
3. **Clause 5 as implemented.** A carrier's target is `AnyCommittedEnvelope`
   verbatim. That is faithful to the frozen text, but it means the ledger never
   names a concrete feed target anywhere. Does anything then check that a
   *runtime* `subject_refs` occurrence carries a member and not an arbitrary
   kind, or does that check now live only in the resolver?
4. **The corpus authors the mutations and the gate in the same idiom.** Thirteen
   step cases came from the same mental model as the code they test. Which
   plausible v2 drafting error is still uncovered?
5. **Blob pinning and the next legitimate move.** `PINNED_BLOB` fails the run if
   Phase G's document changes at all — including a supersede that Phase G's own
   rules permit. Is "re-pin deliberately" enough of a procedure, or does a
   supersede need a recorded ceremony here the way it does in the contract?
6. **Whether this is now PR-ready.** The gate is RED by content and green by
   construction; the suggestion was to open a draft PR at exactly this point.
   Is the mechanism stable enough that external review would be reviewing the
   machine rather than a moving target?
