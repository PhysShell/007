# Q-Deck A1: coder / reviewer / human contracts

**Status: NORMATIVE DRAFT — NOT FROZEN.** This document is the normative
A1 contract text produced by the adjudicated freeze-gap review
(`docs/tasks/a1-freeze-gap-inventory.md`, maintainer-ratified 2026-08-07;
rule 3 carve-out of `docs/evidence-and-decision-discipline.md`). It
supersedes the schema prose of issue #95 where they disagree; #95 remains
the scope/rationale record. The freeze sequence is: this text → contract
review (checklist §18) → types + construction API → RED matrix → FREEZE.
A1 implementation does not begin before the freeze. After the freeze,
changes follow the supersede path only.

Review rounds: `f65b21f` — CHANGES_REQUESTED, six P1 findings
(P1-1 input-state binding incomplete; P1-2 rank cannot represent legal
controller→controller refs; P1-3 attention lifecycle mutated an immutable
artifact; P1-4 duplicate idempotency identities on the human command;
P1-5 replay/`accepted_at` ordering permitted a canonical-blob fork;
P1-6 normalized-output provenance unbound) — all six incorporated
(§7.1a, §11, §8.7, §8.8, §4.6, §8.6; checklist items 6–8 added to §18).
`a5615b8` — CHANGES_REQUESTED, nine P1 findings on cross-section seams
(P1-7 happy-path round violated its own execution cardinality; P1-8
opaque initial-input refs + unbound reviewer execution input; P1-9
undefined `round_binding_ref`; P1-10 the DAG was not a closed universe —
open node classes, unlisted producer-binding/cause edges, no per-kind
producer mapping; P1-11 crash window between idempotency claim and blob
store; P1-12 transport session inside semantic identity; P1-13
HumanDecision source binding contradiction; P1-14 denormalized
antecedent identities without equality proof; P1-15 three open
semantics: attention transitions, budget predicate, per-outcome
normalized-output presence; plus the writer-supplied `artifact_refs`
second reference surface) — all incorporated (§2.1/§2.3, §7.1/§7.1a,
§8.3, §11.1–11.4, §4.6, §8.8, §8.4, §8.6/§8.7, §12.4, §3
respectively). `4f51457` — CHANGES_REQUESTED, six P1 findings, now
almost entirely about provability of already-correct abstractions
(P1-16 `round_ordinal` had no replayable canonical authority; P1-17 the
exact graph was still non-exhaustive — undefined `causation`, omitted
producer/cause edges, a mislabeled `[producer]`, untyped SafeRedrive
evidence, unfrozen edge tags; P1-18 `ArtifactImportedV1` contradicted
§6/§11/checklist #3; P1-19 `RESERVED` lacked a durable construction
seed and a single-representation byte contract; P1-20 the Provider
binding could be provenance-spliced — collapsed to
`{ invocation_receipt_ref }`, `adapter_version` moved into the receipt;
P1-21 `InitialMaterialization` proved coexistence, not
contract↔worktree correspondence — `correspondence_ref` evidence blob
added, grounded in what `o7-worktree` attestation actually proves) —
all incorporated (§2.1/§2.3, §3/§11.1–11.4, §6, §4.5/§4.6, §3/§8.6,
§7.1a respectively). The review ratified as ACCEPTED: per-role chains,
reviewer input binding, no RoundBinding authority, intra/causal split,
Controller-produced receipt, RESERVED/COMMITTED+fencing, session ≠
semantic identity, `HumanCommandRequestRefV1`, immutable attention,
budget predicate, per-outcome output presence, derived `ref_manifest`.
A FOURTH, narrowly-scoped review (these seams + the §18 checklist)
precedes APPROVED FOR TYPES.
`8eb5ec3` — CHANGES_REQUESTED, three narrow contract blockers, no new
architecture ("wire cleanup after the power plant is built"): P1-22 a
stale pre-P1-20 lookup path in §8.3 naming a Provider-binding field
that no longer exists — replaced by the receipt-resolution path, and
§11.2's role annotation defined as a classifier obligation through the
receipt; P1-23 the reservation did not bind its construction seed —
seed now digest-bound and immutable at RESERVED, binding digest
recomputed from the resolved seed before first AND recovery
serialization (mismatch fails closed), unsupported seed writer_version
fails closed with no fallback, and `message-payload-blob` added to the
class-2 registry; P1-24 the ref_manifest collection rule now includes
`CausationV1`, `Artifact` causation gets the standard cross-object
verification (resolved kind/id must equal carried claims), and the
correspondence-blob edges reclassified `intra` (class-2 CAS target,
not an external wrapper). Stale `input_candidate_binding` vocabulary
removed from §7.3. Next: narrow diff-verification of these paragraphs
+ a §18 re-run — expected verdict APPROVED FOR TYPES.

