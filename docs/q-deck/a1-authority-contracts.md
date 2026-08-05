# Q-Deck A1: coder/reviewer/human authority contracts

## Status

**FROZEN — contract-first.** This document is the normative source for the A1
slice named in `docs/autonomy-controller.md` ("A1 ReviewVerdict and
CorrectiveDirective contracts"). It freezes the artifact set, the digest and
identity rules, and the authority direction *before* any A1 implementation
exists, following the A0 precedent (`docs/q-deck/a0-candidate-state.md`, frozen
by the first commit of PR #92 and only then implemented).

Design input: issue #95 (`A1: coder/reviewer/human contracts (draft)`), which
carries the rationale and the discussion history. Where this document and that
draft differ, **this document is authoritative** — the draft names the gaps, the
freeze resolves them.

After this freeze, changes follow the supersede path in §7. Implementation
(A1-V0, §5) begins only after the freeze is accepted.

What "frozen" means here, precisely: the field names, their required/optional
status, the digest and identity rules, and the fail-closed outcomes are fixed.
Wire encodings for types the freeze marks `opaque` (id string forms, reason
codes beyond the frozen set) may be extended additively under the version rules
of FD-1.

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

Every rule below exists to keep those three apart while still allowing one
autonomous corrective loop to run unattended.

## 0. What this contract binds to and may not redefine

| Bound contract | Authoritative artifact | What A1 consumes, never redefines |
|---|---|---|
| A0 candidate state | `docs/q-deck/a0-candidate-state.md` (accepted head `52627c3`, merged `f1ac458`) | `CandidateRef` representation, base-commit semantics, the one-cumulative-patch model, `RepositoryIdentity`, materialization attestation, sealing/materialization ordering |
| R1 command vertical | `docs/q-deck/r1-command.md` §11.1–§11.2 | the durable dispatch boundary, `ValidUnsealedPreDispatch` vs `ValidUnsealedDispatchAmbiguous`, the four `DispatchProgress` values, fail-closed post-dispatch ambiguity |
| Controller architecture | `docs/autonomy-controller.md` (accepted `c5b3ae0b`, PR #93) | campaign phases, the transition-authority principle, the `ReviewVerdict` minimum |
| Admission protocol | `docs/decision-and-admission-protocol.md` §5, §6, §7 | diagnostic vs admission evidence, exact-head review, head drift invalidates acceptance |
| Canonical digest discipline | `crates/o7-run/src/event.rs` (module docs, `RunEvent::compute_digest`, `frame`) | digests by explicit length-prefixed field framing, never by hashing a serialized JSON blob |
| Provider call primitive | `docs/o7-invoke.md` | engine set, capability profile `read-only-data`, the `PASS`/`FAIL_*`/`BLOCKED_*` status vocabulary, key handling |

A1 adds no new claim about any of these. In particular it does **not**
strengthen R1's delivery semantics into an exactly-once claim, and it does not
weaken the A0 sealing boundary into "the process exited".

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
Controller:               HumanDecision        (attested, accepted)
```

GitHub PR comments are a human-readable **projection** of these objects. No
agent consumes a mutable PR comment as authoritative input; a projection is
never an input channel.

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

At-most-once applies to **provider dispatch** in the supported single-host
model, not to message delivery. A1 makes no exactly-once claim about anything.

## 2. The frozen decisions

### FD-1 — canonical bytes, digests, limits, unknown fields, versions

**FD-1.1 The artifact model.** An A1 artifact is a pair:

- a **payload** — the typed body of one `message_kind`, stored as its own
  immutable byte string in 007-owned content-addressed storage;
- an **envelope** — the record that identifies, binds, and references it.

`payload_digest` is SHA-256 over *those exact stored payload bytes*, lowercase
hex, 64 chars — the existing `o7_run::event::Digest256::of_bytes` form. The
envelope's own identity is its framed digest (FD-1.2), which commits to
`payload_digest`. No reader ever re-serializes anything to verify an identity.

**FD-1.2 Envelope digests are computed by field framing, not by JSON.** The
envelope digest follows the precedent frozen in `crates/o7-run/src/event.rs`
(module docs: "Digests are computed by explicit field FRAMING (length-prefixed),
not by hashing a serialized JSON blob, so they are byte-stable regardless of map
ordering or serializer whitespace"). The A1 envelope digest is:

```text
H = SHA-256
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
frame(provider_invocation_receipt_ref digest or the empty string when absent)
frame(artifact_refs count.to_le_bytes()), then for each ref in stored order:
    frame(kind tag byte) frame(media_type) frame(digest) frame(size.to_le_bytes())
```

where `frame(x)` is `u64-le length prefix || bytes`, identical to
`crates/o7-run/src/event.rs:906`. Absent optional fields are framed as the empty
string; they are never omitted from the framing, so "absent" and "empty" hash
distinctly from "different value" and the field order is fixed forever.

`created_at` is deliberately **excluded** from the framing. It is an observation,
not identity (FD-5), and excluding it is what makes a redelivered artifact
carrying a fresh observation timestamp an idempotent replay rather than a
digest conflict (FD-6).

**No canonical-JSON scheme is introduced.** Payload identity is the digest of
stored bytes; envelope identity is field framing. Neither requires two
serializers to agree on whitespace or key order, so the whole class of
"semantically identical, digest-unequal" failures disappears without anyone
inventing a normalization standard.

The corollary is equally deliberate: a payload re-serialized with different
whitespace is a *different* payload. Bytes are the artifact, and an artifact is
never rewritten in place (§7).

**FD-1.3 Encoding.** Stored artifact bytes are UTF-8 JSON. Invalid UTF-8, a
leading BOM, and non-object top-level values are rejected at ingest. **No Unicode
normalization is performed** — content is never rewritten, and a digest is never
computed over a normalized copy of something that was stored differently.

**FD-1.4 Limits (frozen constants, not configuration).**

```text
typed control artifact (any A1 artifact body)   <=   1 MiB
referenced evidence blob (diff, raw provider
  event stream, gate log, patch)                <=  64 MiB
JSON nesting depth                              <=  32
array length, any array                         <= 4096
string length, any single string field          <= 65536 bytes
artifact_refs per envelope                      <=  256
interaction_sequence entries per manifest       <= 4096
findings per ReviewerReport / ReviewVerdict     <=   256
```

Exceeding any limit is a parse-time rejection, never a truncation. A truncated
artifact that still parses is the failure mode these bounds exist to forbid.

**FD-1.5 Unknown fields are rejected.** Every A1 artifact type deserializes with
`#[serde(deny_unknown_fields)]`, and every enum is closed — no
`#[serde(other)]` catch-all variant. Both follow the A0 precedent:
`CandidateStateReceiptV1` (`crates/o7-run/src/candidate.rs:78`) and
`CandidatePatchKind` (`crates/o7-run/src/event.rs:363–371`, whose doc comment
states the rule outright: "an unrecognized/future value on the wire fails closed
at deserialization … there is deliberately no `#[serde(other)]` catch-all"). An
artifact carrying an extra field is a foreign or corrupted record and is refused
at parse time.

**FD-1.6 Versions.** Two independent version fields:

- `envelope_version: u32` — the envelope framing and field set. Frozen v1 = `1`.
- `message_kind_version: u32` — the payload schema of one `message_kind`. Every
  kind frozen here is at version `1`.

An unrecognized value of either is refused; the artifact is never parsed "as
well as we can". This mirrors `RUN_EVENT_SCHEMA_VERSION`'s rule in
`crates/o7-run/src/event.rs:25`: "A reader that encounters a different version
must refuse to replay it as if it understood it."

Additive evolution mints a new `message_kind_version` and a new media type. A
campaign is bound to one `(envelope_version, message_kind_version)` set at
creation; a mid-campaign version change is a supersede (§7), never an in-place
upgrade.

**FD-1.7 Media types.** `application/vnd.o7.a1.<kind>+json; v=<version>` for
typed artifacts; evidence blobs carry their own concrete type
(`application/json`, `text/x-diff`, `application/octet-stream`). Media type is
part of every `ArtifactRef` and part of the envelope digest framing: the same
bytes under a different declared type are a different reference.

**FD-1.8 Artifact refs.** `(kind, media_type, digest, size)` into 007-owned CAS
only. There is no path field and no URL field to populate. An agent-supplied
path or URL appearing anywhere in a payload is inert text and is never
dereferenced (FD-7).

### FD-2 — the evidence graph is acyclic by construction

Authority flows one way, and content-addressed references must flow the same
way. Instead of a runtime cycle check, A1 freezes a **rank rule**:

| Rank | Class | May reference |
|---|---|---|
| 0 | opaque evidence bytes: canonical provider request, raw provider event stream, adapter-normalized output, usage/cost records, diffs, patches, gate logs, contract documents | nothing |
| 1 | interaction manifest (§3.10) | rank 0 |
| 2 | `ProviderInvocationReceipt` | rank ≤ 1 |
| 3 | untrusted reports: `CoderReport`, `ReviewerReport`, `HumanCommandRequest` | rank ≤ 2 |
| 4 | controller-accepted artifacts: `CandidateReceipt`, `ReviewVerdict`, `HumanDecision` | rank ≤ 3 |
| 5 | controller-issued instructions and notices: `WorkOrder`, `ReviewRequest`, `CorrectiveDirective`, `HumanAttentionRequest`, `CampaignFeedItem` | rank ≤ 4 |

**A content-addressed reference may only target a strictly lower rank.** Rank
monotonicity implies acyclicity; no cycle detector is needed, and the property
is checkable on a single artifact in isolation.

**Lineage and causation are identifiers, not digests.** `causation_id`,
`correlation_id`, `campaign_id`, `round_id`, `question_id`, `attention_id`, and
`finding_id` are opaque ids and are therefore *not edges in this graph*. This is
what lets a `CoderReport` (rank 3) be caused by a `WorkOrder` (rank 5) without
inverting the reference direction.

Consequences, frozen:

- No acceptance pointer lives inside an immutable evidence object. A
  `ProviderInvocationReceipt` never carries `accepted_artifact_ref`; the forward
  directions are `CandidateReceipt.coder_report_ref` and
  `ReviewVerdict.reviewer_report_ref`.
- Back-links required by projections, indexes, or a UI live in projections.
  A projection may compute any inverse it likes; it never becomes canonical.

### FD-3 — raw provider evidence is separate from adapter-normalized output

Three distinct artifacts with three distinct digests, for every provider
dispatch that produced anything at all:

```text
canonical_request_ref        the exact provider-facing request after adapter
                             construction — not the logical WorkOrder
raw_provider_event_stream_ref  provider bytes/events as received (when capturable)
normalized_output_ref        adapter-normalized payload, PRE-envelope bytes
```

`normalized_output_ref` is the payload *before* an A1 envelope is attached. That
keeps rank 0 genuinely inert (FD-2) and stops the envelope from containing a
digest of something that contains the envelope.

**A 2xx response is not an accepted artifact.** `o7 invoke`'s own `PASS` status
(`docs/o7-invoke.md`, "Response handling") means: HTTP 2xx, content parsed,
schema-valid. Under A1 that is rank-3 raw report material and nothing more. The
transition from `PASS` to a `CoderReport`, and from a `CoderReport` to a
`CandidateReceipt`, are two further, separately recorded acts of the controller.

Where the provider or adapter cannot establish a fact, it is recorded as
unavailable under an explicit status. A requested model alias is never recorded
as a resolved backend identity.

### FD-4 — untrusted report vs controller-accepted artifact

Acceptance is an act of the controller, recorded as a canonical event, with the
accepted artifact referencing the raw report forward (FD-2). The report's own
`status`/`verdict` field is an input to validation, never its outcome:

- A `CoderReport` may say `candidate_produced`. That claim is checked against
  the controller-derived candidate (FD-5); a mismatch fails closed.
- A `ReviewerReport` may say `accepted`. Its JSON authorizes nothing. Only a
  controller-issued `ReviewVerdict` enables a transition (FD-11).
- A `HumanCommandRequest` may say anything. Only a `HumanDecision` is authority.

### FD-5 — authority direction: exact head, contract, lineage

**Lineage.** The controller never adopts lineage from an incoming envelope. It
resolves the expected `(root_goal_id, task_id, campaign_id, round_id)` from the
causation target and the canonical campaign binding, then verifies the carried
fields match. Mismatch fails closed. Comparing one incoming envelope against
another incoming envelope is not verification.

The controller mints and durably binds `root_goal_id`, `task_id`, and
`campaign_id` atomically **before any agent dispatch** — the natural extension of
R1's "durable acceptance before provider invocation". A campaign without a
complete lineage binding cannot exist, so no later backfill or heuristic linking
is ever required.

v1 active topology: exactly one root goal, one task under it, at most one active
campaign executing that task. A replacement execution mints a new `campaign_id`
and records `supersedes` against the prior terminal campaign; it never mutates
or reuses it.

**Head.** Every artifact that reasons about code carries `expected_input_head`,
and the controller verifies it against the current `CandidateReceipt.candidate_head`
at the moment of use. A `ReviewerReport` whose `reviewed_head` is not the current
candidate head is stale: it is retained as evidence and never becomes a
`ReviewVerdict`.

**Contract.** `contract_digest` on an incoming artifact is verified against the
campaign's bound contract digest. A verdict against a different contract digest
is a verdict on a different task.

**Ordering.** `created_at` is an observation, never an ordering primitive. Order
is the canonical append sequence plus the causation graph — the rule already
frozen for run events (`crates/o7-run/src/event.rs:622`: "Metadata only — NEVER
the ordering key").

### FD-6 — duplicate identity: replay vs conflict

```text
same message_id, same envelope digest    -> idempotent replay:
                                            return the existing accepted artifact,
                                            perform no new side effect,
                                            dispatch nothing
same message_id, other envelope digest   -> IdConflict: fail closed,
                                            no acceptance, no dispatch,
                                            HumanAttentionRequest raised
```

The comparison is on the **envelope digest** (FD-1.2), not on `payload_digest`
alone: the envelope digest already commits to `payload_digest`, every
`artifact_ref`, the lineage fields, and the receipt reference, so two artifacts
that agree on their payload bytes but disagree on which candidate head, which
contract, or which provider invocation they belong to are a conflict, not a
replay.

Identity scope: `message_id` is unique within a `campaign_id`. `idempotency_key`
on a `HumanCommandRequest` is unique within a `campaign_id`, and reuse with a
different request body is the same `IdConflict` — matching R1's existing `409`
for "idempotency key reused with a different request"
(`docs/q-deck/r1-command.md` §8).

An `IdConflict` never resolves itself by minting a new id. Two different payloads
claiming one identity means the system's view of who said what is broken, and
that is a human-facing fact.

### FD-7 — no model-supplied executable authority

Forbidden, spelled out because otherwise someone writes it:

```text
reviewer.required_regression_evidence = "run this shell command"
controller: shell -c reviewer_text            # NEVER
```

Frozen rules:

- Required evidence is named by `required_evidence_gate_ids`, resolved against a
  controller-owned gate/verifier registry, plus a `verifier_policy_digest`. An
  unknown id fails closed.
- A reviewer may *propose* a new kind of evidence in prose. The controller maps
  it to a known registry id or raises `HUMAN_REQUIRED`. It never executes a
  model-authored string.
- `CoderReport.diagnostic_runs[].command_recorded` is forensic text. The
  controller never re-executes it. Per
  `docs/decision-and-admission-protocol.md` §5 it is diagnostic evidence, not
  admission evidence: the `GATING -> CI_WAIT` transition is satisfied only by
  controller-owned gate/CI runs against the receipt head.
- Every `artifact_ref` resolves inside 007-owned CAS only. Agent-composed paths
  and URLs are inert text.
- A recorded `tool_call_requested` interaction (§3.10) is an observation, not an
  authorization. Only controller-owned registry resolution, policy validation,
  and execution produce an accepted `tool_result` interaction.

### FD-8 — replay never invokes a provider

Reducer replay, campaign replay, reconciliation, recovery, and historical
verification reconstruct results from immutable recorded evidence. None of them
may call a provider to rebuild an earlier answer.

This generalizes the property `o7 replay` already has for a single run record
(`src/main.rs:388` → `events::replay_record`: chain continuity, per-event
digests, artifact content digests, verdict recomputation — no provider call on
that path) to the campaign level.

A new provider call is always a *new* canonical invocation with a fresh identity.
It never silently completes or replaces the history of an earlier one.

### FD-9 — post-dispatch ambiguity fails closed

R1 froze the durable dispatch boundary and its classification
(`docs/q-deck/r1-command.md` §11.1–§11.2). A1 generalizes it from command
continuation to **every model role, including read-only ones**:

```text
dispatch boundary not reached (established)  -> safe redrive, fresh dispatch_id,
                                                explicit retry_of (FD-10)
dispatch occurred or may have occurred,
  outcome unknown                            -> dispatch_ambiguous:
                                                no redrive, no completion,
                                                no rejection, no mutation;
                                                HumanAttentionRequest raised
```

A fresh identifier does not make a duplicate side effect safe. A read-only
reviewer invocation is still an external side effect: repeating it can produce a
different verdict, which rewrites campaign history exactly as effectively as
repeating a coder invocation rewrites code.

Retry is permitted **only** when non-dispatch is established by the existing
protocol. The retry receives a fresh identity and an explicit `retry_of`
relation naming its grain (FD-10).

### FD-10 — provider invocation identity grains

Issue #95 recorded this as the dangerous open question; the freeze resolves the
minimum. Two grains, both mandatory from A1 onward:

```text
provider_execution_id   one bounded role execution (one coder execution or one
                        reviewer execution), spanning its whole tool loop and
                        every continuation dispatch inside it
dispatch_id             one external provider request (one HTTPS request, or
                        one CLI process invocation)
```

Frozen bindings:

- Every `dispatch_id` belongs to exactly one `provider_execution_id`. The
  relation is recorded when the dispatch is minted, never inferred later.
- **The dispatch boundary and ambiguity classification of FD-9 apply per
  `dispatch_id`.** An execution containing one ambiguous dispatch is an
  ambiguous execution; ambiguity never collapses into "the execution retried
  fine".
- The interaction manifest groups by `provider_execution_id`; every entry names
  its `dispatch_id`.
- `retry_of` must name its grain explicitly:

```yaml
retry_of:
  grain: execution | dispatch
  id: ...
```

- Four distinct things that must never share one word:

```text
whole-execution retry     new provider_execution_id, retry_of.grain = execution
single-dispatch retry     new dispatch_id, same execution,
                          retry_of.grain = dispatch
tool-loop continuation    new dispatch_id, same execution, NOT a retry,
                          no retry_of
new session               new provider_execution_id, no retry_of;
                          a fresh conversation, not a repetition
```

The full incarnation taxonomy (run/attempt/session/campaign incarnations and
their relations) remains deferred to A2. What A1 freezes is that no A1 recovery
or retry code may be written without naming which of the two grains it operates
on.

`producer_execution_id` in the envelope (§3.0) is the `provider_execution_id` of
the execution that produced the artifact, or the controller's own execution
identity for controller-derived artifacts.

### FD-11 — transition authority

Extends the table in `docs/autonomy-controller.md` ("Transition authority") with
the A1 objects. Each row names the **single canonical artifact** that authorizes
the transition. A1 freezes the requirement; the reducer that enforces it is A2.

| Transition | Authorizing canonical artifact | Never sufficient |
|---|---|---|
| campaign start → `BUILDING` | `WorkOrder` accepted, lineage bound before dispatch | a task description alone |
| `BUILDING` → `GATING` | `CandidateReceipt` (controller-derived behind the A0 seal), `claim_check.claimed_head_matches = true` | `CoderReport.status = candidate_produced` |
| `GATING` → `CI_WAIT` | controller-owned gate results bound to `candidate_head` | `CoderReport.diagnostic_runs` |
| `CI_WAIT` → `REVIEWING` | required CI results bound to the same exact head | a green workflow on another head |
| `REVIEWING` → `CORRECTING` | `ReviewVerdict.verdict = changes_requested` at the current head | `ReviewerReport` saying so |
| `REVIEWING` → `READY_TO_MERGE` | `ReviewVerdict.verdict = accepted`, `reviewed_head == candidate_head`, no drift, required gates green | `ReviewerReport.verdict = accepted` |
| `CORRECTING` → `BUILDING` | `CorrectiveDirective` derived from an accepted `ReviewVerdict`, scope unchanged | reviewer prose |
| any → `HUMAN_REQUIRED` | `HumanAttentionRequest` (controller-raised) | an agent asking for a human |
| `HUMAN_REQUIRED` → resumed | `HumanDecision` bound to the exact head, contract digest, and campaign state version the human saw | an acknowledged alert |
| any → `CANCELLED` | `HumanDecision` (CANCEL) followed by the observed cancellation sequence (§3.9) | a UI flag |
| merge | **not authorized by A1.** Merge stays manual, outside the system | any artifact in this document |

A later event never retroactively makes an earlier unsafe transition valid. If
the candidate head changes after review, the review is stale and the campaign
returns to the appropriate verification state
(`docs/decision-and-admission-protocol.md` §7).

### FD-12 — human command binding and actor attestation

Every `HumanCommandRequest` carries the binding fields in §3.9 and is rejected
if any of `expected_head`, `expected_contract_digest`, or
`expected_campaign_state_version` does not match current canonical state. This
closes the stale-screen TOCTOU window: a decision applies to what the human
actually saw, or it does not apply.

On authentication, the honest position: `o7d` has **no authentication story**
today — it binds loopback by default and refuses a non-loopback bind without an
explicit `--allow-non-loopback` flag (`crates/o7d/src/main.rs:24–92`). A1
therefore freezes *recording*, not a mechanism it does not have:

```yaml
actor:
  identity: ...                  # opaque, recorded verbatim
  attestation: loopback_local_operator | authenticated | unattested
```

- `loopback_local_operator` is admissible in v1 **only** when the request
  arrived over a loopback bind. It is recorded on the `HumanDecision`, so every
  accepted decision states the strength of the identity behind it.
- A non-loopback deployment must supply `authenticated`; a command arriving with
  `unattested` is refused.
- `attestation` is never inferred from the payload — it is determined by the
  controller from the transport it actually observed.

## 3. The frozen artifact set

Eleven envelope-bearing message kinds, plus the `ProviderInvocationReceipt`
(§3.10), which is rank-2 evidence and carries no envelope of its own — it is
referenced *by* envelopes, and giving it one would invert the reference
direction (FD-2).

### 3.0 Common envelope (v1)

```yaml
envelope_version: 1
message_kind: work_order | coder_report | candidate_receipt
            | review_request | reviewer_report | review_verdict
            | corrective_directive | human_attention_request
            | campaign_feed_item | human_command_request | human_decision
message_kind_version: 1

message_id: ...                 # unique within campaign_id (FD-6)
root_goal_id: ...               # mandatory (FD-5)
task_id: ...
campaign_id: ...
round_id: ...
causation_id: ...               # the message_id this artifact answers
correlation_id: ...

producer_role: controller | coder | reviewer | human
producer_execution_id: ...      # FD-10
producer_adapter_version: ...
model_identity: ...             # controller-normalized identity for routing,
                                # policy, and logical provenance — NOT runtime
                                # evidence; runtime facts live in the receipt
prompt_digest: ...
tool_policy_digest: ...

contract_digest: ...
expected_input_head: ...
payload_digest: ...             # digest of the exact stored payload bytes
artifact_refs: [...]            # (kind, media_type, digest, size), CAS only
provider_invocation_receipt_ref: ...   # mandatory iff produced by a provider
                                       # invocation; absent for controller-
                                       # derived artifacts
created_at: ...                 # metadata only (FD-5, ordering rule)
```

Validation, in this order, all fail-closed: version support (FD-1.6) → limits
(FD-1.4) → unknown fields (FD-1.5) → `payload_digest` matches stored bytes →
lineage resolution and match (FD-5) → rank rule on every ref (FD-2) → refs
resolvable in owned CAS → receipt presence rule → head and contract binding.

### 3.1 WorkOrder (Controller → Coder)

```yaml
role: coder
goal:
  contract_digest: ...
  summary: "..."
input:
  base_sha: ...
  input_candidate_ref: ...                # A0 CandidateRef
  materialization_attestation_ref: ...    # A0 / o7-worktree attestation
scope:
  allowed_paths: [...]
  forbidden_paths: [...]
  frozen_properties: [...]
required_evidence:
  gate_ids: [...]                         # registry ids only (FD-7)
  acceptance_case_ids: [...]
budget:
  max_provider_turns: ...
  max_wall_time_seconds: ...
```

The coder never receives "address the review comments". It receives the frozen
contract identity, concrete findings via a `CorrectiveDirective`, and explicit
scope limits.

### 3.2 CoderReport (Coder → Controller, untrusted, rank 3)

```yaml
status: candidate_produced | failed | question_blocked
claimed_head: ...
claimed_state_digest: ...
change_summary: ...
intent: ...
claims:
  - claim_id: ...
    statement: ...
    evidence_refs: [...]
diagnostic_runs:
  - command_recorded: ...        # forensic text, never re-executed (FD-7)
    result: passed | failed | unknown
    artifact_ref: ...
known_residuals: [...]
questions:
  - question_id: ...
    text: ...
```

Everything here is advisory. There is no field by which a coder can emit
`accepted`, and no field the controller treats as admission evidence.

### 3.3 CandidateReceipt (controller-derived, rank 4)

Derived **behind the A0 sealing boundary**, which A1 consumes as a capability
and does not redefine:

```text
seal_candidate(worktree_attestation, producer_execution) -> CandidateRef
```

Quiescence means no live holder of write authority remains — a durably revoked
or advanced write-capability/lease epoch under which no previously issued writer
capability remains valid. Process termination and descendant absence are proof
mechanisms, not the definition (`docs/q-deck/a0-candidate-state.md`).

The **entire descriptor** is controller-derived, not just the head:

```yaml
candidate_head: ...
candidate_tree_identity: ...
base_ancestry: ...
repository_identity: ...          # A0 RepositoryIdentity
changed_paths: [...]              # controller-observed
file_modes: [...]
diff_scope: ...
admission_profile: LIGHTWEIGHT | STANDARD | STRICT | CRITICAL
applicable_gate_ids: [...]
coder_report_ref: ...             # forward reference (FD-2)
claim_check:
  claimed_head_matches: true | false
```

`admission_profile` is classified from **controller-observed** changed paths (per
issue #94 §4), never from coder claims, and classification ambiguity fails closed
to the stricter tier. Autonomous code mutation is at least `STRICT`.

```text
coder says:      docs only
controller sees: verifier changed
-> CRITICAL or HUMAN_REQUIRED
```

`claimed_head_matches: false` → fail closed, no review dispatch.

### 3.4 ReviewRequest (Controller → Reviewer)

```yaml
candidate_head: ...              # from CandidateReceipt, never from CoderReport
base_sha: ...
contract_digest: ...
required_properties: [...]
evidence_refs: [...]             # contract, diff, deterministic evidence FIRST
coder_report_ref: ...            # optional, delivered LAST, marked advisory
```

Input ordering is normative, not stylistic: contract, diff, and deterministic
evidence first; the coder's narrative last and explicitly labeled as advisory
claims.

### 3.5 ReviewerReport (Reviewer → Controller, untrusted, rank 3)

Same payload shape as the verdict below minus every controller-owned field
(`review_id`, acceptance). It may contain `accepted`; its JSON authorizes
nothing.

### 3.6 ReviewVerdict (controller-accepted, rank 4)

Accepted only when **all** hold:

```text
ReviewerReport schema-valid, supported version
reviewed_head == current CandidateReceipt.candidate_head
contract_digest == the campaign's bound contract digest
reviewer identity / prompt_digest / tool_policy_digest established
every finding_id unique within the report
every required_evidence_gate_id resolves in the registry (FD-7)
every evidence_ref resolvable in owned CAS, rank rule satisfied (FD-2)
provider_invocation_receipt_ref present and resolvable (FD-3)
```

```yaml
review_id: ...
reviewed_head: ...
verdict: accepted | changes_requested | blocked
findings:
  - finding_id: ...
    severity: blocker | major | minor | note
    affected_property: ...
    evidence_refs: [...]
    required_change: ...
    required_evidence_gate_ids: [...]     # registry ids only
properties_checked: [...]
properties_preserved: [...]
residual_risks: [...]
reviewer:
  identity: ...
  model: ...
  prompt_version: ...
reviewer_report_ref: ...
```

### 3.7 CorrectiveDirective (Controller → Coder)

```yaml
derived_from_review_id: ...
target_findings: [...]           # finding_ids from the accepted verdict only
required_changes: [...]
required_evidence_gate_ids: [...]
scope:                           # byte-identical to the WorkOrder's scope
  allowed_paths: [...]
  forbidden_paths: [...]
  frozen_properties: [...]
contract_digest: ...             # unchanged from the campaign binding
```

A directive that would widen `scope` or change `contract_digest` is not a
directive: it is a contract revision, and it fails closed into
`HUMAN_REQUIRED`. `changes_requested` is never permission to modify the frozen
goal, acceptance contract, verifier, or baseline.

Rounds are **forward-only**. A corrective round produces a new candidate head; it
never amends, rebases, or force-pushes an earlier one
(`docs/decision-and-admission-protocol.md` §8).

### 3.8 CampaignFeedItem and HumanAttentionRequest

Two objects, not one alert table:

```text
CampaignFeedItem        informative lifecycle history; no ack, no decision
HumanAttentionRequest   requires acknowledgement and/or a decision
```

```yaml
attention_id: ...
campaign_id: ...
candidate_head: ...
reason:
  code: HUMAN_REQUIRED | READY_TO_MERGE | EXTERNAL_DRIFT | ID_CONFLICT
      | DISPATCH_AMBIGUOUS | AGENT_FAILED | NO_PROGRESS
      | CONFLICTING_EVIDENCE | BUDGET_EXHAUSTED | CI_FAILED_REPEATEDLY
      | SCOPE_EXPANSION_REFUSED | UNMAPPED_EVIDENCE_PROPOSAL
  summary: ...
severity: info | attention | urgent
required_decision_kind: none | ack | choose_resolution
options:                          # server-defined; the client never invents one
  - action_id: ...
    consequence: "..."
evidence_refs: [...]
lifecycle: OPEN | ACKNOWLEDGED | RESOLVED | SUPERSEDED
```

`dedupe_key` is computed by the **controller**, never by an agent. Repeated
reconciliation updates the one durable attention record; occurrence counts live
in projection. A new head or a new problem class creates a new attention
identity. **ACK ≠ RESOLVED.**

An attention request answers: what happened, why the system stopped, which exact
head is affected, which actions are permitted, and what each action causes.

### 3.9 HumanCommandRequest (untrusted, rank 3) and HumanDecision (accepted, rank 4)

```yaml
command_id: ...
idempotency_key: ...
actor:
  identity: ...
  attestation: loopback_local_operator | authenticated | unattested   # FD-12
authorization_context: ...
campaign_id: ...
expected_campaign_state_version: ...
expected_contract_digest: ...
expected_head: ...                 # where applicable
attention_id | question_id: ...    # where applicable
```

v1 command set — frozen:

```text
ACK
CANCEL
ANSWER_QUESTION
SELECT_ATTENTION_ACTION
```

`SELECT_ATTENTION_ACTION` is the one safe generic intervention: the controller
publishes permitted `action_id`s per attention request; the client selects a
server-provided id and never composes one. `accept_residual_risk` exists in the
vocabulary and is **not offered in v1**. Deferred post-v1: `PAUSE`, `RESUME`,
`REQUEST_MORE_EVIDENCE` as a standalone command, `APPROVE_RESIDUAL_RISK`,
`REJECT_CANDIDATE`, `AUTHORIZE_MERGE`.

**ANSWER_QUESTION.** Free-form human text is not deterministically classifiable,
and an LLM classifier is not authority either, so the human declares the effect:

```yaml
question_id: ...
contract_digest: ...
round_id: ...
expected_head: ...
answer:
  text: ...
  declared_scope_effect: none | revise_contract
```

```text
declared none + controller finds no scope change -> delivered as clarification
declared revise_contract                         -> ContractRevisionProposal
ambiguous                                        -> HUMAN_REQUIRED, explicit re-ask
```

An answer targeting a superseded `question_id` is never delivered to the coder.
`revise_contract` never silently continues the same campaign: the old contract is
superseded, a new contract version is minted, and revalidation follows.

**CANCEL is a process, not a flag:**

```text
CancelRequested
-> prevent new dispatches
-> request termination of the active execution
-> observe process/worktree state
-> cleanup or preserve forensic state
-> CampaignCancelled
```

A UI showing "cancelled" while the coder is still writing files is a lovely
interface to the wrong reality.

### 3.10 ProviderInvocationReceipt v1 (rank 2) and the interaction manifest

```yaml
schema_version: 1

provider_execution_id: ...            # FD-10
dispatch_id: ...                      # FD-10
retry_of:                             # null, or an explicit grain
  grain: execution | dispatch
  id: ...

producer_role: coder | reviewer
producer_adapter_version: ...

provider:
  identity: ...
  endpoint_mode: ...
  request_id: null | ...

model:
  requested_model: ...
  resolution:
    status: provider_reported | fingerprint_only | unavailable
    provider_reported_model: null | ...
    backend_fingerprint: null | ...

request:
  canonical_request_ref: ...          # exact provider-facing request (FD-3)
  prompt_digest: ...
  tool_policy_digest: ...
  decoding_policy_digest: ...
  budget_policy_digest: ...

capture:
  status: exact_provider_events | adapter_observations | normalized_output_only
  interaction_manifest_ref: ...
  raw_provider_event_stream_ref: null | ...
  normalized_output_ref: null | ...   # PRE-envelope bytes (FD-3)

outcome:
  state: completed | refused | incomplete
       | failed_pre_dispatch | dispatch_ambiguous
  stop_reason: null | ...
  provider_error_code: null | ...     # e.g. o7-invoke's BLOCKED_*/FAIL_* kind
  usage_ref: null | ...
  cost_ref: null | ...
```

The receipt carries **no** reference to any artifact accepted from it (FD-2).

Interaction manifest — one bounded execution's observable route:

```yaml
provider_execution_id: ...
interaction_sequence:
  - sequence: 0
    dispatch_id: ...
    kind: provider_request
    input_ref: ...
    output_ref: ...
  - sequence: 1
    dispatch_id: ...
    kind: tool_call_requested
    tool_call_id: ...
    tool_id: ...
    arguments_ref: ...
  - sequence: 2
    kind: tool_result                 # controller-executed only (FD-7)
    tool_call_id: ...
    result_ref: ...
  - sequence: 3
    dispatch_id: ...
    kind: provider_continuation
    input_ref: ...
    output_ref: ...
```

The manifest records externally observable requests, responses, tool calls, tool
results, errors, and ordering. It claims no access to hidden provider reasoning.
Partial interaction history is still evidence: if dispatch occurred but the final
result is unavailable, the invocation is `dispatch_ambiguous` and recovery does
not ask the provider to recreate the missing answer (FD-8, FD-9).

### Determinism vocabulary (non-normative)

```text
D0  deterministic admission        same candidate + same canonical evidence
                                   -> same controller verdict
D1  structurally constrained generation
D2  deterministic historical replay from recorded evidence, no re-invocation
D3  controlled decoding
D4  reproducible inference execution
```

**A1 correctness depends on D0 and D2 only.** D1 reduces malformed output but
never replaces controller-side semantic validation. No safety or admission
invariant may depend on D3 or D4.

## 4. Reviewer independence (mechanically enforced)

Not prompt-requested. Enforced by construction:

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

Implementation begins here, after this freeze is accepted.

### 5.1 Role assignment

```text
coder      claude CLI, mutation confined to its own worktree
controller 007 — terminates writer authority, seals, derives CandidateReceipt
reviewer   arliai — read-only, schema-bound, no mutation credentials at all
human      Q-Deck feed + attention requests, v1 command set only
```

`--engine codex` is not a V0 coder: `docs/o7-invoke.md` records that none of its
confinement flags has been exercised against a real `codex` binary, so its
honest posture today is "ambient context refused, writes denied" — not "no
shell", not "no network". It becomes eligible once that gap is live-verified
("What's needed to lift the Codex restriction"), which is not A1 work.

Arli AI is the natural first reviewer backend and the reason is mechanical, not
aesthetic: `docs/o7-invoke.md` freezes it as a direct HTTPS call with **no
subprocess and no tool surface at all**, `tool_choice: "none"`, a compile-time
constant endpoint, and a `tool_calls`-present response rejected as
`FAIL_INVALID_OUTPUT`. Reviewer independence (§4) then holds across two different
execution surfaces, not merely across two prompts.

### 5.2 The live corrective cycle V0 must demonstrate

One real campaign, end to end, no human relaying anything between agents:

```text
coder produces a candidate containing a real defect
-> controller seals, derives CandidateReceipt, dispatches review
-> reviewer finds the defect, returns a schema-bound ReviewerReport
-> controller accepts it as ReviewVerdict{changes_requested}
-> controller issues one CorrectiveDirective (scope unchanged)
-> coder fixes at a new head
-> reviewer accepts the exact new head
-> controller raises a READY_TO_MERGE HumanAttentionRequest
-> merge stays manual
```

Plus one full campaign replay after restart that reaches the same state with
**zero provider invocations** (FD-8) — the provider invocation count is the proof,
exactly as it was for A0.

### 5.3 Required negative tests

Happy path is not acceptance. Every row is a required test with a frozen
expected outcome:

| Input | Required outcome |
|---|---|
| malformed `CoderReport` / `ReviewerReport` | parse rejection, no acceptance, no dispatch |
| unsupported `envelope_version` or `message_kind_version` | refused, never best-effort parsed (FD-1.6) |
| artifact exceeding a limit in FD-1.4 | rejected, never truncated |
| unknown field present | rejected at parse time (FD-1.5) |
| duplicate `message_id`, same envelope digest | idempotent replay, no new side effect (FD-6) |
| duplicate `message_id`, different envelope digest | `IdConflict`, fail closed, attention raised (FD-6) |
| same payload bytes, different `expected_input_head` | `IdConflict`, not a replay (FD-6) |
| `claimed_head` ≠ controller-derived candidate head | fail closed, no review dispatch (§3.3) |
| stale `reviewed_head` | no `ReviewVerdict`; report retained as evidence (FD-5) |
| wrong `contract_digest` | rejected (FD-5) |
| lineage fields inconsistent with the canonical campaign binding | rejected (FD-5) |
| unknown `finding_id` referenced by a directive | rejected (§3.7) |
| reviewer proposes a shell command as required evidence | mapped to a registry id or `HUMAN_REQUIRED`; never executed (FD-7) |
| `artifact_ref` outside owned CAS | inert, unresolvable, rejected (FD-1.8) |
| a ref that violates the rank rule | rejected (FD-2) |
| provider-produced artifact with no `provider_invocation_receipt_ref` | rejected (§3.0) |
| replay path attempts a provider call | test fails; invocation count must be zero (FD-8) |
| retry attempted without established non-dispatch | refused; `dispatch_ambiguous` raised (FD-9) |
| retry that does not name its grain | refused (FD-10) |
| model alias recorded as resolved backend identity | rejected; `resolution.status` must be honest (FD-3) |
| reviewer execution holding mutation credentials | refused before dispatch (§4) |
| directive that widens scope or changes the contract digest | refused, `SCOPE_EXPANSION_REFUSED` (§3.7) |
| answer targeting a superseded `question_id` | not delivered to the coder (§3.9) |
| attention action outside the server-provided set | rejected (§3.8) |
| human command with stale expected head/state/contract | rejected (FD-12) |
| `unattested` actor on a non-loopback bind | refused (FD-12) |

### 5.4 The v1-lite cut (what V0 may leave out)

Safe to cut — the four autonomy properties survive:

- one campaign in flight (mirrors R1's single in-flight command);
- human commands: the §3.9 v1 set only;
- no push notifications: feed + SSE, tier named honestly as **v1-lite** (timely
  only while the client is open; "operational v1" means background-capable
  delivery and is not claimed until it exists);
- merge manual, triggered by the ready-to-merge attention request;
- reviewer = same provider family, different execution surface per §4;
- A3 reconciliation = polling only, no webhooks.

Not cuttable — object identity, not ceremony:

- exact-head binding everywhere, including human decisions;
- controller-derived candidate descriptor behind the A0 sealing boundary;
- the raw-report/accepted-artifact split;
- no model-supplied executable authority;
- provider invocation evidence and no-recall-on-replay;
- fail-closed directive validation;
- attention and decision records as canonical events;
- forward-only corrective rounds.

## 6. Out of scope

- No architect or planner role in the core loop.
- No multi-agent negotiation, no model-to-model channel, no PR comments as agent
  input.
- No automatic merge, under any artifact defined here.
- No A5 goal-graph runtime. `root_goal_id` is identity and lineage only — no
  scheduling, dependency discovery, replanning, or root-goal budget accounting.
- No A2 reducer, campaign state machine, or progress frontier. A1 freezes which
  artifact authorizes which transition; A2 implements the enforcement.
- No capability transport beyond registry-bound action references
  (`docs/architecture/capability-fd-transport.md` stays its own slice, and gains
  a concrete consumer only once A1-V0 has real actions to authorize).
- No redefinition of the accepted A0 contract.
- No constrained decoder, no Kani/TLA+ modelling commitment. D3/D4 are not
  prerequisites for anything here.

## 7. Supersede path

This contract is frozen. A change requires:

1. a superseding revision of this document, naming the exact decision(s) it
   replaces by FD number;
2. a new `message_kind_version` (and media type) for every artifact whose
   payload shape changes, per FD-1.6;
3. recorded migration semantics for existing campaigns — replay-for-verification
   keeps the recorded version's semantics; replay-for-resume is where version
   gates bite (issue #94 §5);
4. no in-place mutation of an accepted artifact, ever. Correction is forward-only
   at this level too.

An in-flight campaign is never migrated mid-flight: it is superseded, with the
supersedes relation recorded (FD-5).

## 8. Order after this freeze

```text
A1-F  this contract freeze                      <- you are here
A1-V0 one real coder/reviewer/human loop (§5)
SB-B  capability transport, with A1 actions as its concrete consumer
A2    campaign reducer, progress frontier, incarnation taxonomy
```

`research/b1-context/` continues in parallel and remains read-only and
non-authoritative: it must not drive an A-series transition.
