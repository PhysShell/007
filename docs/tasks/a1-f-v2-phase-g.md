# A1-F v2 — Phase G: graph adjudication

**Status: DECIDED (revision G-R9) — AWAITING RE-REVIEW.**

Seven review rounds. G-R1 moved the count from 13 to 11; G-R2 corrected the
binding lifecycle and the rank domain; G-R3 attempted the exact edge universe
and measured it against the wrong baseline; G-R4 rebuilt it on the frozen
contract but stopped at node *classes*; G-R5 added the missing layer, a semantic
edge registry; G-R6 made that registry exact enough to reject; G-R7 fixes the
three things an exact registry could finally expose — a flat extractor that
never saw a nested schema, a bound external contract this document had quietly
redefined, and an open target miscounted as a closure terminal; G-R8 closes the
two specification holes left between the graph and any implementation of it: the
field↔edge realization contract, and the meta-target's membership; G-R9 gives
both a checkable carrier — a required machine-checked realization ledger, and a
replay-checkable `COMMITTED` predicate — and rejects one invariant §8 had
proposed. The registry is 69 exact semantic edges over a 41-slot frozen baseline
(§4.2), unchanged by G-R8 and G-R9 alike — no row has moved in either. Field-path
spelling remains owed to the v2 draft. See §9.

Phase G is one decision, written and reviewed on its own, before any v2 drafting.
The node set determines ranks, edges, imported roots, closure and digest domains;
drafting v2 with the node set still open means drafting it twice.

## 0. Inputs and authority

```yaml
frozen_v1_baseline:
  commit: b84e9419e751179319925bbc57a434df3583a29a
  blob:   7db92f1b3dc9d7040da074956a0b3f2f200174c8
  role:   the model being superseded; its rules still bind the argument

design_input:
  commit: 37502e3ce5c397a7437445aafb88c13d84ba4ac0
  crate:  crates/o7-a1-protocol
  role:   EVIDENCE. Never authority.

implementation_probe:
  pr:     124 @ b2ba165
  role:   EVIDENCE for E-V0-4 only (see section 6)
```

**Prototype code is evidence, never a presumption.** The design input has fifteen
envelope kinds. That fact carries exactly as much authority as any other
observation about how somebody once wrote a Rust enum. The number this document
reaches is derived from consumers, not from a `#[derive]`.

## 1. Method

Default disposition for every existing v1 classification is `KEEP_V1_MODEL`. The
burden of proof lies on promoting a support object, introducing a node, changing
imported-root semantics, or replacing rank as the authority.

Per-object test, applied identically to every candidate:

```text
Does it independently require:
  1. logical message identity?
  2. causation / lineage?
  3. acceptance / idempotency lifecycle?
  4. independent replay addressability?
  5. controller / provider / human producer authority?
Insufficient answers -> it remains a support object.
```

The leading evidence is **in-degree**: which other objects reference it, from how
many distinct kinds, and across what edge class.

It is not sufficient evidence, and this document's own table shows why:
`WorkOrder` has in-degree 0 and is unambiguously a message — it is a round's
entry point, with its own lifecycle and identity. So zero degree establishes only

```text
no downstream reference-driven evidence for independent graph identity
```

and never "no identity needed". Where in-degree is zero, the second half of the
test — an independent lifecycle, acceptance or producer-authority requirement —
has to be answered on its own, not skipped because the first half came out
empty.

## 2. Evidence: the reference graph of the design input

Mechanically derived from `crates/o7-a1-protocol/src/edges.rs` at `37502e3`.
The metric needs a name and an exclusion rule, or an independent reviewer runs
"the same" script and gets a different number — which, after the 141-row
episode, would be a particularly avoidable way to repeat a lesson.

```text
registry total                                59 entries
  envelope-source edges          e(...)       53
  A2 attention-transition edges  t(...)        6
plus, in the registry KAT, one global rule:
  GLOBAL | causation.blob_ref | AnyCommittedEnvelope | Causal

specific V0 consumer in-degree(X) :=
  count of RETAINED V0 typed-node edges whose target is exactly X

  where a typed-node source is
    - an envelope-bearing message kind, OR
    - a typed support object / authority participating in the graph

  EXCLUDING
    - the 6 A2 transition-source edges          (not V0)
    - any open/generic target (AnyCommittedEnvelope, AnyImportableCas) —
      generic reachability is not evidence that a specific consumer needs
      this object
    - edges retained only for a POST-V0 feature (§3.5)
    - edges of objects that leave V0 (§3.4)

  "retained" is defined exactly by the V0 edge ledger of §4.1.
```

**This is the historical prototype projection @ `37502e3`, not the retained V0
edge set (G-R7, renamed in G-R8).** The formula above is preserved as the
definition that *was* used, including its now-false closing line — "retained is
defined exactly by the V0 edge ledger of §4.1" — because reproducing the numbers
requires the definition that produced them. It is not a live metric, and §4.2.4
is the edge authority.

It stays because it is what drove §3's adjudications. Several counts are
prototype-era — `ProviderInvocationReceipt` is shown with `HumanAttentionRequest`
as a consumer, an evidence target G-R6 removed — and regenerating the table from
§4.2.4 would be circular, since the registry is downstream of the decisions this
evidence supported. The post-decision in-degrees are published separately at the
end of this section.

The definition was rewritten in G-R3. Until then it said "envelope-source edges",
which stopped being derivable the moment G-R1/G-R2 retyped three sources to
support: the numbers in the table were right, but they no longer followed from
the definition above them.

The open-target exclusion matters for honesty, not only for arithmetic: as a
prototype envelope kind, `ArtifactImported` is formally reachable through the
global causation rule, so its raw in-degree is not zero. Its *specific consumer*
in-degree is. The conclusion in §3.4 is unchanged, but it now rests on a metric
that says what it counts.

The table gives three separate quantities, because conflating them is how the
previous revision ended up with one table holding two projections and no labels.
`V0 out` and `V0 in` are over the retained V0 edge set (§4.1); `raw out` is the
prototype's own out-degree at `37502e3`, shown only where it differs.

| Candidate kind | V0 out | **V0 in** | raw out | referencing kinds |
|---|---:|---:|---:|---|
| `WorkOrder` | 6 | 0 | 6 | — (entry point; see §1 on why this is not evidence against message status) |
| `CoderReport` | 3 | 1 | 3 | CandidateAdmissionReceipt |
| `CandidateAdmissionReceipt` | 3 | 2 | 3 | ReviewRequest, … |
| `ReviewRequest` | 4 | 0 | 4 | — |
| `ReviewerReport` | 2 | 1 | 2 | ReviewVerdict |
| `ReviewVerdict` | 3 | 3 | 3 | incl. `ProviderInvocationReceipt` — a support node referencing a message |
| `CorrectiveDirective` | 3 | 0 | 3 | — |
| **`ProviderInvocationReceipt`** *(typed support)* | 6 | **3** | 9 | CoderReport, ReviewerReport, HumanAttentionRequest. Raw 9 includes the three SafeRedrive edges that §3.5 sends POST-V0 |
| **`InteractionManifest`** *(typed support)* | 3 | **1** | 3 | ProviderInvocationReceipt only |
| `CampaignFeedItem` | 1 | 0 | 1 | — |
| `HumanAttentionRequest` | 5 | 1 | 5 | — |
| `HumanCommandRequest` | 1 | 1 | 1 | HumanDecision |
| `HumanDecision` | 3 | 0 | 3 | — |
| **`CampaignRunBinding`** *(typed support)* | 5 | **2** | 5 | CandidateAdmissionReceipt, ProviderInvocationReceipt — both `Intra`, both controller-produced. A third raw in-edge, `safe_redrive.prior_run_binding_ref` (`Causal`), is excluded: its only consumer is POST-V0 (§3.1) |
| **`ArtifactImported`** *(leaves V0)* | — | **0** | 2 | **no specific consumer** (formally reachable only through the global open-target causation rule, which the metric excludes) |

External wrapper **raw explicit in-degree** — a deliberately different metric,
counted without the V0 exclusions so that POST-V0 references stay visible:
`CandidateStateReceipt` 6, `CandidateMaterialization` 3,
`WorktreeMaterialization` 2, `RunContractCandidateState` 2,
`RunArtifactSource` 1 (from `ArtifactImported` alone),
`EstablishedNonDispatchEvidence` 1 (from the SafeRedrive cause alone — a POST-V0
reference, which is exactly why this column is not the V0 metric).

**Post-decision in-degrees, derived from §4.2.4 (G-R7).** These are the numbers
that now hold; the table above is what the decision was made *on*. Event-kind
sources are excluded from the "referencing kinds" column, since every message is
referenced by its own event.

| Kind | out | **in** | referencing kinds (non-event) |
|---|---:|---:|---|
| `WorkOrder` | 3 | 1 | — |
| `CoderReport` | 2 | 3 | `CandidateReceipt`, `ReviewRequest` |
| `CandidateReceipt` | 3 | 2 | `ReviewRequest` |
| `ReviewRequest` | 6 | 1 | — |
| `ReviewerReport` | 4 | 2 | `ReviewVerdict` |
| `ReviewVerdict` | 4 | 2 | `CorrectiveDirective` |
| `CorrectiveDirective` | 2 | 1 | — |
| **`ProviderExecutionReceipt`** | 7 | **3** | `CoderReport`, `ReviewerReport`, `ProviderExecutionRecordedPayload` |
| **`InteractionManifest`** | 3 | **1** | `ProviderExecutionReceipt` |
| **`CampaignRunBinding`** | 5 | **2** | `CandidateReceipt`, `ProviderExecutionReceipt` |
| `ScopeContract` | 0 | 4 | `WorkOrder`, `ReviewRequest`, `CorrectiveDirective`, `CampaignCreatedPayload` |
| `CampaignFeedItem` | 1 | 1 | — |
| `HumanAttentionRequest` | 0 | 1 | — |
| `HumanCommandRequest` | 0 | 2 | `HumanDecision` |
| `HumanDecision` | 1 | 1 | — |

The three support decisions of §3 survive the regeneration unchanged, which is
the point of publishing it: `ProviderExecutionReceipt` still has in-degree 3 from
distinct consumers, `InteractionManifest` 1, `CampaignRunBinding` 2 — the exact
pair §3.1's argument rests on. The one substantive movement is that the receipt's
third consumer is now `ProviderExecutionRecordedPayload` rather than
`HumanAttentionRequest`, which strengthens §3.2 rather than weakening it: a
reducer payload is a more specific V0 consumer than an attention's evidence list
ever was.

## 3. Q1 + Q2 — node universe, and the envelope/support boundary

### 3.1 `CampaignRunBinding` → existence **REQUIRED_V0**, classification **TYPED SUPPORT AUTHORITY**

Existence was never the question: it is the only object bridging logical
campaign/round/role to physical execution/conversation/run/attempt plus the input
state actually materialized, and V0 needs that bridge.

Classification is a different question, and the first pass answered it with two
arguments that do not survive checking:

- **The `Causal` SafeRedrive edge cannot count.**
  `ProviderInvocationReceipt.cause.safe_redrive.prior_run_binding_ref` is the
  only cross-round in-edge, and SafeRedriveV2 is POST-V0 (§3.5). Using it as
  V0 promotion evidence lets a post-V0 consumer determine pre-V0 wire ontology,
  which is backwards.
- **"Two distinct producer lanes" is factually wrong.** `required_producer()` at
  `37502e3` (`edges.rs:538-541`) classifies `CoderReport` as `Provider("coder")`,
  `ReviewerReport` as `Provider("reviewer")`, `HumanCommandRequest` as `Human`,
  and *everything else*, including both `CandidateAdmissionReceipt` and
  `ProviderInvocationReceipt`, as `Controller`. Both referencing kinds are one
  lane.

What remains is specific V0 consumer in-degree **2**, both `Intra`, both
controller-produced. The envelope-specific question is then:

```text
Which V0 invariant requires of CampaignRunBinding
    a message_id, envelope causation/lineage, and an
    acceptance/idempotency lifecycle
that cannot be provided by
    typed support identity + the exact edge registry
    + a controller uniqueness invariant?
```

No such invariant was found. Content identity suffices, since it is referenced by
digest, and its lineage is carried in-band, which is exactly what a support
object is entitled to do (§3.2 explains why that is not a defect).

**Uniqueness, corrected.** G-R1 wrote that "one binding per execution" is
enforceable when the *referencing* artifact is accepted. That is the wrong
authority point, and wrong in the direction this project exists to avoid. The
design input allocates `run_id`/`attempt_id` durably *before* provider dispatch,
and the binding is the source of that physical identity, so the real order is:

```text
allocate execution E -> construct binding B -> PROVIDER DISPATCH
                                            -> ... later ...
                                            -> ProviderInvocationReceipt
                                            -> CandidateAdmissionReceipt
```

Checking uniqueness only when a later referencing artifact is accepted checks it
after the side effect has already happened. Two bindings for one execution could
both clear the pre-dispatch stretch, and rejecting one of them afterwards repairs
nothing. That is the exact inversion of R1's "durable acceptance before provider
invocation".

The invariant Phase G requires, without choosing an event schema for it:

```text
Pre-dispatch binding admission

Before the first provider dispatch for execution E, the controller durably
establishes exactly one binding identity B for E.

  E + same B       -> idempotent replay
  E + different B  -> conflict, fail closed, NO provider dispatch

After a restart, E resolves to the same B before the execution may continue.
```

Where that authority point lives — a campaign event, reducer state, or another
controller-owned durable record — is a v2 drafting decision, not a Phase G one.
What Phase G fixes is that it must exist and must precede dispatch.

**This is a controller/reducer lifecycle obligation and is not, by itself, a
reason to promote the binding to a message.** Requiring durable pre-dispatch
admission says the *controller* must hold state before acting; it says nothing
about the binding needing a `message_id`, envelope causation, or an acceptance
lifecycle of its own.

**Reopening trigger, recorded so it cannot be quietly skipped:** if the v2 draft
turns out to be unable to express pre-dispatch binding admission without giving
the binding a message lifecycle, this adjudication reopens. No present evidence
suggests it will.

Independently addressable graph authority is not the same thing as an
envelope-bearing message, and v1 already demonstrates the difference: the
execution receipt and the interaction manifest participate in the evidence DAG,
carry typed refs and are replay-addressable, without envelopes of their own.

**Disposition:** a new typed support authority in v2. Promotion remains available
if a later phase produces the envelope-specific invariant this one could not.

### 3.2 `ProviderInvocationReceipt` → **KEEP_V1_MODEL** (support object)

The first pass promoted it. The argument was wrong, and it is worth recording
exactly how, because the error is a general one.

