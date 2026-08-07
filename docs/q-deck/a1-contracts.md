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
  role                       # coder | reviewer
  provider_execution_id
  conversation_id
  command_id: Option<CommandId>   # present only where the execution
                                  # really was an R1 continuation
  run_id
  attempt_id
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
  `CampaignRunBindingV1`; it is never adopted from an incoming envelope.

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

A round is created before the first coder dispatch of that round and
binds: `campaign_id`, `contract_digest`, `input_candidate_ref: Option`
(absent for the campaign's first round on a fresh task), and the
`WorkOrder` or `CorrectiveDirective` ref that opened it. One round may
contain more than one `provider_execution_id` ONLY through proven safe
pre-dispatch redrive. A round admits at most one accepted
`CandidateAdmissionReceiptV1`. Round outcomes (closed set):

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
  round_id                   # where applicable to the kind (typed per kind)
  causation                  # typed identity of the causing artifact
  producer: ProducerBindingV1
  payload_digest
  artifact_refs              # typed refs, canonical order
  recorded                   # controller-assigned acceptance metadata (§4.4)
```

```text
ProducerBindingV1:
  Controller {
    component_version
    policy_digest
  }
  Provider {
    role                     # coder | reviewer
    campaign_run_binding_ref
    provider_execution_id
    invocation_receipt_ref
    adapter_version
    model_route_ref
    prompt_digest
    tool_policy_digest
  }
  Human {
    authenticated_actor_ref
  }
```

A provider-produced artifact without a `Provider` binding, a
controller-derived artifact carrying one, or a human artifact without an
authenticated actor are all unrepresentable at the wire-type level
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
`root_goal_id`; `task_id`; `campaign_id`; `round_id` where applicable;
causation artifact identity; producer binding; contract binding where
applicable; input candidate binding where applicable; `payload_digest`;
typed artifact refs in canonical order; provider execution/invocation
binding where applicable.

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
CAS requires an explicit bridge receipt with proven byte/digest equality:

```text
ArtifactImportedV1:
  source_run_id
  source_run_artifact_ref
  cas_object_ref
```

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

InputCandidateBindingV1:
  candidate_state_ref        # CandidateStateReceiptRefV1
  materialization_ref        # CandidateMaterializationRefV1
```

`InputCandidateBindingV1` proves that a specific run materialized a
specific accepted candidate state before dispatch. All candidate identity
(repository, base commit, tree OID, patch) resolves through the accepted
A0 `CandidateStateReceiptV1` — A1 re-declares none of it.

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
(`input_candidate_binding`, `candidate_state_ref`,
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
input:     InputCandidateBindingV1        # absent only for the first
                                          # round of a fresh task
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
  round_binding_ref
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

### 8.4 ReviewRequest / ReviewerReport / ReviewVerdict

ReviewRequest carries `admission_receipt_ref` (the accepted
`CandidateAdmissionReceiptV1`), `contract_digest`, `required_properties`,
`evidence_refs` (contract, diff, deterministic evidence first), and
optionally `coder_report_ref` — delivered last, labeled advisory-claims.

Reviewer independence is mechanical, not prompted: fresh provider session
(no continuation of the coder session, enforced by the C5 binding rules —
its own conversation in v1); no coder transcript; detached exact-candidate
fresh attested worktree; no repository or GitHub mutation credentials;
separate prompt/tool-policy identities (digests in the producer binding).

ReviewerReport is untrusted; it may contain `accepted`, but its JSON
authorizes nothing. ReviewVerdict is minted by the acceptance classifier
only when: schema-valid, supported version; `reviewed_candidate_state_ref`
equals the current accepted admission receipt's candidate ref; reviewer
identity/prompt/tool-policy established; required fields and evidence refs
present and resolvable.

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
  reviewer: { model_route_ref, resolution_evidence_ref, prompt_version }
  reviewer_report_ref
```

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
                established_non_dispatch_evidence_ref }

DispatchCauseV1:
  Initial
  ToolContinuation { prior_dispatch_id, tool_result_ref }
  SafeRedrive { prior_dispatch_id,
                established_non_dispatch_evidence_ref }
```

ToolContinuation is not a retry. A new session is not a retry. A
corrective round is not a retry. `SafeRedrive` requires evidence of
established non-dispatch — a fresh identifier does not make a duplicate
side effect safe. The full incarnation taxonomy stays A2; these two
grains and causes freeze now because the receipt schema needs them.

The receipt (per execution) binds: execution id + cause; the campaign run
binding; `LogicalModelRouteV1` ref + `ModelResolutionEvidenceV1` (§9);
canonical request ref (the exact provider-facing request after adapter
construction); capture status (`exact_provider_events` /
`adapter_observations` / `normalized_output_only`); interaction manifest
ref; outcome (closed set: `completed | refused | incomplete |
failed_pre_dispatch | dispatch_ambiguous`) with typed stop/error/usage
refs; `producer_observed_at` (untrusted). Fields the provider or adapter
cannot establish remain absent under an explicit status — never inferred
from an alias, SDK default, or previous invocation.

The InteractionManifest records the observable route (ordered dispatches,
tool calls, tool results, errors) grouped by execution. Recording a
requested tool call does not authorize it (§11 rank; §12.8 classifiers;
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
HumanAttentionRequest:
  attention_id
  campaign_id
  candidate_state_ref: Option
  reason: { code, summary }        # includes EXTERNAL_DRIFT (code
                                   # frozen now; detection lands in A3)
  severity: info | attention | urgent
  required_decision_kind: none | ack | choose_resolution
  options: [{ action_id, consequence }]   # server-defined only
  evidence_refs
  lifecycle: OPEN | ACKNOWLEDGED | RESOLVED | SUPERSEDED
```

`dedupe_key` is computed by the controller, never an agent; repeated
reconciliation updates the one durable record. ACK ≠ RESOLVED.

### 8.8 Human lane

```text
HumanCommandRequest (untrusted):
  command_id
  idempotency_key
  control_session_id
  campaign preconditions:
    campaign_id
    expected_campaign_state_version
    expected_contract_digest
    expected_candidate_state_ref     # where applicable
    attention_id / question_id       # where applicable
  requested action                   # v1: ACK | CANCEL | ANSWER_QUESTION
                                     #     | SELECT_ATTENTION_ACTION
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
that trigger. The controller derives:

```text
AuthenticatedActorV1:
  principal_id
  credential_epoch
  authn_method
  control_session_id
```

and places it (by ref) into the accepted `HumanDecision`.

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

## 11. Evidence-graph acyclicity

Edge definition: `A -> B` iff artifact A directly contains a
digest-reference to B. Canonical references point from the DERIVED object
to its ANTECEDENT evidence:

```text
CandidateAdmissionReceipt -> CoderReport
  -> ProviderInvocationReceipt -> InteractionManifest
    -> raw provider blobs
```

(the authority/justification flow reads in the opposite direction).
Frozen rank:

```text
controller-accepted derived artifact
> accepted raw report
> invocation receipt / interaction manifest
> raw request-response blobs
```

Every embedded reference must target a strictly lower rank. Back-links
live only in indexes/projections, never in canonical bytes. A dedicated
`ArtifactAcceptance` event is NOT introduced; its trigger stays: a real
consumer of acceptance-as-an-event, or multiple acceptance outcomes for
one source artifact.

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
  remain permitted as safety operations; no new coder/reviewer dispatch;
  outcome `BUDGET_EXHAUSTED` or `HUMAN_REQUIRED` per frozen policy. If
  cost is only known post-response and shows overshoot: the receipt is
  accepted as evidence and the next progress transition is forbidden.
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
  verified (§2.4); A0 receipt verified through the accepted A0 semantic
  layer; claim check (§8.2); controller-observed change set derived.
- ReviewerReport → ReviewVerdict: §8.4 conditions.
- HumanCommandRequest → HumanDecision: authenticated actor; precondition
  bindings fresh; conditional atomic consume.

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
resolved ReviewerReport; a HumanDecision without an authenticated actor;
a CandidateAdmissionReceipt without a verified A0 receipt; a provider
receipt before a terminal/ambiguous outcome. Canonical types have closed
fields; construction only через checked constructors.

### 15.3 RED tests (observable effect, never a stub)

Stale candidate/contract/state preconditions; lineage mismatch; duplicate
ID with a different binding digest; unknown registry IDs; directive scope
escalation; resolver escape (path/symlink/oversize); reviewer holding
mutation credentials; coder claim vs derived candidate mismatch; answer
to a superseded question; unauthorized/stale human command; retry without
established non-dispatch; a replay path attempting a provider call; a
digest cycle; the disputed budget/cancellation transitions of §12.4–12.6.
A newtype around `String` is not a semantic proof.

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
`merge(sha = accepted_candidate's external head)` as the conditional
atomic mutation — an A3+ concern); no cross-family reviewer (existing
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
   inside the A0 wrappers, `CasObjectRefV1` elsewhere;
4. no generic `retry_of` — only `ExecutionCauseV1`/`DispatchCauseV1`;
5. no authoritative caller-supplied actor, model, or lineage fields.
