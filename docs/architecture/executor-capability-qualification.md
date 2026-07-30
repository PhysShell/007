# Executor capability qualification from a proven reference vertical

Status: design authority for a future implementation slice. This document does not add a
provider, make a live call, or change the current `o7-run` event schema.

Audited 007 base: `main` at `3386b810d6794863e640ae3cf037d37c0ea3d8f5`.

Reference implementation lineage: `PhysShell/qodec` PR #16, "Add reproducible ModelHubby
provider discovery matrix". The qodec branch is still under review while this note is written.
Implementation in 007 MUST bind to the final accepted qodec head, not to a moving branch name or
to the provisional head mentioned below.

## Problem

007 can bind an executable, run it through a constrained process boundary, collect verifier
evidence, reduce canonical events, and replay the verdict. It cannot yet answer a separate
question before assigning work to an external executor:

> Does this exact executor identity actually satisfy the versioned capability contract this run
> requires?

A configuration flag such as `supports_tools = true`, a provider catalogue entry, a successful
availability request, or an executor's own claim is not evidence. A live run that discovers the
mismatch after receiving the task has already crossed the wrong boundary: it may have exposed a
credential, spent money, produced ambiguous evidence, or silently exercised a different model or
protocol.

The qodec provider-matrix vertical exists because this distinction became concrete. It separates:

- discovery from trusted authority;
- availability from protocol qualification;
- requested model identity from reported identity;
- transport failure before response from response-capture failure after service may have occurred;
- provider error prose from response fields that decide a PASS;
- process success from protocol success;
- a green test suite from evidence that the tests and gates can fail.

007 should reuse those established invariants instead of recreating them independently for each
provider, model, CLI, verifier, remote worker, or future execution backend.

## Decision

Introduce a provider-neutral **Executor Capability Qualification** subsystem as a future 007
vertical.

The universal kernel owns:

- exact executor identity;
- versioned capability-contract identity;
- qualification request and canonical evidence events;
- typed exchange stages and failure classes;
- positive-control requirements for the qualification verifier;
- fail-closed deterministic reduction;
- replay and freshness policy;
- the rule that an unqualified executor cannot discharge a required capability obligation.

Domain adapters own:

- request and response dialects;
- tool, command, or protocol schemas;
- protocol-specific replay messages;
- domain-specific result validation;
- provider-specific endpoint and credential binding;
- classifications whose meaning exists only inside that adapter.

The kernel MUST NOT learn qodec's `qodec_answer`, C1 tool schemas, OpenAI message fields, ModelHubby
catalogue shape, or Groq endpoint. Those are the first adapter and reference corpus, not universal
007 concepts.

## Relationship to existing 007 authorities

This is additive, not a second run engine.

| Existing authority | Role in capability qualification |
|---|---|
| `o7-worker` / `ProcessBoundary` | Runs the adapter or external executor under the required boundary; lifecycle and cleanup remain worker authority. |
| Sandboy evidence | Proves the qualification process ran under the declared confinement policy. It does not prove the executor supports the requested protocol. |
| `o7-verifier` | Natural home for trusted qualification-verifier acquisition and adjudication. The executor does not own its examination. |
| `o7-run` | Declares the qualification obligation, stores canonical events, reduces the verdict, and replays it. |
| `o7-ledger` | Durable storage for attempts, evidence references, and accepted qualification records once production run events are wired into the ledger. |
| `o7-harness-policy` | Enforces that test-only qualification controls and stand-ins cannot enter the production feature graph. |
| Q-Deck / `o7d` | Later read-only projection of qualification state and evidence. It is not an authority. |

This design complements issue #42's action-level evidence and issue #74's universal admission
harness. A qualification attempt can later be represented as an action, but qualification must not
wait for the whole `ActionRecord` design. Conversely, action records must not reduce provider or
executor claims directly; they consume the qualification verdict and its evidence references.

## Core identities

A qualification result is valid only for the full identity tuple:

