Proposal: Agent Memory Layer for 007

Status

Draft.

Target repositories

- "PhysShell/007"
- "PhysShell/OwnAudit"
- "PhysShell/Own.NET"

Summary

Introduce an agent memory layer for "007" that stores, retrieves, and reuses engineering context from past agent runs.

The goal is not to give the LLM vague “memory”. The goal is to give "007" a structured, provenance-backed operational memory over:

- previous runs;
- tasks;
- prompt versions;
- touched files;
- diffs;
- gate results;
- failed attempts;
- successful fixes;
- known traps;
- analyzer findings;
- human-confirmed decisions.

The memory layer must remain subordinate to "007".

"007" remains the orchestrator, authority, gate runner, artifact harvester, and policy boundary. The memory system is an index and recall layer over trusted run artifacts, not a replacement for "runs/", ".007/gate.toml", TaskSpec, O7Plan, or deterministic analyzers.

Motivation

"007" already has the right execution shape:

isolate -> run agent -> gate -> harvest artifacts

The current MVP stores canonical run artifacts:

runs/<target>/<run-id>/
  task.md
  meta.json
  agent.stdout
  diff.patch
  gate/
    <name>.log
    verdict.json

This is enough for a single run. It is not enough for a growing agentic engineering workflow.

Without a memory layer, every new run starts too cold:

- the agent does not know which fixes were already tried;
- previous gate failures are not surfaced automatically;
- prompt regressions are hard to compare;
- false-positive triage decisions are rediscovered;
- similar findings are not clustered across runs;
- failed patches do not become reusable negative examples;
- successful fixes do not become known repair patterns.

The result is a system that records evidence but does not learn from it.

That is not AgentOps. That is an archive with a superiority complex.

Design principle

The memory layer must follow one rule:

Memory is derived from artifacts.
Artifacts are not derived from memory.

A memory item is not trusted because an agent wrote it. It is trusted only to the degree that it is backed by:

- a run record;
- a gate verdict;
- a deterministic analyzer result;
- a source span;
- a diff;
- a prompt/task version;
- a human confirmation.

Every memory entry must have provenance.

A memory entry without provenance is not knowledge. It is a rumor with JSON formatting.

Non-goals

This proposal does not introduce:

- autonomous long-term agent self-editing;
- agent-written trusted facts;
- direct write access from the LLM into canonical memory;
- replacement of "runs/";
- replacement of ".007/gate.toml";
- replacement of TaskSpec / O7Plan;
- replacement of deterministic Own.NET / OwnAudit analysis;
- cloud-only memory;
- team-wide shared state as the first step.

The first version should be local, boring, append-only, inspectable, and hard to lie to.

Prior art: agentmemory

"rohitg00/agentmemory" is a persistent memory server for coding agents. It supports Claude Code, Codex CLI, Cursor, Gemini CLI, OpenCode and other MCP-compatible clients.

Relevant ideas:

- automatic capture through lifecycle hooks;
- MCP and REST access;
- BM25 + vector + graph retrieval;
- memory lifecycle and consolidation;
- provenance-oriented recall;
- multi-agent role tags;
- real-time viewer;
- local-first deployment;
- optional context injection.

Useful parts for "007":

- memory server as external process;
- REST/MCP recall surface;
- session/run indexing;
- hybrid search;
- provenance traceability;
- optional read-only context retrieval for agents.

Risky parts for "007":

- letting the agent write canonical memory directly;
- injecting memory into every tool call too early;
- enabling write-capable MCP tools during full-auto runs;
- treating compressed agent observations as verified facts;
- mixing private subscription-auth details into shared memory;
- depending on regex-only secret stripping as a complete security boundary.

Conclusion: "agentmemory" is useful as a memory runtime, but "007" should wrap it behind its own artifact-derived adapter.

Proposed architecture

             +------------------+
             |   TaskSpec       |
             |   task.yaml      |
             +--------+---------+
                      |
                      v
             +------------------+
             |  007 Context     |
             |  Builder         |
             +--------+---------+
                      |
             queries memory
                      |
                      v
+------------------+       +----------------------+
| agentmemory      |<----->|  007 Memory Adapter  |
| or local store   |       |  provenance filter   |
+------------------+       +----------------------+
                      |
                      v
             +------------------+
             | rendered task.md |
             | context.md       |
             +--------+---------+
                      |
                      v
             +------------------+
             | isolated run     |
             | claude/codex     |
             +--------+---------+
                      |
                      v
             +------------------+
             | gates / re-audit |
             +--------+---------+
                      |
                      v
             +------------------+
             | run record       |
             | runs/...         |
             +--------+---------+
                      |
              ingest after run
                      |
                      v
             +------------------+
             | memory facts     |
             | with provenance  |
             +------------------+