**Withdrawn: the FD-5.3 argument.** The claim was that v1 forces the receipt to
violate FD-5.3 by carrying `campaign_id` and `round_id` in its own payload. It
does not. FD-5.3 governs *payloads of envelope-bearing artifacts* — "payloads
never restate an envelope-owned field". The receipt has no envelope by
construction, so those fields are not restating anything; they are its own
provenance.

**Withdrawn: "promotion removes two congruence predicates".** It does not. FD-11
checks `report.envelope.campaign_id == receipt.campaign_id` to prove the receipt
belongs to the same campaign and round as the report being accepted. That is
cross-object provenance congruence. After promotion it would simply read
`receipt.envelope.campaign_id` — the obligation survives verbatim. The first pass
mistook a cross-object equality proof for intra-object denormalization. Those are
different things, and one of them is not a defect at all.

**Insufficient on its own: in-degree 3.** A content-addressed support object may
have any number of consumers, and v1 demonstrates exactly that. Being referenced
is not being a message.

No envelope-specific invariant remains. Under this document's own default rule,
the disposition is `KEEP_V1_MODEL`: the receipt stays a support object, and the
frozen classification stands.

### 3.3 `InteractionManifest` → **KEEP as a typed support object**

In-degree 1, from `ProviderInvocationReceipt` alone. No consumer outside its own
receipt, no independent acceptance, and its identity is meaningful only relative
to the execution it describes. Tests 1, 3 and 4 fail. Promotion in the design
input is symmetry with the receipt, not a consumer.

That symmetry is precisely what the burden of proof exists to refuse. Two objects
that appear together in one paragraph are not thereby the same kind of thing.

### 3.4 `ArtifactImported` → **NOT PROVEN — out of A1-V0**

**Specific V0 consumer in-degree = 0** (§2). No V0 object references it; its only
edges are outgoing, to `AnyImportableCas` and to `RunArtifactSource`. Its *raw*
degree is not zero — the global open-target causation rule formally reaches it —
which is why the metric is named rather than assumed.

Three questions were kept separate, and only the first two are answered here:

```text
run -> CAS import operation needed?     plausibly YES  (mechanism)
durable import proof needed?            plausibly YES  (mechanism)
independent graph node needed?          NOT PROVEN
envelope-bearing message needed?        NOT PROVEN
```

An import mechanism can exist as a controller procedure whose product is an
ordinary CAS object with a recorded provenance, without minting a node.

v1's refusal of `ArtifactAcceptance` (FD-2.3) is worth remembering here as a
**caution against speculative nodes**, and not as proof: the two objects do the
same thing to a graph but not the same thing to a system, and treating the
resemblance as identity would import a conclusion instead of an argument. The
disposition rests on two findings together, since §1 establishes that the first
alone is not sufficient:

```text
specific V0 consumer in-degree = 0
AND no independent V0 message identity, lifecycle, acceptance or producer
    invariant has been demonstrated for it
=> node / message status NOT PROVEN
```

The second conjunct is what separates this from `WorkOrder`, which also has
in-degree 0 and is a message on the strength of exactly that second test.

Consequence: `RunArtifactSource`, whose only in-edge comes from
`ArtifactImported`, leaves the V0 wrapper set with it.

### 3.5 `EstablishedNonDispatchEvidence` → **POST_V0_DEPENDENT_ON_SAFE_REDRIVE**

In-degree 1, exclusively from `ExecutionCause::SafeRedrive`. With SafeRedriveV2
deferred, it has no V0 consumer. The design is preserved as a prepared record for
v2.1 rather than discarded.

This carries a standing condition, recorded because it is exactly the kind of
thing a word can quietly take with it: **the FD-14.7a barrier is MUST-V0 and is
unaffected.** Post-dispatch ambiguity safety is not automatic pre-dispatch
redrive. Deferring `EstablishedNonDispatchEvidence` defers the *resolution path*,
never the *barrier*.

### 3.6 Result — the v2 node universe

```text
envelope-bearing kinds: 11
  the eleven v1 kinds, unchanged — KEEP_V1_MODEL on the boundary question

typed support objects / authorities:
  + CampaignRunBinding          (3.1, REQUIRED_V0, new support authority)
    ProviderInvocationReceipt   (3.2, kept — promotion argument withdrawn)
    InteractionManifest         (3.3, kept)
    ScopeContractV1             (v1, unchanged)
    CampaignEventPayload        (v1, unchanged)

out of V0:
    ArtifactImported            (3.4, not proven)
    RunArtifactSource           (3.4, falls with it)
    EstablishedNonDispatchEvidence (3.5, post-V0)
```

**Eleven.** The envelope boundary of the frozen contract survives adjudication
unchanged, and V0 gains one new object that is an authority without being a
message.

The first pass reached thirteen. It got there by promoting two objects on
arguments that did not survive checking (§3.1, §3.2) — and the shape of that
error is worth keeping in view, because both promotions were reached after
reading a fifteen-variant enum. `KEEP_V1_MODEL` was declared the legitimate
default at the top of this document; the discipline is only real when the default
actually wins something.

## 4. Q3 — edge and rank model

**The exact edge registry becomes authoritative. Rank becomes derived.**

Evidence from v1's own history rather than from preference: rank had to be
redefined twice under pressure. R5.1 restated rank 0 from "opaque bytes" to
"terminal in the reference graph" for `ScopeContractV1`, and FD-2.4 had to carve
imported roots into it separately. A property that needs redefining every time a
new object arrives is behaving like a derived quantity being used as a primitive.

Frozen for v2:

```text
authority:  a closed per-kind allowed-edge registry — exact source, exact
            field-path tag, exact target, edge class

derived_rank :=
    topological level over the closed INTRA A1 TYPED-NODE subgraph, where the
    typed nodes are:
      - envelope-bearing message kinds, and
      - typed support objects / authorities that participate in the graph
        (ProviderInvocationReceipt, InteractionManifest, CampaignRunBinding,
         ScopeContract, CampaignEventPayload)
    Graph-terminal external wrapper and CAS targets terminate traversal.

    No global rank is defined over Causal edges, and none can be: the full
    graph does not topologically sort, because cross-round causal references
    are acyclic per instance rather than per kind.
    Retained as a checkable property and as review shorthand, never as the rule.

acyclicity: Intra   — proved by topological sort over the typed-node subgraph
            Causal  — proved per instance by create-before-reference
            never asserted from rank monotonicity
```

Two rounds of narrowing, in opposite directions, and the second is the one worth
explaining. The first pass wrote "rank, computable from the registry", which is
too wide. G-R1 then narrowed it to the *message-kind* subgraph, which is too
narrow — and wrong precisely because of what G-R1 had just decided. The three
demoted objects are not terminals: `ProviderInvocationReceipt` has out-degree 9,
`CampaignRunBinding` 5, `InteractionManifest` 3. A chain like

```text
CoderReport [message] -> ProviderInvocationReceipt [support]
                           |-> InteractionManifest [support]
                           `-> CampaignRunBinding  [support] -> External / CAS
```

would fall out of the proof entirely if the domain were messages only. Frozen v1
had this right and is the precedent: its ranks put `InteractionManifestV1` at 1
and `ProviderExecutionReceiptV1` at 2, beneath the reports — a ladder over the
whole typed reference graph, not over envelopes.

Consequence for the registry: a typed support object must be admissible as an
edge **source**, not only as a target. §4.1 discharges that, and closes the
mandate's "exact registry" clause.

Deciding an object is not a message does not make it stop being a node. Naming
the two classes separately is what makes that mistake visible instead of
structural.

The registry's two edge classes are adopted as they stand: `Intra` (within one
round's derivation flow, must topologically sort at kind level) and `Causal`
(crossing rounds, chains or attention lineage; instance-acyclic by
create-before-reference). The distinction is load-bearing and rank cannot
express it at all, which is independent evidence for this change: the two classes
are proved acyclic by different arguments — kind-level topological sort versus
per-instance create-before-reference — and a single scalar cannot carry two proof
obligations. (G-R1's earlier justification, that §3.1's promotion turned on a
`Causal` edge, died with that promotion and is not reused here.)

One open target is sanctioned, unchanged from the design input's rationale:
`AnyCommittedEnvelope`, for `CampaignFeedItem` causation. `AnyImportableCas`
leaves with `ArtifactImported` (§3.4).

This is the one place where §4.2.4's "no wildcard" and this paragraph would
otherwise contradict each other, and G-R6 resolves it in one direction rather
than leaving both statements standing: the sanction survives, and it survives as
a **row in the registry** (`CampaignFeedItem → AnyCommittedEnvelope`, `Causal`).
What §4.2.4 excludes is a wildcard *source* and a class row; a single named open
target, adjudicated once and visible in the table, is not the same thing as rank
admitting targets by rule. If v2 wants a second one it has to add a row here.

### 4.1 The exact V0 edge universe

**G-R3's ledger measured against the wrong side, and this subsection replaces
it.** It enumerated all 59 rows of the prototype registry and labelled them
`KEEP` / `RETYPE` / `POST_V0` / `REMOVE` / `A2`. Every one of those labels is
relative to `37502e3`, so `KEEP` meant *unchanged relative to the prototype* —
while §0 of this document says the prototype is evidence and never authority,
and §1 makes `KEEP_V1_MODEL` the default. The registry, of all things, was
derived from the one input that has no authority.

The scale is not marginal. Matching every prototype tag against the frozen
schemas by name:

```text
prototype envelope-source rows                         53
  with a frozen v1 slot of the same name               15
  with NO frozen counterpart                           38
```

So 30 rows were labelled `KEEP` when the frozen contract does not contain them
at all. The clearest single case: `WorkOrder.goal.contract_blob → ContractBlob`
was `KEEP`, and the string `contract_blob` appears **zero** times in blob
`7db92f1b`. Frozen `WorkOrderV1` carries `scope_ref → ScopeContractV1` instead.
That is not an unchanged edge; it is a v1 surface removed and a prototype
surface proposed in its place, and the two need different words.

Two further consequences of the same error:

- `ProviderInvocationReceipt → CampaignRunBinding` was labelled `RETYPE`, but
  `CampaignRunBinding` does not exist in v1 at all. It cannot be a retyping of
  something that was never there; §3.1 argued its existence precisely because it
  is **new**.
- The prototype branch never implemented the reducer, so its registry has zero
  rows for `CampaignEventV1` and its payloads — while §4 of this document lists
  `CampaignEventPayload` among the typed nodes participating in the graph. A
  registry claiming to be closed while missing an entire declared node class is
  not closed.

#### 4.1.1 The baseline: the frozen v1 reference inventory

Extracted mechanically from blob `7db92f1b` across §3.0–§3.15.2: every schema
row whose type is `ArtifactRef` or `[ArtifactRef]`, **plus every such row reached
by transitively expanding a cross-schema type reference**.

```text
frozen ArtifactRef-valued slots                        41
  message payloads                                     18
  typed support objects                                11
  event payload schemas                                 7
  common envelope                                        2
  campaign event log root                                2
  reachable only through a referenced sub-schema         1
```

Two extraction misses are recorded here, because an inventory whose own
derivation goes unchecked is the failure mode this track has now hit four times.

**Miss 1 — combined rows.** A first pass returned 34; a completeness check found
six more carried in combined table rows (`usage_ref` / `cost_ref`, and the two
`interaction_sequence[]` pairs). That gave 40.

**Miss 2 — the extractor was flat, not recursive (G-R7).** 40 counted only rows
whose *own* type cell reads `ArtifactRef` or `[ArtifactRef]`. Frozen §3.6 types
`ReviewVerdictV1.findings` as **`as §3.5, validated`**, and §3.5's finding
structure contains `findings[].evidence_refs | [ArtifactRef]`. The verdict
therefore carries a reference surface reachable only by expanding a referenced
sub-schema — and frozen §3.6's acceptance predicate proves it is live: *"every
evidence_ref resolvable, rank rule and closure bounds satisfied"* is one of the
conditions for accepting a `ReviewVerdict`.

G-R6 half-knew this: §4.2.3 discussed the prototype's
`ReviewVerdict.findings[].evidence_refs` edge while §4.2.1's open-surface list
had no frozen counterpart for it. A document arguing with itself across two
subsections is the shape this failure takes.

**The fix is the extractor, not the number.** Every type cell is now expanded
transitively. Running that over the normative body finds 37 direct rows (→ 40
slots after the combined-row expansion) and exactly **five** cross-schema type
references:

```text
3.6   findings                                  as §3.5, validated   -> +1 slot
3.6   properties_checked/_preserved/residual...  as §3.5             -> [Text], 0
3.11  command                                    enum as §3.10       -> enum,   0
3.15.2 execution_outcome                         enum as §3.12       -> enum,   0
3.15.2 attempted_event_kind                      enum as §3.15.1     -> enum,   0
```

So the total is **41**, and — this is the part the number alone does not say —
there is no *second* hidden slot. The four remaining cross-references expand to
scalars. Without the recursive pass that would have been a hope; with it, it is
a result.

| # | owner | owner class | frozen field path | V0 disposition |
|---:|---|---|---|---|
| 1 | `Common envelope v1` | envelope | `artifact_refs` | KEEP |
| 2 | `Common envelope v1` | envelope | `provider_execution_receipt_ref` | KEEP |
| 3 | `WorkOrderV1` | message | `input.candidate_ref` | KEEP |
| 4 | `WorkOrderV1` | message | `input.materialization_attestation_ref` | KEEP |
| 5 | `WorkOrderV1` | message | `scope_ref` | KEEP |
| 6 | `CoderReportV1` | message | `claims[].evidence_refs` | KEEP |
| 7 | `CoderReportV1` | message | `diagnostic_runs[].artifact_ref` | KEEP |
| 8 | `CandidateReceiptV1` | message | `candidate_ref` | KEEP |
| 9 | `CandidateReceiptV1` | message | `coder_report_ref` | KEEP |
| 10 | `ReviewRequestV1` | message | `candidate_receipt_ref` | KEEP |
| 11 | `ReviewRequestV1` | message | `scope_ref` | KEEP |
| 12 | `ReviewRequestV1` | message | `evidence_refs` | KEEP |
| 13 | `ReviewRequestV1` | message | `coder_report_ref` | KEEP |
| 14 | `ReviewerReportV1` | message | `findings[].evidence_refs` | KEEP |
| 15 | `ReviewVerdictV1` | message | `reviewer_report_ref` | KEEP |
| 16 | `CorrectiveDirectiveV1` | message | `review_verdict_ref` | KEEP |
| 17 | `CorrectiveDirectiveV1` | message | `scope_ref` | KEEP |
| 18 | `CampaignFeedItemV1` | message | `subject_refs` | KEEP |
| 19 | `HumanAttentionRequestV1` | message | `evidence_refs` | KEEP |
| 20 | `HumanDecisionV1` | message | `command_request_ref` | KEEP |
| 21 | `ProviderExecutionReceiptV1` | typed support | `interaction_manifest_ref` | KEEP |
| 22 | `ProviderExecutionReceiptV1` | typed support | `final_normalized_output_ref` | KEEP |
| 23 | `ProviderExecutionReceiptV1` | typed support | `canonical_request_ref` | KEEP |
| 24 | `ProviderExecutionReceiptV1` | typed support | `raw_provider_event_ref` | KEEP |
| 25 | `ProviderExecutionReceiptV1` | typed support | `normalized_output_ref` | KEEP |
| 26 | `CampaignEventV1` | event log root | `source_ref` | KEEP |
| 27 | `CampaignEventV1` | event log root | `evidence_refs` | KEEP |
| 28 | `Event payload schemas` | event payload | `scope_ref` | KEEP |
| 29 | `Event payload schemas` | event payload | `receipt_ref` | KEEP |
| 30 | `Event payload schemas` | event payload | `results[].log_ref` | KEEP |
| 31 | `Event payload schemas` | event payload | `checks[].observation_ref` | KEEP |
| 32 | `Event payload schemas` | event payload | `detail_ref` | KEEP |
| 33 | `Event payload schemas` | event payload | `termination_observation_refs` | KEEP |
| 34 | `Event payload schemas` | event payload | `evidence_refs` | KEEP |
| 35 | `ProviderExecutionReceiptV1` | typed support | `dispatches[].usage_ref` | KEEP |
| 36 | `ProviderExecutionReceiptV1` | typed support | `dispatches[].cost_ref` | KEEP |
| 37 | `InteractionManifestV1` | typed support | `interaction_sequence[].input_ref` | KEEP |
| 38 | `InteractionManifestV1` | typed support | `interaction_sequence[].output_ref` | KEEP |
| 39 | `InteractionManifestV1` | typed support | `interaction_sequence[].arguments_ref` | KEEP |
| 40 | `InteractionManifestV1` | typed support | `interaction_sequence[].result_ref` | KEEP |
| 41 | `ReviewVerdictV1` | message | `findings[].evidence_refs` *(via `as §3.5`)* | KEEP |

**Disposition.** Every frozen slot is `KEEP` under §1's default: no adjudication
in §3 removes a v1 reference surface. What §3 changed is the *node* set, and its
effect on this inventory is narrower than G-R3 implied:

```text
ProviderExecutionReceipt, InteractionManifest
    already typed support in v1 -> the retypings of G-R3 restore v1's own
    classification rather than change it, so their slots are unaffected