```text
ExecutorIdentity
  implementation kind
  provider or distribution identity
  exact model / engine / binary identity
  endpoint or executable-object identity
  adapter identity + version/digest
  relevant environment identity

CapabilityContractIdentity
  contract name
  schema version
  canonical contract digest

QualificationPolicyIdentity
  required confinement policy digest
  verifier identity/version/digest
  freshness policy
```

A result for `provider=A, model=X` does not qualify `provider=A, model=auto`, an alias, a fallback,
or a provider-reported substitute. A result produced by adapter version `N` does not automatically
qualify version `N+1`. A result under a weaker boundary cannot satisfy a stronger run contract.

Opaque provider names and model strings remain adapter data. The kernel compares canonical typed
identities and digests, not friendly labels.

## Proposed universal contract

The exact Rust layout belongs to the implementation PR. The following information is binding:

```rust
struct CapabilityRequirement {
    executor: ExecutorIdentity,
    capability: CapabilityContractRef,
    policy: QualificationPolicyRef,
}

struct CapabilityQualification {
    qualification_id: QualificationId,
    requirement: CapabilityRequirement,
    started_at: Timestamp,
    finished_at: Timestamp,
    verdict: QualificationVerdict,
    evidence: Vec<ArtifactRef>,
    final_event_digest: Digest256,
    normalized_state_digest: Digest256,
}

enum QualificationVerdict {
    Pass,
    Fail(QualificationFailure),
    Blocked(QualificationBlock),
    Error(QualificationError),
}
```

The distinction is load-bearing:

- `Pass`: every required protocol obligation was observed and identity remained exact;
- `Fail`: the executor answered and demonstrated that it does not satisfy the capability contract;
- `Blocked`: policy or environment prevented a meaningful qualification attempt;
- `Error`: the verifier, adapter, evidence sink, or transport could not produce a trustworthy
  verdict.

`Error`, missing evidence, unknown schema, stale qualification, or identity mismatch can never be
reduced to `Pass` or a milder domain failure.

## Exchange stages

The qodec vertical showed that `(status, body)` is insufficient. A universal exchange record must
state what was actually established without inferring service or billing semantics from nullable
fields.

Initial stage vocabulary:

```text
NotAcknowledged
ResponseFramingFailed
HeadersReceived
BodyCaptured
```

Semantics:

- `NotAcknowledged`: no response framing or headers were established; the request may not have
  been served.
- `ResponseFramingFailed`: the request was attempted, but a valid response line/headers were not
  established; it is unsafe to claim the executor was unavailable or that no service occurred.
- `HeadersReceived`: status and response metadata are known, but the body was not captured
  completely.
- `BodyCaptured`: status and complete bounded body are available.

Every field must agree with its stage. Response-derived identifiers or byte counts cannot exist in
`NotAcknowledged`; a captured body must agree with its recorded length; a body without a status is
structurally invalid. Adapter code may refine these stages but may not collapse an uncertain
response into a retryable pre-response failure.

Untrusted wire text and provider prose are not durable error details. Evidence may retain bounded
raw bytes as a content-addressed artifact under explicit policy; canonical events and verdict
reasons carry local reason codes, exception categories, status, request identifiers, byte counts,
and digests.

## Canonical event vocabulary

Do not infer qualification truth later from stdout. A future schema version should add explicit
qualification events, either to `o7-run` or to a sibling pure protocol crate consumed by it.

Minimum event sequence:

```text
CapabilityQualificationStarted
QualificationExecutorBound
QualificationExchangeObserved      # repeatable, stage + bounded evidence refs
QualificationProtocolStepObserved  # adapter-owned typed payload reference
QualificationIdentityObserved      # requested vs reported exact identity
QualificationPositiveControlChecked
CapabilityQualificationSealed
```

`CapabilityQualificationStarted` fixes the requirement, adapter, verifier, policy, environment,
and freshness rule before execution. Observed events cannot weaken or invent the requirement.

