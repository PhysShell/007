# Graph engineering in 007: execution, evidence, and control

Status: design note, not a specification  
Verified: 2026-07-27 against `main` at `dce4a49e3e324ad3dc17c5103447dc7e89880e1a`

## Summary

“Loop engineering is dead; enter graph engineering” is useful only after
removing the funeral rhetoric. A graph does not replace loops. It makes the
larger system explicit: which bounded loops and deterministic stages exist,
what data crosses between them, which component has authority to decide, and
how failure propagates.

That distinction matters in 007 because the repository is no longer accurately
described as one shell-shaped agent loop with a future ledger. The current tree
already contains separate, typed foundations for:

- durable run history (`o7-ledger`);
- external process lifecycle (`o7-worker`);
- committed-revision materialization (`o7-worktree`);
- trusted command execution and evidence (`o7-verifier`);
- a canonical event protocol, pure reducer, and independent replay (`o7-run`);
- sandbox wire contracts (`o7-sandbox-protocol`);
- and a compile-time/repository policy preventing test-harness surfaces from
  entering the production feature graph (`o7-harness-policy`).

The useful graph-engineering task is therefore not “add a graph framework” or
“spawn more agents.” It is to finish and preserve the contracts between these
existing authorities.

## Terminology

### Loop engineering

Loop engineering designs one bounded cycle:

```text
observe -> decide -> act -> verify -> retry / stop
```

Its questions are local:

- What is the stop condition?
- Who checks completion?
- What is the retry budget?
- What state survives the next iteration?
- Which side effects are permitted?

### Graph engineering

Graph engineering composes loops and deterministic stages:

```text
node --typed edge--> node --typed edge--> node
```

Its questions are relational:

- What is the identity of each node and execution subject?
- What exact payload crosses an edge?
- Is the payload an observation, evidence, a verdict, or merely a suggestion?
- Which node owns authority over the next transition?
- How are cancellation, partial failure, replay, and retry represented?
- Can the graph be reconstructed without trusting the component that executed it?

A graph may contain cycles. Calling graphs the successor to loops is therefore a
category error: the graph is the composition boundary around them.

## The current 007 graph

The repository currently exposes the following architectural shape:

```text
 task + committed base
          |
          v
  o7-worktree materialization
          |
          | attested worktree identity
          v
  o7-worker + ProcessBoundary
          |
          | typed WorkerObservation stream
          v
  WorkerObservation -> RunEvent adapter        [integration seam]
          |
          | canonical, digest-chained RunEvent
          v
  o7-ledger append                              [integration seam]
          |
          +--------------------------+
          |                          |
          v                          v
  o7-run pure reducer         o7-run independent replay
          |                          |
          +------------+-------------+
                       |
                       v
              o7d/control-plane verdict
```

A trusted verification branch joins that graph:

```text
 trusted command + trust anchor + worktree identity
                       |
                       v
                 o7-verifier
                       |
                       | VerifierEvidence
                       v
              RunEvent / reducer obligations
```

This drawing deliberately distinguishes **implemented contracts** from
**integration seams**. `o7-run` already defines the event, reducer, and replay
contracts, but its crate documentation still names the
`WorkerObservation -> RunEvent` adapter, ledger append path, and replay CLI as
later wiring. A diagram that silently depicts those seams as finished would be
architecture fan fiction, which is not improved by using more arrows.

## Node inventory and authority

| Node | Current implementation | Owns | Must not own |
|---|---|---|---|
| Work intake / control plane | root `o7` today; future daemon/control plane | run creation, scheduling, final transition decisions | execution evidence it did not observe |
| Worktree materializer | `o7-worktree` | materializing one committed revision, binding it to run/repository identity, safe attested cleanup | process confinement; a worktree is explicitly not a sandbox |
| Worker runtime | `o7-worker` | one external process lifecycle, typed observations, cancellation, teardown, exactly one result | Claude/Codex semantics, verification verdicts, ledger policy |
| Process boundary | `ProcessBoundary`, production target `SandboyBoundary` | launch and confinement evidence | silently falling back from required confinement to the host boundary |
| Trusted verifier | `o7-verifier` | executing a pre-trusted absolute program and argv under bounded cwd/env/time/output rules; returning evidence | final accept/reject policy |
| Run protocol | `o7-run::event` | versioned event vocabulary and obligation identities | storage and scheduling |
| Pure reducer | `o7-run::reduce` | deriving run state and verdict from the complete event sequence | inventing missing evidence or treating absence as success |
| Independent replay | `o7-run::replay` | validating event/artifact digest chains and recomputing state | claiming authenticity against an actor able to rewrite the entire stream |
| Durable ledger | `o7-ledger` | committed append-only history, monotonic sequences, idempotency, crash recovery | deciding business transitions without a caller request |
| Harness policy | `o7-harness-policy` | proving the production Cargo graph and API exclude test-only surfaces | runtime sandboxing |