CampaignRunBinding
    new in v2 (§3.1) -> adds edges, removes none

ArtifactImported, RunArtifactSource, EstablishedNonDispatchEvidence
    absent from v1 -> nothing to remove from this inventory; the prototype rows
    that carried them are proposals that are not adopted (§4.1.2)
```

#### 4.1.2 The prototype rows, demoted to evidence

The 59 rows keep their value as evidence — they are how §3's in-degree arguments
were made — but they are classified against the baseline, not as the baseline:

```text
MATCHES_V1              15   a frozen slot exists with the same name
PROPOSED_REPLACEMENT         a prototype surface offered for a frozen one
PROPOSED_NEW                 a surface with no frozen counterpart
POST_V0                  3   the SafeRedrive path (§3.5)
REJECTED                 2   ArtifactImported's own edges (§3.4)
A2                       6   attention transitions, outside V0
```

The middle two classes together hold the 38 unmatched rows, and splitting them
row by row is **not a Phase G decision**. A `PROPOSED_REPLACEMENT` is a claim
about what a v2 payload should carry — `contract_blob` versus `scope_ref`,
whether `CorrectiveDirective` carries its own input state, whether
`HumanCommandRequest` gains an authenticated-principal reference. Those are
payload-schema questions, and §7 of this document already says payload shapes
are v2 drafting output.

#### 4.1.3 What Phase G closes, and the boundary it cannot cross

There is a real circularity in "close the exact registry": an exact registry is
keyed by field path, field paths are payload schema, and payload schemas are the
v2 draft. Phase G cannot write field paths for objects whose payloads it does
not decide without either adopting the prototype's schemas wholesale — which is
the authority inversion this subsection just corrected — or inventing v2 schemas
under the heading of graph adjudication.

So the registry closes at the level that actually determines what Phase G owns —
acyclicity, the rank domain, and closure traversal — and no further:

```text
CLOSED by Phase G — the semantic edge registry (§4.2)

  semantic edge := exact source kind -> exact target kind + edge class
  field path    := the wire realization of an admitted semantic edge,
                   decided by the v2 draft

OWED to the v2 draft — an obligation, not a hope

  the field-path spelling of every admitted semantic edge
  a proof that each field realizes exactly ONE admitted edge and adds none
```

G-R4 stopped one layer short of this and published a table of node *classes*
instead — `message -> message | typed support | external | CAS` and three more
rows like it. That is very nearly the cartesian product of the class set, and it
cannot prove an `Intra` DAG: it says a message may reference a message, not
which messages, so a two-cycle between two message kinds is not excluded by
anything in it.

It was also not even a superset of the graph it claimed to retain. Frozen
`CampaignTerminalErrorPayloadV1.evidence_refs` carries `rank ≤ 4`, so an event
payload may reference rank-3 and rank-4 objects — messages — while G-R4's table
had no `event payload -> message` row at all. A class universe that forbids a
frozen retained edge is not a widening of the model; it is a different model
that happens to be vaguer.

#### 4.1.4 `CampaignEventV1` is a source-only log root

The frozen contract does not make the campaign event an `ArtifactRef` target,
and nothing should. Frozen:

```text
CampaignEventV1     persisted log entry; source of source_ref and evidence_refs;
                    identified by its own framed digest and its chain position;
                    NEVER the target of an ArtifactRef

CampaignEventPayload  typed stored object; its own outgoing ref slots are the
                    seven in §4.1.1; a legitimate ArtifactRef target
```

This is a Phase G question rather than a drafting one because it decides whether
the closure resolver walks event references at all. It does: FD-14.2's
`resolve_event` enumerates `immediate_refs` over `source_ref`, `evidence_refs`
and the payload's declared slots, under the same bounds as artifact closure.
Event references are traversed exactly as deterministically as artifact
references, and the log root's exclusion from target position is what keeps that
traversal a DAG rather than a cycle through the log.

#### 4.1.5 The renamings are a supersede decision, not a script artefact

`CandidateReceipt` versus `CandidateAdmissionReceipt`, and
`ProviderExecutionReceipt` versus `ProviderInvocationReceipt`, are different
spellings of the same object, and FD-1.9 closes `ArtifactKindV1` spellings as a
digest input. G-R3 used the prototype's names throughout its ledger without
noticing that it was thereby proposing a wire change.

Phase G records the choice as open and owed:

```text
if v2 keeps the frozen spellings   no version consequence
if v2 adopts the prototype names   an explicit supersede of FD-1.9, with the
                                   envelope_version bump that already applies,
                                   recorded as a decision rather than inherited
                                   from whichever branch a script read
```

Phase G does not decide it. Phase G refuses to let it happen by accident.

### 4.2 The semantic edge registry

**What this layer is.** Between "exact field path" and "four boxes of node
classes" sits the layer that actually carries the graph:

```text
semantic edge := exact source semantic kind
              -> exact target semantic kind (or a sanctioned open target)
               + edge class (Intra | Causal)

field path    := which field of which payload carries that relation
```

Phase G owns the first and not the second. That resolves G-R4's circularity
without denying it: edge *meaning* now, field *spelling* later.

#### 4.2.1 The frozen surfaces split two ways

The 41-slot inventory of §4.1.1 is not 41 semantic edges. Nine of those slots
are **open reference surfaces** — they name no target kind, and something other
than the field itself decides what may appear there:

```text
envelope.artifact_refs                        FD-2.1 rank rule, fully generic
CoderReport.claims[].evidence_refs            rank <= 2
ReviewerReport.findings[].evidence_refs       rank <= 2
ReviewVerdict.findings[].evidence_refs        rank <= 2, via `as §3.5`
CampaignFeedItem.subject_refs                 rank <= 4
HumanAttentionRequest.evidence_refs           rank <= 4
CampaignEventV1.evidence_refs                 rank <= 4
CampaignTerminalErrorPayload.evidence_refs    rank <= 4
ReviewRequest.evidence_refs                   no rank bound at all; the only
                                              constraint is ordering — "contract,
                                              diff, deterministic evidence first"
```

The last one is the sharpest case and was missed on this section's first pass.
Eight are governed by rank; `ReviewRequestV1.evidence_refs` is governed by
nothing but an ordering rule, so in v1 it admits *any* resolvable artifact. It
is the reviewer's entire input surface, and it is the widest hole in the frozen
graph.

G-R4 marked all 40 `KEEP`, which conflated keeping a *field* with keeping a
*graph relation*. None of these nine can be kept semantically unchanged. For
eight, what admits their targets is the rank rule — and §4 already demoted rank
from authority to derived property, so keeping them as written would let rank
become the admission authority again through the one door the registry does not
watch. For the ninth, there is not even a rank bound to demote.

**Frozen for v2:**

```text
A generic ArtifactRef-valued field creates no graph authority.
A field may realize only semantic edges admitted by the registry of §4.2.4.
Rank admits nothing; it is computed from the registry and checked against it.
```

So the nine surfaces survive as *surfaces* — the v2 draft may keep, narrow or
drop each one — while every target they may carry has to appear in the registry
on its own account. `KEEP_SURFACE` and `KEEP_EXACT` are different dispositions,
and only the second is a Phase G decision.

**And that obliges re-admission, not just removal.** Stripping rank authority
from a surface does not delete the references V0 actually needs across it; it
moves them from "admitted by a rule" to "admitted by name, or not at all". G-R5
did the stripping and stopped, which silently *narrowed* the graph. Each of the
nine is walked here, and each disposition is stated:

| Open surface | V0 disposition |
|---|---|
| `ReviewRequest.evidence_refs` | **NARROWED — three targets admitted** (§4.2.3). |
| `ReviewerReport.findings[].evidence_refs` | **NARROWED — three targets admitted** (§4.2.3). |
| `ReviewVerdict.findings[].evidence_refs` | **NARROWED — three targets admitted** (§4.2.3). |
| `CampaignFeedItem.subject_refs` | **Sanctioned OPEN target** — `AnyCommittedEnvelope`, adopted in §4 for feed causation. It appears in the registry as a row, so the sanction is visible rather than implied, and §4.2.5 treats it as a meta-target rather than a terminal. |
| `envelope.artifact_refs` | **No admitted target.** Fully generic and producer-populated; no frozen text names a required target. v2 must narrow it or justify each target it keeps. |
| `CoderReport.claims[].evidence_refs` | **No admitted target.** Frozen authority column: *advisory*. No V0 controller acceptance path reads it — `CandidateReceipt` acceptance turns on the A0 seal and claim-check, not on a claim's citations. |
| `HumanAttentionRequest.evidence_refs` | **No admitted target.** The V0 need was not shown (§4.2.3). This leaves the object isolated in the `Intra` subgraph, which is the visible price of the rule. |
| `CampaignEventV1.evidence_refs` | **No admitted target.** Generic across all 21 event kinds; unlike `source_ref`, frozen §3.15.1 fixes nothing per kind. |
| `CampaignTerminalErrorPayload.evidence_refs` | **No admitted target.** Rank ≤ 4 was the whole admission rule, and this is the row that broke G-R4's class table. |

"No admitted target" is a real V0 restriction, not a deferral in disguise: a v2
field over one of those surfaces has nothing to realize, so it cannot be drafted
without coming back here. That is the rule of §4.2.6 biting before the draft
exists rather than after.

That leaves **32 exact frozen slots**, and those are what §4.2.4 must account
for, row by row.

#### 4.2.2 Event kind and payload variant are semantic source identity

Frozen §3.15.1 fixes, per `event_kind`, which artifact kind `source_ref` points
at: `WorkOrderIssued → WorkOrder`, `CandidateAccepted → CandidateReceipt`,
`ReviewVerdictAccepted → ReviewVerdict`, and so on for the eleven of twenty-one
kinds that carry one. One field path, eleven distinct semantic edges.

G-R5 said that and then keyed the rows on `CampaignEventLog`, which throws the
distinction away at exactly the point it has to hold. A registry keyed that
coarsely admits `CampaignEventLog → ReviewVerdict` and therefore cannot reject

```text
CoderReportReceived.source_ref -> ReviewVerdict
```

The rejection would have to come from the per-event wire schema — and §4.2.6 has
just finished saying wire fields *realize* graph authority and never create or
narrow it. A registry that needs the wire to fix its own admissions is not the
authority it claims to be.

**Frozen for v2:**

```text
Where the frozen contract fixes targets per variant, the VARIANT is part of
semantic source identity. The registry is keyed on CampaignEvent(<event_kind>)
and on <PayloadVariant>, never on the log root or "the payload".
```

The same applies to the event payload. G-R5's six payload rows were the union of
targets belonging to six *different* payload schemas: `receipt_ref` is
`ProviderExecutionRecordedPayloadV1`'s and nothing else's, `detail_ref` is
`TransitionRejectedPayloadV1`'s, `termination_observation_refs` is
`CampaignCancelledPayloadV1`'s. Keyed on `CampaignEventPayload`, the registry
admitted every one of them for every payload kind.

Discriminating both costs 21 event-kind nodes and 11 payload nodes and buys the
property the layer exists for. It also yields a completeness check the coarse
form could not express: all **21** frozen event kinds now appear in the registry
— eleven as `Causal` `source_ref` sources, eleven as `Intra` payload containers,
`HumanCommandRejected` as both.

The `source_ref` edges are `Causal`: the log references artifacts created before
the entry, and create-before-reference is their acyclicity argument. The
containment edges are `Intra` — the payload is part of its own event.

#### 4.2.3 The 38 prototype rows, reduced to semantic proposals

Each unmatched prototype row reduces to a claim of the form *source kind may
reference target kind*, independent of what the field is called.

G-R5 then wrote that "the remaining 34 rows are field-path proposals over
already-admitted relations", and that sentence is false under G-R5's own new
rule. **Eleven** of the 53 prototype rows originate on one of the nine open
surfaces — four on `HumanAttentionRequest.evidence_refs`, two each on
`CoderReport.claims[].evidence_refs` and `ReviewRequest.evidence_refs`, one each
on `ReviewerReport`, `ReviewVerdict.findings[]`, and
`CampaignFeedItem.subject_refs`. Once rank stops admitting anything, none of
those targets is already admitted. They are semantic proposals, and they had
been filed as spelling.

**The `ReviewRequest` case, which is forced rather than optional.** Frozen §3.4:

```text
| evidence_refs | [ArtifactRef] | yes | ordered: contract, diff,
                                        deterministic evidence first |
