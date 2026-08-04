# Loop canvas after Q-Deck R1

Status: reconciliation note · Scope: delta from `docs/loop-canvas.md`

`docs/loop-canvas.md` remains useful as the original design map for the one-shot `o7 run` MVP, but several of its statements are now historical rather than current:

- a canonical digest-chained event record and pure reducer now exist;
- `o7 replay` independently verifies stored records;
- `o7-ledger` now receives live canonical projections from real runs;
- Q-Deck observes those projections through `o7d` over REST/SSE;
- R1 added one durable follow-up Command with provider-session continuity, exact lineage, idempotency, and fail-closed crash recovery;
- the control layer is therefore no longer entirely absent, but it still stops after one accepted Command and has no autonomous builder/reviewer cycle.

This note prevents the original canvas from being misread as the current implementation inventory while preserving it as the record of the MVP's starting assumptions.

## Current canvas delta

| Field | Post-R1 state | Remaining gap |
|---|---|---|
| Goal | Per-run gates and canonical sealed verdicts establish the result of one run. | No durable broad goal, dependency graph, or parent-task completion semantics. |
| Trigger | CLI can start a run; Q-Deck can submit a follow-up Command to an existing conversation. | Q-Deck cannot yet start an initial run from a server-side target profile. |
| Actions | Real provider invocation, gates, worktree lifecycle, and Command continuation are wired. | Provider abstraction remains Claude-first; general typed tool-loop execution is not built. |
| State | Canonical events, replay, ledger projection, conversations, runs, and durable Commands exist. | Candidate file state does not yet continue across Commands; A0 tracks that prerequisite. |
| Limits | Agent turns and several process-level safety boundaries exist. | Campaign-wide corrective-round, cost, wall-clock, and no-progress budgets are not yet defined. |
| Control | R1 supports one durable follow-up, safe retry/recovery classification, and exact parent lineage. | No autonomous builder -> gates -> CI -> reviewer -> correction controller. |
| Observability | Q-Deck shows live runs, conversations, events, connection state, and Command submission from a phone. | No campaign-level phase, blocker, budget, or human-attention projection. |
| Model & Prompt | Claude run and Claude session continuation are real; model choice remains server-side. | Codex and provider-neutral loop contracts remain deferred. |

## New control-plane decision

The next autonomous control layer is defined in `docs/autonomy-controller.md`:

```text
Goal graph / planner
        -> selects the next meaningful task
Campaign state machine
        -> controls its durable lifecycle
Bounded workflows
        -> builder, gates, CI, reviewer, correction
Canonical event log + reducer
        -> defines truth, replay, and recovery
```

The immediate requirement is a durable campaign state machine, not a general GOAP planner. A planner may later choose tasks and replan broad goals, but it does not authorize safety-sensitive lifecycle transitions.

## Relationship to A0

A0 candidate-state continuity is a prerequisite for autonomous corrective rounds that modify code. Without it, a later agent can continue the provider conversation while receiving a fresh worktree that lacks the exact file state produced by its parent.

The order therefore remains:

```text
A0  candidate-state continuity
A1  machine-readable review/correction contracts
A2  durable campaign state machine
A3  exact-head GitHub/CI autonomous corrective loop
```

This note is descriptive. It neither expands A0's implementation scope nor defines the final campaign event schema.
## Execution status reconciliation (2026-08-04)

Everything above is the accepted post-R1 design record and is preserved
unchanged as history. Actual execution has since moved past it:

```text
R1                  ACCEPTED / CLOSED / FROZEN (PR #90)
A0.0 contract       COMPLETED — contract-first commit 71800fc
                    ("docs(q-deck): define A0 candidate-state continuity
                    contract", the first commit of PR #92, frozen before
                    any implementation)
A0                  ACCEPTED at head 52627c3, merged as f1ac458 (PR #92)
A1 contract freeze  NEXT (issue #95; A0.0 precondition satisfied)
A1 implementation   after the A1 freeze
A2 / A3             later
B1 research         parallel, non-authoritative (research/b1-context/)
```

In particular, the delta-table claim above that "Candidate file state does
not yet continue across Commands; A0 tracks that prerequisite" is now
historical: A0 candidate-state continuity is merged. The normative A0
source is `docs/q-deck/a0-candidate-state.md` at the accepted head.
Post-A0 hardening residuals are tracked as a separate follow-up issue and
do not reopen A0 acceptance.