The separation is load-bearing. For example, `o7-verifier` returns evidence and
explicitly does not decide acceptance; the control plane owns that policy.
Likewise, `o7-ledger` records durable facts but opening the database never
silently rewrites an interrupted run into a new status.

## Edge contracts

A useful graph is not a list of boxes. The edges are the architecture. Every
new cross-component edge should answer all of the following.

### 1. Identity

The payload names the execution subject precisely:

- run and attempt identity;
- canonical repository identity;
- committed revision;
- worktree identity;
- worker or verifier identity;
- command and policy identity where applicable.

A path, branch label, process ID, or human-readable name is not sufficient on
its own.

### 2. Typed payload

The receiver must know whether it is receiving:

- a request;
- an observation;
- attested enforcement evidence;
- verifier evidence;
- an obligation result;
- a control decision;
- or a terminal verdict.

Unstructured agent prose may be retained as an artifact, but it must not double
as a protocol message.

### 3. Provenance and integrity

The edge records the digest and schema needed to bind payload to producer and
inputs. `o7-run` already makes this concrete with digest-chained events and
artifact references. Replay must reject truncation, reordering, substitution,
and in-place mutation rather than reconstructing a comforting story from a
partial archive.

### 4. Obligation semantics

Required work is represented as an obligation before execution. A required gate
or sandbox proof that never ran is not a negative result and certainly not a
pass. It is blocked or erroneous. This prevents “nothing contradicted success”
from becoming the system's most dangerous theorem.

### 5. Failure class

The edge must distinguish at least:

- domain failure: trusted execution ran and the target failed;
- infrastructure error: no trustworthy answer was obtained;
- policy refusal: execution was prohibited before it began;
- cancellation/interruption;
- incompatible or legacy state that cannot be replay-verified.

Collapsing these into a boolean is a fail-open conversion disguised as API
simplification.

### 6. Idempotency and retry

Any retryable request needs a stable idempotency scope/key and a request digest.
A repeated key with different input is a conflict, not an invitation to guess.
Retries must create or resume an explicit attempt; they must not overwrite the
first trajectory and make the graph look as though only the successful path
ever existed.

### 7. Authority

The producer may report only what it is entitled to know. A worker reports
observations; a verifier reports evidence; a reducer derives state; the control
plane decides the next action. An LLM may propose that a task is complete, but
it cannot grant itself the evidence or authority required to make that true.

## What the current code already gets right

Several graph-engineering rules are already structural properties of the tree,
not aspirations in a diagram:

- `PASS`, `FAIL`, and `ERROR` are separate states.
- A required obligation that never executed cannot reduce to green.
- Pre-protocol records remain readable but are explicitly
  `LegacyNonReplayable`, never retroactively stamped verified.
- Event and artifact digests make run truth independently recomputable.
- The ledger refuses a schema newer than the binary before making persistent
  changes.
- Ledger recovery is caller-driven; database open does not mutate run state.
- Worktree identity is re-attested before deletion, and dirty/untrusted paths are
  not treated as owned merely because a record says so.
- A worktree is not called a sandbox.
- Production verification requires a fully enforced process boundary with no
  unconfined fallback.
- The worker's test harness is guarded both by manifest policy and a production
  compile probe; Cargo feature convention alone is not mistaken for isolation.
- Recent CI fixes materialize and inspect real command output under fail-closed
  shell semantics rather than letting an upstream error masquerade as an empty,
  therefore safe, result.

These are stronger guarantees than the usual agent-architecture sketch of
“planner -> coder -> reviewer.” That sketch names roles. 007 increasingly names
obligations, identities, evidence, and refusal modes.

## Where loops belong inside the graph

### Worker lifecycle loop

The worker owns a bounded process lifecycle: launch, observe, heartbeat,
cancellation, teardown, terminal result. This is a local loop behind one node,
not a reason to let worker state leak into every other crate.

### Verification loop

A future repair cycle can be:

```text
attempt -> deterministic gates/verifier -> verdict
   ^                                  |
   |--------- bounded feedback -------|
```

The feedback is an artifact derived from failed obligations. The retry ceiling,
no-progress rule, and permission to create another attempt belong outside the
agent. Asking the model to “stop after a few tries” is not a budget.

### Recovery/resume loop

Interrupted work may be resumed only through explicit ledger transitions and a
new attempt. Recovery is not replaying arbitrary side effects until something
looks green.