`QualificationProtocolStepObserved` should reference an adapter-owned typed artifact rather than
forcing every protocol into one universal payload. The universal envelope binds subject,
contract digest, ordinal, artifact kind/digest, and outcome class.

`CapabilityQualificationSealed` finalizes the deterministic verdict. A stream without a valid
seal is incomplete, never implicitly failed or passed.

## Reducer invariants

The pure reducer must enforce at least:

1. The requirement is declared before any executor interaction.
2. The executor, adapter, verifier, policy, and environment identities remain bound throughout.
3. Every required protocol obligation is discharged exactly as declared.
4. A claimed exact identity is established by evidence from every successful deciding response.
5. Missing, substituted, or contradictory identity prevents `Pass`.
6. Evidence events are ordered and cannot appear after seal.
7. A positive-control result is present and valid for the verifier version that produced the
   qualification.
8. Any structural error or unverifiable artifact is `Error`, not a domain `Fail`.
9. A protocol failure cannot erase transport evidence already established.
10. Replay recomputes the same verdict and rejects changed, truncated, reordered, or
    digest-mismatched evidence.

The run-level reducer later consumes a qualification obligation by reference. If a run requires a
capability and no fresh matching `Pass` exists, the executor cannot start. A waiver, if supported,
must be pre-declared and auditable like `GateApplicability::Waived`; runtime absence cannot invent
one.

## Positive controls

A qualification verifier is itself untrusted until it demonstrates that it can distinguish the
required outcomes.

Each adapter version must ship hermetic controls proving at minimum:

```text
known-good fixture      -> PASS
known protocol defect   -> FAIL with the intended typed cause
broken verifier/runtime -> ERROR
blocked environment     -> BLOCKED when the policy defines it
```

A positive control must exercise the same production parsing, replay, reduction, and evidence
publication seam. A private test helper that imitates the production path is not sufficient.

The qodec reference adds a stricter lesson: gates that claim repository cleanliness, full test
discovery, mutation effectiveness, or parser parity must prove they can report the opposite and
must fail with a verdict rather than traceback. Host-global Git configuration, credentials,
ambient hooks, or other machine state may not determine a positive control unless that environment
is explicitly part of the qualification identity.

## Qualification records and freshness

A matching `Pass` is reusable only when policy permits it. Freshness is not a timestamp chosen for
convenience; it is a predicate over identity and risk.

A qualification becomes unusable when any bound input changes, including:

- executor/model/binary identity;
- endpoint authority or provider registry digest;
- adapter or verifier digest;
- capability-contract digest;
- sandbox/confinement policy digest;
- environment class when the contract depends on it;
- an explicit maximum age or revocation record.

Revocation is append-only evidence. Do not mutate an old `Pass` into a different historical fact.
A new reduction decides whether it remains eligible for a new run.

## qodec reference mapping

The migration must be performed from the final accepted qodec PR #16 revision. At the time this
note was authored, the reviewed lineage had reached `94d396ed79664dde415072dd5f0580bd9519770d`
and an eleventh correction round was being prepared. That SHA is a historical checkpoint, not the
implementation authority.

The final migration PR must record:

```text
qodec repository
qodec PR number
final accepted head SHA
relevant file/blob digests
final CI run ids
final review disposition
```

Transfer map:

| qodec reference | 007 destination |
|---|---|
| trusted provider registry and exact provider×model target | adapter-owned `ExecutorIdentity` construction and authority binding |
| `SendResult` plus stage validation | universal exchange evidence type and structural validator |
| availability probe vs forced-tool qualification | generic availability is evidence-only; versioned capability adapter owns the real protocol obligations |
| requested/reported model fold | universal exact-identity obligation with adapter extraction |
| strict successful-response parsing | adapter contract; every field replayed or consumed by the future mapper is validated before use |
| deciding vs describing JSON | universal evidence policy distinction; adapter marks fields/artifacts that can influence verdict |
| bounded capture and local reason codes | universal evidence-safety policy |
| receipt | canonical events + adapter artifacts + pure reducer + replay report |
| stand-in transport and exhaustive classification fixtures | adapter positive controls |
| mutation harness and gates-that-can-fail | verifier-qualification evidence; not necessarily copied as Python machinery |
| ModelHubby catalogue import/planning | remains qodec-specific; not part of the universal kernel |
| C1 tools, canned results, `qodec_answer` | remains qodec adapter semantics |

