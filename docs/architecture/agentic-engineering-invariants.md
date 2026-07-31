# Agentic engineering invariants

Status: **current-tree architecture map**  
Verified against: `main` at `3386b810d6794863e640ae3cf037d37c0ea3d8f5`  
Verified on: `2026-07-30`

This document maps common agentic-engineering concepts onto the authorities that
actually exist in 007. It is not a product checklist and it does not make an open
pull request part of the current architecture.

The purpose is to prevent two opposite errors:

1. rebuilding generic agent-framework features that 007 already implements more
   strictly; and
2. describing a planned or demonstrated component as production authority before
   it is merged and wired into the live path.

## Status vocabulary

| Status | Meaning |
|---|---|
| **enforced** | A current-tree component owns the behavior and tests or replayable evidence defend it. |
| **partial** | Some required contracts exist, but the end-to-end authority or production wiring is incomplete. |
| **planned** | A design exists, but current code must not depend on the capability. |
| **rejected as foundation** | The technique may be used as an adapter or convenience, but it is not an architectural authority. |
| **out of scope** | The concept does not advance 007's control-plane purpose. |

## Authority map

| Concept | 007 status | Current authority | Binding interpretation |
|---|---|---|---|
| Agent | **partial** | `o7 run`, `o7-worker` | 007 has bounded agent execution, but not yet a durable autonomous task-control loop. A subprocess lifecycle loop is not the same thing as a goal/repair/escalation loop. |
| Execution model | **partial** | `o7-worker`, `ProcessBoundary` | Start, observation, cancellation, timeout and teardown are typed and fail closed. Model-level `think -> act -> observe` decisions are not yet canonical ledger events. |
| Agent state | **enforced** | `o7-ledger`, run artifacts | Durable state lives outside model context. The ledger owns run/conversation transitions; artifacts own evidence. Context is a working set, never the source of truth. |
| Planner/executor, router/specialist, map/reduce | **planned** | delegation fields and architecture notes only | Add a topology only after one reliable end-to-end provider vertical exists. Handoffs require typed identity, provenance, obligations and outcome authority. |
| Project agent configuration | **enforced** | `AGENTS.md` plus binding architecture docs | Keep always-loaded instructions short, repository-specific and reviewable. Mechanical rules belong in CI, not repeated prose. |
| Reusable workflow files | **planned** | `docs/workflow-scripting.md` | Start with narrow, observed procedures. Do not add a skill registry, generic DAG engine or generated instruction library before repeated work justifies it. |
| Workflow framework | **rejected as foundation** | 007's own typed workflow and evidence contracts | External frameworks may inspire procedures, but they do not own execution, verdicts, identity or recovery. |
| Prompt caching | **rejected as foundation** | provider implementation detail | Caching may reduce provider cost or latency. It must not shape correctness, state, security or replay semantics. |
| Context-rot control | **partial** | short config, externalized state, task-specific context designs | Keep stable instructions lean and retrieve task evidence on demand. Context quality still needs explicit measurement and bounded payloads. |
| MCP | **rejected as foundation** | provider/tool adapters only | MCP may bridge to a tool. It does not supply 007 run identity, idempotency, cancellation, durable cursors, verdict authority or recovery. |
| Live documentation retrieval | **planned** | task-scoped capability, not general worker authority | Retrieval must be explicit, origin-tagged, digestible as evidence and treated as untrusted input. Network access is not granted to an entire coding run merely to fetch one document. |
| AI-oriented web search | **planned** | same retrieval boundary | Search is a task capability. Clean extraction does not make results trusted instructions. |
| Visual generation | **out of scope** | none | Q-Deck is an observation surface, not a general design, slide or video generator. |
| Persistent memory | **planned** | `docs/agent-memory-layer.md` | Canonical memory must be derived from accepted artifacts and retain provenance. A model-written `MEMORY.md` is not authority. |
| Knowledge search | **planned** | provenance-backed retrieval design | Search may select context; it may not silently rewrite ledger state, evidence or accepted decisions. |
| Subagents | **planned** | delegation identity reserved in the ledger | A child task must be narrow, isolated and return typed evidence. Parallel writers require separate worktrees or another explicit isolation boundary. |
| Agent loops | **planned** | future control loop above worker + verifier | Retries must be bounded and classified. Repeated failure signatures, no-progress detection, escalation and an external completion oracle are mandatory. |
| Orchestration tools | **rejected as foundation** | 007 control plane | A board or session UI can display work. It cannot become the owner of run state, evidence or verdicts. |
| Managed/cloud-hosted agents | **out of scope for the core** | local-first CLI/provider boundary | A hosted provider may be added later, but provider location must not change security, evidence or completion policy. |
| Sandboxing | **partial** | `ProcessBoundary`; real Sandboy backend remains pending until merged and promoted | A worktree and `cwd` are not a sandbox. Production authority requires externally enforced and attested confinement; silent downgrade is forbidden. |
| Permissions | **partial** | explicit provider modes, deny rules and process boundary | Model permission settings are defense in depth. They do not replace OS enforcement or capability minimization. |
| Hooks and command validators | **rejected as sole authority** | optional pre-execution defense | Hooks may reject suspicious actions, but a probabilistic or pattern-based validator is not a security boundary. |
| Prompt-injection defense | **partial** | trusted/untrusted separation, restricted judge/invoke paths, sandbox roadmap | Repository instructions, fetched documents, tool output and third-party MCP/config are untrusted unless explicitly promoted. Content cannot grant itself capabilities. |
| Structural code linting | **enforced** | compiler lints, clippy restrictions, fuzzing, Kani and targeted tests | Repeated machine-detectable defects become mechanical rules. Reviews focus on concrete P0/P1 failure scenarios. |
| Pre-commit gates | **optional convenience** | CI and independent re-gate are authoritative | Local hooks improve feedback speed but are bypassable. A clean checkout and server-side gate decide acceptance. |
| Tracing | **enforced for run evidence; partial for model internals** | canonical events, artifacts and replay | Record the externally observable execution path. Private chain-of-thought is neither required nor an acceptable correctness dependency. |
| Logging | **partial** | canonical run/event artifacts; raw `o7 run` / `o7 invoke` stdout/stderr still captured whole | Structured canonical events and selected artifacts exist, but raw provider/agent stdout and stderr are still stored whole on some production paths — root `o7 run` writes the full captured agent stdout to `agent.stdout`, and `o7 invoke` writes the full stdout/stderr to `stdout.raw` / `stderr.log`. Those captures are not generally bounded or redacted; credential safety currently relies partly on process configuration and review, not a complete durable-output enforcement boundary. A future accepted contract must bound capture size, classify truncation, redact or prevent secret-bearing output, and preserve replay. "Log everything" is not a safe policy. |
| Metrics | **partial** | verdicts, gate outcomes, cleanup and replay integrity | Outcome metrics are authoritative. Add latency, tokens, cost, tool-call counts, retries and time-in-state without weakening redaction or evidence semantics. |
| Outcome evaluation | **enforced** | reducer, verifier, CI and acceptance evidence | "The agent says done" is never acceptance. Tests, verifier results, artifacts and merge/deploy outcomes decide success. |

