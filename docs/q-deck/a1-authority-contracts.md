# Q-Deck A1: coder/reviewer/human authority contracts

## Status

**PROPOSED FREEZE / REVIEW REQUIRED / NON-AUTHORITATIVE.**

This document is the *proposed* normative source for the A1 slice named in
`docs/autonomy-controller.md` ("A1 ReviewVerdict and CorrectiveDirective
contracts"). It is a freeze **candidate**: nothing here is authoritative, and no
A1 implementation may bind to it, until it is reviewed and merged. On
acceptance this header becomes `ACCEPTED / CLOSED / FROZEN` and the supersede
path (§7) begins to apply — not before. A draft that invokes supersede ceremony
against itself is an efficient way to forbid fixing its own first mistakes.

Design input: issue #95 (`A1: coder/reviewer/human contracts (draft)`), which
carries the rationale and discussion history. Where this document and that draft
differ, this document is the proposal.

Revision R1 (corrective round) is recorded in §9.

What the freeze will cover, once accepted: the wire schema of every message
kind — field names, types, required/optional status, null policy, bounds, and
the authority that establishes each value — plus the digest, identity, evidence
closure, and transition rules. Opaque id string forms and additive reason codes
remain extensible under FD-1.6.

## Purpose

007 already produces, separately and correctly: exact candidate state (A0),
sealed worktrees, provider invocation with recorded evidence
(`docs/o7-invoke.md`), digest-chained run records with independent replay
(`o7 replay`, `src/main.rs:388` → `events::replay_record`), exact-head CI, and
an admission discipline (`docs/decision-and-admission-protocol.md`).

What does not yet exist is the object that turns those into a *permitted state
transition*. A1 introduces exactly one thing:

```text
a model emitted something
  != the system holds it as a fact
  != the system is permitted to proceed
```

Every rule below exists to keep those three apart while still letting one
autonomous corrective loop run unattended.

## 0. What this contract binds to and may not redefine

| Bound contract | Authoritative artifact | What A1 consumes, never redefines |
|---|---|---|
| A0 candidate state | `docs/q-deck/a0-candidate-state.md` (accepted head `52627c3`, merged `f1ac458`) | `CandidateRef` representation, base-commit semantics, the one-cumulative-patch model, `RepositoryIdentity`, materialization attestation, sealing/materialization ordering |
| R1 command vertical | `docs/q-deck/r1-command.md` §11.1–§11.2 | the durable dispatch boundary, `ValidUnsealedPreDispatch` vs `ValidUnsealedDispatchAmbiguous`, the four `DispatchProgress` values, fail-closed post-dispatch ambiguity |
| Controller architecture | `docs/autonomy-controller.md` (accepted `c5b3ae0b`, PR #93) | campaign phases, the transition-authority principle, the `ReviewVerdict` minimum |
| Admission protocol | `docs/decision-and-admission-protocol.md` §5, §6, §7 | diagnostic vs admission evidence, exact-head review, head drift invalidates acceptance |
| Canonical digest discipline | `crates/o7-run/src/event.rs` (module docs, `RunEvent::compute_digest`, `frame`) | digests by explicit length-prefixed field framing, never by hashing a serialized JSON blob |
| Provider call primitive | `docs/o7-invoke.md` | engine set, capability profile `read-only-data`, the `PASS`/`FAIL_*`/`BLOCKED_*` status vocabulary, the `stdout.raw` / `result.json` artifact split, key handling |

A1 adds no new claim about any of these. In particular it does **not** strengthen
R1's delivery semantics into an exactly-once claim, and it does not weaken the A0
sealing boundary into "the process exited".

## 1. Roles, topology, and the authority boundary

Three roles, one controller, no direct model-to-model channel:

```text
Controller -> Coder:      WorkOrder
Coder      -> Controller: CoderReport          (untrusted)
Controller:               CandidateReceipt     (derived behind the A0 seal)
Controller -> Reviewer:   ReviewRequest
Reviewer   -> Controller: ReviewerReport       (untrusted)
Controller:               ReviewVerdict        (validated, accepted)
Controller -> Coder:      CorrectiveDirective
Controller -> Human:      CampaignFeedItem | HumanAttentionRequest
Human      -> Controller: HumanCommandRequest  (untrusted)
Controller:               HumanDecision        (recorded, accepted)
```

GitHub PR comments are a human-readable **projection** of these objects. No agent
consumes a mutable PR comment as authoritative input; a projection is never an
input channel.

### 1.1 Untrusted input vs controller-accepted artifact

```text
untrusted input          controller-accepted artifact
---------------          ----------------------------
CoderReport           -> CandidateReceipt
ReviewerReport        -> ReviewVerdict
HumanCommandRequest   -> HumanDecision
```

Free-form model prose is an attachment on an artifact, never the artifact.

### 1.2 Delivery semantics (R1-faithful)

```text
artifact creation and acceptance   durable and idempotent
artifact delivery and observation  may occur more than once

duplicate message_id + same envelope digest  -> idempotent replay (FD-6)
duplicate message_id + other envelope digest -> conflict, fail closed (FD-6)
```

At-most-once applies to **provider dispatch** in the supported single-host model,
not to message delivery. A1 makes no exactly-once claim about anything.

## 2. The frozen decisions

### FD-1 — artifact model, digests, encoding, bounds, unknown fields, versions

**FD-1.1 The artifact model.** An A1 artifact is a pair:

- a **payload** — the typed body of one `message_kind`, stored as its own
  immutable byte string in 007-owned content-addressed storage;
- an **envelope** — the record that identifies, binds, and references it.

`payload_digest` is SHA-256 over *those exact stored payload bytes*, lowercase
hex, 64 chars — the existing `o7_run::event::Digest256::of_bytes` form. The
envelope's own identity is its framed digest (FD-1.2), which commits to
`payload_digest`. No reader ever re-serializes anything to verify an identity.

**For a provider-produced artifact, the payload bytes ARE the adapter-normalized
provider output bytes.** The controller attaches an envelope; it never edits,
re-orders, re-encodes, or enriches the body. This is what makes the congruence
check of FD-13 meaningful, and it is also what stops the controller from quietly
improving a model's claims on the way in.

**FD-1.2 Envelope digests are computed by field framing, not by JSON.** Following
the precedent frozen in `crates/o7-run/src/event.rs` (module docs: "Digests are
computed by explicit field FRAMING (length-prefixed), not by hashing a serialized
JSON blob, so they are byte-stable regardless of map ordering or serializer
whitespace"):

```text
h = SHA-256
h.update(b"o7-a1-envelope\0v1\0")
frame(envelope_version.to_le_bytes())
frame(message_kind tag byte)
frame(message_kind_version.to_le_bytes())
frame(message_id)
frame(root_goal_id) frame(task_id) frame(campaign_id) frame(round_id)
frame(causation_id) frame(correlation_id)
frame(producer_role tag byte)
frame(producer_execution_id) frame(producer_adapter_version)
frame(model_identity)
frame(prompt_digest) frame(tool_policy_digest)
frame(contract_digest) frame(expected_input_head)
frame(payload_digest)
frame(provider_execution_receipt_ref digest, or the empty string when absent)
frame(artifact_refs count.to_le_bytes()), then for each ref in stored order:
    frame(kind tag byte) frame(media_type) frame(digest) frame(size.to_le_bytes())
```

`frame(x)` is `u64-le length prefix || bytes`, identical to
`crates/o7-run/src/event.rs:906`. Absent optional fields are framed as the empty
string; they are never skipped, so "absent" and "empty" hash distinctly from
"different value", and the field order is fixed forever.

`created_at` is deliberately **excluded** from the framing (FD-5.4).

**No canonical-JSON scheme is introduced.** Payload identity is the digest of
stored bytes; envelope identity is field framing. Neither requires two
serializers to agree on whitespace or key order, so the whole class of
"semantically identical, digest-unequal" failures disappears without anyone
inventing a normalization standard.

The corollary is equally deliberate and has teeth elsewhere in this document: a
payload re-serialized with different whitespace is a *different* payload. Bytes
are the artifact. Nothing in A1 may require two independently serialized payloads
to be "byte-identical" in some field — equality between payloads is expressed by
sharing one immutable referenced document and comparing its digest (see
`ScopeContractV1`, §3.13).

**FD-1.3 Encoding and null policy.** Stored payload bytes are UTF-8 JSON with a
top-level object. Invalid UTF-8, a leading BOM, and non-object top-level values
are rejected at ingest. **No Unicode normalization is performed** — content is
never rewritten, and a digest is never computed over a normalized copy of
something stored differently.

One uniform null policy, everywhere in A1: **an optional field is either absent
or carries a value. Explicit JSON `null` is rejected at parse time.** There is no
field in this contract where `null` and absent mean different things, because
that distinction has never once been worth what it costs.

**FD-1.4 Per-object bounds (protocol hard maxima).**

```text
control artifact payload (any typed A1 payload)   <=    1 MiB
single evidence blob (diff, raw provider bytes,
  gate log, patch, manifest)                      <=   64 MiB
JSON nesting depth                                <=     32
array length, any array                           <=   4096
string length, any single string field            <=  65536 bytes
opaque id length                                  <=    256 bytes
artifact_refs per envelope                        <=    256
interaction_sequence entries per manifest         <=   4096
dispatches per execution receipt                  <=    256
findings per ReviewerReport / ReviewVerdict       <=    256
```

Exceeding any bound is a parse-time rejection, never a truncation. A truncated
artifact that still parses is the failure mode these bounds exist to forbid.

**FD-1.5 Aggregate bounds (evidence closure).** Per-object bounds do not bound
the cost of *resolving* an artifact. 256 refs × 64 MiB is 16 GiB of direct
references alone, and the rank DAG of FD-2 permits a branching reachable closure
beneath that. The graph being acyclic does not make it small; an acyclic graph
kills a process just as efficiently as a cyclic one, with better manners.

Two layers, frozen:

```text
protocol hard maxima (compile-time constants, never configurable)
  max_direct_referenced_bytes        128 MiB   # sum of artifact_refs[].size
  max_reachable_closure_bytes        256 MiB   # sum over the deduplicated closure
  max_reachable_closure_objects     2048
  max_refs_per_execution            4096       # all refs minted by one execution
  max_evidence_bytes_per_campaign     8 GiB

campaign policy (selected per campaign, MUST be <= the hard maximum)
  evidence_budget_bytes               64 MiB   # default for a V0 campaign
  closure_object_budget              512
```

Two invariants on those numbers, checked at campaign creation and in a unit test,
because a bound that contradicts another bound is decoration:

```text
max_direct_referenced_bytes <= max_reachable_closure_bytes
      (direct refs are a subset of the closure)
campaign policy value       <= the corresponding hard maximum
```

The **effective** bound for a resolution is `min(hard maximum, campaign policy)`.
The campaign's selected values are exactly what `budget_policy_digest` commits to
(§3.1), which is how that field stops being decorative.

Closure traversal is frozen as an algorithm, not as an intention:

```text
resolve_closure(root_envelope):
  seen := {}                       # typed object identity: (ref.kind, ref.digest)
  bytes := 0 ; objects := 0
  queue := root_envelope.artifact_refs (+ provider_execution_receipt_ref)
  if sum(declared sizes of that queue) > effective(max_direct_referenced_bytes):
      REJECT before reading anything
  while queue not empty:
    ref := pop(queue)
    if ref in seen: continue       # deduplicate BEFORE accounting
    seen += ref
    objects += 1
    bytes += ref.size              # the DECLARED size, accounted before any read
    if bytes   > effective(max_reachable_closure_bytes):   REJECT whole resolution
    if objects > effective(max_reachable_closure_objects): REJECT whole resolution
    verify stored size == ref.size and digest(stored bytes) == ref.digest
    if the reference slot expects a typed object (FD-2.4): parse and enqueue its refs
    else: do not parse, do not enqueue      # rank-0 bytes stay bytes
```

Accounting uses the *declared* `size` before reading, so an oversized blob is
refused rather than streamed. A stored object whose real size disagrees with its
declared `size` is an integrity failure. Resolution is all-or-nothing: a closure
that exceeds a bound is never partially accepted, and the artifact that
referenced it is rejected — it does not degrade into "accepted with missing
evidence".

**FD-1.6 Unknown fields and versions.** Every A1 type deserializes with
`#[serde(deny_unknown_fields)]`, and every enum is closed — no `#[serde(other)]`
catch-all. Both follow the A0 precedent: `CandidateStateReceiptV1`
(`crates/o7-run/src/candidate.rs:78`) and `CandidatePatchKind`
(`crates/o7-run/src/event.rs:363–371`, whose doc comment states the rule
outright: "an unrecognized/future value on the wire fails closed at
deserialization … there is deliberately no `#[serde(other)]` catch-all").

Three independent version fields:

- `envelope_version: u32` — envelope framing and field set. Frozen v1 = `1`.
- `message_kind_version: u32` — the payload schema of one kind. Every kind
  frozen here is at version `1`.
- `campaign_protocol_version: u32` — the reducer semantics of FD-14. v1 = `1`.

An unrecognized value of any of them is refused; the artifact is never parsed "as
well as we can". This mirrors `RUN_EVENT_SCHEMA_VERSION`'s rule in
`crates/o7-run/src/event.rs:25`: "A reader that encounters a different version
must refuse to replay it as if it understood it."

A campaign is bound to one version set at creation. A mid-campaign version change
is a supersede (§7), never an in-place upgrade.

**FD-1.7 Media types.** `application/vnd.o7.a1.<kind>+json; v=<version>` for
typed artifacts; evidence blobs carry their own concrete type
(`application/json`, `text/x-diff`, `application/octet-stream`). Media type is
part of every `ArtifactRef` and part of the envelope framing: the same bytes
under a different declared type are a different reference (FD-2.5).

**FD-1.8 Artifact refs.** `ArtifactRef = (kind, media_type, digest, size)` into
007-owned CAS only. There is no path field and no URL field to populate. An
agent-supplied path or URL appearing anywhere in a payload is inert text and is
never dereferenced (FD-7).

### FD-2 — the evidence graph is acyclic by construction

**FD-2.1 Rank rule.** Authority flows one way, and content-addressed references
must flow the same way. Instead of a runtime cycle check:

| Rank | Class | May reference |
|---|---|---|
| 0 | opaque evidence bytes and imported authority roots (FD-2.4): canonical provider request, raw provider bytes, adapter-normalized output, usage/cost records, diffs, patches, gate logs, contract documents, `ScopeContractV1`, A0 `CandidateRef`, materialization attestation | nothing (not parsed by A1) |
| 1 | `InteractionManifestV1` | rank 0 |
| 2 | `ProviderExecutionReceiptV1` | rank ≤ 1 |
| 3 | untrusted reports: `CoderReport`, `ReviewerReport`, `HumanCommandRequest` | rank ≤ 2 |
| 4 | controller-accepted artifacts: `CandidateReceipt`, `ReviewVerdict`, `HumanDecision` | rank ≤ 3 |
| 5 | controller-issued instructions and notices: `WorkOrder`, `ReviewRequest`, `CorrectiveDirective`, `HumanAttentionRequest`, `CampaignFeedItem` | rank ≤ 4 |

**A content-addressed reference may only target a strictly lower rank.** Rank
monotonicity implies acyclicity; no cycle detector is needed, and the property is
checkable on one artifact in isolation.

**FD-2.2 Lineage and causation are identifiers, not digests.** `causation_id`,
`correlation_id`, `campaign_id`, `round_id`, `question_id`, `attention_id`,
`finding_id`, `review_id`, `command_id` are opaque ids and are therefore *not
edges in this graph*. That is what lets a `CoderReport` (rank 3) be caused by a
`WorkOrder` (rank 5) without inverting the reference direction.

**FD-2.3 No acceptance pointer inside an immutable evidence object.** A
`ProviderExecutionReceiptV1` never carries a reference to an artifact accepted
from it; the forward directions are `CandidateReceipt.coder_report_ref` and
`ReviewVerdict.reviewer_report_ref`. Back-links needed by projections, indexes,
or a UI live in projections. A projection may compute any inverse it likes; it
never becomes canonical.

**On `ArtifactAcceptance`:** it does **not** exist in V1, and this table is not
built to absorb it. A dedicated acceptance record would sit between rank 3 and
rank 4 and would require re-ranking those classes. Adding it therefore requires a
supersede of FD-2, a new `envelope_version`, and new `message_kind_version`s —
not an additive patch. Reserving a rank now for an object whose necessity is
unestablished would be the same mistake in a more expensive font.

**FD-2.4 Imported authority roots.** Some content-addressed references leave the
A1 type system entirely:

```text
ImportedAuthorityRoot :=
    A0 CandidateRef
  | A0 / o7-worktree MaterializationAttestation
  | accepted Contract document
  | controller gate/verifier registry definition
  | ScopeContractV1
```

Frozen rules:

- Imported roots occupy **virtual rank 0**. A1 never parses them into A1
  semantics and never re-validates them under A1 rules; each is validated
  against its own frozen schema and digest contract by the crate that owns it
  (A0 objects by `o7-run`/`o7-worktree`, registry definitions by the controller's
  registry).
- The **authority to import** comes from the controller-owned registry or the
  campaign binding — never from an incoming `ArtifactRef`. An agent cannot
  introduce a new imported root by naming one.
- An imported root that fails its own owner's validation fails the A1 artifact
  that referenced it, closed.

**FD-2.5 The resolver's duties.** `ArtifactRef.kind` is a claim by whoever wrote
the reference. Frozen:

- Every reference slot has one expected `kind` and expected media type, fixed by
  the schemas in §3. The resolver checks the stored object against the *slot's*
  expectation, not against the sender's declaration; a mismatch fails closed.
- Rank-0 bytes are never parsed and never promoted to an authority object because
  they happen to look like one. Only a slot that expects a typed object causes a
  parse (FD-1.5).
- The same bytes referenced through two different typed slots are **two distinct
  nodes** in the closure. Deduplication is by `(kind, digest)`, never by digest
  alone.

### FD-3 — raw provider evidence is separate from adapter-normalized output

Per **dispatch**, three distinct artifacts with three distinct digests:

```text
canonical_request_ref     the exact provider-facing request after adapter
                          construction — not the logical WorkOrder
raw_provider_event_ref    provider bytes/events as received, when capturable
normalized_output_ref     adapter-normalized payload, PRE-envelope bytes
```

In V0 these map onto artifacts `o7 invoke` already writes
(`docs/o7-invoke.md`, "Artifacts and `meta.json` mapping"): `stdout.raw` is "the
raw HTTP response body, byte-for-byte … the unmodified provider evidence";
`result.json` is "the extracted (normalized) content value".

**A 2xx response is not an accepted artifact.** The ladder is three steps, and
every step is a separate recorded act with its own rank:

```text
normalized provider output (o7 invoke result.json)        rank 0
controller ingest + A1 envelope  -> ReviewerReport        rank 3
controller validation            -> ReviewVerdict         rank 4
```

`o7 invoke`'s `PASS` means only: HTTP 2xx, content parsed, schema-valid. It is
the precondition of step one, not a shortcut to step three.

Where the provider or adapter cannot establish a fact, it is recorded as
unavailable under an explicit status. A requested model alias is never recorded
as a resolved backend identity.

### FD-4 — untrusted report vs controller-accepted artifact

Acceptance is an act of the controller, recorded as a canonical event, with the
accepted artifact referencing the raw report forward (FD-2.3). The report's own
`status`/`verdict` field is an input to validation, never its outcome:

- A `CoderReport` may say `candidate_produced`. That claim is checked against the
  controller-derived candidate (FD-5); a mismatch fails closed.
- A `ReviewerReport` may say `accepted`. Its JSON authorizes nothing. Only a
  controller-issued `ReviewVerdict` enables a transition (FD-12).
- A `HumanCommandRequest` may claim anything, including its own actor identity.
  Only a `HumanDecision` is authority (FD-15).

### FD-5 — authority direction: lineage, head, contract, time

**FD-5.1 Lineage.** The controller never adopts lineage from an incoming
envelope. It resolves the expected `(root_goal_id, task_id, campaign_id,
round_id)` from the causation target and the canonical campaign binding, then
verifies the carried fields match. Mismatch fails closed. Comparing one incoming
envelope against another incoming envelope is not verification.

The controller mints and durably binds `root_goal_id`, `task_id`, and
`campaign_id` atomically **before any agent dispatch** — the natural extension of
R1's "durable acceptance before provider invocation". A campaign without a
complete lineage binding cannot exist, so no later backfill or heuristic linking
is ever required.

v1 active topology: exactly one root goal, one task under it, at most one active
campaign executing that task. A replacement execution mints a new `campaign_id`
and records `supersedes` against the prior terminal campaign; it never mutates or
reuses it.

**FD-5.2 Head.** Every artifact that reasons about code carries
`expected_input_head`, verified against `CampaignStateV1.current_candidate_head`
at the moment of use. A `ReviewerReport` whose `reviewed_head` is not the current
candidate head is stale: retained as evidence, never promoted to a
`ReviewVerdict`.

**FD-5.3 Contract.** `contract_digest` lives in the envelope and is verified
against the campaign binding. Payloads never restate an envelope-owned field —
two copies of one fact in one artifact is a divergence waiting for a maintainer.

**FD-5.4 Time.** `created_at` is the producer's observation. It is metadata: it
is excluded from the envelope framing (FD-1.2), it is never an ordering
primitive, and it never participates in identity — the rule already frozen for
run events (`crates/o7-run/src/event.rs:622`: "Metadata only — NEVER the ordering
key"). Order is the canonical append sequence plus the causation graph.

Because it is excluded from identity, one envelope digest can arrive attached to
several serializations differing only in `created_at`. Frozen resolution:

```text
the FIRST accepted occurrence is stored canonically, verbatim
the controller records first_observed_at on the acceptance record (controller-owned)
a redelivery NEVER mutates the stored envelope, created_at, or first_observed_at
redelivery observation times live only in the delivery log / projection
```

So a redelivered artifact is a replay (FD-6), and the canonical record still has
exactly one `created_at`: the one that came with the occurrence that was accepted.

### FD-6 — duplicate identity: replay vs conflict

```text
same message_id, same envelope digest   -> idempotent replay:
                                           return the existing accepted artifact,
                                           perform no new side effect,
                                           dispatch nothing,
                                           do not advance state_version
same message_id, other envelope digest  -> IdConflict: fail closed,
                                           no acceptance, no dispatch,
                                           HumanAttentionRequest raised
```

The comparison is the **envelope digest** (FD-1.2), not `payload_digest` alone:
the envelope digest commits to `payload_digest`, every `artifact_ref`, the
lineage fields, and the receipt reference — so two artifacts agreeing on payload
bytes but disagreeing on which candidate head, which contract, or which provider
execution they belong to are a conflict, not a replay.

Identity scope: `message_id` is unique within a `campaign_id`; `idempotency_key`
on a `HumanCommandRequest` is unique within a `campaign_id`, and reuse with a
different request is the same `IdConflict` — matching R1's existing `409` for
"idempotency key reused with a different request" (`docs/q-deck/r1-command.md`
§8).

An `IdConflict` never resolves itself by minting a new id. Two different payloads
claiming one identity means the system's view of who said what is broken, and
that is a human-facing fact.

### FD-7 — no model-supplied executable authority

```text
reviewer.required_regression_evidence = "run this shell command"
controller: shell -c reviewer_text            # NEVER
```

- Required evidence is named by `required_evidence_gate_ids`, resolved against
  the controller-owned gate/verifier registry, plus `verifier_policy_digest`. An
  unknown id fails closed.
- A reviewer may *propose* new evidence in prose. The controller maps it to a
  known registry id or raises `HUMAN_REQUIRED`. It never executes a model-authored
  string.
- `CoderReport.diagnostic_runs[].command_recorded` is forensic text, never
  re-executed. Per `docs/decision-and-admission-protocol.md` §5 it is diagnostic
  evidence, not admission evidence: `GATING → CI_WAIT` is satisfied only by
  controller-owned gate/CI runs against the receipt head.
- Every `artifact_ref` resolves inside 007-owned CAS only (FD-1.8, FD-2.5).
- A recorded `tool_call_requested` interaction is an observation, not an
  authorization. Only controller-owned registry resolution, policy validation,
  and execution produce an accepted `tool_result` interaction.

### FD-8 — replay never invokes a provider

Reducer replay, campaign replay, reconciliation, recovery, and historical
verification reconstruct results from immutable recorded evidence. None may call
a provider to rebuild an earlier answer.

This generalizes the property `o7 replay` already has for a single run record
(`src/main.rs:388` → `events::replay_record`: chain continuity, per-event
digests, artifact content digests, verdict recomputation — no provider call on
that path) to the campaign level.

A new provider call is always a *new* canonical execution with a fresh identity.
It never silently completes or replaces the history of an earlier one.

### FD-9 — post-dispatch ambiguity fails closed

R1 froze the durable dispatch boundary and its classification
(`docs/q-deck/r1-command.md` §11.1–§11.2). A1 generalizes it from command
continuation to **every model role, including read-only ones**:

```text
dispatch boundary not reached (established) -> safe redrive: fresh dispatch_id,
                                               explicit retry_of (FD-10)
dispatch occurred or may have occurred,
  outcome unknown                           -> dispatch_ambiguous:
                                               no redrive, no completion,
                                               no rejection, no mutation;
                                               HumanAttentionRequest raised
```

Classification is **per `dispatch_id`** (FD-10). An execution containing one
ambiguous dispatch is an ambiguous execution: `execution_outcome` is
`dispatch_ambiguous`, and no artifact produced under it may be accepted.
Ambiguity never collapses into "the execution retried fine".

A fresh identifier does not make a duplicate side effect safe. A read-only
reviewer invocation is still an external side effect: repeating it can produce a
different verdict, which rewrites campaign history exactly as effectively as
repeating a coder invocation rewrites code.

### FD-10 — provider invocation identity grains, and the shape that carries them

Two grains, both mandatory:

```text
provider_execution_id   one bounded role execution (one coder execution or one
                        reviewer execution), spanning its whole tool loop and
                        every continuation dispatch inside it
dispatch_id             one external provider request (one HTTPS request, or
                        one CLI process invocation)
```

**FD-10.1 The receipt is execution-level.** A per-dispatch receipt cannot be
referenced by a single `provider_execution_receipt_ref` without losing the rest
of a multi-dispatch tool loop, and a receipt carrying one `dispatch_id` beside a
whole-execution manifest does not say which of the two it represents. Frozen: one
`ProviderExecutionReceiptV1` per execution, with dispatch records **nested inside
it** (§3.12). The envelope references exactly that one object.

**FD-10.2 Boundary classification stays per dispatch.** Each nested dispatch
record carries its own `dispatch_boundary` and `outcome`; the execution's
`execution_outcome` is derived and fails closed (FD-9).

**FD-10.3 Retry names its grain.**

```text
whole-execution retry   new provider_execution_id, retry_of_execution_id set
single-dispatch retry   new dispatch_id in the SAME execution,
                        retry_of_dispatch_id set
tool-loop continuation  new dispatch_id, kind = continuation, NOT a retry,
                        no retry_of_*
new session             new provider_execution_id, no retry_of_*;
                        a fresh conversation, not a repetition
```

A retry is permitted only when non-dispatch is established (FD-9). The full
incarnation taxonomy (run/attempt/session/campaign incarnations) stays deferred
to A2; what A1 freezes is that no recovery or retry code may be written without
naming which grain it operates on.

`producer_execution_id` in the envelope is the `provider_execution_id` of the
execution that produced the artifact, or the controller's own execution identity
for controller-derived artifacts.

### FD-11 — the receipt must prove *this* execution produced *this* artifact

Presence and resolvability of a receipt establish nothing about provenance: a
valid receipt from an unrelated execution attached to a valid report is a
cryptographically tidy lie, and it is the worst genre of lie because it reads as
an audit.

For every artifact whose `producer_role` is `coder` or `reviewer`, acceptance
requires **all** of these to hold, checked before any other semantic validation:

```text
envelope.producer_execution_id      == receipt.provider_execution_id
envelope.producer_role              == receipt.producer_role
envelope.producer_adapter_version   == receipt.producer_adapter_version
envelope.prompt_digest              == receipt.request.prompt_digest
envelope.tool_policy_digest         == receipt.request.tool_policy_digest
envelope.model_identity             == receipt.model.requested_model
envelope.payload_digest             == receipt.final_normalized_output_ref.digest
envelope.campaign_id                == receipt.campaign_id
envelope.round_id                   == receipt.round_id
receipt.execution_outcome           == completed
```

Any mismatch fails closed as `ReceiptIncongruent`, with no acceptance and no
transition. The `payload_digest` equality is what makes the chain load-bearing:
it is only checkable because a provider-produced payload *is* the normalized
output bytes (FD-1.1), so the receipt binds the exact bytes that became the
artifact, not merely an execution that happened nearby in time.

Conversely, an artifact whose `producer_role` is `controller` or `human` must
carry **no** `provider_execution_receipt_ref`. A controller-derived artifact
holding a provider receipt is claiming a provenance it does not have.

### FD-12 — transition authority

Extends the table in `docs/autonomy-controller.md` ("Transition authority"). Each
row names the single canonical artifact that authorizes the transition; the
reducer that enforces it is frozen in FD-14 and implemented in A1-V0.

| Transition | Authorizing canonical artifact | Never sufficient |
|---|---|---|
| campaign start → `BUILDING` | `WorkOrder` accepted, lineage bound before dispatch | a task description alone |
| `BUILDING` → `GATING` | `CandidateReceipt` (controller-derived behind the A0 seal), `claim_check.claimed_head_matches = true` | `CoderReport.status = candidate_produced` |
| `GATING` → `CI_WAIT` | controller-owned gate results bound to `candidate_head` | `CoderReport.diagnostic_runs` |
| `CI_WAIT` → `REVIEWING` | required CI results bound to the same exact head | a green workflow on another head |
| `REVIEWING` → `CORRECTING` | `ReviewVerdict.verdict = changes_requested` at the current head | `ReviewerReport` saying so |
| `REVIEWING` → `READY_TO_MERGE` | `ReviewVerdict.verdict = accepted`, `reviewed_head == current_candidate_head`, no drift, required gates green | `ReviewerReport.verdict = accepted` |
| `CORRECTING` → `BUILDING` | `CorrectiveDirective` derived from an accepted `ReviewVerdict`, same `scope_ref` digest | reviewer prose |
| any → `HUMAN_REQUIRED` | `HumanAttentionRequest` (controller-raised) | an agent asking for a human |
| `HUMAN_REQUIRED` → resumed | `HumanDecision` bound to the exact head, contract digest, and `state_version` the human saw | an acknowledged alert |
| any → `CANCEL_REQUESTED` | `HumanDecision` (CANCEL) | a UI flag |
| `CANCEL_REQUESTED` → `CANCELLED` | the observed cancellation sequence completed (§3.10) | the request alone |
| merge | **not authorized by A1.** Merge stays manual, outside the system | any artifact in this document |

A later event never retroactively makes an earlier unsafe transition valid. If
the candidate head changes after review, the review is stale and the campaign
returns to the appropriate verification state
(`docs/decision-and-admission-protocol.md` §7).

### FD-13 — evidence is bound to the head it examined

Gate and CI results are admission evidence only when bound to the exact
`candidate_head` they ran against, per
`docs/decision-and-admission-protocol.md` §5. A result whose head is unknown, or
whose head is not the current candidate head, is diagnostic evidence: recorded,
never transition-bearing.

### FD-14 — campaign state, `state_version`, and the V0 reducer

A1-V0 must accept a verdict, move to `CORRECTING`, accept a new candidate, reach
`READY_TO_MERGE`, replay to the same state after restart, and validate
`expected_campaign_state_version` on a human command. That *is* a reducer.
Calling it a small orchestration loop would not change what it does, and bytes
are famously unmoved by euphemism. So A1 freezes the minimum reducer contract
here, A1-V0 implements exactly that minimum, and A2 extends it.

**FD-14.1 The state.** `CampaignStateV1` (§3.14) is the total state a V0
campaign needs. It is derived, never authored: no artifact carries it, and no
producer may submit it.

**FD-14.2 The reducer is pure.**

```text
fold: (CampaignStateV1, AcceptedEvent) -> CampaignStateV1 | TransitionRejected
```

Total, deterministic, no clock, no I/O, no provider (FD-8). Given the same
accepted-event log and the same `campaign_protocol_version`, replay yields the
same `CampaignStateV1`, byte for byte — the campaign-level analogue of the
per-run property `o7 replay` already verifies.

**FD-14.3 The event log has two classes.**

```text
authority-bearing (advance state_version by exactly 1):
  CampaignCreated
  WorkOrderIssued
  CandidateAccepted            (a CandidateReceipt was accepted)
  GateResultsAccepted
  CiResultsAccepted
  ReviewRequested
  ReviewVerdictAccepted
  CorrectiveDirectiveIssued
  HumanAttentionRaised
  HumanDecisionRecorded
  CancelRequested
  CampaignCancelled
  CampaignTerminalError

evidence-only (never advance state_version):
  CoderReportReceived
  ReviewerReportReceived
  ProviderExecutionRecorded
  HumanCommandRejected
  TransitionRejected
  CampaignFeedItemEmitted
```

Frozen rule:

```text
state_version increases by exactly 1 on each accepted authority-bearing event.
Evidence events, projections, feed items, rejected commands, rejected
transitions, redelivery, and idempotent replay never change it.
last_accepted_sequence advances on EVERY accepted event of either class.

An acknowledgement is not an exception: the ACK's own HumanDecisionRecorded is
authority-bearing (+1) — a human who has acknowledged is looking at a different
campaign than one who has not — while the attention record's lifecycle move to
ACKNOWLEDGED carries no separate increment, and ACK still never means RESOLVED.
```

Two counters, because they answer two different questions: `last_accepted_sequence`
is where the log is, `state_version` is what a human was looking at. A human's
stale-command check must not fire because a feed item scrolled past.

**FD-14.4 Guards come from FD-12.** A `fold` that receives an authority-bearing
event whose guard is unsatisfied returns `TransitionRejected` and does not
advance either counter. Rejection is recorded as evidence; it never mutates
state.

**FD-14.5 What A1 does not freeze here.** Progress frontier and `NO_PROGRESS`
semantics, terminal/escalation taxonomy beyond the phases listed, external
reconciliation, budget accounting beyond a stop condition, and the full
incarnation taxonomy — all A2 (issue #94 §3, §5).

### FD-15 — human command binding and honest actor attestation

**FD-15.1 Binding.** Every `HumanCommandRequest` carries the binding fields in
§3.10 and is rejected unless `expected_campaign_state_version`,
`expected_contract_digest`, and (where applicable) `expected_head` all match
current canonical state. This closes the stale-screen TOCTOU window: a decision
applies to what the human actually saw, or it does not apply.

**FD-15.2 Claim and observation are separate objects.** `o7d` has no
authentication mechanism today — it binds loopback by default and refuses a
non-loopback bind without an explicit `--allow-non-loopback` flag
(`crates/o7d/src/main.rs:24–92`). That is a deployment policy, not an
authenticator, and the record must not imply otherwise.

A loopback connection proves exactly one thing: *the connection arrived over
loopback transport*. It does not prove that the sender is a human, that the
sender is the operator, that `actor_identity` belongs to the sender, or that the
local process is uncompromised. Every local process is local; the kernel does not
issue humanity at `accept()`.

Frozen split:

```text
HumanCommandRequest (untrusted, rank 3)
  claimed_actor_identity: ...        # a claim, and named as one

HumanDecision (accepted, rank 4)
  actor:
    claimed_identity: ...            # copied verbatim from the request
    authentication_strength: loopback_unauthenticated
                           | authenticated
                           | unattested
    observed_transport:    loopback | non_loopback
    authenticator_id:      absent | <id of the authenticator that verified it>
```

- `authentication_strength` and `observed_transport` are determined by the
  controller from the transport it actually observed. They are never read from
  the request, and the request has no field in which to assert them.
- `authenticated` requires a real authenticator; `authenticator_id` is then
  mandatory. Absent an authenticator, that value is unreachable.
- `loopback_unauthenticated` is admissible in V0 **only** under an explicit
  deployment policy, and it is recorded on every decision it produced — so an
  audit reads "a loopback caller claiming to be X", which is what happened,
  rather than "operator X", which was never established.
- `unattested` is refused.

## 3. Frozen wire schemas

**Notation.** Every table gives: field, type, required, constraints, and the
authority that establishes the value. Global rules that are not repeated per
row: explicit `null` is rejected everywhere (FD-1.3); optional means
absent-or-value; all bounds of FD-1.4 apply; payloads never restate an
envelope-owned field (FD-5.3).

Shared scalar types:

| Type | Form | Constraints |
|---|---|---|
| `Id` | opaque non-empty UTF-8 string, never parsed for meaning | ≤ 256 bytes |
| `Digest256` | lowercase hex SHA-256 | exactly 64 chars |
| `CommitId` | full object id, the repository's object-format width | never abbreviated (`docs/decision-and-admission-protocol.md` §4) |
| `ArtifactRef` | `{kind, media_type, digest: Digest256, size: u64}` | CAS-only (FD-1.8); slot-checked (FD-2.5) |
| `Text` | UTF-8 string | ≤ 65536 bytes unless stated |
| `Timestamp` | RFC 3339 UTC | metadata only (FD-5.4) |

Eleven envelope-bearing message kinds (§3.1–§3.11), plus three referenced typed
objects that carry no envelope of their own: `ProviderExecutionReceiptV1`
(§3.12), `ScopeContractV1` (§3.13), `InteractionManifestV1` (§3.12.1), and one
derived object no producer may author: `CampaignStateV1` (§3.14).

### 3.0 Common envelope v1

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `envelope_version` | u32 | yes | `= 1` | protocol |
| `message_kind` | enum (11 kinds) | yes | closed enum | protocol |
| `message_kind_version` | u32 | yes | `= 1` | protocol |
| `message_id` | `Id` | yes | unique within `campaign_id` | producer; conflict per FD-6 |
| `root_goal_id` | `Id` | yes | — | controller binding (FD-5.1) |
| `task_id` | `Id` | yes | — | controller binding |
| `campaign_id` | `Id` | yes | — | controller binding |
| `round_id` | `Id` | no | absent only for campaign-scope artifacts | controller binding |
| `causation_id` | `Id` | no | absent only for the campaign-initiating `WorkOrder` | controller resolution |
| `correlation_id` | `Id` | yes | — | controller |
| `producer_role` | enum{`controller`,`coder`,`reviewer`,`human`} | yes | closed | controller (from the dispatch it made) |
| `producer_execution_id` | `Id` | yes | FD-10 | controller (minted before dispatch) |
| `producer_adapter_version` | `Text` | yes | ≤ 128 bytes | adapter build identity |
| `model_identity` | `Text` | iff role ∈ {coder, reviewer} | ≤ 256 bytes; logical identity for routing/policy, **not** runtime evidence | controller routing decision |
| `prompt_digest` | `Digest256` | iff role ∈ {coder, reviewer} | — | controller (prompt it sent) |
| `tool_policy_digest` | `Digest256` | iff role ∈ {coder, reviewer} | — | controller policy |
| `contract_digest` | `Digest256` | yes | must equal the campaign binding | campaign binding (FD-5.3) |
| `expected_input_head` | `CommitId` | iff the artifact reasons about code | — | controller state (FD-5.2) |
| `payload_digest` | `Digest256` | yes | digest of the exact stored payload bytes | computed |
| `artifact_refs` | `[ArtifactRef]` | yes (may be empty) | ≤ 256; rank rule (FD-2.1); closure bounds (FD-1.5) | producer; slot-checked |
| `provider_execution_receipt_ref` | `ArtifactRef` | iff role ∈ {coder, reviewer}; **forbidden** otherwise | congruence per FD-11 | controller |
| `created_at` | `Timestamp` | yes | metadata only; excluded from framing | producer observation |

`first_observed_at` is **not** an envelope field: it is controller-owned and
recorded on the acceptance record (FD-5.4).

### 3.1 WorkOrderV1 (Controller → Coder, rank 5)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `role` | enum{`coder`} | yes | closed | controller |
| `goal.summary` | `Text` | yes | ≤ 8192 bytes | controller (from the frozen contract) |
| `input.base_sha` | `CommitId` | yes | — | A0 campaign base |
| `input.candidate_ref` | `ArtifactRef` | no | A0 `CandidateRef`; absent for the first round | imported root (FD-2.4) |
| `input.materialization_attestation_ref` | `ArtifactRef` | iff `candidate_ref` present | A0/`o7-worktree` attestation | imported root |
| `scope_ref` | `ArtifactRef` | yes | `ScopeContractV1` (§3.13) | controller-owned document |
| `required_evidence.gate_ids` | `[Id]` | yes | ≤ 256; every id resolvable in the registry | gate registry (FD-7) |
| `required_evidence.acceptance_case_ids` | `[Id]` | yes (may be empty) | ≤ 256 | contract |
| `verifier_policy_digest` | `Digest256` | yes | — | controller policy |
| `budget.max_provider_turns` | u32 | yes | ≥ 1 | campaign policy |
| `budget.max_wall_time_seconds` | u32 | yes | ≥ 1 | campaign policy |
| `budget.evidence_budget_bytes` | u64 | yes | ≤ the FD-1.5 hard maximum | campaign policy |
| `budget.closure_object_budget` | u32 | yes | ≤ `max_reachable_closure_objects` | campaign policy |
| `budget_policy_digest` | `Digest256` | yes | commits to the four budget values above | computed |

The coder never receives "address the review comments". It receives the frozen
contract identity, concrete findings via a `CorrectiveDirective`, and explicit
scope limits.

### 3.2 CoderReportV1 (Coder → Controller, untrusted, rank 3)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `status` | enum{`candidate_produced`,`failed`,`question_blocked`} | yes | closed | **claim** |
| `claimed_head` | `CommitId` | iff `status = candidate_produced` | checked against the derived candidate | **claim** |
| `claimed_state_digest` | `Digest256` | no | — | **claim** |
| `change_summary` | `Text` | yes | ≤ 16384 bytes | advisory |
| `intent` | `Text` | no | ≤ 16384 bytes | advisory |
| `claims[].claim_id` | `Id` | yes | unique within the report | advisory |
| `claims[].statement` | `Text` | yes | ≤ 4096 bytes | advisory |
| `claims[].evidence_refs` | `[ArtifactRef]` | yes (may be empty) | rank ≤ 2 | advisory |
| `diagnostic_runs[].command_recorded` | `Text` | yes | ≤ 4096 bytes; **never executed** (FD-7) | forensic |
| `diagnostic_runs[].result` | enum{`passed`,`failed`,`unknown`} | yes | closed | diagnostic only (FD-13) |
| `diagnostic_runs[].artifact_ref` | `ArtifactRef` | no | rank 0 | diagnostic only |
| `known_residuals` | `[Text]` | yes (may be empty) | ≤ 256 entries | advisory |
| `questions[].question_id` | `Id` | yes | unique within the campaign | producer |
| `questions[].text` | `Text` | yes | ≤ 8192 bytes | advisory |

There is no field by which a coder can emit `accepted`, and no field the
controller treats as admission evidence.

### 3.3 CandidateReceiptV1 (controller-derived, rank 4)

Derived **behind the A0 sealing boundary**, consumed as a capability and not
redefined:

```text
seal_candidate(worktree_attestation, producer_execution) -> CandidateRef
```

Quiescence means no live holder of write authority remains — a durably revoked or
advanced write-capability/lease epoch under which no previously issued writer
capability remains valid. Process termination and descendant absence are proof
mechanisms, not the definition (`docs/q-deck/a0-candidate-state.md`).

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `candidate_ref` | `ArtifactRef` | yes | A0 `CandidateRef` | imported root, produced by `seal_candidate` |
| `candidate_head` | `CommitId` | yes | — | controller-observed |
| `candidate_tree_identity` | `Text` | yes | tree object id | controller-observed |
| `base_ancestry` | `[CommitId]` | yes | ≤ 256 | controller-observed |
| `repository_identity` | A0 `RepositoryIdentity` | yes | — | `o7-worktree` canonical repo id |
| `changed_paths` | `[Text]` | yes | ≤ 4096 | controller-observed diff |
| `file_modes` | `[Text]` | yes | parallel to `changed_paths` | controller-observed |
| `diff_scope` | enum{`within_scope`,`out_of_scope`} | yes | computed against `scope_ref` | controller |
| `admission_profile` | enum{`LIGHTWEIGHT`,`STANDARD`,`STRICT`,`CRITICAL`} | yes | ≥ `STRICT` for autonomous mutation; ambiguity → stricter | controller classification over observed paths (#94 §4) |
| `applicable_gate_ids` | `[Id]` | yes | registry ids | gate registry |
| `sealed_under_epoch` | `Text` | yes | the revoked/advanced write epoch | A0 sealing boundary |
| `coder_report_ref` | `ArtifactRef` | yes | rank 3, forward reference (FD-2.3) | controller |
| `claim_check.claimed_head_matches` | bool | yes | `false` ⇒ fail closed, no review dispatch | controller comparison |

```text
coder says:      docs only
controller sees: verifier changed
-> CRITICAL or HUMAN_REQUIRED
```

### 3.4 ReviewRequestV1 (Controller → Reviewer, rank 5)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `candidate_receipt_ref` | `ArtifactRef` | yes | rank 4 | controller |
| `candidate_head` | `CommitId` | yes | from the receipt, never from a `CoderReport` | controller |
| `base_sha` | `CommitId` | yes | — | campaign base |
| `scope_ref` | `ArtifactRef` | yes | same digest as the `WorkOrder`'s | controller |
| `required_properties` | `[Text]` | yes | ≤ 256 entries | contract |
| `required_evidence_gate_ids` | `[Id]` | yes | registry ids | gate registry |
| `verifier_policy_digest` | `Digest256` | yes | — | controller policy |
| `evidence_refs` | `[ArtifactRef]` | yes | **ordered**: contract, diff, deterministic evidence first | controller |
| `coder_report_ref` | `ArtifactRef` | no | delivered last, labeled advisory | controller |

Input ordering is normative, not stylistic.

### 3.5 ReviewerReportV1 (Reviewer → Controller, untrusted, rank 3)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `reviewed_head` | `CommitId` | yes | stale ⇒ never promoted (FD-5.2) | **claim** |
| `verdict` | enum{`accepted`,`changes_requested`,`blocked`} | yes | closed; authorizes nothing (FD-4) | **claim** |
| `findings[].finding_id` | `Id` | yes | unique within the report | producer |
| `findings[].severity` | enum{`blocker`,`major`,`minor`,`note`} | yes | closed | **claim** |
| `findings[].affected_property` | `Text` | yes | ≤ 4096 bytes | **claim** |
| `findings[].evidence_refs` | `[ArtifactRef]` | yes (may be empty) | rank ≤ 2; resolvable | **claim** |
| `findings[].required_change` | `Text` | yes | ≤ 8192 bytes | **claim** |
| `findings[].required_evidence_gate_ids` | `[Id]` | yes (may be empty) | **registry ids only**; unknown ⇒ `HUMAN_REQUIRED` (FD-7) | gate registry |
| `findings[].proposed_evidence_prose` | `Text` | no | ≤ 4096 bytes; inert, never executed | **claim** |
| `properties_checked` | `[Text]` | yes | ≤ 256 entries | **claim** |
| `properties_preserved` | `[Text]` | yes | ≤ 256 entries | **claim** |
| `residual_risks` | `[Text]` | yes (may be empty) | ≤ 256 entries | **claim** |
| `reviewer.identity` | `Text` | yes | ≤ 256 bytes | **claim**; cross-checked against the receipt (FD-11) |
| `reviewer.model` | `Text` | yes | ≤ 256 bytes | **claim**; the receipt is the evidence |
| `reviewer.prompt_version` | `Text` | yes | ≤ 128 bytes | **claim** |

There is no `review_id` field: that identity is controller-minted and cannot be
proposed by a model.

### 3.6 ReviewVerdictV1 (controller-accepted, rank 4)

Accepted only when **all** hold:

```text
FD-11 congruence passes
ReviewerReport schema-valid, supported version, within bounds
reviewed_head == CampaignStateV1.current_candidate_head
envelope.contract_digest == the campaign binding
every finding_id unique; every required_evidence_gate_id resolves
every evidence_ref resolvable, rank rule and closure bounds satisfied
reviewer independence preconditions recorded (§4)
```

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `review_id` | `Id` | yes | controller-minted | controller |
| `reviewed_head` | `CommitId` | yes | `== current_candidate_head` | controller-verified |
| `verdict` | enum{`accepted`,`changes_requested`,`blocked`} | yes | closed | controller acceptance of the claim |
| `findings` | as §3.5, validated | yes | every `finding_id` exists in the referenced report | derived from the report |
| `properties_checked` / `properties_preserved` / `residual_risks` | as §3.5 | yes | — | derived from the report |
| `reviewer.identity` / `.model` / `.prompt_version` | as §3.5 | yes | must match the receipt (FD-11) | controller-verified |
| `reviewer_report_ref` | `ArtifactRef` | yes | rank 3; the source of truth for every derived field | controller |
| `accepted_under.verifier_policy_digest` | `Digest256` | yes | — | controller policy |
| `accepted_under.gate_registry_digest` | `Digest256` | yes | — | gate registry |

The verdict's derived fields are a validated projection of the report, and
`reviewer_report_ref` remains the source of truth: the controller does not
improve a reviewer's findings on the way through.

### 3.7 CorrectiveDirectiveV1 (Controller → Coder, rank 5)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `derived_from_review_id` | `Id` | yes | must name an accepted verdict | controller |
| `review_verdict_ref` | `ArtifactRef` | yes | rank 4 | controller |
| `target_finding_ids` | `[Id]` | yes | non-empty; every id exists in that verdict | controller |
| `required_changes` | `[Text]` | yes | ≤ 256 entries, ≤ 8192 bytes each | derived from the verdict |
| `required_evidence_gate_ids` | `[Id]` | yes | registry ids | gate registry |
| `scope_ref` | `ArtifactRef` | yes | **digest identical** to the `WorkOrder`'s `scope_ref` | controller |
| `budget_policy_digest` | `Digest256` | yes | unchanged from the `WorkOrder` | campaign policy |

Scope equality is expressed as one immutable `ScopeContractV1` referenced by
digest from both artifacts (§3.13) — not as a "byte-identical" comparison between
two independently serialized payloads, which FD-1.2 deliberately makes
meaningless.

A directive that would reference a different `scope_ref` digest, or a different
`contract_digest`, is not a directive: it is a contract revision, and it fails
closed into `HUMAN_REQUIRED` with reason `SCOPE_EXPANSION_REFUSED`.
`changes_requested` is never permission to modify the frozen goal, acceptance
contract, verifier, or baseline.

Rounds are **forward-only**: a corrective round produces a new candidate head and
never amends, rebases, or force-pushes an earlier one
(`docs/decision-and-admission-protocol.md` §8).

### 3.8 CampaignFeedItemV1 (Controller → Human, rank 5)

Informative lifecycle history. No acknowledgement, no decision, and never a
transition (FD-14.3).

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `item_kind` | enum{`candidate_produced`,`gates_completed`,`ci_completed`,`review_completed`,`round_started`,`command_rejected`,`execution_recorded`} | yes | closed | controller |
| `summary` | `Text` | yes | ≤ 4096 bytes | controller |
| `subject_refs` | `[ArtifactRef]` | yes (may be empty) | rank ≤ 4 | controller |
| `at_sequence` | u64 | yes | the log sequence being described | controller |
| `at_state_version` | u64 | yes | unchanged by this item | controller |

### 3.9 HumanAttentionRequestV1 (Controller → Human, rank 5)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `attention_id` | `Id` | yes | stable canonical identity | controller |
| `dedupe_key` | `Text` | yes | ≤ 512 bytes; **computed by the controller**, never by an agent | controller |
| `candidate_head` | `CommitId` | no | absent when no candidate exists yet | controller state |
| `reason.code` | enum: `HUMAN_REQUIRED`, `READY_TO_MERGE`, `EXTERNAL_DRIFT`, `ID_CONFLICT`, `RECEIPT_INCONGRUENT`, `DISPATCH_AMBIGUOUS`, `AGENT_FAILED`, `NO_PROGRESS`, `CONFLICTING_EVIDENCE`, `BUDGET_EXHAUSTED`, `EVIDENCE_BUDGET_EXCEEDED`, `CI_FAILED_REPEATEDLY`, `SCOPE_EXPANSION_REFUSED`, `UNMAPPED_EVIDENCE_PROPOSAL`, `CONTRACT_REVISION_REQUESTED` | yes | closed; additive codes need a new kind version | controller |
| `reason.summary` | `Text` | yes | ≤ 4096 bytes | controller |
| `severity` | enum{`info`,`attention`,`urgent`} | yes | closed | controller |
| `required_decision_kind` | enum{`none`,`ack`,`choose_resolution`} | yes | closed | controller |
| `options[].action_id` | `Id` | iff `choose_resolution` | **server-defined**; a client may only select one | controller |
| `options[].consequence` | `Text` | yes with the option | ≤ 4096 bytes | controller |
| `evidence_refs` | `[ArtifactRef]` | yes | rank ≤ 4 | controller |
| `lifecycle` | enum{`OPEN`,`ACKNOWLEDGED`,`RESOLVED`,`SUPERSEDED`} | yes | closed | controller |
| `raised_at_state_version` | u64 | yes | what the human is looking at | reducer (FD-14) |

Repeated reconciliation updates the one durable record; occurrence counts live in
projection. A new head or a new problem class creates a new attention identity.
**ACK ≠ RESOLVED** — a human seeing a problem does not make the problem leave,
and acknowledgement is evidence-only (FD-14.3).

### 3.10 HumanCommandRequestV1 (Human → Controller, untrusted, rank 3)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `command_id` | `Id` | yes | — | client |
| `idempotency_key` | `Text` | yes | ≤ 256 bytes; unique within the campaign | client; conflict per FD-6 |
| `command` | enum{`ACK`,`CANCEL`,`ANSWER_QUESTION`,`SELECT_ATTENTION_ACTION`} | yes | closed | client |
| `claimed_actor_identity` | `Text` | yes | ≤ 256 bytes; **a claim** (FD-15.2) | client |
| `authorization_context` | `Text` | no | ≤ 4096 bytes; opaque, non-authoritative | client |
| `expected_campaign_state_version` | u64 | yes | must equal current (FD-15.1) | reducer state |
| `expected_contract_digest` | `Digest256` | yes | must equal the campaign binding | campaign binding |
| `expected_head` | `CommitId` | iff the command concerns a candidate | must equal `current_candidate_head` | reducer state |
| `attention_id` | `Id` | iff `ACK` or `SELECT_ATTENTION_ACTION` | must be an `OPEN`/`ACKNOWLEDGED` attention | controller |
| `selected_action_id` | `Id` | iff `SELECT_ATTENTION_ACTION` | must be in that request's `options` | controller |
| `question_id` | `Id` | iff `ANSWER_QUESTION` | must not be superseded | controller |
| `answer.text` | `Text` | iff `ANSWER_QUESTION` | ≤ 16384 bytes | human |
| `answer.declared_scope_effect` | enum{`none`,`revise_contract`} | iff `ANSWER_QUESTION` | closed | human declaration |

There is deliberately **no** field for attestation, transport, or authenticator:
those are observations, and a request cannot observe itself (FD-15.2).

**ANSWER_QUESTION resolution:**

```text
declared none + controller finds no scope change -> delivered as clarification
declared revise_contract                         -> campaign superseded:
                                                    HUMAN_REQUIRED,
                                                    reason CONTRACT_REVISION_REQUESTED
ambiguous                                        -> HUMAN_REQUIRED, explicit re-ask
```

A contract revision never continues the same campaign. V1 has **no**
`ContractRevisionProposal` artifact: the campaign terminates into
`HUMAN_REQUIRED`, a new contract is frozen out of band, and a new campaign is
minted with an explicit `supersedes` relation (FD-5.1). Adding a revision-proposal
artifact would be a new message kind under §7, not a footnote.

**CANCEL is a process, not a flag:**

```text
CancelRequested -> phase CANCEL_REQUESTED
-> prevent new dispatches
-> request termination of the active execution
-> observe process/worktree state
-> cleanup or preserve forensic state
-> CampaignCancelled -> phase CANCELLED
```

A UI showing "cancelled" while the coder is still writing files is a lovely
interface to the wrong reality.

Deferred post-v1: `PAUSE`, `RESUME`, `REQUEST_MORE_EVIDENCE` as a standalone
command, `APPROVE_RESIDUAL_RISK`, `REJECT_CANDIDATE`, `AUTHORIZE_MERGE`.
`accept_residual_risk` exists in the vocabulary and is not offered in v1.

### 3.11 HumanDecisionV1 (controller-accepted, rank 4)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `decision_id` | `Id` | yes | controller-minted | controller |
| `command_request_ref` | `ArtifactRef` | yes | rank 3 | controller |
| `command` | enum as §3.10 | yes | closed | controller-verified |
| `actor.claimed_identity` | `Text` | yes | copied verbatim from the request | **claim, preserved as one** |
| `actor.authentication_strength` | enum{`loopback_unauthenticated`,`authenticated`,`unattested`} | yes | `unattested` ⇒ refused; never read from the request | controller observation |
| `actor.observed_transport` | enum{`loopback`,`non_loopback`} | yes | never read from the request | controller observation |
| `actor.authenticator_id` | `Id` | iff `authenticated` | — | the authenticator |
| `effect` | enum{`acknowledged`,`cancel_requested`,`answer_delivered`,`attention_action_selected`,`campaign_superseded`} | yes | closed | reducer (FD-14) |
| `applied_at_state_version` | u64 | yes | the version this decision advanced *to* | reducer |

### 3.12 ProviderExecutionReceiptV1 (rank 2, no envelope)

One receipt per `provider_execution_id`, with dispatch records nested (FD-10.1).

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `schema_version` | u32 | yes | `= 1` | protocol |
| `provider_execution_id` | `Id` | yes | — | controller (minted before dispatch) |
| `retry_of_execution_id` | `Id` | no | present iff a whole-execution retry (FD-10.3) | controller |
| `campaign_id` / `round_id` | `Id` | yes | congruence per FD-11 | controller binding |
| `producer_role` | enum{`coder`,`reviewer`} | yes | closed | controller |
| `producer_adapter_version` | `Text` | yes | ≤ 128 bytes | adapter build |
| `provider.identity` | `Text` | yes | ≤ 256 bytes | adapter |
| `provider.endpoint_mode` | `Text` | yes | ≤ 128 bytes | adapter |
| `provider.request_id` | `Text` | no | absent when the provider returned none | provider |
| `model.requested_model` | `Text` | yes | ≤ 256 bytes | controller routing |
| `model.resolution.status` | enum{`provider_reported`,`fingerprint_only`,`unavailable`} | yes | closed | adapter observation |
| `model.resolution.provider_reported_model` | `Text` | iff `provider_reported` | never inferred from an alias (FD-3) | provider |
| `model.resolution.backend_fingerprint` | `Text` | iff `fingerprint_only` | — | adapter observation |
| `request.prompt_digest` | `Digest256` | yes | congruence per FD-11 | controller |
| `request.tool_policy_digest` | `Digest256` | yes | congruence per FD-11 | controller policy |
| `request.decoding_policy_digest` | `Digest256` | yes | — | adapter |
| `request.budget_policy_digest` | `Digest256` | yes | — | campaign policy |
| `dispatches[]` | list | yes | non-empty; ≤ 256 | see below |
| `interaction_manifest_ref` | `ArtifactRef` | yes | rank 1 (§3.12.1) | controller |
| `final_normalized_output_ref` | `ArtifactRef` | iff `execution_outcome = completed` | rank 0; **its digest is the artifact's `payload_digest`** (FD-11) | adapter |
| `execution_outcome` | enum{`completed`,`refused`,`incomplete`,`failed_pre_dispatch`,`dispatch_ambiguous`} | yes | derived from `dispatches`, fail-closed (FD-9) | controller |

Each `dispatches[]` entry:

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `dispatch_id` | `Id` | yes | unique within the execution | controller |
| `sequence` | u32 | yes | 0-based, contiguous | controller |
| `kind` | enum{`initial`,`continuation`} | yes | a continuation is **not** a retry (FD-10.3) | controller |
| `retry_of_dispatch_id` | `Id` | no | present iff a single-dispatch retry; forbidden when `kind = continuation` | controller |
| `canonical_request_ref` | `ArtifactRef` | yes | rank 0; the exact provider-facing request | adapter |
| `raw_provider_event_ref` | `ArtifactRef` | no | rank 0; `o7 invoke`'s `stdout.raw` in V0 | provider bytes |
| `normalized_output_ref` | `ArtifactRef` | no | rank 0; `o7 invoke`'s `result.json` in V0 | adapter |
| `dispatch_boundary` | enum{`not_reached`,`reached`,`ambiguous`} | yes | R1 §11 semantics, per dispatch | durable boundary record |
| `outcome` | enum{`completed`,`refused`,`incomplete`,`failed_pre_dispatch`,`dispatch_ambiguous`} | yes | closed | controller |
| `provider_error_code` | `Text` | no | e.g. an `o7 invoke` `BLOCKED_*`/`FAIL_*` kind | adapter |
| `usage_ref` / `cost_ref` | `ArtifactRef` | no | rank 0 | provider |

Derivation rule for `execution_outcome`, frozen:

```text
any dispatch.dispatch_boundary == ambiguous
  or any dispatch.outcome == dispatch_ambiguous   -> dispatch_ambiguous
else all boundaries not_reached                   -> failed_pre_dispatch
else last dispatch.outcome                        -> that value
```

The receipt carries no reference to any artifact accepted from it (FD-2.3).

#### 3.12.1 InteractionManifestV1 (rank 1, no envelope)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `schema_version` | u32 | yes | `= 1` | protocol |
| `provider_execution_id` | `Id` | yes | must equal the receipt's | controller |
| `interaction_sequence[].sequence` | u32 | yes | 0-based, contiguous | controller |
| `interaction_sequence[].dispatch_id` | `Id` | iff `kind ≠ tool_result` | must exist in the receipt's `dispatches` | controller |
| `interaction_sequence[].kind` | enum{`provider_request`,`provider_continuation`,`tool_call_requested`,`tool_result`,`provider_error`} | yes | closed | observation |
| `interaction_sequence[].input_ref` / `.output_ref` | `ArtifactRef` | per kind | rank 0 | observation |
| `interaction_sequence[].tool_call_id` | `Id` | iff a tool kind | pairs request with result | observation |
| `interaction_sequence[].tool_id` | `Id` | iff `tool_call_requested` | registry id; recording ≠ authorizing (FD-7) | registry |
| `interaction_sequence[].arguments_ref` / `.result_ref` | `ArtifactRef` | per kind | rank 0 | observation |

The manifest records externally observable requests, responses, tool calls, tool
results, errors, and ordering. It claims no access to hidden provider reasoning.
A `tool_result` entry exists only for a tool the controller itself resolved and
executed. Partial history is still evidence: if dispatch occurred but the final
result is unavailable, the execution is `dispatch_ambiguous` and recovery never
asks the provider to recreate the missing answer (FD-8, FD-9).

### 3.13 ScopeContractV1 (rank 0 imported root, no envelope)

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `schema_version` | u32 | yes | `= 1` | protocol |
| `scope_id` | `Id` | yes | — | controller |
| `allowed_paths` | `[Text]` | yes | ≤ 4096 entries | contract |
| `forbidden_paths` | `[Text]` | yes (may be empty) | ≤ 4096 entries | contract |
| `frozen_properties` | `[Text]` | yes (may be empty) | ≤ 256 entries | contract |

Immutable. `WorkOrder` and every `CorrectiveDirective` in the campaign reference
the same digest; scope equality is digest equality (§3.7).

### 3.14 CampaignStateV1 (derived, never authored)

No producer may submit this object, and it carries no envelope: it is the output
of the FD-14 fold.

| Field | Type | Req | Constraints | Authority |
|---|---|---|---|---|
| `schema_version` | u32 | yes | `= 1` | protocol |
| `campaign_protocol_version` | u32 | yes | `= 1`; bound at campaign creation | protocol |
| `campaign_id` | `Id` | yes | — | controller binding |
| `state_version` | u64 | yes | +1 per accepted authority-bearing event only (FD-14.3) | reducer |
| `last_accepted_sequence` | u64 | yes | +1 per accepted event of either class | reducer |
| `phase` | enum{`BUILDING`,`GATING`,`CI_WAIT`,`REVIEWING`,`CORRECTING`,`HUMAN_REQUIRED`,`READY_TO_MERGE`,`CANCEL_REQUESTED`,`CANCELLED`,`TERMINAL_ERROR`} | yes | closed | reducer |
| `current_candidate_head` | `CommitId` | no | absent until the first `CandidateAccepted` | reducer |
| `contract_digest` | `Digest256` | yes | the campaign binding | reducer |
| `scope_digest` | `Digest256` | yes | the `ScopeContractV1` digest | reducer |
| `active_round_id` | `Id` | no | absent in terminal phases | reducer |
| `active_execution` | `{role, provider_execution_id}` | no | absent when no execution is in flight | reducer |
| `open_attention_ids` | `[Id]` | yes (may be empty) | ≤ 256 | reducer |
| `supersedes` / `superseded_by` | `Id` | no | campaign lineage (FD-5.1) | reducer |

## 4. Reviewer independence (mechanically enforced)

Not prompt-requested. Enforced by construction, and each item is recorded on the
execution receipt or refused before dispatch:

```text
fresh provider session — no continuation from the coder session
no coder transcript in the reviewer's context
detached exact-head worktree, freshly attested
no repository mutation credentials
no GitHub mutation credentials
separate prompt identity and tool-policy identity (digests in the envelope)
distinct provider_execution_id (FD-10)
```

Anything less is the same agent handed a pair of glasses and asked to be
objective.

v1 permits the same provider family for both roles. Cross-family review stays in
the existing consensus backlog.

## 5. A1-V0 — the first vertical, and what it must prove

Implementation begins after this freeze is accepted.

### 5.1 Role assignment

```text
coder      claude CLI, mutation confined to its own worktree
controller 007 — terminates writer authority, seals, derives CandidateReceipt,
           folds the campaign log (FD-14)
reviewer   arliai — read-only, schema-bound, no mutation credentials at all
human      Q-Deck feed + attention requests, v1 command set only
```

Arli AI is the natural first reviewer backend for a mechanical reason:
`docs/o7-invoke.md` freezes it as a direct HTTPS call with **no subprocess and no
tool surface at all**, `tool_choice: "none"`, a compile-time constant endpoint,
and a `tool_calls`-present response rejected as `FAIL_INVALID_OUTPUT`. Reviewer
independence then holds across two different execution surfaces, not merely two
prompts. Its artifact split maps directly onto the receipt: `stdout.raw` →
`raw_provider_event_ref`, `result.json` → `normalized_output_ref` and, for the
final dispatch, `final_normalized_output_ref` — whose digest must equal the
`ReviewerReport`'s `payload_digest` (FD-11).

`--engine codex` is not a V0 coder: `docs/o7-invoke.md` records that none of its
confinement flags has been exercised against a real `codex` binary, so its honest
posture today is "ambient context refused, writes denied" — not "no shell", not
"no network". It becomes eligible once that gap is live-verified, which is not A1
work.

### 5.2 Scope of the V0 implementation

```text
in scope:  the 11 message kinds, the receipt, the manifest, ScopeContractV1,
           CampaignStateV1 and the FD-14 fold, the FD-11 congruence check,
           closure resolution with FD-1.5 bounds, one live corrective cycle
out:       progress frontier, NO_PROGRESS, reconciliation, webhooks,
           full incarnation taxonomy, budget accounting beyond a stop condition
```

### 5.3 The live corrective cycle V0 must demonstrate

One real campaign, end to end, with no human relaying anything between agents:

```text
coder produces a candidate containing a real defect
-> controller seals, derives CandidateReceipt, dispatches review
-> reviewer finds the defect, returns a schema-bound ReviewerReport
-> controller accepts it as ReviewVerdict{changes_requested}
-> controller issues one CorrectiveDirective (same scope_ref digest)
-> coder fixes at a new head
-> reviewer accepts the exact new head
-> controller raises a READY_TO_MERGE HumanAttentionRequest
-> merge stays manual
```

Plus a full campaign replay after restart that reaches the same `CampaignStateV1`
— identical `state_version`, `phase`, and `current_candidate_head` — with **zero
provider invocations** (FD-8). The provider invocation count is the proof,
exactly as it was for A0.

### 5.4 Required negative tests

Happy path is not acceptance. Every row is a required test with a frozen expected
outcome.

**Encoding, bounds, identity**

| Input | Required outcome |
|---|---|
| malformed `CoderReport` / `ReviewerReport` | parse rejection, no acceptance, no dispatch |
| unsupported `envelope_version`, `message_kind_version`, or `campaign_protocol_version` | refused, never best-effort parsed (FD-1.6) |
| unknown field present | rejected at parse time (FD-1.6) |
| explicit JSON `null` in an optional field | rejected (FD-1.3) |
| payload exceeding a per-object bound | rejected, never truncated (FD-1.4) |
| `artifact_refs` whose declared sizes exceed `max_direct_referenced_bytes` | rejected before any read (FD-1.5) |
| a closure exceeding `max_reachable_closure_bytes`/`_objects` | whole resolution rejected, never partially accepted (FD-1.5) |
| a closure whose object count is inflated by repeated refs | deduplicated by `(kind, digest)`; not a rejection (FD-1.5) |
| stored object whose real size ≠ declared `size` | integrity failure, rejected (FD-1.5) |
| campaign policy budget above the protocol hard maximum | refused at campaign creation (FD-1.5) |
| duplicate `message_id`, same envelope digest | idempotent replay; `state_version` unchanged (FD-6) |
| duplicate `message_id`, different envelope digest | `IdConflict`, fail closed, attention raised (FD-6) |
| same payload bytes, different `expected_input_head` | `IdConflict`, not a replay (FD-6) |
| redelivery with a different `created_at` | replay; stored envelope and `first_observed_at` unchanged (FD-5.4) |

**Provenance**

| Input | Required outcome |
|---|---|
| provider-produced artifact with no receipt ref | rejected (§3.0) |
| controller-derived artifact carrying a receipt ref | rejected (FD-11) |
| valid receipt from a *different* execution attached to a valid report | `ReceiptIncongruent`, rejected (FD-11) |
| receipt whose `prompt_digest` / `tool_policy_digest` / `adapter_version` / `role` / `campaign_id` / `round_id` differs from the envelope | `ReceiptIncongruent`, rejected (FD-11) |
| `final_normalized_output_ref.digest` ≠ envelope `payload_digest` | `ReceiptIncongruent`, rejected (FD-11) |
| controller edits the normalized bytes before enveloping | breaks the digest equality above; rejected (FD-1.1) |
| receipt with `execution_outcome = dispatch_ambiguous` | no artifact from it accepted; attention raised (FD-9) |
| receipt with one ambiguous dispatch among completed ones | `execution_outcome` derives to `dispatch_ambiguous` (§3.12) |
| manifest referencing a `dispatch_id` absent from the receipt | rejected (§3.12.1) |
| model alias recorded as a resolved backend identity | rejected; `resolution.status` must be honest (FD-3) |
| retry attempted without established non-dispatch | refused; `dispatch_ambiguous` raised (FD-9) |
| retry that names no grain, or a continuation carrying `retry_of_dispatch_id` | refused (FD-10.3) |

**Graph and references**

| Input | Required outcome |
|---|---|
| a ref that violates the rank rule | rejected (FD-2.1) |
| `artifact_ref` outside owned CAS | inert, unresolvable, rejected (FD-1.8) |
| ref whose declared `kind` disagrees with the slot's expected kind | rejected; the slot wins (FD-2.5) |
| the same bytes referenced through two typed slots | two distinct closure nodes, both accounted (FD-2.5) |
| rank-0 bytes that happen to parse as a typed object | never parsed, never promoted (FD-2.5) |
| agent-supplied `ArtifactRef` naming a new imported authority root | refused; imports come from the registry/binding (FD-2.4) |
| imported A0 ref failing its own owner's validation | referencing artifact rejected (FD-2.4) |

**Authority and transitions**

| Input | Required outcome |
|---|---|
| `claimed_head` ≠ controller-derived candidate head | fail closed, no review dispatch (§3.3) |
| stale `reviewed_head` | no `ReviewVerdict`; report retained as evidence (FD-5.2) |
| wrong `contract_digest` | rejected (FD-5.3) |
| lineage fields inconsistent with the canonical campaign binding | rejected (FD-5.1) |
| unknown `finding_id` referenced by a directive | rejected (§3.7) |
| directive with a different `scope_ref` digest | refused, `SCOPE_EXPANSION_REFUSED` (§3.7) |
| reviewer proposes a shell command as required evidence | mapped to a registry id or `HUMAN_REQUIRED`; never executed (FD-7) |
| reviewer execution holding mutation credentials | refused before dispatch (§4) |
| gate result whose bound head is not the current candidate head | diagnostic only, no transition (FD-13) |
| transition attempted with an unsatisfied guard | `TransitionRejected`; neither counter advances (FD-14.4) |
| evidence-only event (feed item, ACK, report received) | `last_accepted_sequence` advances, `state_version` does not (FD-14.3) |
| replay of the same log twice | identical `CampaignStateV1`, zero provider calls (FD-8, FD-14.2) |

**Human lane**

| Input | Required outcome |
|---|---|
| human command with stale `expected_campaign_state_version` | rejected (FD-15.1) |
| human command with stale `expected_head` or contract digest | rejected (FD-15.1) |
| request asserting its own attestation or transport | rejected — no such field exists (FD-15.2) |
| `authenticated` recorded with no `authenticator_id` | rejected (FD-15.2) |
| loopback caller | recorded as `loopback_unauthenticated` + `claimed_identity`, never as an attested operator (FD-15.2) |
| `unattested` actor | refused (FD-15.2) |
| answer targeting a superseded `question_id` | not delivered to the coder (§3.10) |
| answer declaring `revise_contract` | campaign superseded into `HUMAN_REQUIRED`; never continued (§3.10) |
| attention action outside the server-provided set | rejected (§3.9) |
| ACK on an open attention | one `HumanDecisionRecorded` (`state_version` +1); attention lifecycle `ACKNOWLEDGED`, never `RESOLVED` (§3.9) |
| the same ACK redelivered | idempotent replay; `state_version` unchanged (FD-6) |

### 5.5 The v1-lite cut

Safe to cut — the four autonomy properties survive:

- one campaign in flight (mirrors R1's single in-flight command);
- human commands: the §3.10 v1 set only;
- no push notifications: feed + SSE, tier named honestly as **v1-lite** (timely
  only while the client is open; "operational v1" means background-capable
  delivery and is not claimed until it exists);
- merge manual, triggered by the ready-to-merge attention request;
- reviewer = same provider family, different execution surface per §4;
- reconciliation = polling only, no webhooks.

Not cuttable — object identity, not ceremony:

- exact-head binding everywhere, including human decisions;
- controller-derived candidate descriptor behind the A0 sealing boundary;
- the raw-report/accepted-artifact split;
- receipt congruence (FD-11);
- no model-supplied executable authority;
- provider execution evidence and no-recall-on-replay;
- fail-closed directive validation;
- attention and decision records as canonical events;
- forward-only corrective rounds.

## 6. Out of scope

- No architect or planner role in the core loop.
- No multi-agent negotiation, no model-to-model channel, no PR comments as agent
  input.
- No automatic merge, under any artifact defined here.
- No A5 goal-graph runtime. `root_goal_id` is identity and lineage only.
- No A2 reducer *extensions*: progress frontier, `NO_PROGRESS`, terminal taxonomy
  beyond FD-14's phases, external reconciliation, full incarnation taxonomy
  (issue #94 §3, §5). The V0 fold of FD-14 is in scope precisely because A1-V0
  cannot execute without it.
- No `ArtifactAcceptance` artifact (FD-2.3) and no `ContractRevisionProposal`
  artifact (§3.10); both require a new message kind under §7.
- No capability transport beyond registry-bound action references
  (`docs/architecture/capability-fd-transport.md` stays its own slice and gains a
  concrete consumer once A1-V0 has real actions to authorize).
- No redefinition of the accepted A0 contract.
- No constrained decoder, no Kani/TLA+ modelling commitment. Determinism levels
  D3/D4 are not prerequisites; A1 correctness depends only on D0 (deterministic
  admission) and D2 (deterministic historical replay from recorded evidence).

## 7. Supersede path (applies only after acceptance)

1. A superseding revision of this document, naming the exact decisions it
   replaces by FD number.
2. A new `message_kind_version` and media type for every artifact whose payload
   shape changes; a new `envelope_version` for envelope or rank changes; a new
   `campaign_protocol_version` for reducer-semantics changes (FD-1.6).
3. Recorded migration semantics: replay-for-verification keeps the recorded
   version's semantics; replay-for-resume is where version gates bite (#94 §5).
4. No in-place mutation of an accepted artifact, ever. Correction is forward-only
   at this level too.

An in-flight campaign is never migrated mid-flight: it is superseded, with the
supersedes relation recorded (FD-5.1).

## 8. Order after this freeze

```text
A1-F   this contract, reviewed and accepted        <- pending
A1-V0  the 11 kinds + receipt + FD-14 fold + one real corrective cycle (§5)
SB-B   capability transport, with A1 actions as its concrete consumer
A2     reducer extensions: progress frontier, NO_PROGRESS, terminal taxonomy,
       reconciliation, full incarnation taxonomy
```

`research/b1-context/` continues in parallel and remains read-only and
non-authoritative: it must not drive an A-series transition.

## 9. Revision history

### R1 — first corrective round (review of `144ebf6`)

Six freeze-blocking findings, all accepted and corrected forward:

1. **Receipt collapsed the grain split it declared.** A single receipt carrying
   one `dispatch_id` beside a whole-execution manifest could not say which grain
   it represented, and a multi-dispatch tool loop had no correct single receipt
   to reference. Replaced by an execution-level `ProviderExecutionReceiptV1` with
   nested dispatch records, a frozen `execution_outcome` derivation, and
   per-dispatch boundary classification (FD-10.1, FD-10.2, §3.12).
2. **Presence of a receipt proved nothing.** Added FD-11: nine congruence
   equalities between envelope and receipt, including `payload_digest ==
   final_normalized_output_ref.digest`, which is only checkable because FD-1.1
   now freezes that a provider-produced payload *is* the normalized output bytes.
   A valid receipt from an unrelated execution is now `ReceiptIncongruent`.
3. **"Same shape minus some fields" is not a wire schema.** §3 now carries
   complete field tables for all eleven message kinds plus the receipt, manifest,
   `ScopeContractV1`, and `CampaignStateV1` — type, required, constraints, and
   authority per field, with one global null policy (FD-1.3).
   `ContractRevisionProposal` was removed from the semantics rather than left
   dangling: `revise_contract` supersedes the campaign (§3.10).
4. **A1-V0 required the reducer A2 was going to write later.** FD-14 freezes
   `CampaignStateV1`, a pure fold, the authority-bearing/evidence-only event
   split, and the `state_version` rule (+1 per authority-bearing event only,
   with `last_accepted_sequence` tracking the log separately). A2 keeps the
   extensions; V0 gets the minimum it cannot run without, and
   `expected_campaign_state_version` finally has an origin.
5. **`loopback_local_operator` overclaimed.** Loopback proves transport, not
   humanity, identity, or an uncompromised local process. Split into a claim on
   the request (`claimed_actor_identity`) and controller observations on the
   decision (`authentication_strength`, `observed_transport`, `authenticator_id`),
   with no field in the request able to assert any of them (FD-15.2).
6. **Bounds were per-object only.** 256 refs × 64 MiB is 16 GiB before the DAG
   even branches. Added FD-1.5: protocol hard maxima, a campaign policy budget
   that `budget_policy_digest` actually commits to, and a frozen closure
   traversal that deduplicates by `(kind, digest)`, accounts declared sizes
   before reading, and rejects all-or-nothing.

Point corrections in the same round: A0 and other foreign references are now
**imported authority roots** at virtual rank 0 with resolver duties and an
anti-promotion rule (FD-2.4, FD-2.5); FD-3's rank ladder was corrected (normalized
output is rank 0, the report rank 3, the verdict rank 4); scope equality moved
from an unmeetable "byte-identical" comparison to one immutable
`ScopeContractV1` compared by digest (§3.13); `created_at` gained a
`first_observed_at` companion and an explicit redelivery rule (FD-5.4); the
`ArtifactAcceptance` question is answered honestly — absent in V1, and adding it
requires superseding FD-2 rather than an additive rank; and the document status
was corrected to `PROPOSED FREEZE / REVIEW REQUIRED / NON-AUTHORITATIVE` until a
review actually accepts it.