Inputs at pinned revisions: accepted A0
(`docs/q-deck/a0-candidate-state.md`, contract-first `71800fc`, accepted
`52627c3`, merged `f1ac458`), R1 (`docs/q-deck/r1-command.md`, PR #90),
`docs/autonomy-controller.md` (accepted `c5b3ae0`, merged `c5c51e06`),
`docs/decision-and-admission-protocol.md` §5,
`docs/evidence-and-decision-discipline.md` (ratified). Issue #94 is a
non-normative risk note; the fragments A1 needs are internalized here
(§13) and never cited as authority.

## 1. Scope

One autonomous corrective loop of two specialized model roles plus a human
control lane (issue #95 topology, unchanged):

```text
coder -> report -> controller (derive, validate) -> review request
-> reviewer -> report -> controller (validate -> verdict)
   |- accepted          -> ready
   |- changes requested -> corrective directive -> coder
   |- ambiguity / risk  -> human
```

Autonomy means four properties: (1) the coder receives an exact task and
exact code state; (2) the reviewer verifies an exact candidate; (3) the
controller converts a validated verdict into the next permitted step;
(4) a human receives a timely alert (per delivery tier, §16) and can
intervene.

No direct model-to-model channel. All communication flows through the
controller as durable, typed, replayable artifacts. Every artifact class
exists as an untrusted raw input and a controller-accepted canonical
artifact; nothing a model or client emits is authority until a controller
classifier accepts it.

## 2. Identity model

### 2.1 Two axes, one bridge

A1 has a LOGICAL lineage axis and a PHYSICAL execution axis. They are
never identified with each other; a canonical binding receipt connects
them:

```text
logical lineage:     root_goal_id -> task_id -> campaign_id -> round_id
physical execution:  round -> CampaignRunBindingV1
                           -> conversation / command / run / attempt
```

A **campaign is a distinct logical authority, not a new name for an R1
conversation**. `campaign_id != conversation_id`, always. One campaign may
bind multiple conversations. Rationale (ratified): a 1:1 identification
breaks reviewer independence — the reviewer would either continue the
coder transcript or become the conversation's new tail and poison further
coder continuation.

```text
CampaignRunBindingV1:
  campaign_id
  round_id
  round_ordinal              # §2.3 — inline, replayable (P1-16)
  role                       # coder | reviewer
  provider_execution_id
  conversation_id
  command_id: Option<CommandId>   # present only where the execution
                                  # really was an R1 continuation
  run_id
  attempt_id
  input_state_binding: InputStateBindingV1   # §7.1a — what THIS execution
                                             # actually materialized
```

Rules:

- the coder corrective lane MAY continue one R1 conversation (provider
  session continuity is useful there);
- every reviewer execution gets a fresh provider session and its own run
  binding; the v1 default is a separate conversation per reviewer
  invocation;
- `run_id`/`attempt_id` are durably allocated before provider dispatch
  (the R1 "durable acceptance before invocation" rule, unchanged);
- `producer_run_id` is DERIVED by the controller from
  `CampaignRunBindingV1`; it is never adopted from an incoming envelope;
- **execution-input equality** (review round a5615b8, P1-8): the
  binding's `input_state_binding` must cross-verify (§7.1a rule) against
  the execution's actual A0 materialization, and the acceptance
  classifier of the role's report verifies it EQUALS the input the
  dispatching artifact named — the coder binding equals the
  WorkOrder/CorrectiveDirective input; the reviewer binding is
  `ContinuedCandidate` of exactly the candidate the ReviewRequest names.
  Otherwise "review X" can execute against a materialized Y and
  exact-candidate review becomes a literary genre.

A reference to a binding is itself typed (needed by §8.3):

```text
CampaignRunBindingRefV1:
  campaign_id
  round_id
  round_ordinal
  role
  provider_execution_id
  blob_ref                   # CasObjectRefV1 of the canonical binding
```

Its checked constructor proves the blob resolves to a
`CampaignRunBindingV1` carrying exactly that identity tuple.

### 2.2 Topology and supersede

The v1 active topology: exactly one root goal, one task under it, at most
one active campaign per task. The controller mints and durably binds
`root_goal_id`/`task_id`/`campaign_id` atomically before any dispatch. A
replacement execution mints a new `campaign_id` with an explicit
`supersedes_campaign_id`; a terminal campaign is never mutated or reused
(§12.6 for the supersede barrier).

### 2.3 RoundId

```text
RoundId:        opaque, controller-minted, unique within its namespace
round_ordinal:  monotonic within a campaign, begins at 0
```

**The pair travels together, canonically** (review round 4f51457,
P1-16 — §11.4's strictly-lower-ordinal predicate was a check only a
live controller context could make; "we knew the ordinal once" is not
replayable evidence, and D2 forbids exactly that). `round_id` and
`round_ordinal` appear as ONE inline identity pair in every round-scoped
canonical object — the envelope core, `CampaignRunBindingV1`,
`CampaignRunBindingRefV1` — and BOTH enter `message_binding_digest`.
This is identity metadata, not reducer authority: no `RoundBinding`
artifact exists, and historical replay re-proves every ordinal
comparison from canonical bytes alone.

A round is created before the first coder dispatch of that round and
binds: `campaign_id`, `contract_digest`,
`input_state_binding: InputStateBindingV1` (§7.1a — always present:
`InitialMaterialization` for the first round of a fresh task,
`ContinuedCandidate` afterwards), and the `WorkOrder` or
`CorrectiveDirective` ref that opened it.

**Execution cardinality is per ROLE CHAIN, not per round** (review round
a5615b8, P1-7 — the happy path contains a coder execution AND a separate
fresh-session reviewer execution: two execution ids, no redrive anywhere;
a per-round "one execution unless redrive" rule outlawed the contract's
own §1 flow). A round contains: one coder execution chain, and — only
after an accepted `CandidateAdmissionReceiptV1` — one reviewer execution
chain. Within each chain, additional `ProviderExecutionId`s exist ONLY
as proven safe pre-dispatch redrive predecessors
(`ExecutionCauseV1::SafeRedrive`), and exactly one usable terminal
result per role chain may be accepted. A round admits at most one
accepted `CandidateAdmissionReceiptV1` and at most one accepted
`ReviewVerdict`. Round outcomes (closed set):

```text
ACCEPTED | CHANGES_REQUESTED | BLOCKED | HUMAN_REQUIRED
| BUDGET_EXHAUSTED | CANCELLED | SUPERSEDED | FAILED
```

`CHANGES_REQUESTED` mints a new round; a finished round never reopens.

### 2.4 Lineage authority rule (unchanged from #95)

Lineage fields carried by an incoming artifact are claims. The controller
resolves the expected lineage from the causation target and the canonical
campaign binding, then verifies the carried fields match; mismatch fails
closed. Two matching claims are not proof.

## 3. Envelope core and producer binding

The common envelope is SMALL. Everything role-conditional lives in a
tagged producer binding or in the typed payload — no nullable soup.

```text
EnvelopeCoreV1:
  envelope_version
  message_kind
  message_kind_version
  message_id                 # idempotency key (§4.3)
  root_goal_id
  task_id
  campaign_id
  round_id + round_ordinal   # the §2.3 pair, where applicable to the
                             # kind (typed per kind)
  causation: CausationV1
  producer: ProducerBindingV1
  payload_digest
  ref_manifest               # controller-DERIVED, see rule below
  recorded                   # controller-assigned acceptance metadata (§4.4)
```

**`ref_manifest` rule** (review round a5615b8 — a second, writer-supplied
reference surface next to the payload's own refs lets two correct
writers produce different binding digests, and lets the envelope list
and the payload diverge). `ref_manifest` is the controller-derived EXACT
manifest of ALL direct digest references of this artifact — collected
mechanically from the typed payload, the producer binding, AND
`CausationV1` (review round 8eb5ec3, P1-24: causation was declared a
digest edge but omitted from the collection rule two paragraphs later),
deduplicated, sorted by the total order `(edge kind tag, target digest
bytes)` lexicographically. It is never writer-supplied; a manifest that
does not equal the mechanical collection is not constructible. This
manifest is what "typed artifact refs in canonical order" means in
`message_binding_digest` (§4.3), and it is the input the §11 DAG check
runs on.

**`CausationV1`** (review round 4f51457, P1-17 — "typed identity of the
causing artifact" was undefined: as a digest ref it bypassed the DAG, as
anything else it was unspecified):

```text
CausationV1 =
  Artifact {
    message_kind
    message_id
    blob_ref: CasObjectRefV1
  }
| CampaignGenesis            # valid ONLY for the campaign's first
                             # WorkOrder (round_ordinal 0); names no
                             # artifact — the campaign lineage binding
                             # is the cause
```

`Artifact` causation IS a digest edge: it participates in
`ref_manifest` and the §11 DAG as a `causal`-class edge whose target may
be any envelope-bearing kind already committed. And it gets the SAME
cross-object verification every typed ref gets (P1-24 — otherwise
`message_kind`/`message_id` are two independently valid claims standing
next to the real blob identity, and §2.4 then uses the causation target
as lineage authority): the checked constructor requires `blob_ref` to
resolve to a COMMITTED canonical artifact whose envelope's
`message_kind` equals the carried `message_kind` and whose `message_id`
equals the carried `message_id` — mismatch is not constructible.

```text
ProducerBindingV1:
  Controller {
    component_version
    policy_digest
  }
  Provider {
    invocation_receipt_ref   # the ONLY field (P1-20): role, execution,
                             # run binding, model route, adapter,
                             # prompt/tool-policy digests all resolve
                             # THROUGH the receipt and its canonical
                             # request — no denormalized copy to splice
  }
  Human {
    authenticated_principal_ref      # AuthenticatedPrincipalV1, §8.8
  }
```

A provider-produced artifact without a `Provider` binding, a
controller-derived artifact carrying one, or a human artifact without an
authenticated principal are all unrepresentable at the wire-type level
(§15.1). `contract_digest`, candidate preconditions, and action-specific
bindings live in the typed payload of the kinds that need them, not in the
core. Unknown `envelope_version`/`message_kind_version` is rejected fail
closed — never best-effort parsed.

Removed from the draft envelope, deliberately: `created_at` (§4.4),
`correlation_id` (no closed semantics; `campaign_id` is the correlation
scope; spare fields have a habit of becoming future authority),
`expected_input_head` (§7.3).

## 4. Digests, canonical bytes, message identity

### 4.1 Two digest families

```text
BlobDigest        = SHA-256(exact stored bytes)
                    content identity: CAS and artifact refs.
                    No normalization, no re-serialization, no
                    "these JSONs are semantically equal".
                    NOT domain-separated (it is a content address;
                    type protection lives in the typed ref).

Protocol digests  = MessageBindingDigest, ContractDigest,
                    RegistryDigest, PolicyDigest, ...
                    computed over explicitly framed, typed fields
                    with domain separation — never a hash of an
                    arbitrary JSON serialization.
```

This matches the two existing precedents: `ArtifactRef` binds exact
bytes; the event chain and `LaunchRequest::spec_digest` use explicit
length-prefixed framing with set-fields sorted.

### 4.2 Domain separation registry

A single compile-time registry of digest contexts lives in the protocol
crate. Context form:

```text
o7-a1\0<purpose>\0v1\0

o7-a1\0message-binding\0v1\0
o7-a1\0contract\0v1\0
o7-a1\0gate-registry\0v1\0
o7-a1\0verifier-policy\0v1\0
o7-a1\0model-route\0v1\0
```

Constants only — never assembled from user input; one purpose is never
reused for two types; the registry carries a uniqueness test and a
known-answer test per context.

### 4.3 Message identity

Three distinct things, never conflated:

```text
payload_digest          = BlobDigest of exact stored payload bytes,
                          envelope excluded
blob_digest             = BlobDigest of the exact complete stored
                          artifact bytes
message_binding_digest  = domain-separated framed digest of all
                          semantic bindings
```

`message_id` is the idempotency key; `message_binding_digest` is its
request digest (the same key-vs-request-digest split the ledger's
`idempotency_record` already uses). `message_id` itself is NOT part of
the binding digest.

In `message_binding_digest`: envelope version; kind + kind version;
`root_goal_id`; `task_id`; `campaign_id`; the `round_id + round_ordinal`
pair where applicable; `CausationV1`; producer binding; contract binding
where applicable; `InputStateBindingV1` — MANDATORY for every work-dispatching
kind (WorkOrder, CorrectiveDirective), never optional (§7.1a);
`payload_digest`; the derived `ref_manifest` (§3 — deduplicated, sorted
by `(edge kind tag, target digest bytes)`); provider
execution/invocation binding where applicable.

NOT in it: `message_id`; `accepted_at`/`recorded_at`; delivery attempt;
transport connection/session metadata; UI correlation metadata.

Duplicate rule: same `message_id` + same `message_binding_digest` →
idempotent replay; same `message_id` + different binding digest →
conflict, fail closed.

### 4.4 Time

`created_at` does not exist in canonical A1 artifacts. A provider receipt
may carry `producer_observed_at` as an untrusted observation; the
controller/ledger assigns `accepted_at` at acceptance. Neither defines
order — order is canonical append/sequence and the causation graph.
Replay returns the stored bytes; it never re-creates an artifact with a
fresh time.

### 4.5 Canonical encoding

Canonical A1 artifacts are UTF-8 JSON with: `deny_unknown_fields`;
duplicate fields rejected; no Unicode normalization; no float fields.
OS-byte values (`RepoPathBytes` and kin) use one frozen `ByteStringV1`
representation: base64url without padding — A0 deliberately preserves
non-UTF-8 paths and patch bytes, and A1 must not undo that guarantee with
a convenient `String`. No "semantically equal" comparisons outside parsed
types.

**One writable representation** (review round 4f51457, P1-19 — "recovery
rebuilds the SAME bytes" is impossible while §4.5 only forbids bad JSON
without defining the single good one). Canonical A1 bytes are produced
by ONE frozen writer, `CanonicalJsonV1`: object keys sorted by byte
order; no insignificant whitespace; a fixed minimal escaping table
(escape only what JSON mandates, one form each); integers only, no
floats (§ above); byte values via `ByteStringV1`. `recorded` has a
frozen shape, `RecordedMetadataV1`: `accepted_at` as an unsigned integer
count of nanoseconds since the Unix epoch, UTC (no string formats, no
precision ambiguity), plus `writer_version`. The writer carries a
version constant and a known-answer corpus (input model → exact bytes)
maintained with the same discipline as the digest-context registry
(§4.2); an upgrade that changes any byte for any corpus entry is a new
`writer_version`, and recovery of a `RESERVED` reservation uses the
reservation's recorded writer version — never the current binary's
default.

### 4.6 Acceptance construction ordering — replay can never fork the blob

"Replay returns stored bytes" (§4.4) is an intention; this ordering is
its enforcement (review round f65b21f, P1-5). Because `accepted_at` lives
outside `message_binding_digest` but inside the stored artifact bytes, an
implementation that assigns `accepted_at` BEFORE the idempotency check
would serialize a second canonical blob (`B2 { accepted_at = T2 }`) for
an idempotent duplicate of `B1 { accepted_at = T1 }`. Frozen ordering:

The idempotency record is a TWO-PHASE state machine (review round
a5615b8, P1-11 — a single-phase claim leaves a crash window between
claim and blob store in which a duplicate is told "existing" while the
canonical bytes do not exist yet):

```text
ABSENT
-> RESERVED {
     message_id,
     message_binding_digest,
     accepted_at,             # assigned HERE, durable BEFORE the blob
     fencing_generation,      # is built
     construction_seed_ref    # see below — recovery must be able to
   }                          # rebuild the SAME bytes, not admire the
                              # irreversible digest
-> COMMITTED {
     canonical_blob_ref
   }
```

**`CanonicalConstructionSeedV1`** (review round 4f51457, P1-19 — after a
crash the payload, producer binding, causation, and ref_manifest may
exist only in the dead process's memory; `message_binding_digest` is
irreversible, and no recovery scan can reconstruct an artifact from it
regardless of how convincingly the word "recovery" reads in Markdown):

```text
CanonicalConstructionSeedV1:
  message kind + kind version
  lineage (incl. the round pair) + causation
  producer binding
  payload blob ref             # the exact payload bytes, already in CAS
  ref_manifest
  action-specific bindings
  writer_version               # §4.5 — the writer that must rebuild
```

The seed is durable BEFORE or atomically WITH the `RESERVED`
transition. Recovery then holds every construction input — the seed plus
the reservation's `accepted_at` — and rebuilds the byte-identical blob
under the seed's `writer_version` (§4.5). The seed is an idempotency-
store record, not a canonical artifact: it does not enter the §11
universe, and its production durability is A2's, like the rest of the
store (C6).

**Seed integrity** (review round 8eb5ec3, P1-23 — `message_binding_digest`
is irreversible, so a reservation holding a binding digest NEXT TO a
seed ref proves nothing about their marriage; a corrupted or swapped
seed would let recovery serialize semantic input Y under a reservation
made for X, the exact "two valid values standing together" failure this
contract keeps killing). Three frozen requirements:

1. `construction_seed_ref` binds the seed BY DIGEST
   (content-addressed), and the seed bytes are immutable from the
   moment the `RESERVED` transition commits;
2. before the FIRST serialization and before EVERY recovery
   serialization, the acceptor recomputes `message_binding_digest`
   from the resolved seed and requires it to equal
   `RESERVED.message_binding_digest` — mismatch fails closed, the
   reservation is left for investigation, nothing is committed;
3. an unsupported `seed.writer_version` at recovery fails closed —
   NEVER a fallback to the current binary's writer.

The seed's `payload blob ref` targets the closed CAS kind
`message-payload-blob` (§11.1) — the exact payload bytes; no separate
seed-store byte-object species is introduced.

Acceptance flow:

```text
derive message_binding_digest
-> atomic idempotency transition
     COMMITTED, same binding digest -> return the EXISTING canonical
                                       bytes verbatim; assign NOTHING
     any state, other binding digest-> IdempotencyConflict, fail closed
     RESERVED, same binding digest  -> typed IN_PROGRESS (retriable) —
                                       a duplicate never pretends the
                                       bytes already exist
     ABSENT                         -> RESERVE (winner): durably record
                                       accepted_at + fencing_generation
                                       + construction_seed_ref
-> winner serializes the canonical blob (deterministic from the seed +
   accepted_at under the seed's writer_version — same inputs, same
   bytes)
-> idempotent CAS put
-> fenced RESERVED -> COMMITTED (only the fencing_generation owner)
```

Recovery takes FENCED ownership of a stale `RESERVED` (bumping
`fencing_generation` so a resurrected original writer cannot complete a
transition it no longer owns), rebuilds the blob from the durable
reservation — same `accepted_at`, same bytes — re-puts (CAS put is
idempotent), and commits. A replay never receives a fresh `accepted_at`
and never produces a new `blob_digest`.

**A1/A2 boundary held (C6)**: A1 freezes this state machine and the
test-only store interface; A2 owns the production durable
implementation. Otherwise SQLite authority returns through the window
wearing a moustache.

The RED matrix (§15.3) carries the crash/race oracles: two parallel
deliveries converge on ONE canonical blob (byte-identical, one
`accepted_at`), the loser returning the winner's bytes; a crash after
RESERVE and before COMMIT is repaired by recovery to exactly one blob
with the ORIGINAL reserved `accepted_at`; a duplicate arriving during
RESERVED gets IN_PROGRESS, never fabricated bytes.

## 5. Limits — frozen v1 profile

Violation is REJECT, never truncation. A streaming reader stops at
cap + 1 so the limit check cannot happen after a gigabyte is already in
memory (the existing hard-ceiling + refusal shape).

```text
max nesting depth                    32
max artifact refs per artifact       256
max collection items                 4096
max one inline string/byte-string    64 KiB
max aggregate inline prose           256 KiB

WorkOrder                            256 KiB
CoderReport                          512 KiB
CandidateAdmissionReceipt            256 KiB
ReviewRequest                        512 KiB
ReviewerReport                       512 KiB
ReviewVerdict                        512 KiB
CorrectiveDirective                  256 KiB
ProviderInvocationReceipt            256 KiB
InteractionManifest                    2 MiB
CampaignFeedItem                     128 KiB
HumanAttentionRequest                256 KiB
HumanCommandRequest                   64 KiB
HumanDecision                        128 KiB

max one provider evidence blob        64 MiB
max aggregate evidence per execution 128 MiB
```

## 6. Two artifact address models (never one type)

```text
o7_run::ArtifactRef        existing run-relative ref {kind, locator,
                           digest} — name and semantics UNCHANGED,
                           owned by A0/R1.

CasObjectRefV1             global content-addressed object:
                           { digest, size, media_type, content_kind }
```

These are different address models (run-relative artifact vs global CAS
object). No typedef of one into the other. Importing a run artifact into
CAS requires an explicit bridge receipt — and the run-relative source
ref is itself wrapped (review round 4f51457, P1-18: a bare
`o7_run::ArtifactRef` field both violated checklist #3 and carried a
digest that §11 pretended "is not a digest edge" — a digest does not
stop being a digest at a border crossing):

```text
RunArtifactSourceRefV1:              # external-sink wrapper into R1/A0
  source_run_id                      # canonical records, same class as
  run_artifact_ref: o7_run::ArtifactRef   # the A0 wrappers (§11.1)

ArtifactImportedV1:
  source: RunArtifactSourceRefV1
  cas_object_ref: CasObjectRefV1
```

The checked constructor of `ArtifactImportedV1` proves the chain, not
the vibe: resolve the source bytes from the verified run record; the
source `run_artifact_ref.digest` matches those bytes;
`SHA-256(bytes) == cas_object_ref.digest`; the size, `content_kind`, and
`media_type` rules of the target CAS kind are satisfied.

CAS refs resolve only inside 007-owned storage; agent-composed paths and
URLs are inert text. Resolvers follow the descriptor-based, no-follow,
bounded-read discipline the o7d bounded reader already established.

## 7. A0 bindings and the head vocabulary rule

### 7.1 Exact wrappers, no bare refs

```text
CandidateStateReceiptRefV1:
  source_run_id
  run_artifact_ref           # must be ArtifactKind::CandidateState

CandidateMaterializationRefV1:
  child_run_id
  materialization_event_id
  materialization_event_digest

RunContractCandidateStateRefV1:      # review round a5615b8, P1-8: in
  run_id                             # accepted A0 the obligation is NOT
  run_started_event_id               # a standalone artifact — it lives
  run_started_event_digest           # inside RunStarted.contract
                                     # .candidate_state

WorktreeMaterializationRefV1:        # materialization evidence enters
  run_id                             # A0 via WorktreeCreated
  worktree_created_event_id          # { worktree: ArtifactRef }
  worktree_created_event_digest
```

### 7.1a InputStateBindingV1 — the exact input state, never optional

The input state of a round is a CLOSED type, not an `Option` (review
round f65b21f, P1-1: an absent input binding let the same
`message_binding_digest` describe a WorkOrder run once from base X and
once from base Y — exact task, no exact input state):

```text
InputStateBindingV1 =
  InitialMaterialization {
    run_contract_ref:   RunContractCandidateStateRefV1
    worktree_ref:       WorktreeMaterializationRefV1
    correspondence_ref: CasObjectRefV1   # worktree-correspondence-
                                         # evidence-blob, §7.1a rule
  }
| ContinuedCandidate {
    candidate_state_ref: CandidateStateReceiptRefV1
    materialization_ref: CandidateMaterializationRefV1
  }
```

No repository/base/tree fields are re-declared — refs to A0 authority
only. `InputStateBindingV1` is ALWAYS part of `message_binding_digest`
for the kinds that dispatch work (§4.3).

**Cross-object verification rule (frozen, not "proves" by adjacency).**
A pair of independently valid refs proves nothing. The checked
constructor of `ContinuedCandidate` must verify: the
`materialization_ref`'s child run's verified canonical record (full
`verify_prefix`, which since A0 round 2 includes the candidate semantic
layer) contains exactly the named `CandidateStateMaterialized` event with
the named event digest; that event's copied source receipt is
byte/digest-identical to the receipt `candidate_state_ref` names, and its
`source_run_id` equals `candidate_state_ref.source_run_id`. The checked
constructor of `InitialMaterialization` must verify: ONE verified run
(both refs name the same `run_id`, and the run's canonical record passes
full `verify_prefix`); the named events have the right KINDS
(`run_started` carrying a present `contract.candidate_state`,
`worktree_created`) and the named event digests; and the A0 structural
ordering holds (`RunStarted` precedes `WorktreeCreated` in that record).

**Contract↔worktree correspondence** (review round 4f51457, P1-21 —
same-run + ordering proves the two events coexisted in time, not that
THIS worktree materializes THIS repository/base obligation; artifact
says, per rule 4: the accepted `o7-worktree` attestation proves
FILESYSTEM identity and ownership — `dev`/`ino`, uid, `0o700`,
no-follow (`crates/o7-worktree/src/attest.rs`) — and no accepted A0
verifier proves the semantic cross-object predicate). Therefore the
binding carries `correspondence_ref`: a
`worktree-correspondence-evidence-blob` (closed CAS kind, §11.1)
recorded by a NAMED controller-owned verifier (gate-registry
discipline: verifier id + version + policy digest inside the blob) that
observed the attested worktree LIVE at materialization time and
verified: its checked-out commit equals the obligation's base commit,
and its repository identity equals the obligation's repository — stored
as equality verdicts with the verifier's observations, evidence, not a
re-declaration of A1-level identity fields. The constructor requires
the blob to name the same `run_id` and both event digests, and its
verdict to be `verified`. Two independently valid refs from one run
with the right ordering but a non-corresponding worktree are NOT
constructible — the RED oracle for exactly that case is in §15.3.

A binding whose refs do not cross-verify is not constructible.

### 7.2 What A1 must not redefine (unchanged from #95, now with real names)

`CandidateStateReceiptV1` representation, base-commit semantics, the
cumulative patch model, `RepositoryIdentity`, materialization attestation,
and sealing/materialization ordering are owned by
`docs/q-deck/a0-candidate-state.md`.

### 7.3 The head rule

The unqualified word `head` does not exist in canonical A1 vocabulary. A
Git tree OID and an external commit SHA are different things. Removed:
`envelope.expected_input_head`, `WorkOrder.input.base_sha`,
`ReviewRequest.base_sha`, `ReviewRequest.candidate_head`,
`HumanCommand.expected_head`. Replacements are candidate-state refs
(`InputStateBindingV1`, `candidate_state_ref`,
`expected_candidate_state_ref`). If a provider-facing prompt should show a
base commit or tree OID, the controller renders it as a labeled projection
from the A0 receipt. `external_head_sha` is reserved for A3
(GitHub/CI materialization) and does not appear in A1.

## 8. Artifact catalogue

Every kind: envelope core (§3) + typed payload. Raw inputs are untrusted;
acceptance is a classifier (§12.8).

### 8.1 WorkOrder (Controller → Coder)

```text
role: coder
goal:      { contract_digest, summary }
input:     InputStateBindingV1            # ALWAYS present (§7.1a):
                                          # InitialMaterialization for the
                                          # first round of a fresh task,
                                          # ContinuedCandidate afterwards
scope:     { allowed_paths: [RepoPathBytes],
             forbidden_paths: [RepoPathBytes],
             frozen_properties: [...] }
required_evidence:
  gate_requirements: [GateRequirementV1]  # §10
  acceptance_case_ids: [...]
budget:    { max_provider_turns, max_wall_time_seconds }
```

The coder never receives "address the review comments"; it receives the
frozen contract identity, concrete findings (via the directive), and
explicit scope limits.

### 8.2 CoderReport (Coder → Controller, untrusted)

```text
status: candidate_produced | failed | question_blocked
claimed_candidate_tree_oid: Option<GitTreeOid>   # candidate_produced only
change_summary, intent
claims:          [{ claim_id, statement, evidence_refs }]
diagnostic_runs: [{ command_recorded, result, artifact_ref }]
known_residuals: [...]
questions:       [{ question_id, text }]
```

Everything here is advisory. `claimed_state_digest` is deleted (a generic
digest without type/domain/subject becomes a warehouse of stray SHAs).
The optional tree-OID claim: absence is legitimate; presence with a
mismatch against the controller-derived tree is report rejection, fail
closed; no claims about repository/base identity exist; the
controller-derived candidate never depends on the claim.
`diagnostic_runs` are forensic evidence only (`decision-and-admission`
§5); the controller never re-executes `command_recorded`. The coder
cannot emit `accepted` in any form (unrepresentable, §15.1).

### 8.3 CandidateAdmissionReceiptV1 (controller-derived)

Renamed from the draft's `CandidateReceipt` — too close to the A0
candidate-state receipt.

```text
CandidateAdmissionReceiptV1:
  candidate_state_ref: CandidateStateReceiptRefV1
  coder_run_binding_ref: CampaignRunBindingRefV1
  coder_report_ref
  observed_change_set:
    changed_paths: [RepoPathBytes]
    file_modes:    [...]
    diff_scope:    ...
  admission:
    admission_profile
    gate_requirements: [GateRequirementV1]
    classification_policy_digest
  optional_claim_check:
    claimed_candidate_tree_oid
    matches_derived_tree
```

Contains NONE of: `candidate_head`, `candidate_tree_identity`,
`base_ancestry`, `repository_identity`, `base_commit`,
`candidate_tree_oid` — all resolve through the accepted A0 receipt. The
admission profile is derived from the controller-observed diff, never
from coder claims; a claim mismatch fails closed with no review dispatch.
The candidate sealing boundary is consumed from A0 as a capability.

`coder_run_binding_ref` (review round a5615b8, P1-9 — the draft's
`round_binding_ref` was a bare name with no type and no answer whether it
meant the logical round or the run binding): it is the SOURCE CODER
EXECUTION, typed as `CampaignRunBindingRefV1` (§2.1), and the checked
constructor proves it EQUALS the binding resolved through the accepted
report's ONLY provider surface (review round 8eb5ec3, P1-22 — the
pre-P1-20 wording named a `campaign_run_binding_ref` field the Provider
binding no longer has):

```text
CoderReport -> Provider.invocation_receipt_ref
            -> ProviderInvocationReceipt -> campaign_run_binding
``` The logical round
identity needs no new authority artifact — `round_id` already lives in
the envelope and the canonical constructor context; a separate A1
"RoundBinding" authority would be a shadow-A2 object and is deliberately
NOT introduced.

### 8.4 ReviewRequest / ReviewerReport / ReviewVerdict

ReviewRequest carries `admission_receipt_ref` (the accepted
`CandidateAdmissionReceiptV1`), `contract_digest`, `required_properties`,
and `evidence_refs` (contract, diff, deterministic evidence first). The
coder's advisory narrative resolves ONLY through
`admission_receipt_ref -> coder_report_ref` — ReviewRequest carries NO
separate coder-report field (review round a5615b8, P1-14: a second,
independently settable ref let the reviewer receive admission from
report A and narrative from report B; "advisory" does not help — the
model reads it anyway). Delivery order (narrative last, labeled
advisory-claims) is a rendering rule, not a second reference.

Reviewer independence is mechanical, not prompted: fresh provider session
(no continuation of the coder session, enforced by the C5 binding rules —
its own conversation in v1); no coder transcript; detached exact-candidate
fresh attested worktree; no repository or GitHub mutation credentials;
separate prompt/tool-policy identities (digests bound by the invocation
receipt's canonical request, resolved through the report's
`Provider { invocation_receipt_ref }` binding — §3, P1-20).

**Reviewer execution-input binding** (review round a5615b8, P1-8): the
reviewer's `CampaignRunBindingV1.input_state_binding` must be
`ContinuedCandidate` of EXACTLY the candidate `admission_receipt_ref`
names, cross-verified per §7.1a — the ReviewRequest naming X while the
reviewer process materializes Y is not constructible/acceptable.

ReviewerReport is untrusted; it may contain `accepted`, but its JSON
authorizes nothing. ReviewVerdict is minted by the acceptance classifier
only when: schema-valid, supported version; `reviewed_candidate_state_ref`
equals the current accepted admission receipt's candidate ref; the
reviewer run binding's input equals it too (above); reviewer
identity/prompt/tool-policy established through the report's producer
binding; required fields and evidence refs present and resolvable.

```text
ReviewVerdict:
  review_id
  reviewed_candidate_state_ref
  verdict: accepted | changes_requested | blocked
  findings:
    - finding_id, severity: blocker|major|minor|note
      affected_property
      evidence_refs
      required_change
      required_evidence: [GateRequirementV1]
  properties_checked / properties_preserved / residual_risks
  reviewer_report_ref
```

The draft's denormalized `reviewer { model_route_ref,
resolution_evidence_ref, prompt_version }` block is DELETED (P1-14):
reviewer identity resolves ONLY through `reviewer_report_ref -> Provider
producer binding -> invocation receipt`. The rule, general and frozen: a
derived artifact either resolves an identity exclusively through its
antecedent ref, or its checked constructor proves the denormalized copy
EQUAL to the antecedent — never two independently-valid versions of one
truth.

### 8.5 CorrectiveDirective (Controller → Coder)

Bound to the exact `ReviewVerdict` and the exact candidate refs; carries
finding IDs and `GateRequirementV1`s from the verdict; a directive that
attempts a scope change (contract digest drift) is rejected fail closed.
`changes_requested` is never permission to modify the frozen goal,
acceptance contract, verifier, or baseline.

### 8.6 ProviderInvocationReceipt and InteractionManifest

Two grains, closed vocabulary; the generic `retry_of_invocation_id` is
DELETED (it conflated different operations):

```text
ProviderExecutionId   one bounded role execution, spans its tool loop
ProviderDispatchId    one external provider request