```

Required, and the ordering is normative — frozen §3.4 closes with "Input
ordering is normative, not stylistic." So the frozen contract itself names three
target categories the reviewer's input must carry, and G-R5's registry gave
`ReviewRequest` no contract target, no diff target and no evidence target at
all. That is not a narrowing of v1; it is a `ReviewRequest` the reviewer cannot
act on.

G-R5 also rejected the contract target on a premise that does not hold —
that `ContractBlob` would *replace* `scope_ref → ScopeContractV1`. Frozen v1
carries both concepts and keeps them apart: `ScopeContractV1` is a rank-0 typed
leaf declaring `allowed_paths` (§3.13, and FD-2's rank-0 note), while frozen
`ArtifactKindV1` independently lists `contract_document`, `diff` and `gate_log`
as distinct kinds. The scope contract says what may change; the contract
document is the input describing what is being built. They are not competitors,
and the rejection was answering a question nobody asked.

`gate_log` is the deterministic-evidence target, and the argument is again from
frozen text rather than from the prototype: `ReviewRequestV1
.required_evidence_gate_ids` is a **required** field naming registry gate ids,
so the request already commits to which gates must have run. The artifact that
carries a gate's outcome is `gate_log`. `ci_observation` and `diagnostic_log`
are deliberately **not** admitted here — CI reaches the reducer through
`CiResultsAcceptedPayloadV1`, and diagnostics through the `CoderReport`, so
neither needs a second path into the reviewer's input.

| Semantic proposal | Disposition | Reason |
|---|---|---|
| `ReviewRequest → ContractDocument` | **ACCEPT** | Frozen §3.4 requires `evidence_refs` and normatively orders *contract* first. Distinct from `scope_ref`, per frozen `ArtifactKindV1`. |
| `ReviewRequest → Diff` | **ACCEPT** | Same row, second ordered category; `diff` is a frozen imported A0 kind. |
| `ReviewRequest → GateLog` | **ACCEPT** | Same row, third category. Forced by `required_evidence_gate_ids` being a required field of the same schema. |
| `CorrectiveDirective → CandidateStateReceiptRef` / `CandidateMaterializationRef` | **REJECT for V0 — `KEEP_V1_MODEL`** (reverses G-R5) | See below. |
| `HumanAttentionRequest → CandidateStateReceiptRef` | **REJECT for V0** | v1 carries `candidate_head` as a `CommitId`. No V0 need for a typed A0 reference was shown; the v2 draft may propose it again with one. |
| `HumanCommandRequest` / `HumanDecision → AuthenticatedPrincipal` | **REJECT for V0** | FD-15.2 deliberately records attestation as controller *observations*, not as a referenced object. A principal node is a new authority object, and §3 adjudicated none. |
| `ReviewerReport → ContractDocument` / `Diff` / `GateLog` | **ACCEPT** | See *review evidence*, below. |
| `ReviewVerdict → ContractDocument` / `Diff` / `GateLog` | **ACCEPT** | Same; frozen §3.6 types `findings` as `as §3.5, validated` and validates every `evidence_ref` before accepting the verdict. |
| `CoderReport.claims[] → GateLog` / `Diff` | **REJECT for V0** | Frozen authority column: *advisory*. No V0 controller acceptance path reads a claim's citations — `CandidateReceipt` acceptance turns on the A0 seal and the claim-check, not on what the coder cited. |

**Review evidence, re-adjudicated (G-R7).** G-R6 gave `ReviewerReport` and the
verdict's inherited surface *no admitted target*, on the argument that report
evidence is claim-authority and the controller re-derives. That conflates two
questions which FD-4 keeps apart:

```text
does reviewer evidence AUTHORIZE a transition?    no
may a reviewer canonically REFERENCE evidence?    yes
```

FD-4 answers the first. It says nothing about the second, and frozen §3.6
answers it in the affirmative twice over: `every evidence_ref resolvable, rank
rule and closure bounds satisfied` is one of the seven conditions for *accepting*
a `ReviewVerdict`, and `findings` is typed `as §3.5, validated` — a required
field. Evidence references are validated content of the accepted artifact, not
decoration on an untrusted one.

The decisive point is external to this document. §0 of the frozen contract binds
A1 to `docs/autonomy-controller.md` (accepted `c5b3ae0b`, PR #93) for "the
`ReviewVerdict` minimum", which A1 *consumes, never redefines*. That minimum
reads, at `autonomy-controller.md:151–163`:

```text
A future ReviewVerdict should at minimum bind:
    reviewed candidate head
    verdict: accepted | changes_requested | blocked
    finding identities and severities
    property or invariant affected
    evidence references          <-- this line
    ...
```

Zeroing both surfaces leaves no canonical review-evidence path anywhere in the
graph, and `reviewer_report_ref` cannot rescue it if the report is itself
forbidden to reference evidence. That is not Phase G narrowing a v1 surface; it
is Phase G redefining a bound contract it declared it would not touch.

**The admitted set, and why it stops where it does.** G-R7 justified it as "what
the reviewer was canonically given", which is too loose to carry the weight: the
previous coder execution's receipt is formally reachable along `ReviewRequest →
CoderReport → ProviderExecutionReceipt`, so *given* does not by itself exclude
anything. Two tighter reasons, in order:

```text
1. V0 finding evidence is narrowed to the three canonical review-evidence
   classes frozen for the reviewer task: ContractDocument, Diff, GateLog.
   No additional V0 finding-evidence consumer has been demonstrated.

2. The reviewer's OWN execution receipt is additionally IMPOSSIBLE as a
   payload citation, not merely unneeded.
```

The second is worth stating because it is structural rather than a judgement
call. FD-11 requires

```text
envelope.payload_digest == receipt.final_normalized_output_ref.digest
```

so the receipt already commits to the exact report payload bytes. A payload that
cited its own execution's receipt would close a content-address cycle:

```text
report payload digest -> receipt digest -> final_normalized_output_ref
                      -> report payload digest
```

The frozen receipt architecture runs deliberately the other way: the receipt
proves the provenance of normalized-output bytes that already exist, and only
then does the report's *envelope* reference the receipt. `ProviderExecutionReceipt`
is therefore excluded from both evidence surfaces on an argument that does not
depend on what "given" means.

The three admitted kinds are:

```text
ReviewerReport.findings[].evidence_refs -> ContractDocument | Diff | GateLog
ReviewVerdict.findings[].evidence_refs  -> ContractDocument | Diff | GateLog
```

Both surfaces are frozen at `rank <= 2`, and all three targets are rank 0, so the
narrowing is strictly inside the frozen bound rather than an extension of it.

The alternative — the verdict binding evidence only transitively through
`reviewer_report_ref`, with no direct refs of its own — is defensible and would
remove three rows. It is **not** taken here, because it requires dropping
`evidence_refs` from a projection frozen as `findings: as §3.5`, and that is a
supersede of §3.6 needing its own argument. §1's default is `KEEP_V1_MODEL`. If
v2 wants the transitive-only model it must say so on the record; what it may not
do is arrive at it by way of "claims are not authority", which is an answer to a
different question.

**Why the directive edges come back out.** G-R5 accepted them on this chain:

```text
R5.1: a CorrectiveDirective starts the coder execution
   -> an execution needs its exact input state
   -> therefore the directive must reference that input state
```

The last step does not follow, and it fails for a reason this document has
already written down once. §3.1 admitted `CampaignRunBinding` *precisely* as the
durable execution-to-input-state authority, admitted before dispatch. So:

```text
CorrectiveDirective  commits scope, findings, and target_provider_execution_id
                     (all frozen §3.7 fields; frozen §3.7 carries NO input refs)
controller           admits binding B for execution E, with the exact
                     continued candidate/materialization pair, pre-dispatch
dispatch E
```

"The directive starts the execution" and "the execution has an exact input
state" are both satisfied, with the input state referenced once instead of
twice. Accepting the directive edges was the G-R1 error repeating in a new
place: proving that an *execution* needs a fact, and concluding that every
artifact near it must carry that fact. Frozen §3.7 carries no input refs;
`KEEP_V1_MODEL` wins.

This does leave a genuine question the v2 draft inherits rather than Phase G
answering it: frozen `WorkOrderV1` *does* carry `input.candidate_ref` and
`input.materialization_attestation_ref`, and if the binding is now the
input-state authority, those two frozen slots are duplicated by it. §1's default
keeps them. Whether v2 keeps, narrows or drops them is a supersede decision with
an argument, not a side effect of introducing the binding.

#### 4.2.4 The registry

69 semantic edges: 56 `Intra`, 13 `Causal`. Sources are exact — discriminated by
event kind and payload variant per §4.2.2 — and there is no wildcard source and
no class row. Exactly one **sanctioned open target** appears,
`AnyCommittedEnvelope` (§4); it is a meta-target rather than a node, and §4.2.5
says what that costs.

Slot numbers in the origin column are the frozen inventory rows of §4.1.1.

| # | source semantic kind | target semantic kind | edge | origin |
|---:|---|---|---|---|
| 1 | `WorkOrder` | `CandidateStateReceiptRef` | Intra | frozen slot 3 |
| 2 | `WorkOrder` | `CandidateMaterializationRef` | Intra | frozen slot 4 |
| 3 | `WorkOrder` | `ScopeContract` | Intra | frozen slot 5 |
| 4 | `CoderReport` | `ProviderExecutionReceipt` | Intra | frozen slot 2 (role=coder) |
| 5 | `CoderReport` | `DiagnosticLog` | Intra | frozen slot 7 |
| 6 | `CandidateReceipt` | `CandidateStateReceiptRef` | Intra | frozen slot 8 |
| 7 | `CandidateReceipt` | `CoderReport` | Intra | frozen slot 9 |
| 8 | `ReviewRequest` | `CandidateReceipt` | Intra | frozen slot 10 |
| 9 | `ReviewRequest` | `ScopeContract` | Intra | frozen slot 11 |
| 10 | `ReviewRequest` | `CoderReport` | Intra | frozen slot 13 |
| 11 | `ReviewRequest` | `ContractDocument` | Intra | §4.2.3 ACCEPT (slot 12) |
| 12 | `ReviewRequest` | `Diff` | Intra | §4.2.3 ACCEPT (slot 12) |
| 13 | `ReviewRequest` | `GateLog` | Intra | §4.2.3 ACCEPT (slot 12) |
| 14 | `ReviewerReport` | `ProviderExecutionReceipt` | Intra | frozen slot 2 (role=reviewer) |
| 15 | `ReviewerReport` | `ContractDocument` | Intra | §4.2.3 ACCEPT (slot 14) |
| 16 | `ReviewerReport` | `Diff` | Intra | §4.2.3 ACCEPT (slot 14) |
| 17 | `ReviewerReport` | `GateLog` | Intra | §4.2.3 ACCEPT (slot 14) |
| 18 | `ReviewVerdict` | `ReviewerReport` | Intra | frozen slot 15 |
| 19 | `ReviewVerdict` | `ContractDocument` | Intra | §4.2.3 ACCEPT (slot 41) |
| 20 | `ReviewVerdict` | `Diff` | Intra | §4.2.3 ACCEPT (slot 41) |
| 21 | `ReviewVerdict` | `GateLog` | Intra | §4.2.3 ACCEPT (slot 41) |
| 22 | `CorrectiveDirective` | `ReviewVerdict` | Causal | frozen slot 16 |
| 23 | `CorrectiveDirective` | `ScopeContract` | Intra | frozen slot 17 |
| 24 | `HumanDecision` | `HumanCommandRequest` | Intra | frozen slot 20 |
| 25 | `ProviderExecutionReceipt` | `InteractionManifest` | Intra | frozen slot 21 |
| 26 | `ProviderExecutionReceipt` | `NormalizedOutput` | Intra | frozen slot 22, 25 |
| 27 | `ProviderExecutionReceipt` | `CanonicalRequest` | Intra | frozen slot 23 |
| 28 | `ProviderExecutionReceipt` | `RawProviderBytes` | Intra | frozen slot 24 |
| 29 | `ProviderExecutionReceipt` | `UsageRecord` | Intra | frozen slot 35 |
| 30 | `ProviderExecutionReceipt` | `CostRecord` | Intra | frozen slot 36 |
| 31 | `InteractionManifest` | `ProviderMessage` | Intra | frozen slot 37, 38 |
| 32 | `InteractionManifest` | `ToolArguments` | Intra | frozen slot 39 |
| 33 | `InteractionManifest` | `ToolResult` | Intra | frozen slot 40 |
| 34 | `CandidateReceipt` | `CampaignRunBinding` | Intra | §3.1 new object |
| 35 | `ProviderExecutionReceipt` | `CampaignRunBinding` | Intra | §3.1 new object |
| 36 | `CampaignRunBinding` | `CandidateStateReceiptRef` | Intra | §3.1 new object |
| 37 | `CampaignRunBinding` | `CandidateMaterializationRef` | Intra | §3.1 new object |
| 38 | `CampaignRunBinding` | `RunContractCandidateStateRef` | Intra | §3.1 new object |
| 39 | `CampaignRunBinding` | `WorktreeMaterializationRef` | Intra | §3.1 new object |
| 40 | `CampaignRunBinding` | `WorktreeCorrespondence` | Intra | §3.1 new object |
| 41 | `CampaignCreatedPayload` | `ScopeContract` | Intra | frozen slot 28 |
| 42 | `ProviderExecutionRecordedPayload` | `ProviderExecutionReceipt` | Intra | frozen slot 29 |
| 43 | `GateResultsAcceptedPayload` | `GateLog` | Intra | frozen slot 30 |
| 44 | `CiResultsAcceptedPayload` | `CiObservation` | Intra | frozen slot 31 |
| 45 | `TransitionRejectedPayload` | `DetailDocument` | Intra | frozen slot 32 |
| 46 | `CampaignCancelledPayload` | `TerminationObservation` | Intra | frozen slot 33 |
| 47 | `CampaignEvent(WorkOrderIssued)` | `WorkOrder` | Causal | frozen §3.15.1 |
| 48 | `CampaignEvent(CoderReportReceived)` | `CoderReport` | Causal | frozen §3.15.1 |
| 49 | `CampaignEvent(CandidateAccepted)` | `CandidateReceipt` | Causal | frozen §3.15.1 |
| 50 | `CampaignEvent(ReviewRequested)` | `ReviewRequest` | Causal | frozen §3.15.1 |
| 51 | `CampaignEvent(ReviewerReportReceived)` | `ReviewerReport` | Causal | frozen §3.15.1 |
| 52 | `CampaignEvent(ReviewVerdictAccepted)` | `ReviewVerdict` | Causal | frozen §3.15.1 |
| 53 | `CampaignEvent(CorrectiveDirectiveIssued)` | `CorrectiveDirective` | Causal | frozen §3.15.1 |
| 54 | `CampaignEvent(HumanAttentionRaised)` | `HumanAttentionRequest` | Causal | frozen §3.15.1 |
| 55 | `CampaignEvent(HumanDecisionRecorded)` | `HumanDecision` | Causal | frozen §3.15.1 |
| 56 | `CampaignEvent(HumanCommandRejected)` | `HumanCommandRequest` | Causal | frozen §3.15.1 |
| 57 | `CampaignEvent(CampaignFeedItemEmitted)` | `CampaignFeedItem` | Causal | frozen §3.15.1 |
| 58 | `CampaignEvent(CampaignCreated)` | `CampaignCreatedPayload` | Intra | frozen structure §3.15.2 |
| 59 | `CampaignEvent(ProviderExecutionRecorded)` | `ProviderExecutionRecordedPayload` | Intra | frozen structure §3.15.2 |
| 60 | `CampaignEvent(GateResultsAccepted)` | `GateResultsAcceptedPayload` | Intra | frozen structure §3.15.2 |
| 61 | `CampaignEvent(CiResultsAccepted)` | `CiResultsAcceptedPayload` | Intra | frozen structure §3.15.2 |
| 62 | `CampaignEvent(AttentionResolved)` | `AttentionResolvedPayload` | Intra | frozen structure §3.15.2 |
| 63 | `CampaignEvent(AttentionSuperseded)` | `AttentionSupersededPayload` | Intra | frozen structure §3.15.2 |
| 64 | `CampaignEvent(HumanCommandRejected)` | `HumanCommandRejectedPayload` | Intra | frozen structure §3.15.2 |
| 65 | `CampaignEvent(TransitionRejected)` | `TransitionRejectedPayload` | Intra | frozen structure §3.15.2 |
| 66 | `CampaignEvent(CampaignCancelled)` | `CampaignCancelledPayload` | Intra | frozen structure §3.15.2 |
| 67 | `CampaignEvent(CampaignSuperseded)` | `CampaignSupersededPayload` | Intra | frozen structure §3.15.2 |
| 68 | `CampaignEvent(CampaignTerminalError)` | `CampaignTerminalErrorPayload` | Intra | frozen structure §3.15.2 |
| 69 | `CampaignFeedItem` | `AnyCommittedEnvelope` | Causal | §4 sanctioned open target |

**Reconciliation against §4.1.1.** Every row traces to one of five origins, and
every exact frozen slot is accounted for:

```text
32 exact frozen slots
  -> 26 slots        one row each
  -> slot 2          envelope.provider_execution_receipt_ref is one slot and two
                     rows: required iff role in {coder, reviewer}, so exactly
                     two message kinds carry it
  -> slots 22, 25    two slots onto one target kind (NormalizedOutput)
  -> slots 37, 38    two slots onto one target kind (ProviderMessage)
  -> slot 26         CampaignEventV1.source_ref is one slot and ELEVEN rows,
                     per frozen 3.15.1 (4.2.2)
                     26 + 1 + 2 + 2 + 1 = 32 slots -> 30 + 11 = 41 rows

