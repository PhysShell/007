# Autonomy controller architecture

Status: design decision · Scope: post-R1 autonomous task execution · Implementation: not yet started

## Purpose

`007` already has the durable execution primitives required for one isolated run and one accepted follow-up Command: canonical run events, replay, ledger projection, worktree ownership, gates, provider-session continuation, and Q-Deck observation/control.

What it does not yet have is a durable controller that can carry one engineering task through repeated builder, gate, CI, reviewer, and corrective rounds without a human relaying every message.

This document fixes the architecture for that controller before implementation. It is deliberately not a database schema, HTTP API, planner specification, or promise of general autonomous software development.

## Decision

Autonomy is split into four layers with different authorities:

```text
Goal graph / planner
        |
        | selects the next meaningful task
        v
Campaign state machine
        |
        | controls the durable lifecycle of that task
        v
Bounded workflows
        |
        | builder, gates, CI, reviewer, correction
        v
Canonical event log + reducer
        |
        | defines durable truth, replay, and recovery
        v
Artifacts / ledger projections / UI
```

The layers complement one another. None may silently substitute for another.

- The planner proposes decomposition and ordering.
- The campaign state machine decides which lifecycle transitions are permitted.
- Workflows execute bounded side effects.
- Canonical events and reducers determine what actually happened.
- The ledger and Q-Deck expose projections of canonical state; they are not alternate authorities.

## Why a state machine is required

The near-term autonomy target is not open-ended planning. It is the repeated lifecycle demonstrated by Q-Deck R1's corrective rounds:

```text
builder -> gates -> CI -> independent review -> correction -> repeat
```

That lifecycle has a small number of explicit states and safety-sensitive transitions. A durable finite-state machine or statechart is therefore required before introducing a general planner.

A language model may generate code, explain evidence, propose a correction, or suggest a next task. It must not decide that a campaign has entered `READY_TO_MERGE`, that an exact head was accepted, or that missing evidence is equivalent to success.

## Campaign lifecycle

The first controller should support this minimal phase machine:

```text
PLANNED
  -> BUILDING
  -> GATING
  -> CI_WAIT
  -> REVIEWING
  -> CORRECTING
  -> GATING
  -> ...
  -> READY_TO_MERGE
  -> MERGED
```

Stop and escalation states:

```text
HUMAN_REQUIRED
BUDGET_EXHAUSTED
CANCELLED
FAILED
```

`READY_TO_MERGE` does not authorize an unconditional merge. It means the exact current candidate head has an accepted review verdict and all required gates are green. A maintainer policy still decides whether merge is automatic or human-approved.

### Candidate events

The exact schema is deferred, but transitions must be driven by typed, replayable events such as:

```text
CampaignPlanned
BuilderStarted
BuilderFinished
CandidateHeadRecorded
LocalGatesPassed
LocalGatesFailed
CiStarted
CiPassed
CiFailed
ReviewStarted
ReviewAccepted
ReviewChangesRequested
ReviewBlocked
CorrectionStarted
CorrectionCommitted
BudgetExceeded
HumanDecisionRequired
CampaignCancelled
MergeAuthorized
CampaignMerged
```

Free-form model prose is an artifact attached to an event, not the event itself.

## Orthogonal state

Do not encode every concurrent concern into one enormous phase enum. The controller may need independent regions such as:

```text
phase             = REVIEWING
ci                = PASSED
connectivity      = ONLINE
budget            = WITHIN_LIMIT
human_attention   = NOT_REQUIRED
```

This is a statechart-style decomposition. It avoids states such as `REVIEWING_CI_PASSED_ONLINE_WITHIN_BUDGET`, which scale about as well as naming files `final-final-2`.

The canonical campaign phase remains small. Independent regions may constrain transitions through deterministic guards.

## Transition authority

A transition is valid only when its guards are established from durable evidence.

Examples:

| Transition | Required evidence |
|---|---|
| `BUILDING -> GATING` | builder process ended; candidate head and artifact identities were recorded |
| `GATING -> CI_WAIT` | all required local gates passed against the recorded head |
| `CI_WAIT -> REVIEWING` | required CI checks passed against the same exact head |
| `REVIEWING -> CORRECTING` | machine-readable review verdict requests changes against that exact head |
| `REVIEWING -> READY_TO_MERGE` | machine-readable acceptance names the exact current head; no head drift; required gates remain green |
| any non-terminal state -> `HUMAN_REQUIRED` | a configured escalation condition was reached |

A later event cannot retroactively make an unsafe transition valid. If the candidate head changes after review, the prior review is stale and the campaign returns to the appropriate verification state.

## Review and correction contract

Autonomous correction requires a machine-readable review artifact. A reviewer comment written only for humans is insufficient as controller input.

A future `ReviewVerdict` should at minimum bind:

```text
reviewed candidate head
verdict: accepted | changes_requested | blocked
finding identities and severities
property or invariant affected
evidence references
required correction
required regression evidence
properties explicitly rechecked or preserved
reviewer identity and version
```

The corrective workflow consumes that artifact and emits a new candidate head. It must not reinterpret `changes_requested` as permission to modify the frozen goal, acceptance contract, verifier, or baseline.

Repeated or contradictory findings trigger escalation rather than unbounded prompt churn.

## Goal graph and replanning

A campaign state machine safely executes one task. It does not decide the full sequence of tasks required for a broad goal.

Broad autonomy therefore needs a durable goal graph above campaigns:

```text
goal
subtasks
explicit dependency edges
acceptance criteria
current status
evidence references
blocked_by edges
supersedes / replaced_by edges
```

When a reviewer discovers a new blocker, the blocker becomes a durable task node rather than disappearing into conversation history:

```text
R1 acceptance
  blocked_by -> P7 correction

P7 correction
  requires -> implementation change
  requires -> real-process regression test
  requires -> exact-head re-review
```

After the blocker closes, scheduling returns to the original goal. Completing a prerequisite is not the same as completing the parent goal.

The graph must distinguish:

```text
observed fact
assumption
proposed plan
accepted decision
completed result
```

An assumption may guide planning, but it cannot satisfy a deterministic execution guard until verified.

## Planner choice

### Required now

- Durable campaign finite-state machine or statechart.
- Event-sourced transitions and deterministic replay.
- Explicit budgets, stop conditions, and escalation.
- Durable task/dependency graph for broad goals.

### Useful but optional

- Hierarchical states for nested execution and recovery phases.
- Behavior-tree-like bounded procedures for local sequencing, retry, and fallback.

Behavior trees may help express an execution procedure such as:

```text
verify exact head
run required gates
wait for CI
invoke independent reviewer
publish verdict
```

They are not the durable source of campaign truth.

### Deferred

A classical GOAP or HTN planner is not required for the first autonomous PR loop.

Software work has incomplete world state, uncertain action cost, newly discovered dependencies, and nondeterministic model effects. Encoding the entire project as Boolean GOAP preconditions before real campaign traces exist would produce a large formal model of our guesses.

Start with the durable goal graph, explicit dependencies, and state-machine execution. Evaluate GOAP, HTN, or another search strategy later using real traces to identify stable actions, preconditions, effects, and costs.

## Recovery model

The controller must survive daemon, process, and host restarts without asking a model to reconstruct what probably happened.

Recovery follows the same rule as existing `o7-run` work:

```text
previous reduced state
+ next validated event
-> next reduced state
```

On startup the controller:

1. replays the campaign record;
2. validates referenced candidate heads and artifacts;
3. reconciles external projections such as GitHub and CI;
4. resumes only transitions whose safety can be re-established;
5. otherwise enters a fail-closed or human-required state.

External mutable systems are observations to reconcile, not canonical campaign history.

Prior art for step 4, recorded and deliberately not adopted: `docs/architecture/prior-art-the-grid.md`. It is a non-normative comparison record — it selects no design, adds no requirement here, and is read only when a concrete controller-lifecycle consumer exists.

## Budgets and stop policy

Every campaign must have explicit finite limits, for example:

```text
maximum corrective rounds
maximum provider invocations
maximum wall-clock duration
maximum cost or token budget
maximum repeated occurrence of one finding
maximum consecutive no-progress rounds
```

The exact fields are deferred. The architectural rule is not: exhaustion is a typed terminal or escalation outcome, never an invitation for the model to quietly raise its own limits.

Typical human escalation triggers include:

- a requested change alters frozen scope or acceptance criteria;
- the same blocker recurs after its required correction;
- reviewers or deterministic evidence conflict;
- the exact candidate head cannot be established;
- a security-sensitive transition lacks required evidence;
- configured budgets are exhausted;
- a product or domain decision has more than one legitimate answer.

## Relationship to existing components

### `o7-run`

Canonical per-run event protocol, reducer, replay, and sealed verdict authority. The campaign controller composes runs; it does not replace their reducer.

### `o7-ledger`

Durable projection and query surface. Campaign state may gain its own durable record, but ledger rows remain projections rather than alternate canonical truth.

### `o7d`

Natural owner of the single-host campaign control plane, HTTP surface, process supervision, and reconciliation with external systems.

### Q-Deck

Untrusted mobile client for observation and explicit control. It should display truthful campaign phase, blockers, budgets, and human-required decisions without calculating state client-side.

### `o7-worktree`

Owner and attestor of fresh worktrees. A campaign must never treat a serialized path as authority. A0 candidate-state continuity remains the prerequisite for continuing code state safely across corrective runs.

### Sandboy / `o7-worker`

Bounded execution and confinement. The controller schedules these boundaries; it does not weaken them to improve liveness.

## Implementation order

The expected order is:

```text
A0  candidate-state continuity
A1  ReviewVerdict and CorrectiveDirective contracts
A2  durable campaign state machine and reducer
A3  GitHub/CI exact-head reconciliation and autonomous corrective loop
A4  Q-Deck campaign observation, stop, and merge-approval surface
A5  durable goal graph and broad-task replanning
A6  evaluate HTN/GOAP/search using accumulated campaign traces
```

The identifiers are directional, not a commitment to exact PR boundaries.

## Non-goals

This decision does not define:

- a universal software-engineering ontology;
- an autonomous merge policy;
- a particular state-machine library;
- a final database or event schema;
- multi-host distributed scheduling;
- arbitrary multi-agent negotiation;
- a workflow DSL;
- a general-purpose GOAP planner;
- replacement of deterministic gates with model judgment;
- silent modification of task contracts, baselines, or verifier policy.

## Acceptance for the first controller slice

The first autonomous controller is sufficient when one frozen task can proceed through:

```text
builder
-> required local gates
-> exact-head CI
-> independent exact-head review
-> one or more corrective rounds
-> accepted exact head or explicit human escalation
```

with no human copying prompts or verdicts between agents, complete replay after restart, bounded retries, and no automatic merge unless separately authorized.