ExecutionCauseV1:
  Initial
  CorrectiveRound { prior_verdict_ref }
  SafeRedrive { prior_execution_id,
                prior_receipt_ref,   # adjudicated addition, see below
                evidence: EstablishedNonDispatchEvidenceRefV1 }  # §11.1

DispatchCauseV1:
  Initial
  ToolContinuation { prior_dispatch_id, tool_result_ref }
  SafeRedrive { prior_dispatch_id,
                prior_receipt_ref,   # adjudicated addition, see below
                evidence: EstablishedNonDispatchEvidenceRefV1 }  # §11.1
```

**Adjudicated amendment (types-foundation review T5, option A).** The
§11.3 matrix carries a digest edge
`cause.safe_redrive.prior_receipt_ref -> ProviderInvocationReceipt`,
but the frozen cause schema had no field to carry it — an
implementation must not ratify that repair silently, so it is
adjudicated here, pre-FREEZE: `SafeRedrive` carries `prior_receipt_ref`
(the prior attempt's invocation receipt, by canonical bytes). The ID
says WHO; the receipt ref proves WHICH canonical bytes of that who —
the same evidence discipline as everywhere else. The checked
constructor cross-verifies: the resolved receipt's execution id equals
`prior_execution_id` (execution grain) / its dispatch record contains
`prior_dispatch_id` (dispatch grain); mismatch is not constructible.

ToolContinuation is not a retry. A new session is not a retry. A
corrective round is not a retry. `SafeRedrive` requires evidence of
established non-dispatch — a fresh identifier does not make a duplicate
side effect safe. The full incarnation taxonomy stays A2; these two
grains and causes freeze now because the receipt schema needs them.

The receipt (per execution) binds: execution id + cause; the campaign run
binding; **`adapter_version`** — moved INTO the receipt (review round
4f51457, P1-20: the adapter is what produced `normalized_output_ref`, so
adapter identity on a LATER report's producer binding was provenance
turned backwards — the receipt must answer "which adapter produced these
normalized bytes" even if no report is ever accepted);
`LogicalModelRouteV1` ref + `ModelResolutionEvidenceV1` (§9); canonical
request ref (the exact provider-facing request after adapter
construction) with its `prompt_digest` / `tool_policy_digest` /
`decoding_policy_digest` / `budget_policy_digest`; capture status
(`exact_provider_events` /
`adapter_observations` / `normalized_output_only`); interaction manifest
ref; **`normalized_output_ref`** — the PRE-ENVELOPE adapter-normalized
provider output blob, whose presence is fixed PER OUTCOME VARIANT
(review round a5615b8, P1-15: "whenever the outcome carries usable
output" was an open predicate; §15.1 promises outcome-inconsistent
fields are wire-unrepresentable, so the ref lives INSIDE the variant):

```text
completed          -> normalized_output_ref REQUIRED
refused            -> normalized_output_ref REQUIRED
                      (a refusal is provider output; under
                       normalized_output_only an absent identity would
                       mean "we kept only X and cannot say which X")