9 open surfaces
  -> slot 12         ReviewRequest.evidence_refs        NARROWED  -> 3 rows
  -> slot 14         ReviewerReport.findings[].evi...   NARROWED  -> 3 rows
  -> slot 41         ReviewVerdict.findings[].evi...    NARROWED  -> 3 rows
  -> slot 18         CampaignFeedItem.subject_refs      SANCTIONED-> 1 row
  -> the other five  no admitted target                           -> 0 rows

rows not backed by a frozen ArtifactRef slot
  7 rows             CampaignRunBinding, the one new object 3.1 admits:
                     five outgoing input-state relations + two in-edges
  11 rows            CampaignEvent(k) -> its payload variant: frozen structure
                     (3.15.2), committed by event_payload_digest rather than by
                     an ArtifactRef

41 + 3 + 3 + 3 + 1 + 7 + 11 = 69
```

A row with none of those five origins is a defect, and so is an exact frozen
slot with no row.

#### 4.2.5 Acyclicity, proved — and what is *not* a terminal

The `Intra` acyclicity obligation binds the typed-node subgraph. After §4.2.2's
discrimination that is **47** nodes: the eleven message kinds, four typed
supports (`ProviderExecutionReceipt`, `InteractionManifest`, `ScopeContract`,
`CampaignRunBinding`), eleven payload variants and all twenty-one event kinds.

**The sanctioned open target is a meta-target, not a terminal (G-R7).** G-R6
counted `AnyCommittedEnvelope` among 21 "graph-terminal" nodes alongside CAS
blobs and the four A0/R1 wrappers. That is false, and falsely in the expensive
direction. A stored `CampaignFeedItem.subject_refs` entry carries a *concrete*
kind — `work_order`, say — and FD-2.5 makes the resolver check the stored object
against the slot's expectation and parse it when the slot expects a typed
object. The concrete target is an ordinary typed message whose own
`ArtifactRef` slots are enqueued recursively, exactly like any other typed
reference.

Calling it terminal would stop that traversal, which under-traverses the graph
and — the part that matters — **under-accounts the closure**. A feed item would
become an elegant way to carry an entire `WorkOrder` subtree past the FD-1.5
evidence budget, on an observability surface. Frozen for v2:

```text
AnyCommittedEnvelope
    IS      a sanctioned meta-target: a named union of admissible concrete kinds
    IS NOT  a graph node, and IS NOT terminal

members := exactly the eleven envelope-bearing message kinds of FD-1.9

    work_order            coder_report          candidate_receipt
    review_request        reviewer_report       review_verdict
    corrective_directive  campaign_feed_item    human_attention_request
    human_command_request human_decision

resolution
    check the concrete ref.kind is a member                  (FD-2.5, fail closed)
    check COMMITTED(target, N) for the emitting event's sequence N   (below)
    resolve that concrete message as a normal typed target
    continue traversal through its own admitted edges
    charge it, and its closure, against the FD-1.5 budget
```

**`COMMITTED`, defined (G-R9).** Row 69's instance-level acyclicity rests on this
predicate, so leaving it as a word would have left the only unproved edge in the
registry resting on prose. It is defined without reference to `CampaignStateV1`,
and therefore without Phase G drafting any reducer policy:

```text
COMMITTED(target, before_event_sequence = N)  iff

    target.kind is a member of AnyCommittedEnvelope           (the list above)
AND there exists an already accepted canonical event E with E.sequence < N
AND the exact (target.kind, target.digest) occurs in the resolved closure of E
AND that closure passed normal integrity / schema / budget resolution

For CampaignFeedItemEmitted at sequence N:
    every subject_ref target MUST satisfy COMMITTED(target, N)
```

Three properties make this the right shape rather than merely a definition.

*It is checkable before `fold`.* FD-14.2 puts sequence contiguity in
`verify_wire` and closure resolution in `resolve_event`, both strictly ahead of
the reducer. So the predicate reads only things those two stages already
compute, and `fold` stays pure, total, clock-free and I/O-free. Phase G decides
nothing about transitions by defining it.

*It is replay-checkable.* No clock, no `created_at`, no mutable store
observation, no "we think the blob was written first". Strictly-earlier position
in canonical history plus closure membership, both recomputable from the log and
CAS — which is the same standard FD-8 holds replay to.

*It is what the `Causal` class actually claims.* `Causal` edges are proved
acyclic per instance by create-before-reference (§4), and until now row 69 had
no witness for that at all. With the predicate, the target provably existed in
canonical history strictly earlier than the referencing event, so a mutual or
self-referential feed cycle is unrepresentable rather than merely unlikely.

It also disposes of the `HumanCommandRequest` question §8 raised. An untrusted
rank-3 kind is admissible here because `COMMITTED` requires it to have entered
canonical history already — an accepted request appears in the closure of an
earlier `HumanDecisionRecorded`, a rejected one in that of an earlier
`HumanCommandRejected`. FD-4 separates canonical evidence from transition
authority: the feed item observes the request, and only `HumanDecision`
authorizes anything. What `COMMITTED` forbids is exactly the dangerous reading —
a feed item pointing at an arbitrary human payload nobody has accepted.

**The membership had to be enumerated, not named (G-R8).** G-R7 defined the
meta-target's *semantics* and left its *extension* undefined, which §4.2.1's own
rule cannot survive: a named union of unknown composition differs from the rank
rule mainly in having acquired a business card. No new decision was needed —
Phase G had already fixed the set twice over. FD-1.9 enumerates the eleven A1
message kinds as a closed group, and §3 decided `KEEP_V1_MODEL` for the envelope
boundary, so "committed envelope" *is* those eleven. The enumeration lives in the
generated dataset beside the registry, so the published table and this list
cannot drift apart.

Two things follow that prose alone would not have given. The `COMMITTED`
precondition turns this `Causal` edge's create-before-reference argument into a
**machine-checkable witness** rather than an appeal to ordering. And a future
promotion of a support object to envelope-bearing now has to revisit this union
explicitly, instead of silently widening it by widening the meaning of the word
*envelope* — which is exactly the failure mode §4.2.1 exists to prevent.

The bookkeeping therefore reads **20 terminal kinds + 1 open meta-target**, not
21 terminals. The `Intra` proof is unaffected — this edge is `Causal` and was
never in that subgraph — which is precisely why the error survived a passing
proof. An acyclicity check cannot see a closure-accounting bug.

The 20 real terminals are the CAS evidence blobs, the four A0/R1 wrappers of
§5.1, and `WorktreeCorrespondence`; they have no outgoing edges by construction.

Machine check over the 69 rows above, by Kahn's algorithm:

```text
edges: 69   Intra: 56   Causal: 13
nodes: 68   typed: 47   terminal kinds: 20   open meta-targets: 1
Intra typed -> typed edges: 26
Kahn: 47/47   sorted: True
non-typed sources: none
event kinds appearing: 21 of 21
```

`Causal` edges are excluded from that subgraph by construction and carry their
own per-instance create-before-reference argument (§4). The thirteen break down
as follows — G-R7 wrote "twelve from event kinds", which was wrong and, worse,
dropped the feed edge from its own accounting:

```text
11   event-source: CampaignEvent(k) -> its source_ref target   (rows 47-57)
 1   cross-round:  CorrectiveDirective -> ReviewVerdict        (row 22)
 1   feed causation: CampaignFeedItem -> AnyCommittedEnvelope  (row 69)
13
```

Event kinds are never targets (§4.1.4); the other two carry the create-before-
reference arguments given in this subsection and by the `COMMITTED` predicate
of §4.2.5.

**That thirteenth is a correction, not an addition.** G-R5 filed it `Intra`.
§4 defines `Intra` as *within one round's derivation flow* and `Causal` as
*crossing rounds*, and frozen §3.7 says a `CorrectiveDirective` "starts the next
coder execution directly", with frozen §3.15.1's transition row giving
`CorrectiveDirectiveIssued` a **new `active_round_id`**. The directive opens
round N+1; its `review_verdict_ref` names round N's verdict. That is the
definition of crossing a round. The prototype classifies the same edge `Causal`
at `edges.rs:281–286`, independently.

The Kahn proof passed with the edge misfiled, which is the part worth stating
plainly: the proof was not wrong, it was answering the question about a slightly
wrong graph. A cross-round obligation sitting in the kind-level proof domain is
a proof that succeeds for the wrong reason.

#### 4.2.6 The realization contract the v2 draft inherits

G-R5 through G-R7 wrote this obligation as *every `ArtifactRef`-valued field must
realize **exactly one** semantic edge*, and §4.2.4 has since made that
unsatisfiable in both directions. `ReviewRequest.evidence_refs` realizes three
edges by frozen ordering; the two review-evidence surfaces realize three each by
§4.2.3; `CampaignFeedItem.subject_refs` realizes a union. And the reverse fails
too — frozen `final_normalized_output_ref` and `dispatches[].normalized_output_ref`
are two fields on one edge, which the reconciliation already states in writing.

The relation is many-to-many. Uniqueness is real, but it lives one level down, on
the **occurrence** rather than the field:

```text
FROZEN — the realization contract

1. For every ArtifactRef-valued field, the v2 draft MUST declare the complete
   set of semantic edges that field may realize.

2. For every concrete ArtifactRef occurrence, the pair
       (source semantic kind, concrete target semantic kind)
   MUST select exactly one admitted edge from that field's declared set.

3. A field MAY realize several admitted edges.
   Several fields MAY realize the same admitted edge.

4. No field and no occurrence may introduce a relation absent from §4.2.4.
   A field whose declared set is not a subset of §4.2.4 is not a drafting
   choice: it reopens Phase G, or it supersedes it.

5. AnyCommittedEnvelope is declared ONCE as a meta-target expansion, never as
   eleven separate edges (§4.2.5).
```

**Where clause 2's two halves come from.** *Uniqueness* is a fact about the
admitted domain: over the 69 rows there are 69 distinct `(source, target)` pairs,
and expanding the meta-target to its eleven members adds no clash, so no
occurrence has two admissible readings today. *Totality* is not supplied by that
count and was overstated in G-R8 — it comes from clause 1 plus fail-closed
rejection: a field declares its set, and a target outside that set is refused
rather than left unmatched. The two halves have different sources and are now
named separately.

**Global `(source, target)` uniqueness is NOT frozen as an invariant (G-R9).**
§8 asked whether it should be. It should not, and the counterexample is already
in this project's own design input rather than hypothetical — `37502e3`
carries both:

```text
ProviderInvocationReceipt -> CampaignRunBinding   Intra
    via campaign_run_binding_ref.blob_ref                  (edges.rs:324-329)

ProviderInvocationReceipt -> CampaignRunBinding   Causal
    via cause.safe_redrive.prior_run_binding_ref           (edges.rs:336-341)
```

The second is POST-V0 (§3.5), which is the only reason the 69 rows are pair-
distinct today. Freezing the global invariant would pre-forbid a SafeRedrive
shape that has already been designed — Phase G quietly deciding a POST-V0
question under cover of a proof convenience, which is the failure mode §3.1 was
corrected for in G-R2.

The uniqueness that *is* frozen is per-field:

```text
FROZEN — within ONE concrete field declaration,
         (source kind, concrete target kind) selects exactly ONE
         semantic edge and therefore exactly one class.

NOT FROZEN — two different fields of the same source may reference the same
         target kind under different classes. The 69/69 pair-distinctness of
         the V0 registry is a sanity fact about V0, not a law about registries.