## Binding invariants

These rules apply regardless of provider, model, workflow format or UI.

1. **External evidence outranks agent self-report.** Completion is decided by the
   reducer, verifier and required gates.
2. **Failure to obtain trustworthy evidence is `ERROR`, not `PASS` or ordinary
   `FAIL`.** Broken observation must never become a green run.
3. **Durable state lives outside the model.** The ledger and artifacts are
   authoritative; context, summaries and memory are projections.
4. **The UI is not a control-plane authority.** It may read or request actions,
   but canonical transitions occur through the run-state owner.
5. **A worktree is isolation for changes, not a security boundary.** Filesystem,
   process-tree, network, environment and descriptor restrictions require
   external enforcement.
6. **Security policy is not prompt text.** Provider permissions, hooks and
   deny-lists are defense in depth beneath a fail-closed process boundary.
7. **Memory is artifact-derived and provenance-preserving.** A model may propose a
   memory item; it may not write canonical project truth directly.
8. **Retries are bounded.** Every retry needs a classified cause, an attempt
   identity, a stop condition and no-progress detection.
9. **Claims about the current tree are revision-bound.** A document that says
   `current`, `today`, `implemented` or `absent` must name the verified commit or
   be generated from current authorities.
10. **Changing provider or model cannot weaken policy.** Routing, caching or
    provider failover must not alter sandbox, evidence, verdict or approval
    requirements.

## Current gaps, in dependency order

1. Promote a real, fully attested Sandboy boundary into the production run path.
2. Complete one live provider vertical through worker observations, canonical
   ledger events, verifier and outcome evidence.
3. Add operational metrics with explicit redaction and retention rules.
4. Build the bounded task-control loop: retry, repair, no-progress detection,
   escalation and stop.
5. Add artifact-derived persistent memory and task-scoped knowledge retrieval.
6. Add reusable workflows only from procedures that have repeated enough to earn
   a stable contract.

## Documentation drift rule

Historical evidence remains valuable, but it must look historical.

A snapshot document must carry at least:

```yaml
status: historical
verified_at_sha: <commit>
verified_at_date: <YYYY-MM-DD>
superseded_by: <path-or-none>
scope: <what was inspected>
```

Files whose names contain `current-state` are especially dangerous when their
claims are revision-bound. New current-state documentation should either be
mechanically generated or fail a documentation gate when its verification commit
is no longer an ancestor of the intended authority.

This is a future `o7-observer` seam: deterministic checks establish filenames,
references, revisions and declared implementation status; semantic review then
checks whether the prose still agrees with the code and evidence. The LLM should
inspect drift, not invent the source of truth it is comparing against.