incomplete         -> normalized_output_ref OPTIONAL — present iff the
                      adapter captured partial normalized output;
                      absence IS the statement "no partial output was
                      captured"
failed_pre_dispatch-> FORBIDDEN (no dispatch, no output)
dispatch_ambiguous -> FORBIDDEN (an ambiguous execution never has
                      accepted-usable normalized output; partial raw
                      observations stay in the interaction manifest)
```

outcome (closed set: `completed | refused | incomplete |
failed_pre_dispatch | dispatch_ambiguous`) with typed stop/error/usage
refs; `producer_observed_at` (untrusted). Fields the provider or adapter
cannot establish remain absent under an explicit status — never inferred
from an alias, SDK default, or previous invocation.

**Normalized-output provenance chain** (review round f65b21f, P1-6; the
existing `o7 invoke` layout already separates `stdout.raw` from
`result.json` — the same split, now digest-bound):

```text
raw provider blob(s)
  <- pre-envelope normalized-output blob
    <- ProviderInvocationReceipt / InteractionManifest
      <- canonical CoderReport / ReviewerReport
```

The normalized-output blob is pre-envelope, so it never references the
receipt back — no digest cycle (§11). The acceptance classifier must
prove the canonical report payload was parsed/derived from THAT exact
blob: in v1 the raw report's `payload_digest` MUST equal the receipt's
`normalized_output_ref` digest — the adapter's normalized output IS the
raw report payload, one identity, no gap for an unrecorded
transformation. Any future recorded transformation between the two is a
contract change via the supersede path and must bind both digests
explicitly. A report whose payload cannot be tied to the receipt's
normalized output is rejected: a receipt that does not identify the bytes
the report came from is a provenance hole, not evidence.

The InteractionManifest records the observable route (ordered dispatches,
tool calls, tool results, errors) grouped by execution. Recording a
requested tool call does not authorize it (§11 DAG; §12.8 classifiers;
no-executable-authority rule §13). Partial history remains evidence; a
`dispatch_ambiguous` execution is never "completed" by asking the
provider again.

Acceptance of normalized output as a canonical artifact is a separate
subsequent fact — never a field inside the immutable receipt (that would
be a digest cycle, §11).

### 8.7 CampaignFeedItem / HumanAttentionRequest

Two objects, not one alert table. Feed items are informative lifecycle
history — no ack, no decision. Attention requests answer: what happened,
why the system stopped, which exact candidate is affected, which actions
are permitted, what each causes.

```text
HumanAttentionRequestV1:              # immutable — this IS the OPEN state
  attention_id
  campaign_id
  candidate_state_ref: Option
  reason: { code, summary }        # includes EXTERNAL_DRIFT (code
                                   # frozen now; detection lands in A3)
  severity: info | attention | urgent
  required_decision_kind: none | ack | choose_resolution
  options: [{ action_id, consequence }]   # server-defined only
  evidence_refs