```

**The ledger this obligation needs to be checkable (G-R9).** Clause 1 says the
v2 draft *must declare* each field's edge set, and §8 asked what enforces that.
Nothing did. Several rounds went into stopping the wire layer from inventing
relations, and the last joint between registry and wire was left to a human
reading a document — which is the same class of gap as the rank rule this layer
replaced, one level up.

Phase G cannot build the artefact, because field paths do not exist yet. It can
and does make it a required output:

```text
FROZEN — V2 WIRE REALIZATION LEDGER, a REQUIRED acceptance artefact of the
         v2 drafting phase, not an optional aid

For every recursively discovered ArtifactRef-valued wire field:
    source semantic kind
    concrete field path
    complete allowed target set
    edge class for each target
    the corresponding §4.2.4 registry edge(s)

The ledger MUST be mechanically checked against:
    1. the v2 schemas   — field completeness, by the SAME recursive extraction
                          §4.1.1 now uses, since a flat pass already missed a
                          slot once
    2. §4.2.4           — every ledger target is an admitted edge
    3. §4.2.5           — meta-target entries expand to the declared members
    4. §4.2.4, reverse  — no admitted edge that the frozen graph requires has
                          silently lost its wire realization

No ArtifactRef-valued field may exist outside the ledger.
No ledger target may exist outside the semantic registry.
```

The format is a v2 drafting decision — YAML, a Rust const table, generated
Markdown over a machine-readable source, anything that a checker can read.
What Phase G freezes is that exactly one normative, machine-checked ledger
exists, so that a violation of §4.2.6 is something a build enumerates rather
than something a reviewer happens to notice.

What changed in G-R9 is the quantifier's enforcement and one rejected invariant.
No registry row moves.

## 5. Q4 + Q5 — imported roots, closure, and the typed external boundary

### 5.1 The wrapper classification: `PARSED_BUT_GRAPH_TERMINAL`

v1's FD-2.4 says imported roots are *never parsed by A1*, while every typed A0/R1
wrapper in the design input is parsed by A1 — `CandidateStateReceiptRefV1` reads
a `source_run_id`, checks `ArtifactKind::CandidateState`, and carries an exact
`RunEventId`. Both cannot be true. This is the same seam R5.1 hit with
`ScopeContractV1` and repaired by redefining rank 0; it recurs because the
underlying distinction was never named.

Frozen for v2:

```text
PARSED_BUT_GRAPH_TERMINAL

A1 owns and validates the wrapper's syntax and local invariants.
A1 may carry enough foreign identity to select an exact A0/R1 fact.
Semantic validity of that fact remains owned by the A0/R1 verifier.
Cross-layer resolution proves the boundary relation and nothing beyond it.
The foreign authority does not become an A1 graph node, and its internal
references are never traversed as A1 edges.
```

So "terminal" means terminal *in the graph*, and says nothing about parseability;
parseability is decided per slot. `OPAQUE_TERMINAL` remains available for any
future import that A1 genuinely hands off unread, and `OTHER` requires explicit
new justification.

V0 wrapper set, after §3.4 and §3.5 remove two — with in-degrees now derived
from the registry of §4.2.4 rather than measured on the prototype:

```text
CandidateStateReceiptRef        in-degree 3   WorkOrder, CandidateReceipt,
                                              CampaignRunBinding