Core rule: 007 writes memory, not the agent

During the first phases, the agent must not directly write trusted memory.

Allowed:

007 -> memory

Allowed later, read-only:

agent -> memory recall

Not allowed initially:

agent -> memory save

The reason is simple: an LLM can hallucinate, rationalize, omit failed checks, or claim that a finding is fixed because the vibes aligned under a full moon.

"007" can write memory after it has:

- the actual diff;
- the actual gate result;
- the actual run metadata;
- the actual analyzer output;
- the actual artifacts.

That is the right trust boundary.

Memory item types

RunMemory

Represents one "o7 run".

{
  "kind": "o7.run",
  "schema": 1,
  "run_id": "2026-07-06T12-30-44Z_abcd1234",
  "target": "OwnAudit",
  "repo": "PhysShell/OwnAudit",
  "base_commit": "abc123",
  "engine": "claude-cli",
  "model": "claude-sonnet",
  "verdict": "FAIL",
  "task_id": "ownaudit.fix.own001.top",
  "prompt_module": "ownaudit.fix-own001",
  "prompt_version": "0.1.0",
  "changed_files": [
    "src/BrokerDataClasses/KTSGoods2.cs",
    "tests/SubscriptionLeakTests.cs"
  ],
  "failed_gates": [
    "no-new-findings"
  ],
  "provenance": {
    "meta": "runs/OwnAudit/<run-id>/meta.json",
    "diff": "runs/OwnAudit/<run-id>/diff.patch",
    "gate_verdict": "runs/OwnAudit/<run-id>/gate/verdict.json"
  }
}

TaskMemory

Represents a task contract.

{
  "kind": "o7.task",
  "schema": 1,
  "task_id": "ownaudit.fix.own001.top",
  "target": "OwnAudit",
  "diagnostic": "OWN001",
  "category": "subscription-token",
  "constraints": {
    "max_files_changed": 3,
    "require_tests": true,
    "require_reaudit": true,
    "forbid_suppression_only_fix": true
  },
  "provenance": {
    "task": "runs/OwnAudit/<run-id>/task.yaml"
  }
}

GateMemory

Represents a gate step result.

{
  "kind": "o7.gate",
  "schema": 1,
  "run_id": "<run-id>",
  "gate": "no-new-findings",
  "status": "FAIL",
  "reason": "2 new OWN001 findings appeared",
  "log": "runs/OwnAudit/<run-id>/gate/no-new-findings.log"
}

FindingMemory

Represents a finding encountered during an audit/fix/triage run.

{
  "kind": "ownaudit.finding",
  "schema": 1,
  "finding_id": "STS:BrokerDataClasses/KTSGoods2:customerChanged",
  "diagnostic": "OWN001",
  "category": "subscription-token",
  "file": "src/BrokerDataClasses/KTSGoods2.cs",
  "confidence": "high",
  "status": "real",
  "source": "OwnAudit",
  "provenance": {
    "findings": "artifacts/findings.json",
    "run_id": "<run-id>"
  }
}

FixPatternMemory

Represents a successful repair pattern.

{
  "kind": "o7.fix_pattern",
  "schema": 1,
  "name": "dispose-unsubscribe-event-token",
  "diagnostic": "OWN001",
  "applies_when": [
    "event subscription acquired",
    "subscription owner has Dispose method",
    "no matching unsubscribe/release exists"
  ],
  "successful_runs": [
    "<run-id>"
  ],
  "failed_runs": [],
  "provenance": {
    "source_run": "runs/OwnAudit/<run-id>/meta.json",
    "diff": "runs/OwnAudit/<run-id>/diff.patch"
  }
}

FailurePatternMemory

Represents a failed attempt that should be avoided.

{
  "kind": "o7.failure_pattern",
  "schema": 1,
  "name": "suppression-without-fix",
  "diagnostic": "OWN001",
  "symptom": "Patch suppresses or hides the finding instead of releasing the resource.",
  "failed_gate": "no-new-findings",
  "failed_runs": [
    "<run-id>"
  ],
  "avoidance_rule": "Do not accept suppress-only patches for OWN001 unless a human decision explicitly allows it.",
  "provenance": {
    "source_run": "runs/OwnAudit/<run-id>/meta.json",
    "gate_log": "runs/OwnAudit/<run-id>/gate/no-new-findings.log"
  }
}