```

**No in-place lifecycle** (review round f65b21f, P1-3). A canonical
content-addressed artifact never mutates: changing state would change the
blob digest, i.e. produce a different artifact while pretending to be the
same one. `HumanAttentionRequestV1` is the immutable OPEN request; the
current lifecycle state is DERIVED from subsequent canonical transition
records, whose kinds A1 freezes now:

```text
AttentionAcknowledged { attention_ref, decision_ref }
AttentionResolved     { attention_ref, decision_ref }
AttentionSuperseded   { attention_ref, superseding_attention_ref }
```

Production APPEND of these transitions belongs to A2 (§14) — A1 freezes
the record kinds, the closed transition set, and the derivation rule,
and provides the test-only append sink. The transitions (review round
a5615b8, P1-15 — "OPEN unless a transition says otherwise" did not
answer what Resolved-after-Superseded means):

```text
OPEN         -> ACKNOWLEDGED
OPEN         -> RESOLVED | SUPERSEDED
ACKNOWLEDGED -> RESOLVED | SUPERSEDED
RESOLVED, SUPERSEDED: terminal, monotone — no transition leaves them
```

At most ONE terminal transition per attention identity. A transition
targeting an already-terminal attention is REJECTED with a typed error
(never silently ignored, never reordered into validity). A duplicate of
an already-accepted identical transition is an idempotent replay (§4.6
semantics — same record, no second canonical blob). ACK ≠ RESOLVED. `dedupe_key` is computed by the controller, never an agent; it
INDEXES the attention identity for reconciliation and projection — it is
never a permission to rewrite the canonical blob. Repeated
reconciliation converges on the one attention identity (occurrence
counts live in projection), creating no second canonical request.

### 8.8 Human lane

```text
HumanCommandRequest (untrusted):
  campaign preconditions:
    campaign_id
    expected_campaign_state_version
    expected_contract_digest
    expected_candidate_state_ref     # where applicable
    attention_id / question_id       # where applicable
  requested action                   # v1: ACK | CANCEL | ANSWER_QUESTION
                                     #     | SELECT_ATTENTION_ACTION
```

**One idempotency surface** (review round f65b21f, P1-4). The envelope's
`message_id` is the ONLY idempotency identity of a human command — the
LOGICAL identity of the accepted command (not "the canonical artifact
identity": content identity is and stays `blob_digest`, §4.3). The
draft's payload-level `command_id` and `idempotency_key` are DELETED:
three near-idempotency identities on the one artifact class that can
trigger CANCEL or supersede is exactly the near-duplicate-authority
failure E9/E10 exist to prevent (which CANCEL is a replay and which is a
new command must have one answer, not three). If a future slice
genuinely needs a distinct `command_id`, introducing it is a contract
change via the supersede path and must define its relation to
`message_id`, its scope, and its conflict semantics.

**Transport session is NOT semantic identity** (review round a5615b8,
P1-12). §4.3 already excludes transport connection/session metadata from
`message_binding_digest` — so `control_session_id` may enter neither the
canonical command payload (it would ride in through `payload_digest`)
nor the authenticated-principal record (it would ride in through the
Human producer binding). Otherwise the legitimate replay — phone sends
CANCEL as `message_id = M` in session S1, connection dies after
acceptance, phone reconnects as the same principal in S2 and replays M —
would classify as `IdempotencyConflict` instead of an idempotent replay.
The split:

```text
AuthenticatedPrincipalV1:            # semantic — referenced by the
  principal_id                       # Human producer binding and the
  credential_epoch                   # accepted HumanDecision
  authn_method