CandidateMaterializationRef     in-degree 2   WorkOrder, CampaignRunBinding
WorktreeMaterializationRef      in-degree 1   CampaignRunBinding
RunContractCandidateStateRef    in-degree 1   CampaignRunBinding
```

The four are **distinct kinds and stay distinct**. G-R5's registry collapsed
`CandidateMaterializationRef` and `WorktreeMaterializationRef` into a single
invented target called `MaterializationAttestation`, which merged two wrapper
kinds this very subsection names separately and which the design input's
`InputStateBindingV1` distinguishes structurally — the continued-candidate
variant pairs `candidate_state_ref` with `materialization_ref`, the initial
variant pairs `run_contract_ref` with `worktree_ref` and a correspondence blob.
An unadjudicated merge of two terminal kinds is the same class of error as an
unadjudicated new edge, and it also meant the node universe proved acyclic in
§4.2.5 was not the one named here.

`WorktreeCorrespondence` is the fifth binding target and is **not** a wrapper: it
is a CAS evidence blob (`Cas(WorktreeCorrespondenceEvidenceBlob)` in the design
input), so it terminates traversal under §5.2 like any other rank-0 blob rather
than under the parse-then-stop rule above.

### 5.2 Closure semantics

Unchanged from v1 in substance, with one clarification the above forces:

```text
closure traversal enters a wrapper to validate it, and stops there.
It never follows the foreign authority's own internal references.
Accounting charges the wrapper, not the foreign object graph behind it.
```

One thing this rule does **not** cover, and G-R6 wrongly assumed it did: a
sanctioned open target is not a wrapper. `AnyCommittedEnvelope` resolves to an
ordinary A1 typed message, so traversal continues through it and the closure is
charged for its whole subtree (§4.2.5). "Terminal" is earned by being a foreign
authority or opaque bytes — never by the referencing slot being generic.

The FD-1.5 bounds, the deduplication key `(kind, digest)`, declared-size-before-read
accounting and all-or-nothing rejection are untouched by Phase G.

## 6. E-V0-4 — the part that belongs to Phase G, and the part that does not

E-V0-4 has two halves. Only one is a taxonomy question.

**In scope — the manifest's class, and nothing else.** §3.3 settles it: a
**typed support object, not envelope-bearing**. That is the whole of Phase G's
mandate here.

**Out of scope — every number.** The first pass went one step further and
concluded that the class implies the bound: typed support object, therefore the
1 MiB control-artifact bound, therefore `manifest` leaves the evidence-blob list.
That does not follow. Class and size are orthogonal — a support object may carry
its own per-kind hard bound, and the design input does exactly that, giving the
manifest 2 MiB. Deciding the bound from the class would have been Phase G quietly
amending FD-1.4, which is the failure this section was written to name and then
committed anyway.

So all of it defers to v2 wire/bounds drafting, together with the envelope-size
seam:

```text
is the manifest 1 MiB, 64 MiB, or a per-kind bound of its own?
does MAX_INTERACTION_SEQUENCE change instead?
what bounds stored envelope bytes, given FD-1.8 sums envelope + payload?
```

Phase G supplies the classification these questions need and answers none of
them.

`E-V0-1`, `E-V0-2` and `E-V0-3` are **not** Phase G material at all. They are
convergence inputs about the checked-parse boundary, scalar bounds and
repository-relative validation. Pulling them in because they sit nearby in
`e202ac1` would turn Phase G into "let us also rewrite the wire layer", which is
the failure this phase exists to prevent.

## 7. What Phase G did not decide

- envelope v2 field set, framing order, or any digest domain;
- `message_kind_version` per kind — an output of v2 drafting, once payload shapes
  are known;
- the envelope-size bound (§6);
- dispositions for any of the 141 negative-matrix rows or the 47 frozen decisions;
- anything about `E-V0-1`, `E-V0-2`, `E-V0-3`;
- whether the import mechanism of §3.4 exists as a controller procedure — only
  that it does not exist as a node.

## 8. For the independent reviewer (revision G-R9)

G-R9 changed no semantic edge, no node, and no disposition — the same scope
discipline as G-R8, one level narrower. Two additions (§4.2.5's `COMMITTED`
predicate, §4.2.6's realization ledger), one rejected invariant, one wording
repair. Everything else stands as approved through G-R8. Attack these:

1. **`COMMITTED` and the word "accepted" (§4.2.5).** The predicate quantifies
   over "an already accepted canonical event E with `E.sequence < N`". At
   `resolve_event` time for sequence N, are events at lower sequence *known
   accepted*, or only known well-formed? If `fold` can still reject an event at
   sequence `N-1` after its closure resolved, the witness is weaker than it
   reads, and the predicate should quantify over resolved-and-folded history
   instead.
2. **`COMMITTED`'s cost.** It requires knowing the resolved closure of earlier
   events, which a strictly streaming resolver does not retain. Is that a
   memory obligation V0 can carry, or does it need a bounded index — and if the
   latter, is that an FD-1.5 concern this document should have named?
3. **The ledger's reverse check (§4.2.6, item 4).** "No admitted edge that the
   frozen graph *requires* has silently lost its wire realization" — but the
   registry does not mark which edges are required versus permitted. Six of the
   nine open surfaces have no admitted target at all; three others are narrowed.
   Is "required" derivable from §4.2.4 as it stands, or does the ledger need a
   requiredness column the registry cannot supply?
4. **The rejected invariant.** Global `(source, target)` uniqueness is refused
   on a POST-V0 counterexample (`edges.rs:324-329` vs `336-341`). Check that the
   two really are the same `(source, target)` under v2's names, given §4.1.5
   leaves `ProviderInvocationReceipt` versus `ProviderExecutionReceipt` an open
   supersede question. If the renaming resolves them apart, the counterexample
   weakens.
5. **Per-field uniqueness as the replacement.** It is frozen for *one concrete
   field declaration*. A repeated field — `findings[].evidence_refs` — has many
   occurrences under one declaration. Confirm the rule binds per occurrence
   within the declaration, not per declaration, or an array field could carry
   two classes to one target kind.
6. **The V0 sanity fact.** 69/69 pair-distinctness is now explicitly *not* an
   invariant. Then nothing checks it. Should the ledger check assert it for V0
   anyway, so that a v2 draft accidentally introducing a second class between an
   existing pair is caught rather than silently permitted?

## 9. Revision record

### G-R9 — ninth independent review, two P1s

`CHANGES_REQUESTED`, narrower than G-R8 and by the same rule: no registry row,
no node, no disposition. The reviewer confirmed from the commit itself that
G-R8 moved nothing — so 56/13, 47 typed nodes, 26 `Intra` typed→typed and Kahn
47/47 remain *the same proof*, not a new proof of a similar graph — and approved
the membership enumeration, `HumanCommandRequest`'s place in it, the receipt
exclusion, and the corrected `Causal` arithmetic.

**P1-21 — clause 1 was correct and unenforceable.** §4.2.6 required the v2 draft
to *declare* each field's admitted edge set, and nothing said where that
declaration lives or what checks it. Several rounds went into stopping the wire
layer from inventing relations; the last joint between registry and wire was
then left to a human reading a document, which is the rank rule's failure mode
one level up. Phase G cannot build the artefact — field paths do not exist yet —
so it freezes it as a **required acceptance artefact of v2 drafting**: one
normative, machine-checked wire realization ledger, keyed by concrete field
path, checked four ways against the v2 schemas (by the same recursive extraction
§4.1.1 now uses), against §4.2.4 forward and reverse, and against the
meta-target's declared members. No `ArtifactRef`-valued field outside the
ledger; no ledger target outside the registry. Format is a drafting decision;
existence is not.

**Global `(source, target)` uniqueness rejected as an invariant.** §8 had asked
whether to freeze it. The counterexample is in this project's own design input
rather than hypothetical: `37502e3` carries `ProviderInvocationReceipt →
CampaignRunBinding` twice — `Intra` via `campaign_run_binding_ref.blob_ref`
(`edges.rs:324-329`) and `Causal` via `cause.safe_redrive.prior_run_binding_ref`
(`edges.rs:336-341`). Only the POST-V0 status of the second makes the 69 rows
pair-distinct today. Freezing the invariant would have pre-forbidden an
already-designed SafeRedrive shape — Phase G deciding a POST-V0 question under
cover of a proof convenience, which is precisely what G-R2 corrected in §3.1.
What is frozen instead is per-field: within one field declaration, `(source
kind, concrete target kind)` selects exactly one edge and therefore one class.
69/69 is recorded as a V0 sanity fact.

**P1-22 — `COMMITTED` was the load-bearing word in an undefined state.** Row
69's instance-level acyclicity rested on it, so it was a proof predicate, not a
term of art awaiting the draft. G-R8 hesitated to define it because the obvious
reading — earlier position in the event log — looked like drafting reducer
policy. It is not: FD-14.2 puts sequence contiguity in `verify_wire` and closure
resolution in `resolve_event`, both strictly ahead of `fold`. So the predicate
is defined without reference to `CampaignStateV1` at all — *there exists an
already accepted canonical event E with `E.sequence < N` whose resolved closure
contains the exact `(kind, digest)`* — and it is checkable pre-`fold`,
replay-checkable with no clock and no mutable-store observation, and it finally
gives row 69 the create-before-reference witness its class has been claiming.

It also settles the `HumanCommandRequest` question §8 raised against the
membership list. An untrusted rank-3 kind is admissible because `COMMITTED`
requires it to be in canonical history already — accepted, in the closure of an
earlier `HumanDecisionRecorded`; rejected, in that of an earlier
`HumanCommandRejected`. FD-4 keeps canonical evidence and transition authority
apart, so the feed item observes and only `HumanDecision` authorizes. The
dangerous reading — a feed item pointing at an arbitrary unaccepted human
payload — is exactly what the predicate forbids.

**One P2.** G-R8 wrote that 69 distinct `(source, target)` pairs make selection
*a total function*, which overstates what a distinctness count proves.
Uniqueness comes from the count; totality comes from clause 1 plus fail-closed
rejection of an unlisted target. The two halves have different sources and are
now named separately — a small thing, except that this document's whole method
is not to let an argument borrow strength from an adjacent one.

### G-R8 — eighth independent review, two P1s

`CHANGES_REQUESTED`, and narrow by design: both findings are specification
holes between the approved graph and any unambiguous implementation of it, and
**no registry row changed**. The reviewer re-verified the 69-row table
(56 `Intra` / 13 `Causal`), confirmed that the six new review-evidence edges
target only rank-0 terminals so the typed→typed subgraph is untouched and 47/47
still holds, and approved the recursive inventory, the review-evidence
restoration, the receipt exclusion and the meta-target traversal model.

**P1-19 — §4.2.6 demanded a cardinality §4.2.4 forbids.** The obligation read
*every `ArtifactRef`-valued field must realize **exactly one** semantic edge*,
while the approved table has `ReviewRequest.evidence_refs` realizing three by
frozen ordering, both review-evidence surfaces realizing three each, and
`CampaignFeedItem.subject_refs` realizing a union. The reverse direction fails
too: frozen `final_normalized_output_ref` and `dispatches[].normalized_output_ref`
are two fields on one edge — which this document's own reconciliation had been
stating in writing for two rounds while the obligation above it said otherwise.

The relation is many-to-many; uniqueness lives on the **occurrence**. §4.2.6 is
reframed accordingly: a field declares the complete set of edges it may realize,
and every concrete occurrence's `(source, concrete target)` pair selects exactly
one edge from that set. Clause 2 was checked rather than asserted — the 69 rows
carry 69 distinct `(source, target)` pairs, so selection is a total function and
no occurrence is ambiguous; expanding the meta-target to its eleven members adds
no clash. Had a pair appeared twice under two classes, the clause would have
been a wish. The quantifier moved to the object where it is true; the graph did
not move at all.

**P1-20 — the meta-target had semantics but no extension.** G-R7 defined how
`AnyCommittedEnvelope` resolves and left its membership undefined, which
§4.2.1's own rule cannot survive: a named union of unknown composition differs
from the rank rule mainly in having acquired a business card. No new decision
was required — FD-1.9 enumerates the eleven A1 message kinds as a closed group
and §3 decided `KEEP_V1_MODEL` for the envelope boundary, so *committed
envelope* is exactly those eleven. Enumerated now in the generated dataset
beside the registry, with a `COMMITTED`-before-acceptance precondition that
turns this `Causal` edge's create-before-reference argument into a
machine-checkable witness, and that forces any future support→message promotion
to revisit the union explicitly instead of widening it by widening the meaning
of a word.

**The receipt-exclusion rationale, strengthened.** G-R7 justified the admitted
evidence set as "what the reviewer was canonically given", which the reviewer
correctly called too loose — the prior coder execution's receipt is formally
reachable along `ReviewRequest → CoderReport → ProviderExecutionReceipt`, so
*given* excludes nothing by itself. Replaced with two reasons, the second
structural: FD-11 requires `envelope.payload_digest ==
receipt.final_normalized_output_ref.digest`, so a payload citing its own
execution's receipt closes a content-address cycle. The frozen architecture runs
the other way — the receipt proves provenance of bytes that already exist, and
only then does the report's *envelope* reference it. That argument does not
depend on what "given" means.

**Two P2s.** §4.2.5 said twelve `Causal` edges issue from event kinds; the table
says eleven (rows 47–57), the remaining two being `CorrectiveDirective →
ReviewVerdict` (row 22) and `CampaignFeedItem → AnyCommittedEnvelope` (row 69).
G-R7's accounting had dropped the very edge G-R7 introduced, which is a small
demonstration of why the breakdown is now published as arithmetic rather than
prose. And §2's metric is retitled *historical prototype projection @ `37502e3`*
rather than keeping a normative-looking formula next to its own death
certificate; the formula stays verbatim, because reproducing the numbers
requires the definition that produced them.

### G-R7 — seventh independent review, three P1s

`CHANGES_REQUESTED`, narrow. The reviewer independently reconstructed the
committed registry — 63 edges, 50/13, 47 typed nodes, 26 `Intra` typed→typed,
Kahn 47/47 — and confirmed all of it, then found three defects the arithmetic
could not see. Nothing in §3 was reopened. Four G-R6 decisions were explicitly
upheld on re-examination: the `ci_observation` exclusion (frozen transition
authority already requires CI to have passed before `REVIEWING`, and
`ReviewVerdictAccepted` guards on canonical CI state, so the reviewer is not
another CI authority), the directive retraction, the event/payload
discrimination, and the `Causal` neighbour test — `CandidateReceipt →
ReviewRequest → ReviewerReport → ReviewVerdict` stays inside one round.

**P1-16 — the frozen extractor was flat, not recursive.** The 40-slot baseline
counted only rows whose own type cell reads `ArtifactRef`. Frozen §3.6 types
`ReviewVerdictV1.findings` as `as §3.5, validated`, and §3.5's finding structure
carries `findings[].evidence_refs | [ArtifactRef]` — a live surface, since
frozen §3.6's acceptance predicate requires *"every evidence_ref resolvable,
rank rule and closure bounds satisfied"* before a verdict may be accepted.

G-R6 incriminated itself here: §4.2.3 discussed the prototype's
`ReviewVerdict.findings[]` evidence edge while §4.2.1's surface list had no
frozen counterpart for it, and §8 said "seven surfaces with no admitted target"
where §4.2.1's table listed six. A document arguing with itself across two
subsections is what this failure looks like from inside.

The fix is the extractor. Every type cell is now expanded transitively, which
also discharges the reviewer's "barring another recursively hidden slot" caveat:
the normative body contains exactly **five** cross-schema type references, and
four expand to scalars (`enum as §3.10`, `enum as §3.12`, `enum as §3.15.1`, and
`[Text]` via the second `as §3.5`). There is no second hidden slot — a result
rather than a hope. Baseline: **41 slots, 32 exact, 9 open**.

**P1-17 — "claim, therefore no evidence edge" redefined a bound contract.** G-R6
gave `ReviewerReport.findings[].evidence_refs` and the verdict's inherited
surface no admitted target, reasoning that report evidence is claim-authority.
That conflates *does reviewer evidence authorize a transition* (no — FD-4) with
*may a reviewer canonically reference evidence* (yes), and FD-4 answers only the
first.

The decisive evidence is external. Frozen §0 binds A1 to
`docs/autonomy-controller.md` (accepted `c5b3ae0b`, PR #93) for "the
`ReviewVerdict` minimum", which A1 **consumes, never redefines**; that minimum,
at `autonomy-controller.md:151–163`, lists **`evidence references`** among the
things a `ReviewVerdict` must bind. Zeroing both surfaces left no canonical
review-evidence path in the graph at all, and `reviewer_report_ref` cannot
rescue one if the report is itself forbidden to reference evidence. Phase G was
not narrowing a v1 surface; it was rewriting a contract it had declared
off-limits.

Both surfaces are **NARROWED** rather than zeroed, to the three kinds the
reviewer is canonically given: `ContractDocument`, `Diff`, `GateLog`. Both are
frozen at `rank <= 2` and all three targets are rank 0, so the narrowing sits
strictly inside the frozen bound. The transitive-only alternative — verdict
evidence bound solely through `reviewer_report_ref` — is recorded as available
and **not taken**, because it requires dropping `evidence_refs` from a
projection frozen as `findings: as §3.5`, which is a supersede of §3.6 needing
its own argument rather than a consequence of FD-4.

`CoderReport.claims[].evidence_refs` keeps its no-target disposition; the
reviewer agreed, and no V0 controller acceptance path reads a coder claim's
citations.

**P1-18 — the sanctioned open target was miscounted as a closure terminal.**
§4.2.5 filed `AnyCommittedEnvelope` among 21 graph-terminal nodes with the CAS
blobs and the A0/R1 wrappers. But a stored `subject_refs` entry carries a
*concrete* kind, and FD-2.5 has the resolver check the stored object against the
slot's expectation and parse it when the slot expects a typed object. The
concrete target is an ordinary typed message whose own slots are enqueued
recursively.

Declaring it terminal stops that traversal — which under-traverses the graph
and, the expensive half, **under-accounts the closure**: a feed item becomes a
way to carry a whole `WorkOrder` subtree past the FD-1.5 evidence budget, on an
observability surface. Now modelled as a meta-target: a named union of
admissible concrete kinds, never a node and never terminal, resolved through to
the concrete message and charged for its closure. Bookkeeping reads **20
terminal kinds + 1 open meta-target**.

The `Intra` proof does not move, because the edge is `Causal` and was never in
that subgraph — which is exactly why a passing proof did not catch it. An
acyclicity check cannot see a closure-accounting bug, and that is worth stating
next to the proof rather than discovering again.

**Two P2s.** §2's metric said *"retained" is defined exactly by the V0 edge
ledger of §4.1*, which stopped being true when §4.2.4 became the edge authority;
its counts are prototype-era, still showing `HumanAttentionRequest` as a
receipt consumer. Regenerating it from §4.2.4 would be circular — the registry
is downstream of the decisions that table supported — so it is labelled
historical evidence and a post-decision in-degree table is published beside it.
The three support decisions survive that regeneration unchanged
(`ProviderExecutionReceipt` in-degree 3, `InteractionManifest` 1,
`CampaignRunBinding` 2), with the receipt's third consumer now
`ProviderExecutionRecordedPayload` rather than `HumanAttentionRequest` — which
strengthens §3.2, a reducer payload being a more specific V0 consumer than an
attention's evidence list. The §8 six-versus-seven discrepancy resolved itself
through P1-16 rather than by editing the noun, as predicted.

Registry after this round: **69 edges, 56 `Intra` / 13 `Causal`**, 47 typed
nodes, 26 `Intra` typed→typed, Kahn 47/47.

### G-R6 — sixth independent review, four P1s

`CHANGES_REQUESTED`. The abstraction layer introduced by G-R5 was approved; all
four findings landed inside it, which is where they belong — a defect in a
registry is catchable by reconciliation, the same defect dissolved into wire
prose is not. The reviewer independently re-ran the committed table and
confirmed the 50/39/11 arithmetic, the 17/17 Kahn result, the 32/8 split
including the eighth surface, and the eleven `source_ref`-bearing event kinds.
All four P1s were verified against blob `7db92f1b` and `37502e3` before being
accepted here.

**P1-12 — the open surfaces were stripped of authority but their required V0
edges were never re-admitted.** Removing rank as an admission rule does not
delete the references V0 needs across those surfaces; it moves them from
"admitted by rule" to "admitted by name, or not at all". G-R5 did the removal
and stopped, which silently *narrowed* the graph. `ReviewRequest` is the
concrete failure: frozen §3.4 makes `evidence_refs` **required** and its
ordering **normative** — *contract, diff, deterministic evidence first* — and
G-R5's registry gave `ReviewRequest` no contract target, no diff target and no
evidence target at all. G-R5 had also rejected the contract target on a false
premise, that it would *replace* `scope_ref → ScopeContractV1`. Frozen v1 keeps
both concepts: `ScopeContractV1` is the rank-0 typed leaf declaring
`allowed_paths`, while frozen `ArtifactKindV1` independently lists
`contract_document`, `diff` and `gate_log`. Scope says what may change; the
contract document says what is being built.

The same finding kills G-R5's line that "the remaining 34 rows are field-path
proposals over already-admitted relations". Counted at `37502e3`: **eleven** of
the 53 prototype rows originate on one of the eight open surfaces, so under
G-R5's own new rule none of their targets was already admitted.

Closed by §4.2.1, which now walks all eight surfaces and states a disposition
for each — one narrowed to three admitted targets, one sanctioned open, six with
no admitted target — and by §4.2.3, which admits `ReviewRequest →
ContractDocument`, `→ Diff` and `→ GateLog`. The deterministic-evidence choice
is argued from frozen text, not the prototype: `required_evidence_gate_ids` is a
**required** field of the same schema, so the request already commits to which
gates must have run, and `gate_log` is what carries a gate's outcome.

**P1-13 — "exact source semantic kind" was not exact.** Rows 31–41 were keyed on
`CampaignEventLog`, so the registry admitted `CampaignEventLog → ReviewVerdict`
and could not reject `CoderReportReceived.source_ref → ReviewVerdict`. The
rejection would have had to come from the per-event wire schema — which is
precisely what §4.2.6 forbids, having just said wire fields *realize* graph
authority and never create or narrow it. A registry that needs the wire to fix
its own admissions is not the authority it claims to be. The six payload rows
had the identical defect one level down: they were the union of targets
belonging to six different payload schemas, so keying on `CampaignEventPayload`
admitted `receipt_ref`'s target for `TransitionRejectedPayloadV1`.

Closed by §4.2.2: **where the frozen contract fixes targets per variant, the
variant is part of semantic source identity.** The registry is keyed on
`CampaignEvent(<event_kind>)` and on `<PayloadVariant>`. This costs 21 event
nodes and 11 payload nodes — the typed universe goes from 17 to 47 — and buys a
completeness check the coarse form could not express: all 21 frozen event kinds
now appear, eleven as `Causal` `source_ref` sources, eleven as `Intra` payload
containers, `HumanCommandRejected` as both.

**P1-14 — `CorrectiveDirective → ReviewVerdict` was in the wrong class.** §4
defines `Intra` as *within one round's derivation flow* and `Causal` as
*crossing rounds*. Frozen §3.7: a `CorrectiveDirective` "starts the next coder
execution directly"; frozen §3.15.1's transition row gives
`CorrectiveDirectiveIssued` a **new `active_round_id`**. So the directive opens
round N+1 while its `review_verdict_ref` names round N's verdict — the
definition of crossing a round. `edges.rs:281–286` classifies the same edge
`Causal` independently. Reclassified; 50 `Intra` / 13 `Causal`.

The Kahn proof passed with the edge misfiled, and that is the part worth stating
plainly rather than filing as cosmetic: the proof was not wrong, it was
answering the question about a slightly wrong graph. A cross-round obligation
sitting inside the kind-level proof domain is a proof that succeeds for the
wrong reason.

**P1-15 — two terminal kinds were merged, and edges 49–50 had not earned
ACCEPT.** §5.1 names four distinct wrappers; §4.2.4 collapsed
`CandidateMaterializationRef` and `WorktreeMaterializationRef` into an invented
`MaterializationAttestation`, and invented a `WorktreeCorrespondence` node
without saying it is a CAS blob rather than a wrapper. The design input
distinguishes the two structurally — the continued-candidate variant pairs
`candidate_state_ref` with `materialization_ref`, the initial variant pairs
`run_contract_ref` with `worktree_ref` plus a correspondence blob — so the
binding has **five** outgoing relations, not four. It also meant the node
universe proved acyclic in §4.2.5 was not the one §5 names. Split, with §5.1's
in-degrees now derived from the registry (3 / 2 / 1 / 1) instead of measured on
the prototype.

The second half reverses G-R5. The accepted chain was: *a directive starts the
execution → an execution needs its exact input state → the directive must
reference that input state.* The last step does not follow, because §3.1 had
already admitted `CampaignRunBinding` as the durable execution-to-input-state
authority, admitted before dispatch. Frozen §3.7 carries no input refs at all.
So `KEEP_V1_MODEL` wins and edges 49–50 come out — this was the G-R1 error
repeating in a new place: proving that an *execution* needs a fact and
concluding that every artifact near it must carry that fact.

One genuine question passes to the v2 draft rather than being answered here:
frozen `WorkOrderV1` *does* carry `input.candidate_ref` and
`input.materialization_attestation_ref`, which the binding now duplicates. §1's
default keeps them; changing that is a supersede decision with an argument, not
a side effect of introducing the binding.

**Two P2s, both closed.** §4 sanctioned `AnyCommittedEnvelope` as an open target
for `CampaignFeedItem` causation while §4.2.4 claimed no wildcard row — one of
the two had to die. The sanction survives, as a **row** in the registry: what
§4.2.4 excludes is a wildcard *source* and a class row, and a single named open
target adjudicated once and visible in the table is not rank admitting targets
by rule. Second: G-R5's reconciliation said "rows 1–30 correspond to 28 slots,
one row each"; it is 26, the other six slots being the two-row and two-slot
special cases. Both the line and the arithmetic are rebuilt at the end of
§4.2.4, and it balances: 26 + 1 + 2 + 2 + 1 = 32 slots → 30 + 11 = 41 rows, then
41 + 3 + 1 + 7 + 11 = 63.

**Method note.** The §4.2.4 table is now generated from the same data the proof
runs on, and the published table was then re-parsed out of the markdown and
re-proved independently. G-R5 hand-wrote a table and hand-checked a proof
against a separate copy of the same list, which is how a row for a relation
nobody proposed survived to commit.

### G-R5 — fifth independent review, two P1s

`CHANGES_REQUESTED`. The review accepted the field-path deferral of §4.1.3 as a
real circularity rather than a dodge — and then showed that G-R4 had retreated
one layer further than the circularity forced.

**P1-10 — the class table is not a registry, and was not even a superset.**
G-R4 published `message -> message | typed support | external | CAS` and three
sibling rows. That is very nearly the cartesian product of the class set, and it
cannot discharge the acyclicity obligation: it says a message may reference a
message, not *which* messages, so nothing in it excludes a two-cycle between two
message kinds. Verified against the frozen blob, it was also not a widening of
the retained graph but a different one. Line 2019 of blob `7db92f1b`:

```text
| `evidence_refs` | `[ArtifactRef]` | yes (may be empty) | rank <= 4; <= 256 |
```

That is `CampaignTerminalErrorPayloadV1` — an event payload admitting rank-3 and
rank-4 targets, i.e. messages. G-R4's table carried no `event payload ->
message` row at all. A class universe that forbids a frozen retained edge is not
a coarser model of the same graph.

**P1-11 — 40 `KEEP` conflated a surface with a semantic relation.** §4.1.1's
inventory is 40 `ArtifactRef`-valued *slots*, and G-R4 marked every one `KEEP`.
Eight of them are not exact relations at all but open surfaces:
`envelope.artifact_refs`, the two report `evidence_refs`,
`CampaignFeedItem.subject_refs`, `HumanAttentionRequest.evidence_refs`,
`CampaignEventV1.evidence_refs`, the terminal-error payload above — all admitted
by the rank rule — and `ReviewRequestV1.evidence_refs`, which carries no rank
bound at all and is constrained only by an ordering requirement. Keeping the
first seven "unchanged" keeps rank as the admission authority, through the one
door the registry does not watch, after §4 demoted rank to a derived property.
Keeping the eighth unchanged keeps an unbounded surface on the reviewer's input.
`KEEP_SURFACE` and `KEEP_EXACT` are now distinct dispositions, and only the
second is a Phase G decision; the split is 8 open, 32 exact.

**What G-R5 adds.** One new layer between the §4.1.1 inventory and the wire
obligations, not a fifth rebuild of the document:

- **§4.2** — the semantic edge layer: `exact source kind -> exact target kind
  (or a sanctioned open target) + edge class`. Field paths stay with the v2
  draft; edge *meaning* is closed here.
- **§4.2.1** — the eight open surfaces are separated from the 32 exact slots,
  under a frozen rule: *a generic `ArtifactRef`-valued field creates no graph
  authority; a field may realize only semantic edges admitted by §4.2.4; rank
  admits nothing and is checked against the registry rather than consulted by
  it.*
- **§4.2.2** — the event log's per-event relations are enumerated from frozen
  §3.15.1 rather than treated as one generic `source_ref`: eleven of the
  twenty-one `event_kind` entries carry a target, so one field path is eleven
  semantic edges, all `Causal`.
- **§4.2.3** — the 38 unmatched prototype rows are reduced to semantic
  proposals. Four are genuinely new relations and are adjudicated here under
  §1's default rather than passed on as spelling: `CorrectiveDirective ->
  CandidateStateReceipt` / `MaterializationAttestation` **ACCEPT** (R5.1 made
  the directive start an execution, and an execution needs its input state);
  `HumanAttentionRequest -> CandidateStateReceipt`, `Human* ->
  AuthenticatedPrincipal`, and `ReviewRequest -> ContractBlob` all **REJECT for
  V0**. The remaining 34 are field-path proposals over already-admitted
  relations.
- **§4.2.4** — the registry: 50 edges, 39 `Intra` and 11 `Causal`, no wildcard
  and no class row, closed with a reconciliation that traces every row to one of
  exactly four origins.
- **§4.2.5** — acyclicity by machine rather than by inspection. Kahn over the
  17 typed nodes and the 17 `Intra` edges among them sorts completely; the other
  17 nodes the registry mentions are graph-terminal by construction.

**Two drafting errors caught inside this revision**, before commit, and recorded
rather than quietly fixed — the whole point of this layer is that a relation
nobody argued for cannot enter the graph, and both of these were exactly that:

1. The first §4.2.4 table closed at 49 rows with `ProviderExecutionReceipt ->
   ReviewVerdict` marked `Causal` and sourced to "accepted proposal" — a
   relation §4.2.3 never proposed — while the two edges it did accept were
   missing. Corrected to 50 rows.
2. §4.2.1 first listed seven open surfaces and called them all rank-governed.
   `ReviewRequestV1.evidence_refs` is an eighth, and it is the one with no rank
   bound at all: frozen §3.4 constrains it only by ordering. Found by requiring
   the registry to reconcile slot-by-slot against §4.1.1 instead of asserting a
   count — 32 exact + 8 open = 40 balances; 33 + 7 did not.

Both were self-caught by the reconciliation now published at the end of §4.2.4,
which is the check the reviewer should run first (§8.1).

**P2 carried out of this round.** `docs/tasks/a1-f-v2-convergence.md` still
described itself as `status: DECIDED (revision G-R2), awaiting re-review` while
listing `review_3` and `review_4`. Corrected in this commit.

### G-R4 — fourth independent review, one P1

`CHANGES_REQUESTED` on a single finding, and it landed on the one place where
this document had stopped applying its own rule.

**§4.1 used the prototype as baseline.** G-R3 enumerated all 59 prototype rows
and labelled them `KEEP` / `RETYPE` / `POST_V0` / `REMOVE` / `A2` — every label
relative to `37502e3`. So `KEEP` meant "unchanged relative to the prototype",
while §0 calls the prototype evidence and never authority and §1 makes
`KEEP_V1_MODEL` the default. Three rounds spent defending that principle, and
then the registry — the most exact artefact in the document — was derived
entirely from the input with no authority.

Quantified during the fix: of 53 prototype envelope-source rows, **15** have a
frozen slot of the same name and **38** have none. Thirty rows were labelled
`KEEP` for edges the frozen contract does not contain. `WorkOrder.goal
.contract_blob` is the plainest: `contract_blob` occurs zero times in blob
`7db92f1b`, where the frozen `WorkOrderV1` carries `scope_ref → ScopeContractV1`.
`ProviderInvocationReceipt → CampaignRunBinding` was `RETYPE` for an object that
did not exist in v1 to be retyped. And the prototype branch never implemented
the reducer, so its registry has no rows at all for `CampaignEventV1` and its
payloads, which §4 simultaneously lists among the participating typed nodes.

Rebuilt in G-R4:

- **§4.1.1** — the baseline is now the frozen reference inventory: 40
  `ArtifactRef`-valued slots extracted from blob `7db92f1b`. The first pass
  returned 34; a completeness check found six more in combined table rows. The
  miss is recorded, since an inventory whose own derivation went unchecked is a
  failure this track has now hit three times.
- **§4.1.2** — the 59 prototype rows are demoted to evidence with their own
  classes, and the 38 unmatched rows are explicitly *not* adjudicated here:
  `contract_blob` versus `scope_ref` is a payload-schema question, and §7
  already assigns payload shapes to v2 drafting.
- **§4.1.3** — the boundary is stated rather than deferred. An exact registry is
  keyed by field path; field paths are payload schema; Phase G does not decide
  payload schemas. So G closes the admissible **node-pair universe** — which is
  what actually determines acyclicity, the rank domain and closure traversal —
  and records the field-path spelling as an obligation owed by the v2 draft,
  with the 38 unmatched rows named as its input.
- **§4.1.4** — `CampaignEventV1` is frozen as a source-only log root, never an
  `ArtifactRef` target, with its payload as a legitimate target. This is a Phase
  G question because it decides whether the closure resolver walks event
  references, and it does, under the same bounds as artifact closure.
- **§4.1.5** — `CandidateReceipt` versus `CandidateAdmissionReceipt` and
  `ProviderExecutionReceipt` versus `ProviderInvocationReceipt` are recorded as
  an open supersede decision. FD-1.9 closes `ArtifactKindV1` spellings as a
  digest input, so G-R3's silent use of the prototype's names was proposing a
  wire change by transcription. Phase G does not choose the names; it refuses to
  let them be chosen by which branch a script happened to read.

Nothing in §3 moved. The node universe, the count of eleven,
`CampaignRunBinding` as typed support authority, and the rank model are all as
G-R2 left them and as the third review approved them.

### G-R3 — third independent review, two P1s

`CHANGES_REQUESTED`; the node universe, the count of eleven, and
`CampaignRunBinding` as a typed support authority were all accepted, as was the
finding that pre-dispatch admission does not smuggle in a message lifecycle.
Both remaining findings accepted.

1. **Phase G had not closed its own Q3.** The mandate asks for the *exact
   registry*; G-R1 and G-R2 supplied a model and deferred the registry to v2
   drafting. That is not a small gap: after three retypings and one removal the
   prototype registry cannot serve as the v2 registry even mechanically, so the
   draft would have inherited the right to decide the graph under the heading
   "concrete shape". §4.1 now carries all 59 entries classified `KEEP` /
   `RETYPE` / `POST_V0` / `REMOVE` / `A2`, giving a retained V0 universe of 48.

   Deriving it surfaced two things the prose had missed: the SafeRedrive path is
   three edges rather than one, and
   `ProviderInvocationReceipt.execution_cause.prior_verdict_ref → ReviewVerdict`
   is a support node referencing a message — which is the sharpest available
   argument for G-R2's own rank domain.

2. **The consumer-degree metric had drifted from the ontology.** It still read
   "envelope-source edges", which stopped being derivable the moment G-R1/G-R2
   retyped three sources to support. The numbers were right; they no longer
   followed from the definition printed above them. Rewritten over retained
   typed-node edges, with `V0 out`, `V0 in` and `raw out` as three labelled
   columns instead of one table quietly holding two projections.

Tail, and the sharpest of the round: §1 claimed "an object nothing references
does not need an identity", and the table one page later refutes it —
`WorkOrder` has in-degree 0 and is plainly a message. Zero degree establishes
only the absence of reference-driven evidence; the lifecycle half of the test
still has to be answered. `ArtifactImported` now rests on both conjuncts
explicitly, which is what actually distinguishes it from `WorkOrder`.

Note carried to the v2 draft, not a Phase G decision: do not identify the
abstract binding identity `B` with a blob digest in advance. Phase G left digest
domains open deliberately, and a canonical-writer version change can make byte
identity and semantic binding identity separate questions.

### G-R2 — second independent review, two P1s

`CHANGES_REQUESTED`; the central conclusion of eleven envelope kinds was not
challenged and is unchanged. Both findings accepted.

1. **Binding uniqueness was enforced too late.** G-R1 delegated "one binding per
   execution" to the acceptance of a later referencing artifact — after the
   provider dispatch that the binding exists to identify. Two bindings for one
   execution could both clear the pre-dispatch stretch, and a post-factum
   rejection repairs nothing. Replaced by a **pre-dispatch binding admission**
   invariant: exactly one binding identity established durably before the first
   dispatch, same-B replay idempotent, different-B a fail-closed conflict with no
   dispatch, and the same resolution after restart. Recorded explicitly as a
   controller/reducer obligation that is *not* a reason to promote, with a
   reopening trigger if the v2 draft cannot express it without a message
   lifecycle.
2. **Derived rank was defined over the wrong graph.** G-R1 narrowed it to the
   `Intra` *message-kind* subgraph, one paragraph after demoting three objects
   that are not terminals — `ProviderInvocationReceipt` (out-degree 9),
   `CampaignRunBinding` (5), `InteractionManifest` (3). Half the receipt chain
   would have fallen out of the acyclicity proof. Widened to the `Intra` **typed-node**
   subgraph, following frozen v1's own precedent, where rank 1 was the manifest
   and rank 2 the receipt. Consequence recorded: a typed support object must be
   admissible as an edge source, not only as a target.

Tails: `ArtifactImported` now reads *specific V0 consumer in-degree = 0* rather
than a bare "in-degree zero"; the external wrapper column is labelled as raw
explicit degree, a deliberately different metric that keeps POST-V0 references
visible; and §4's justification no longer cites the promotion G-R1 cancelled.

The characteristic error this round: having correctly decided an object is not a
message, the document nearly forgot it is still a node — and still a pre-dispatch
authority. Which is a fair demonstration of why the two classes needed separate
names in the first place.

### G-R1 — first independent review, four P1s

`CHANGES_REQUESTED` on the first pass. All four accepted; the central conclusion
changed from thirteen envelope kinds to eleven.

1. **Evidence accounting was imprecise.** "53 edges" named the envelope-source
   projection, not the registry, which holds 59 entries plus a global
   `AnyCommittedEnvelope` causation rule. §2 now defines *specific V0 consumer
   in-degree* with an explicit exclusion rule, so an independent reviewer running
   "the same" script arrives at the same numbers. Raw in-degree for
   `ArtifactImported` is not zero; its specific-consumer degree is.
2. **`CampaignRunBinding`'s promotion evidence did not hold.** The `Causal`
   in-edge belongs to SafeRedrive, which is POST-V0 — letting it decide V0 wire
   ontology is backwards. And "two producer lanes" was simply false:
   `required_producer()` classifies both referencing kinds as `Controller`.
   Re-adjudicated as a typed support authority.
3. **`ProviderInvocationReceipt`'s promotion argument was wrong.** FD-5.3
   governs payloads of envelope-bearing artifacts; the receipt has none, so its
   `campaign_id`/`round_id` restate nothing. And FD-11's two congruence
   predicates survive promotion unchanged, because they prove cross-object
   provenance rather than police intra-object duplication. Reverted to
   `KEEP_V1_MODEL`.
4. **E-V0-4 drifted from taxonomy into bounds.** Class does not imply size — the
   design input gives the manifest its own 2 MiB bound — so concluding "typed
   support object, therefore 1 MiB, therefore out of the evidence-blob list" was
   Phase G amending FD-1.4 one paragraph after warning against exactly that. All
   numeric resolution deferred.

Also: derived rank narrowed to the `Intra` kind-level subgraph, since the full
graph does not topologically sort; and the `ArtifactAcceptance` analogy demoted
from proof to caution.

The pattern behind items 2 and 3 is one error, not two. The script proved
correctly *who references whom*; the document then took one step further and
treated "is referenced" as "is a message". Being referenced is a property of the
graph. Being a message is a claim about identity, lifecycle and acceptance, and it
has to be argued separately every time.