DecisionMemory

Represents human-confirmed decisions.

{
  "kind": "o7.decision",
  "schema": 1,
  "id": "decision.ownaudit.ai-is-not-source-of-truth",
  "title": "AI is not the source of truth for diagnostics",
  "body": "Analyzer output, tests, SARIF, source spans and gates are authoritative. AI may explain, triage or propose fixes, but it cannot certify diagnostics by itself.",
  "status": "accepted",
  "confidence": "human-confirmed",
  "provenance": {
    "doc": "docs/agentops-promptops.md"
  }
}

Trust levels

Memory entries should have explicit trust levels.

agent-claimed
artifact-derived
gate-derived
analyzer-derived
human-confirmed
superseded
rejected

Rules:

- "agent-claimed" must not be injected as authoritative context.
- "artifact-derived" can be used as background context.
- "gate-derived" can be used for fix/failure history.
- "analyzer-derived" can be used for finding context.
- "human-confirmed" can be used as durable project guidance.
- "superseded" must be hidden by default.
- "rejected" can be used only as a negative example.

Commands

"o7 memory ingest-run"

Ingests one run record into memory.

o7 memory ingest-run runs/OwnAudit/<run-id>

Responsibilities:

- parse "meta.json";
- parse "gate/verdict.json";
- parse "diff.patch";
- optionally parse "task.yaml";
- optionally parse "prompt.meta.json";
- optionally parse "agent.trace.jsonl";
- derive memory entries;
- attach provenance to every entry;
- redact secrets before storage;
- refuse to ingest incomplete records unless "--allow-partial" is passed.

Example:

o7 memory ingest-run runs/OwnAudit/2026-07-06T12-30-44Z_abcd1234

Output:

ingested:
  runs: 1
  tasks: 1
  gates: 4
  files: 2
  findings: 3
  fix_patterns: 0
  failure_patterns: 1

"o7 context build"

Builds a pre-run context brief.

o7 context build \
  --repo ../OwnAudit \
  --task tasks/fix-own001-top.task.yaml \
  --out context.md

Responsibilities:

- read TaskSpec;
- query memory for similar tasks;
- query memory for same diagnostic;
- query memory for same files;
- query memory for same failed gates;
- query memory for same prompt module/version;
- rank results;
- emit concise Markdown context;
- store query metadata for reproducibility.

Output files:

context.md
context.meta.json

Example "context.meta.json":

{
  "schema": 1,
  "task_id": "ownaudit.fix.own001.top",
  "queries": [
    "diagnostic:OWN001 category:subscription-token",
    "gate:no-new-findings status:FAIL",
    "prompt_module:ownaudit.fix-own001"
  ],
  "memory_backend": "agentmemory",
  "token_budget": 2000,
  "result_count": 12
}

"o7 memory recall"

Manual query interface.

o7 memory recall --finding OWN001
o7 memory recall --file src/BrokerDataClasses/KTSGoods2.cs
o7 memory recall --gate no-new-findings
o7 memory recall --prompt ownaudit.fix-own001
o7 memory recall --run <run-id>

This gives the human operator direct visibility into what the system “knows”.

"o7 memory audit"

Audits stored memory.

o7 memory audit

Checks:

- entries without provenance;
- entries pointing to missing artifacts;
- duplicate memories;
- stale prompt versions;
- superseded decisions still being retrieved;
- rejected memories being injected as positive context;
- secrets accidentally present in memory payloads.

A memory system without audit is just a haunted cache.

Context brief format

"context.md" should be short, explicit, and provenance-backed.

Example:

# 007 Context Brief

## Similar successful runs

- `run-2026-07-01-a12f`: fixed `OWN001` by adding unsubscribe in `Dispose`.
  - Verdict: PASS
  - Relevant file: `src/BrokerDataClasses/KTSGoods2.cs`
  - Diff: `runs/OwnAudit/run-2026-07-01-a12f/diff.patch`

## Similar failed runs

- `run-2026-07-02-b44c`: failed `no-new-findings`.
  - Cause: patch removed finding from artifacts instead of fixing source.
  - Avoid: do not modify `artifacts/**`.

## Known constraints

- AI is not the source of truth for diagnostics.
- `OWN001` fixes must release the acquired subscription token.
- Suppress-only fixes are rejected unless human-confirmed.

## Required gates

- `dotnet-test`
- `ownaudit-smoke`
- `no-new-findings`

The agent receives this as context, not as authority. Gates remain authority.

Integration with TaskSpec and O7Plan