DeliveryObservationV1:               # audit only — recorded metadata,
  control_session_id                 # OUTSIDE the semantic binding and
  ...                                # outside canonical payload bytes
```

`actor_identity`/`authorization_context` do NOT exist as authoritative
request fields. Authentication (§below) happens before any idempotency
mutation and before the conditional consume. The precondition binding
closes the stale-screen TOCTOU per evidence-discipline rule 2: bind →
refresh → conditionally, atomically consume — never check-then-act.

Authentication, v1 (single-principal — not "trust localhost" as an
incantation): one configured maintainer principal; one
installation-scoped control capability of at least 256 random bits; the
secret is stored only in protected form and never appears in artifacts;
`credential_epoch` supports revoke/rotate; the transport is confidential
and authenticated (local socket, TLS, or trusted tunnel); the caller
never chooses its own `principal_id`. Multi-user/RBAC is deferred with
that trigger. The controller derives an `AuthenticatedPrincipalV1`
(above — no session field) and places it, by ref, into the accepted
`HumanDecision`; the delivery observation is recorded separately for
audit.

**HumanDecision source binding** (review round a5615b8, P1-13 —
"references the request by `message_id`" contradicted §11, where
`HumanDecision -> HumanCommandRequest` is a digest edge; `message_id` is
deliberately not a digest). The decision carries a typed source ref, not
a second command identity:

```text
HumanCommandRequestRefV1:
  message_id                 # stays the sole idempotency key
  message_binding_digest
  blob_ref                   # CasObjectRefV1 — proves WHICH canonical
                             # bytes produced this decision
```

The checked constructor proves the blob resolves to the accepted
canonical HumanCommandRequest whose envelope carries exactly that
`message_id` and binding digest.

`ANSWER_QUESTION` carries `declared_scope_effect: none |
revise_contract`; declared-none with a controller-detected scope change,
or ambiguity, → `HUMAN_REQUIRED` re-ask; `revise_contract` enters the
supersede barrier (§12.6). An answer targeting a superseded question is
not delivered. `SELECT_ATTENTION_ACTION` selects a server-provided
`action_id` only. `accept_residual_risk` exists in the vocabulary but is
not offered in v1.

## 9. Model identity

Two different things; no "family normalization" as identity:

```text
LogicalModelRouteV1:
  provider_id
  route_id
  requested_model
  routing_config_digest

ModelResolutionEvidenceV1:
  ProviderReported { provider_model_id }
  FingerprintOnly { backend_fingerprint }
  ProviderReportedWithFingerprint { provider_model_id,
                                    backend_fingerprint }
  Unavailable
```

An alias stays `requested_model`; it never becomes `provider_model_id`
(unrepresentable: there is no variant that lets a requested alias occupy
a resolved-identity position). `family` may be analytics metadata, never
identity. Controller and human artifacts carry no model fields at all —
the tagged producer binding makes their absence structural, not nullable.

## 10. Gate registry

A bare `gate_id` string is insufficient: a name outlives a semantics
change and will reference a different verifier with a straight face.

```text
GateRequirementV1:
  gate_id: GateId
  gate_contract_digest: Digest256

GateRegistryRefV1:
  registry_artifact_ref
  registry_digest
```

A registry entry binds: `gate_id`; gate schema/version; evidence schema
digest; executor kind; verifier policy digest; applicability semantics.
`gate_contract_digest` covers the exact registry snapshot entry. A gate
result binds: `candidate_state_ref`, `gate_id`, `gate_contract_digest`,
`verifier_policy_digest`, observed outcome, evidence refs. Unknown ID,
duplicate ID, wrong contract digest, or wrong policy digest fail closed.
No shell strings anywhere in a registry requirement — a reviewer may
propose new evidence in prose; the controller maps it to a known
requirement or raises `HUMAN_REQUIRED`; it never executes model-authored
text. The same for the coder's `command_recorded`: forensics, never
re-execution.

## 11. Evidence-graph acyclicity — the per-kind reference DAG

Edge definition: `A -> B` iff artifact A directly contains a
digest-reference to B. Canonical references point from the DERIVED object
to its ANTECEDENT evidence; the authority/justification flow reads in the
opposite direction.

The earlier four-level rank rule ("every reference targets a strictly
lower rank") is RETIRED (review round f65b21f, P1-2): legal
controller→controller references (`ReviewRequest ->
CandidateAdmissionReceipt`, `CorrectiveDirective -> ReviewVerdict`)
already violated it, and human-lane and bridge kinds would violate it
further. Rank is not an axiom here; it is a CONSEQUENCE of the real
graph.

### 11.1 The closed universe (review round a5615b8, P1-10)

Open classes ("accepted artifacts", "evidence blobs") are not a matrix —
they are an invitation. The node universe is CLOSED into five classes;
anything outside it cannot be referenced from canonical bytes:

```text
1. Envelope-bearing message kinds (15):
   WorkOrder, CoderReport, CandidateAdmissionReceipt, ReviewRequest,
   ReviewerReport, ReviewVerdict, CorrectiveDirective,
   ProviderInvocationReceipt, InteractionManifest, CampaignFeedItem,
   HumanAttentionRequest, HumanCommandRequest, HumanDecision,
   CampaignRunBinding, ArtifactImported

2. Typed CAS/support object kinds (closed content_kind registry):
   contract-blob, canonical-request-blob, normalized-output-blob,
   model-route-blob, gate-registry-snapshot-blob, gate-evidence-blob,
   diff-evidence-blob, authenticated-principal-record,
   worktree-correspondence-evidence-blob (§7.1a),
   non-dispatch-classification-blob (below),
   message-payload-blob (§4.6 — the exact payload bytes of a canonical
   message, referenced by the construction seed)

3. External wrappers (cross-universe refs into A0/R1 canonical
   records; TERMINAL from the A1 DAG's perspective — their targets are
   verified by the A0/R1 semantic layers, not addressed by CAS digests):
   CandidateStateReceiptRefV1, CandidateMaterializationRefV1,
   RunContractCandidateStateRefV1, WorktreeMaterializationRefV1,
   RunArtifactSourceRefV1 (§6),
   EstablishedNonDispatchEvidenceRefV1 (below)

4. A2-only transition record kinds (frozen here, appended in A2):
   AttentionAcknowledged, AttentionResolved, AttentionSuperseded

5. Terminal opaque blob kinds:
   raw-provider-event-blob, tool-argument-blob, tool-result-blob
```

Extending any class is a contract change via the supersede path.

**`EstablishedNonDispatchEvidenceRefV1`** (review round 4f51457, P1-17 —
`SafeRedrive` carried `established_non_dispatch_evidence_ref` with no
target type in the universe; `prior_execution_id`/`prior_dispatch_id`
are IDs, not evidence, and a very convincing field name is not an
authority):

```text
EstablishedNonDispatchEvidenceRefV1:
  run_id
  classification: absent | valid_unsealed_pre_dispatch
                              # the ONLY two R1 classes that establish
                              # non-dispatch; any other classification
                              # is unconstructible here
  classifier_version
  classification_record_ref: CasObjectRefV1
                              # non-dispatch-classification-blob — the
                              # recorded classifier output
```

The checked constructor requires the blob to resolve, to name the same
`run_id`, and to carry exactly the named classification under the named
classifier version. This answers "which artifact proves established
non-dispatch", not merely gestures at R1.

**Stable edge tags** (P1-17 — `ref_manifest` sorts by `(edge kind tag,
target digest)` and that manifest enters a protocol digest: the tags are
WIRE SEMANTICS). Every edge in §11.3 carries a stable snake_case
`edge_kind` tag equal to the field path that carries the reference; the
tags form a closed compile-time registry in the protocol crate with a
uniqueness test and a known-answer test, exactly the §4.2 discipline.
Renaming a field that carries an edge is therefore a wire change and
follows the supersede path.

### 11.2 Per-kind producer mapping (frozen)

Exactly one `ProducerBindingV1` variant per envelope-bearing kind — §15's
"invalid producer combination is wire-unrepresentable" now has a defined
set to enforce:

```text
Controller: WorkOrder, CandidateAdmissionReceipt, ReviewRequest,
            ReviewVerdict, CorrectiveDirective,
            ProviderInvocationReceipt, InteractionManifest,
            CampaignFeedItem, HumanAttentionRequest, HumanDecision,
            CampaignRunBinding, ArtifactImported
Provider:   CoderReport (role=coder), ReviewerReport (role=reviewer)
Human:      HumanCommandRequest
```

The role in parentheses is NOT a Provider-binding field (P1-20/P1-22):
it is the classifier obligation `Provider.invocation_receipt_ref ->
ProviderInvocationReceipt -> campaign_run_binding.role == the role this
message kind requires`. The proof path goes through the receipt — the
kind table only names what that path must yield.

`ProviderInvocationReceipt` is CONTROLLER-produced: it is the
controller/adapter's evidence ABOUT a provider invocation. Its
provider-side identities (execution id, model route) are payload
content, not a producer binding — which dissolves the self-reference a
Provider-bound receipt would create through its own mandatory
`invocation_receipt_ref`.

### 11.3 Exact edge sets

The edge sets below are EXHAUSTIVE per kind and include the references
carried by producer bindings and cause fields — an edge is an edge no
matter which struct member carries it (review round 4f51457, P1-17: the
previous matrix omitted producer-binding edges, mislabeled the
receipt's run-binding edge `[producer]` right after §11.2 froze the
receipt as Controller-produced, and claimed HumanCommandRequest has
"no digest edges" while its Human binding carries a principal ref).
Additionally, EVERY envelope-bearing kind carries exactly one
`causation` edge (`CausationV1::Artifact`, classed `causal`, target any
already-committed envelope-bearing kind) or `CampaignGenesis` (first
WorkOrder only, no edge) — not repeated per row below. Every edge is
classed `intra` (within one round's derivation flow) or `causal`
(crossing rounds, chains, or attention lineage); `ext:` marks
external-sink wrapper refs (class 3):

```text
kind                        exact direct digest edges
--------------------------  -------------------------------------------
WorkOrder                -> intra: contract-blob,
                            worktree-correspondence-evidence-blob
                            (initial — class-2 CAS, hence intra, P1-24);
                            ext: InputStateBinding wrapper refs