The transfer is semantic, not a line-for-line port. Python exception names, OpenAI fields, and
qodec reason codes are evidence used to derive the universal types; they do not become core 007
vocabulary unless a second independent adapter demonstrates the same distinction.

## Implementation slices

The implementation must be serialized. Do not land a universal marketplace-sized abstraction in
one PR because humans apparently enjoy discovering type-system mistakes only after building the
entire cathedral.

### EQ-0: freeze the accepted reference and RED contract

Documentation and RED tests only:

- bind the final qodec reference revision and artifact digests;
- define the minimal universal identity, exchange-stage, verdict, and event contracts;
- add RED reducer/replay cases for missing qualification, identity substitution, framing failure,
  incomplete body, broken positive control, and stale/revoked evidence;
- no provider code and no live call.

### EQ-1: pure qualification protocol and reducer

- dependency-light crate or `o7-run` module;
- canonical framing/digests;
- structural validation;
- pure reducer and replay;
- fixtures only.

### EQ-2: qodec/OpenAI-chat reference adapter

- port semantics from the frozen qodec reference;
- adapter-owned schemas and protocol records;
- exact provider/model binding;
- model-free positive controls;
- no production executor selection yet.

The adapter may initially run the qodec reference corpus as cross-language conformance evidence.
The target is semantic equivalence, not permanent Python runtime dependence.

### EQ-3: ledger and run-contract integration

- persist qualification attempts and artifacts through the ledger;
- add capability requirements to the run contract;
- prevent executor launch without a fresh matching `Pass`;
- expose replay and read-only Q-Deck projection.

### EQ-4: one constrained live vertical

Only after Sandboy production promotion and all previous slices are accepted:

- one exact provider/model or one exact external executor;
- one limited credential scoped to the trusted endpoint, if the adapter requires an API key;
- availability then capability qualification;
- append canonical evidence to the ledger;
- run one bounded task only after qualification passes;
- no fallback, `auto`, alias, or silent substitution.

## Acceptance for the future feature

The first production qualification vertical is accepted only when:

- the final qodec reference revision is pinned and independently reproducible;
- the universal contract has RED→GREEN reducer and replay tests;
- every adapter positive control is non-vacuous and hermetic;
- malformed transport, protocol, evidence, and environment paths produce typed verdicts, never
  panics or ambiguous success;
- exact executor identity is proven, not configured;
- the qualification process runs under required Sandboy evidence;
- all durable artifacts are bounded, content-addressed, and secret-safe;
- a stale, revoked, mismatched, missing, or errored qualification blocks launch;
- replay re-derives the exact qualification verdict from ledger events and artifacts;
- one real vertical demonstrates qualification before task execution, with no fallback.

## Non-goals

- no generic provider marketplace;
- no model ranking, cost optimizer, or automatic fallback;
- no claim that one qualification proves future stochastic quality;
- no domain-specific C1 logic in the universal kernel;
- no copying qodec's Python implementation wholesale;
- no weakening of Sandboy, verifier identity, or run evidence requirements for providers that are
  inconvenient to test;
- no live credential or provider call in this design PR.

## Immediate next action

Finish and freeze qodec PR #16 first. Then open EQ-0 from the exact accepted qodec head and this
document, replacing the provisional reference paragraph with immutable provenance and a concrete
RED matrix. Until then this note is the migration authority, not an authorization to start a live
provider integration.