This memory layer should be designed around the strict task/action model.

Recommended run flow:

task.yaml
  ↓
o7 context build
  ↓
task.md + context.md
  ↓
agent run
  ↓
plan.o7plan + diff.patch + summary.md
  ↓
007 parses O7Plan
  ↓
007 policy-checks plan
  ↓
gates / re-audit
  ↓
run record
  ↓
o7 memory ingest-run

Memory should help the agent produce a better plan, but O7Plan and gates decide whether the output is acceptable.

Backend options

Option A: agentmemory

Use "agentmemory" as the first backend.

Pros:

- already supports MCP and REST;
- supports hybrid search;
- supports multiple agents;
- has viewer;
- local-first;
- fast experiment.

Cons:

- Node runtime in a Rust project;
- memory model is generic, not 007-specific;
- agent hooks may capture too much;
- regex redaction is not enough as a hard security boundary;
- direct MCP write access must be restricted.

Recommended use:

007 -> REST/MCP adapter -> agentmemory

Do not let full-auto agents write trusted memory directly.

Option B: local SQLite memory index

Build a minimal Rust-native memory store.

Pros:

- simple;
- inspectable;
- no Node dependency;
- easier to test;
- easier to enforce schema;
- easier to keep private.

Cons:

- no ready MCP;
- no hybrid vector/graph search initially;
- more implementation work.

Recommended if "agentmemory" proves too broad or too leaky.

Option C: graph memory later

A graph backend can come later for relationships such as:

Run -> Task
Run -> GateStep
Run -> TouchedFile
Finding -> File
Patch -> Finding
Decision -> SupersedesDecision
FailurePattern -> Gate
FixPattern -> Diagnostic

Do not start here. A graph without real run data is architecture cosplay.

Security and privacy

Secret handling

Memory ingest must redact:

- API keys;
- tokens;
- bearer headers;
- GitHub tokens;
- npm tokens;
- JWTs;
- private paths if configured;
- subscription-auth details;
- local usernames if configured;
- environment variables marked private.

Support explicit private blocks:

<private>
anything here must not enter memory
</private>

Memory scope

Default scope:

private-local

Do not enable team memory until:

- schema is stable;
- memory audit exists;
- redaction is tested;
- write rules are enforced;
- human-confirmed decisions are separated from agent-claimed notes.

MCP policy

Initial MCP mode:

read-only

Allowed tools:

- recall;
- smart search;
- file history;
- session history.

Forbidden initially:

- remember;
- forget;
- governance delete;
- team share;
- action mutation;
- lease mutation;
- mesh sync.

The agent should not be able to rewrite the memory that will later be used to judge it. That is how you build a self-licking ice cream cone with a shell prompt.

Failure modes

Hallucinated memory

Problem:

Agent writes: "OWN001 fixed by adding Dispose unsubscribe."
Gate says: FAIL.

Mitigation:

- agent-written memory is "agent-claimed";
- only "gate-derived" entries can become fix patterns;
- failed runs become negative examples.

Stale prompt memory

Problem:

Prompt v0.1.0 succeeded.
Prompt v0.2.0 regressed.
Memory retrieves old guidance without version distinction.

Mitigation:

- every run memory stores prompt module and version;
- context builder groups by prompt version;
- superseded prompt decisions are hidden by default.

Secret leakage

Problem:

agent.stdout or trace contains token-like material.

Mitigation:

- redact before ingest;
- keep raw artifacts in private "runs/";
- do not export memory by default;
- add "o7 memory audit --secrets".

Garbage recall

Problem:

Memory returns semantically similar but operationally irrelevant runs.

Mitigation:

- retrieval must combine:
  - task id;
  - diagnostic;
  - gate;
  - file path;
  - prompt module;
  - verdict;
  - trust level;
- context builder must cap result count;
- context brief must include provenance.

Self-reinforcing bad fix

Problem:

Failed patch is retrieved as suggested fix.

Mitigation:

- failed runs are indexed under "FailurePatternMemory";
- only PASS runs can create "FixPatternMemory";
- context brief separates “successful runs” and “failed runs”.

Minimal implementation plan

Phase 0: manual experiment

Run "agentmemory" locally and verify:

- server starts;
- viewer works;
- REST/MCP recall works;
- data stays local;
- redaction behavior is acceptable;
- Codex/Claude integration does not break normal work.

No 007 code changes yet.

Phase 1: artifact-derived ingest

Add:

o7 memory ingest-run <run-dir>

Implement:

- "MemoryBackend" trait;
- "AgentMemoryBackend";
- "NoopMemoryBackend";
- redaction pass;
- JSON payload schema;
- provenance validation.

Trait sketch:

pub trait MemoryBackend {
    fn save(&self, item: MemoryItem) -> anyhow::Result<()>;
    fn search(&self, query: MemoryQuery) -> anyhow::Result<Vec<MemoryHit>>;
}

Phase 2: context builder

Add:

o7 context build --task <task.yaml> --repo <path> --out context.md

Implement:

- TaskSpec-based query generation;
- memory search;
- result ranking;
- token/char budget;
- "context.md";
- "context.meta.json".

Phase 3: run integration

Extend "o7 run":

o7 run \
  --repo ../OwnAudit \
  --base main \
  --task tasks/fix-own001-top.task.yaml \
  --memory-context auto

Behavior:

1. Build context before run.
2. Store "context.md" in run record.
3. Run agent.
4. Gate.
5. Ingest run after harvest.

Phase 4: trace-based memory

Switch from only "agent.stdout" to also storing:

agent.trace.jsonl

Use trace to extract:

- files read;
- files edited;
- commands run;
- repeated failures;
- tool loops;
- skipped verification;
- gate reflection quality.

This also supports future behavior profiling.

Phase 5: read-only MCP inside runs

Allow the agent to use memory recall during a run, but only in read-only mode.

Policy:

allow:
  memory_recall
  memory_smart_search
  memory_file_history
  memory_sessions

deny:
  memory_save
  memory_forget
  memory_team_share
  memory_action_create
  memory_lease
  memory_mesh_sync

Phase 6: trusted write-back after gates

Only "007" writes durable memory after gates.

Rules:

- PASS run can create "FixPatternMemory".
- FAIL run can create "FailurePatternMemory".
- ERROR run can create "RunMemory", but not fix/failure conclusions unless cause is known.
- Human review can promote memory to "human-confirmed".

Suggested repository layout

007/
  src/
    memory/
      mod.rs
      backend.rs
      agentmemory.rs
      item.rs
      query.rs
      ingest.rs
      context.rs
      redact.rs
      audit.rs

  schemas/
    memory-item.v1.schema.json
    context-meta.v1.schema.json

  docs/
    agent-memory-layer.md

  examples/
    memory/
      run-memory.json
      finding-memory.json
      fix-pattern-memory.json
      failure-pattern-memory.json

Run record layout extension:

runs/<target>/<run-id>/
  task.md
  task.yaml
  context.md
  context.meta.json
  meta.json
  prompt.rendered.md
  prompt.meta.json
  agent.stdout
  agent.trace.jsonl
  diff.patch
  memory.ingest.json
  gate/
    verdict.json
    *.log

Acceptance criteria

Phase 1 is complete when:

- "o7 memory ingest-run <run-dir>" works on a real run record;
- every memory item has provenance;
- memory ingest refuses missing "meta.json";
- redaction tests cover token-like strings;
- memory payloads are schema-valid;
- no agent direct-write path exists.

Phase 2 is complete when:

- "o7 context build" produces deterministic "context.md";
- context includes similar successful runs;
- context includes similar failed runs;
- context separates trusted facts from agent claims;
- context includes provenance links;
- context stays under a configured token/char budget.

Phase 3 is complete when:

- "o7 run --memory-context auto" stores "context.md";
- run metadata records memory backend and context query id;
- post-run ingest is automatic;
- failed gates are searchable in the next run.

Recommended first vertical slice

Target: OwnAudit false-positive / fix workflow.

Task:

Fix or triage one OWN001 subscription-token finding.

Memory queries before run:

diagnostic = OWN001
category = subscription-token
gate = no-new-findings
prompt_module = ownaudit.fix-own001

Expected context:

- previous OWN001 successful fixes;
- previous suppress-only failures;
- no-new-findings gate failures;
- human decision that AI is not source of truth;
- current prompt version history.

Expected post-run memory:

- run summary;
- touched files;
- finding status;
- gate verdict;
- failure pattern or fix pattern.

This is narrow enough to implement and useful enough to matter.

Final decision

Adopt a memory layer for "007", but only as an artifact-derived, provenance-backed recall system.

Do not start with autonomous agent memory.

Do not start with write-capable MCP.

Do not start with team memory.

Do not start with graph complexity.

Start with:

ingest run artifacts -> build context for next run -> preserve provenance -> gate everything

That gives "007" the thing it actually needs: not a model that “remembers”, but a harness that accumulates operational experience and can prove where that experience came from.