CoderReport (raw)        -> intra: ProviderInvocationReceipt [producer],
                            gate-evidence-blob, diff-evidence-blob
CandidateAdmissionReceipt-> intra: CoderReport, CampaignRunBinding
                            [coder_run_binding_ref];
                            ext: CandidateStateReceiptRef
ReviewRequest            -> intra: CandidateAdmissionReceipt,
                            contract-blob, gate-evidence-blob,
                            diff-evidence-blob
                            (NO separate CoderReport edge — §8.4)
ReviewerReport (raw)     -> intra: ProviderInvocationReceipt [producer],
                            gate-evidence-blob
ReviewVerdict            -> intra: ReviewerReport, gate-evidence-blob;
                            ext: CandidateStateReceiptRef
CorrectiveDirective      -> causal: ReviewVerdict [prior round,
                            strictly lower round_ordinal];
                            ext: InputStateBinding refs
ProviderInvocationReceipt-> intra: canonical-request-blob,
                            normalized-output-blob, InteractionManifest,
                            model-route-blob, CampaignRunBinding
                            [payload — the receipt is
                            Controller-produced, §11.2];
                            causal: ReviewVerdict
                            [ExecutionCause::CorrectiveRound, strictly
                            lower round_ordinal],
                            ProviderInvocationReceipt [prior execution/
                            dispatch of the SAME chain];
                            ext: EstablishedNonDispatchEvidenceRef
                            [SafeRedrive] (+ intra:
                            non-dispatch-classification-blob through it)
InteractionManifest      -> intra: raw-provider-event-blob,
                            tool-argument-blob, tool-result-blob
CampaignFeedItem         -> causal: any envelope-bearing kind already
                            committed (informative projection feed)
HumanAttentionRequest    -> intra: ProviderInvocationReceipt,
                            gate-evidence-blob;
                            causal: CandidateAdmissionReceipt,
                            ReviewVerdict;
                            ext: CandidateStateReceiptRef
HumanCommandRequest (raw)-> intra: authenticated-principal-record
                            [producer]; no payload digest edges
                            (preconditions are identities and
                            digests-as-values, not refs)
HumanDecision            -> intra: HumanCommandRequest
                            [HumanCommandRequestRefV1.blob_ref],
                            authenticated-principal-record,
                            HumanAttentionRequest
AttentionAcknowledged /
AttentionResolved        -> intra: HumanAttentionRequest, HumanDecision
AttentionSuperseded      -> intra: HumanAttentionRequest;
                            causal: HumanAttentionRequest [superseding]
ArtifactImported         -> intra: CasObjectRef;
                            ext: RunArtifactSourceRef (§6)
CampaignRunBinding       -> intra: worktree-correspondence-evidence-blob
                            (initial — class-2 CAS, hence intra, P1-24);
                            ext: InputStateBinding wrapper refs
                            (identities otherwise)
class 2 / class 5 blobs  -> (terminal, no outgoing edges)
```

### 11.4 Acyclicity — what is actually proven

A kind-level topological sort of the FULL matrix is impossible and is
not claimed: the legal causal edges alone create kind-level cycles
(`ProviderInvocationReceipt -> ReviewVerdict -> ReviewerReport ->
ProviderInvocationReceipt` across rounds). The frozen claim is split
honestly:

1. **The `intra` subgraph is a kind-level DAG** — machine-checked by a
   topological sort at the types stage; failure to sort is a build
   failure. Derived rank, where anyone wants one, is generated from this
   sort — never hand-assigned.
2. **Every `causal` edge is instance-acyclic by construction**: a
   canonical artifact may only reference an artifact that is ALREADY
   durably committed (§4.6 ordering; content addressing makes a forward
   reference unconstructible), and where a round ordinal applies
   (`CorrectiveRound`, CorrectiveDirective) the checked constructor
   requires the target's `round_ordinal` to be STRICTLY lower — a
   comparison that historical replay re-proves from the CANONICAL
   inline `round_id + round_ordinal` pair both sides carry (§2.3,
   P1-16), never from a live controller context. SafeRedrive targets
   must be prior executions/dispatches of the SAME chain (§2.3), with
   non-dispatch established by `EstablishedNonDispatchEvidenceRefV1`
   (§11.1).
3. Any digest reference not present in §11.3 is a wire-type/constructor
   rejection.

Back-links live only in indexes/projections, never in canonical bytes. A
dedicated `ArtifactAcceptance` event is NOT introduced; its trigger
stays: a real consumer of acceptance-as-an-event, or multiple acceptance
outcomes for one source artifact. Extending the matrix (a new kind, a
new edge, or a re-classing of an edge) is a contract change via the
supersede path; the acyclicity checks re-prove the extended matrix.

## 12. State machines, barriers, and classifiers

Frozen machines/tables — exactly seven:

1. campaign phase FSM (§12.1);
2. round FSM (§2.3 outcomes + guards);
3. provider execution FSM (§12.3);
4. provider dispatch FSM (§12.3);
5. human-attention lifecycle (§8.7);
6. cancellation/supersede control barrier (§12.5–12.6);
7. budget/ambiguity policy table (§12.4).

The three acceptances (CoderReport, ReviewerReport, HumanCommand) are NOT
state machines. They are pure authority-specific classifiers (§12.8).

### 12.1 Campaign phase FSM

Phases and stop states are the accepted `docs/autonomy-controller.md`
set: `PLANNED → BUILDING → GATING → CI_WAIT → REVIEWING → CORRECTING →
GATING → … → READY_TO_MERGE → MERGED`, with `HUMAN_REQUIRED`,
`BUDGET_EXHAUSTED`, `CANCELLED`, `FAILED`, plus `SUPERSEDED` (E6). Guards
are the transition-authority table of that document; every guard is
established from durable evidence; a later event cannot retroactively
validate an unsafe transition; candidate drift after review invalidates
the verdict and returns the campaign to verification. `READY_TO_MERGE`
does not authorize a merge; v1 merge is manual (§16).

### 12.2 v1 concurrency

At most one active campaign per task; within a campaign, one round
in-flight; within a round, the coder lane mirrors R1 single-in-flight
command discipline through the campaign run binding.

### 12.3 Provider execution / dispatch FSMs

Execution: `allocated → dispatched* → terminal(completed | refused |
incomplete | failed_pre_dispatch) | dispatch_ambiguous`, where
`dispatched*` is the dispatch sub-machine (ordered dispatches, each
`Initial | ToolContinuation | SafeRedrive`). The R1 dispatch-boundary
protocol governs each dispatch: safe redrive only on established
non-dispatch; once dispatch occurred or may have occurred, an unknown
outcome is `dispatch_ambiguous` and fails closed. Replay, recovery,
reconciliation, and historical verification never invoke the provider.
A read-only role is still a side effect (repeating a reviewer can rewrite
campaign history as effectively as repeating a coder).

### 12.4 Budget/ambiguity policy table

- `dispatch_ambiguous` → campaign `HUMAN_REQUIRED`, always. Sole
  exception: a human CANCEL is already accepted — the ambiguity is
  preserved as evidence, the output is never accepted, and the campaign
  completes cancellation under the quiescence rules.
- Budget is checked BEFORE the side effect it bounds. On exhaustion
  mid-round: new progress-producing side effects are forbidden
  immediately; already-observed provider results are preserved as
  evidence; sealing, revocation, reconciliation, and forensic capture
  remain permitted as safety operations; no new coder/reviewer dispatch.
  If cost is only known post-response and shows overshoot: the receipt
  is accepted as evidence and the next progress transition is forbidden.
- **Exhaustion outcome predicate** (review round a5615b8, P1-15 — an
  `or` without a predicate in a deterministic policy is a human
  hiding in the table). Frozen rule: if at the moment exhaustion is
  established every execution in the campaign is terminal, no receipt is
  `dispatch_ambiguous`, and no OPEN attention/question awaits a
  decision, the campaign terminates `BUDGET_EXHAUSTED`; otherwise —
  any non-terminal execution, any unresolved ambiguity, any open
  decision — it goes `HUMAN_REQUIRED` (carrying the exhaustion evidence
  in the attention request). No third case exists.
- Exhaustion is a typed terminal/escalation outcome — never an invitation
  for the model to raise its own limits.

### 12.5 CANCEL barrier

Accepted from ANY non-terminal campaign state; admission is immediate:

```text
any non-terminal
-> CancelRequested
-> dispatch barrier (no new dispatches)
-> revoke mutating capabilities
-> quiesce / classify active executions
   (terminal or ambiguous receipts recorded for started side effects)