### Improvement loop

Traces may suggest changes to policies, prompts, or skills, but any self-editing
path must be outside the authority it can modify. A loop capable of rewriting
its own gate cannot use that gate as evidence that the rewrite is safe.

## What not to build yet

### No generic DAG engine merely because the system is graph-shaped

`docs/workflow-scripting.md` correctly rejects speculative `depends_on` support
while real workflows remain linear. The foundation graph described here is an
architectural dependency and evidence graph, not automatically a user-authored
workflow language.

A generic scheduler becomes justified only when actual run records require:

- independent branches that can execute concurrently;
- a typed fan-in rule;
- partial-failure and cancellation propagation;
- resumability at node granularity;
- and a stable graph schema worth migrating.

Until then, explicit Rust orchestration is easier to audit and harder to
misconfigure.

### No multi-agent team as a default topology

Parallel agents are useful only for genuinely independent work with isolated
worktrees and a deterministic merge or adjudication contract. Adding a planner,
researcher, coder, and critic to every task increases coordination state and
cost without creating evidence.

Use another agent when it supplies an independent capability or context. Use a
function, verifier, or reducer when the contract is deterministic.

### No LLM self-grading as normative verification

A separate model can find suspicious omissions and generate adversarial review
questions. It is not a substitute for:

1. deterministic verification;
2. trusted execution evidence;
3. replay over canonical events and artifacts;
4. independent CI on the exact commit.

Model review is a sensor. It is not the root of trust.

## Practical next increments

The graph suggests an order, not a promise that every box needs another crate.

1. **Wire observation to canonical event.** Implement and test the exact
   `WorkerObservation -> RunEvent` mapping. Unknown or impossible observations
   must refuse conversion, not disappear.
2. **Append before acting on derived state.** Persist canonical events through
   `o7-ledger`, then reduce the committed sequence. The UI/control plane should
   not advance from an event that was never durably recorded.
3. **Expose replay as an independent operation.** Recompute state and artifact
   bindings without the worker or original process environment.
4. **Bind verifier evidence into obligations.** Preserve trust anchor, command
   digest, cwd policy, boundary attestation, exit policy, timeout, and output
   truncation facts.
5. **Add bounded repair only after failure identity exists.** A retry requires a
   normalized failed-obligation signature and a no-progress rule; otherwise the
   control loop merely spends the same failure repeatedly.
6. **Introduce parallelism only with typed fan-in.** Each branch needs its own
   attempt/worktree, and the join must state whether it requires all branches,
   any branch, quorum, or independent adjudication.

## Review checklist for a new node or edge

Before merging a graph extension, answer:

- What exact current code requires this node?
- Why is it not a pure function inside an existing authority?
- What are the input and output schema versions?
- Which stable identities bind the payload?
- What artifact/evidence digests are retained?
- Who is allowed to emit the payload?
- Who decides the next transition?
- How are duplicate, late, reordered, and incompatible messages handled?
- What happens if the node never runs?
- What is the timeout, output, retry, and cost ceiling?
- How does cancellation propagate?
- Can replay reconstruct the result without trusting the executor?
- Can the node modify the policy that judges it?

If these answers live only in a prompt, the graph is not engineered yet.

## Relationship to the earlier loop canvas

`docs/loop-canvas.md` remains useful vocabulary for the original `o7 run` unit:
goal, trigger, actions, state, limits, control, observability, and model. Its
historical code snapshot is now behind the workspace architecture, especially
where it describes the ledger and control foundations as absent.

This note does not replace the canvas. It supplies the next scale:

```text
canvas: define one bounded loop

graph: compose the repository's bounded loops and deterministic authorities
       without weakening identity, evidence, or failure semantics
```

## Sources and nearby project documents

- [`hardness1020/awesome-agent-architecture`](https://github.com/hardness1020/awesome-agent-architecture) — useful progressive reconstruction of loop, tools, permissions, tasks, worktrees, protocols, observability, and verification.
- [`docs/loop-canvas.md`](loop-canvas.md) — the original one-run canvas.
- [`docs/workflow-scripting.md`](workflow-scripting.md) — why linear host-enforced workflows precede a generic DAG.
- [`docs/security-layers.md`](security-layers.md) — trust and confinement boundaries.
- [`docs/verification.md`](verification.md) — deterministic verification posture.
- [`docs/architecture/ledger-durability.md`](architecture/ledger-durability.md) — committed-state and recovery guarantees.
- Crate-level documentation in `crates/o7-run`, `o7-ledger`, `o7-worker`, `o7-worktree`, `o7-verifier`, and `o7-harness-policy` — the current contracts this note maps.