-> preserve forensic state
-> CampaignCancelled
```

`CANCELLED` is displayed only after all barrier steps hold — a UI showing
"cancelled" while the coder still writes files is an interface to the
wrong reality.

### 12.6 Supersede barrier (REVISE_CONTRACT)

```text
REVISE_CONTRACT accepted
-> SupersedeRequested
-> block new dispatches
-> quiesce / classify active executions
-> CampaignSuperseded
-> atomically mint replacement campaign
```

The replacement: new `campaign_id`; new `contract_digest` and contract
version; explicit `supersedes_campaign_id`; same `root_goal_id`, usually
the same `task_id`; rounds restart a new sequence; the old campaign never
reopens. The replacement is NOT minted until the old campaign's mutating
capabilities are revoked and its side effects classified — otherwise two
campaigns independently continue "exactly one" task.

### 12.7 Recovery

On restart: replay the durable record; validate referenced candidate and
artifact identities; reconcile external projections (A3 machinery later;
in A1 scope only re-validation of local state); resume only transitions
whose safety re-establishes; otherwise fail closed or `HUMAN_REQUIRED`.
External mutable systems are observations, never canonical history.

### 12.8 Acceptance classifiers

```text
raw input + canonical context -> AcceptedArtifact | Rejected(reason)
```

Pure functions per authority (the evidence-discipline constraint:
raw → authority-specific classifier → typed fact → deterministic
policy). A JSON validation function is not a workflow engine. Classifier
preconditions (also enforced by construction API, §15.2):

- CoderReport → CandidateAdmissionReceiptV1: schema/version; lineage
  verified (§2.4); report payload digest equals the invocation receipt's
  `normalized_output_ref` digest (§8.6 provenance chain); A0 receipt
  verified through the accepted A0 semantic layer; claim check (§8.2);
  controller-observed change set derived.
- ReviewerReport → ReviewVerdict: §8.4 conditions, plus the same
  §8.6 normalized-output provenance check.
- HumanCommandRequest → HumanDecision: authenticated principal
  (§8.8, session excluded from semantic identity); typed source ref
  (`HumanCommandRequestRefV1`) resolved to the exact accepted canonical
  bytes; precondition bindings fresh; conditional atomic consume.

## 13. Authority rules (normative, internalized)

The six boundary rules (types + negative tests, not prose): a model
verdict is not a controller decision; a GitHub comment is not inter-agent
authority (PR comments are projections; agents never consume them as
input); a successful provider response is not an accepted report;
matching incoming claims do not prove lineage; a timestamp does not
define order; a retry does not heal an ambiguous side effect.

The nine provider-boundary rules of issue #95 §2 apply to every A1 role,
unchanged, without strengthening delivery semantics into exactly-once.

Internalized from #94 (as this contract's OWN norms; #94 is not an
authority): controller-derived dedupe semantics (§8.7); `EXTERNAL_DRIFT`
as an attention reason code (detection deferred to A3); autonomous code
mutation classifies at least STRICT; risk classification is
controller-derived and deterministic; classification ambiguity fails
toward the stricter profile; A1 artifact/message schema versioning
(envelope + kind versions, §3). NOT imported prematurely: reducer
versioning, campaign replay semantics, the full admission-profile
taxonomy, the level-triggered reconciler — A2/A3.

Determinism: A1 correctness depends on D0 (deterministic admission) and
D2 (deterministic historical replay) only; D1/D3/D4 are not prerequisites
and no safety or admission invariant may depend on them.

## 14. A1/A2 storage boundary

A1 owns NOW (protocol/library layer):

```text
schemas; canonical writer; typed refs; classifiers/validators;
content-addressed blob store interface; acceptance preconditions;
RED matrix; test-only append sink
```

A2 owns production authority:

```text
canonical campaign event append; atomic artifact-acceptance recording;
campaign reducer; replay/resume semantics
```

No shadow campaign authority in `o7-ledger`: acceptance tables from which
campaign state would de facto be derived are a truncated A2 in storage
clothing. The ledger later indexes A2 events and artifact refs; it never
becomes a temporary reducer to be diplomatically re-labeled "projection".
Until A2, A1 is fully implementable as a protocol/library layer and must
not claim a working durable autonomous campaign runtime.

## 15. Unrepresentability and RED matrix

### 15.1 Unrepresentable at the wire type

Unknown enum variant; invalid digest/ID form; a provider artifact without
a provider binding; a controller artifact with one; an alias in a
resolved-identity position; outcome fields inconsistent with the outcome
variant; a raw coder report carrying controller-accepted status; a
filesystem path/URL where a typed artifact ref belongs; an invalid
producer-binding combination.

### 15.2 Unrepresentable through the construction API

An accepted artifact without its classifier; a ReviewVerdict without a
resolved ReviewerReport; a HumanDecision without an authenticated
principal;
a CandidateAdmissionReceipt without a verified A0 receipt; a provider
receipt before a terminal/ambiguous outcome. Canonical types have closed
fields; construction happens only through checked constructors.

### 15.3 RED tests (observable effect, never a stub)

Stale candidate/contract/state preconditions; lineage mismatch; duplicate
ID with a different binding digest; unknown registry IDs; directive scope
escalation; resolver escape (path/symlink/oversize); reviewer holding
mutation credentials; coder claim vs derived candidate mismatch; answer
to a superseded question; unauthorized/stale human command; retry without
established non-dispatch; a replay path attempting a provider call; a
digest cycle / an edge outside the frozen §11.3 matrix / an `intra`
matrix that fails its topological sort; the disputed budget/cancellation
transitions of §12.4–12.6 including the §12.4 exhaustion predicate; the
§4.6 crash/race oracles (two parallel deliveries → one byte-identical
blob; crash between RESERVE and COMMIT repaired to one blob with the
original `accepted_at`; duplicate during RESERVED gets IN_PROGRESS,
never fabricated bytes); an input-state binding whose refs do not
cross-verify (§7.1a); a run binding whose `input_state_binding` differs
from the dispatching artifact's input (§2.1/§8.4); a second execution
chain per role without SafeRedrive evidence (§2.3); a report whose
payload digest does not match the receipt's `normalized_output_ref`
(§8.6); an attempted in-place lifecycle mutation of a canonical artifact
(§8.7); a transition targeting an already-terminal attention (§8.7); a
same-principal different-session replay classifying as conflict instead
of replay (§8.8, P1-12); a `ref_manifest` that differs from the
mechanical collection of the artifact's real refs (§3); an ordinal
comparison that cannot be re-proven from canonical bytes alone during
replay (§2.3, P1-16); a report producer binding spliced onto a receipt
whose execution/binding/model differ while normalized bytes coincide —
must be unconstructible now that the binding has one field (§3, P1-20);
an `ArtifactImportedV1` whose CAS digest does not equal the resolved
source bytes (§6, P1-18); an `InitialMaterialization` with two valid
same-run refs in the right order but a non-corresponding worktree —
constructor reject (§7.1a, P1-21); a `RESERVED` reservation whose seed
is missing or whose rebuild under the seed's `writer_version` does not
reproduce the committed bytes (§4.6, P1-19); a seed whose RECOMPUTED
`message_binding_digest` does not equal the reservation's — recovery
must refuse to serialize Y under a reservation made for X (§4.6,
P1-23); a recovery encountering an unsupported `seed.writer_version` —
fail closed, no fallback writer (§4.6, P1-23); a causation ref whose
resolved envelope `message_kind`/`message_id` differ from the carried
claims (§3, P1-24). A newtype around `String` is not a semantic proof.

## 16. v1-lite cut and delivery honesty

Cut (the four autonomy properties survive; property 4 at the v1-lite
tier): one campaign in flight; §8.8's v1 command set only; no push —
in-app feed + SSE/replay, tier named honestly (`v1-lite`: timely only
while the client is open; `operational v1`: background-capable — a
product requirement if phone intervention becomes the acceptance
criterion); merge manual, outside the system, triggered by the
ready-to-merge attention request; reviewer same-provider allowed with
mechanical independence (§8.4); reconciliation polling only.

Not cuttable (object identity, not ceremony): exact-candidate binding
everywhere, including human decisions; controller-derived admission
receipt behind the A0 sealing boundary; raw/accepted split; no
model-supplied executable authority; provider invocation evidence and
no-recall-on-replay; fail-closed directive validation;
attention/decision as canonical records; forward-only corrective rounds.

## 17. Non-goals

No architect/planner in the core loop; no multi-agent negotiation or
model-to-model channel; no automatic merge default; no A5 goal-graph
runtime (`root_goal_id` is identity only); no A2 incarnation taxonomy or
campaign reducer; no capability subsystem beyond registry-bound
references; no constrained decoding; no `AUTHORIZE_MERGE` (post-v1; its
shape is already prescribed by evidence-discipline rule 2:
`merge(sha = external_head_sha of the accepted candidate)` as the
conditional atomic mutation — an A3+ concern); no cross-family reviewer (existing
backlog); no webhooks (A3); no production campaign runtime before A2.

## 18. Freeze procedure and review checklist

Order (ratified): this contract text → dedicated review → types +
construction API → RED matrix → FREEZE (issue #95 leaves DRAFT; the
supersede path becomes the only way to change) → A1 protocol/library
implementation. Production acceptance authority arrives only with A2's
canonical campaign log.

Review checklist for step 2 (each item is a grep-able discipline):

1. no re-declared A0/R1 identity fields anywhere in A1 schemas;
2. no unqualified `head` — only `candidate_tree_*` or (A3-reserved)
   `external_head_*`;
3. no generic `ArtifactRef` in A1 types — `o7_run::ArtifactRef` only
   inside the class-3 external wrappers (A0 wrappers and
   `RunArtifactSourceRefV1`), `CasObjectRefV1` elsewhere;
4. no generic `retry_of` — only `ExecutionCauseV1`/`DispatchCauseV1`;
5. no authoritative caller-supplied actor, model, or lineage fields;
6. every pair of refs called a "binding" has an explicit cross-object
   verification rule, not merely two independently valid refs;
7. every canonical digest-reference is permitted by the frozen per-kind
   DAG (§11) and the DAG is machine-checked acyclic;
8. no canonical content-addressed artifact has an in-place lifecycle
   transition — lifecycle changes are new records/events or projections.
