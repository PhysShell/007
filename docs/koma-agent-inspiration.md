Proposal: 007 как проверяемый agentic harness, а не клон terminal-agent

«Status: proposal
Scope: "007", Own.NET, OwnAudit, sandboy integration
Date: 2026-07-06
Intent: зафиксировать направление развития "007" после сравнения с "aula-id/koma" и текущими PR-ами "007".»

Summary

"koma" и "007" решают разные задачи.

"koma" — это интерактивный terminal coding agent: TUI, model/tool loop, sessions, project memory, prompt caching, short-send compression, MCP, web tools, background jobs.

"007" не должен становиться клоном "koma". Правильная ниша "007" — проверяемый agentic execution harness:

task contract
  → isolated run
  → gated execution
  → diff policy
  → run record
  → judge / replay / audit

То есть не “агент живёт в терминале”, а “агент выполняет ограниченный эксперимент, оставляет доказательства, проходит gate, а его поведение можно восстановить и проверить”.

Из "koma" стоит переносить не TUI и не product UX, а инженерные идеи:

- non-destructive context shaping;
- per-project memory, но под контролем;
- prompt-cache-friendly stable/volatile prompt split;
- background job model;
- project-scoped resumability.

Но порядок важен. Сначала "007" должен закрыть свой главный trust gap: "o7 run" и ".007/gate.toml" сейчас используют worktree как удобство, а не как настоящую security boundary. Worktree помогает cleanup/diff, но не запрещает чтение, запись наружу или network egress. Поэтому первые code milestones должны быть про "TaskContract", "DiffPolicy", gate timeout, sandboy-backed sandbox policy и реальный Own.NET run record.

Current state

На "main" уже есть основа:

- "o7 run": git worktree → "claude" full-auto → ".007/gate.toml" → harvest run record.
- "o7 judge": read-only FP-triage для analyzer findings.
- Run record: "task.md", "meta.json", "agent.stdout", "diff.patch", "gate/*.log".
- Verification layer: proptest, fuzz targets, Kani harnesses, curated lints, cargo-deny.
- Docs already name the core security problem: worktree is not confinement, deny-list is not sandbox, gate commands are arbitrary "bash -lc".

По открытым/последним PR-ам уже видна стратегическая линия:

- agentic coding discipline: task contracts, negative prompts, plan-then-build, diff policy gates, judge-run, trust levels;
- workflow scripting: typed but host-enforced workflows, scoped down to linear "workflow.toml";
- microVM/container roadmap for "run"/gate isolation;
- Zero Trust roadmap and CUE-based policy authoring;
- sandboy reconciliation: Landlock + seccomp wrap-the-child confinement for the "run"/gate slot;
- behavior profiling deferred until real trace stream exists.

Вывод: проблема не в отсутствии идей. Проблема в риске распухнуть в архитектурный космолёт до первого реально безопасного и полезного "o7 run".

Design position

007 should not compete with Koma on UX

Do not copy:

- full TUI;
- multi-tab chat;
- MCP marketplace;
- browser/web scraping;
- self-update;
- general-purpose assistant memory;
- interactive terminal agent lifestyle features.

That path turns "007" into another coding-agent shell. Там уже толпа, и все машут “AI-native workflow” как флагом на пожаре.

007 should compete on trust, auditability, and gated execution

"007" should own this niche:

Can an agent modify a real repo under a declared contract,
inside a defined boundary,
with machine-checkable gates,
with a durable record proving what happened?

For Own.NET/OwnAudit this is more valuable than yet another terminal chat. The target repos are legacy-heavy, analyzer-heavy, policy-heavy. The useful agent is not the one that types fastest. The useful agent is the one that can be constrained, judged, replayed, and blamed accurately when it does something stupid, because apparently we need software to produce receipts now.

Proposed architecture direction

1. TaskContract: machine-readable task scope

Add "task.o7.toml" as the machine-readable companion to "task.md".

Example:

schema = 1
id = "ownnet-async-rule-001"
title = "Add async void analyzer rule"

[scope]
repo = "Own.NET"
base = "main"
allowed_paths = [
  "src/Own.Analyzers/**",
  "tests/Own.Analyzers.Tests/**"
]
forbidden_paths = [
  "**/*.csproj",
  "Directory.Build.props",
  "global.json",
  "build/**"
]

[change]
kind = "analyzer-rule"
max_files_changed = 8
max_diff_lines = 500
allow_new_dependencies = false
allow_public_api_changes = false
allow_generated_files = false

[agent]
engine = "claude"
model = "opus"
max_turns = 12
mode = "full-auto"

[policy]
trust_level = "trusted-local"
network = "off"
write_boundary = "worktree"
secrets = "none"

[gates]
manifest = ".007/gate.toml"
required = ["format", "lint", "test"]

[output]
require_diff = true
require_run_record = true
require_trace = false

"task.md" remains human-readable. "task.o7.toml" becomes enforceable.

This prevents the classic agent failure mode:

User asks for one analyzer rule.
Agent rewrites project layout, edits build scripts, adds dependencies,
renames public APIs, and calls it "cleanup".

Technical term: absolute fucking shambles.

2. DiffPolicy gate

Before running project gates, "007" should validate the produced diff against the task contract.

Checks:

- changed paths are within "allowed_paths";
- no changed paths match "forbidden_paths";
- file count under limit;
- diff line count under limit;
- no dependency file changes unless explicitly allowed;
- no binary files unless explicitly allowed;
- no generated snapshots unless explicitly allowed;
- no public API changes unless explicitly allowed or approved by a later API-diff gate.

Output:

runs/<target>/<run-id>/
  policy/
    task-contract.normalized.json
    diff-policy.json
    path-policy.json

Verdict classes:

PASS
FAIL
ERROR
NOT_APPLICABLE

"DiffPolicy" should run before expensive gates. No point running tests on a diff that already touched forbidden paths. That’s not discipline, that’s letting the raccoon into the kitchen and then checking whether the soup tastes funny.

3. GateStep extensions: timeout and sandbox policy

Extend ".007/gate.toml" without breaking current manifests.

Current shape:

[[gate]]
name = "test"
cmd = "python tests/run_tests.py"
required = true

Proposed shape:

[[gate]]
name = "test"
cmd = "python tests/run_tests.py"
required = true
timeout_sec = 300
sandbox_policy = "worktree-no-net"

Policy examples:

none
worktree-readonly
worktree-write
worktree-no-net
windows-host

The first implementation can be simple:

- "timeout_sec": enforced by "o7";
- "sandbox_policy = "none"": current behavior;
- "sandbox_policy != "none"": route through "sandboy".

Expected artifacts:

runs/<target>/<run-id>/
  gate/
    test.log
    test.sandbox.json

"sandbox.json" should include:

{
  "policy": "worktree-no-net",
  "backend": "sandboy",
  "network": "blocked",
  "write_roots": ["<worktree>"],
  "read_roots": ["<worktree>"],
  "exit_code": 0,
  "violations": []
}

This is the first real code-level move from “worktree as convention” to “worktree as enforceable boundary”.

4. Real run record as the source of truth

The run record should be treated as the primary artifact, not just logs dumped somewhere.

Target shape:

runs/<target>/<run-id>/
  task.md
  task.o7.toml
  meta.json
  agent.stdout
  agent.trace.jsonl        # later
  diff.patch
  policy/
    task-contract.normalized.json
    diff-policy.json
  gate/
    <name>.log
    <name>.sandbox.json
    verdict.json
  judge/
    review.json            # later
  replay/
    manifest.json          # later

"meta.json" should eventually include:

{
  "schema": 1,
  "kind": "run",
  "run_id": "...",
  "target": "Own.NET",
  "base_commit": "...",
  "head_commit": "...",
  "engine": "claude",
  "model": "opus",
  "task_contract": "task.o7.toml",
  "trust_level": "trusted-local",
  "isolation": "sandboy",
  "verdict": "PASS",
  "policy_verdict": "PASS",
  "gate_verdict": "PASS",
  "diff_stats": {
    "files_changed": 4,
    "insertions": 120,
    "deletions": 18
  }
}

No dashboard, replay, profiling, judge-run, or memory layer should invent its own truth. Everything reads the record. Otherwise we get the classic enterprise masterpiece: six systems of record, all wrong in different fonts.

5. Agent trace stream before behavior profiling

Behavior profiling should stay deferred until "agent.trace.jsonl" exists.

"agent.stdout" as a single final object is not enough. It cannot support detectors like:

- tool loop;
- patch without localization;
- command spam;
- no gate reflection;
- suspicious file access;
- repeated failed fix attempts;
- “agent ignored task contract”.

Required trace event shape:

{"type":"assistant_message","turn":1,"text":"..."}
{"type":"tool_call","turn":1,"tool":"Read","args":{...}}
{"type":"tool_result","turn":1,"tool":"Read","exit_code":0,"summary":"..."}
{"type":"tool_call","turn":2,"tool":"Edit","args":{...}}
{"type":"gate_start","name":"test"}
{"type":"gate_end","name":"test","verdict":"FAIL"}

This enables later:

o7 profile runs/Own.NET/<run-id>
o7 replay runs/Own.NET/<run-id>
o7 judge-run runs/Own.NET/<run-id>

But not before the data exists. Building profiling before trace is astrology with JSON.

6. Repair loop, but only after policy and sandbox

The eventual loop:

agent patch
  → diff policy
  → gate
  → if fail: summarize relevant logs
  → agent repair
  → diff policy again
  → gate again
  → final harvest

Initial limit:

[repair]
enabled = true
max_attempts = 2
log_budget_lines = 200

Rules:

- repair cannot expand scope;
- repair cannot touch forbidden paths;
- each repair attempt gets its own diff and gate logs;
- final "meta.json" records attempt count.

Do not add repair loop before diff policy. Otherwise the agent gets multiple chances to make the mess larger. That’s not repair, that’s giving a raccoon a second screwdriver.

7. Workflow scripting: keep v1 boring

A future "workflow.toml" is useful, but v1 should be deliberately flat.

Example:

schema = 1
name = "ownnet-safe-agent-run"

[[step]]
kind = "plan"
task = "task.md"
out = "plan.md"

[[step]]
kind = "run"
task_contract = "task.o7.toml"

[[step]]
kind = "judge-run"
run = "$previous"

[[step]]
kind = "report"
format = "markdown"

Non-goals for v1:

- DAG;
- TypeScript SDK;
- plugin language;
- dynamic branching;
- multi-agent colony nonsense;
- user-defined shell hooks outside gate policy.

Workflow scripts should propose orchestration. The "o7" host enforces capabilities. A script should not get raw filesystem/network/shell authority.

Phased roadmap

Phase 1: make "o7 run" enforceable

Goal: one real Own.NET task can run with a task contract, diff policy, timeout, and sandboxed gates.

Deliverables:

- "task.o7.toml" parser;
- "TaskContract" model;
- "DiffPolicy" checker;
- "timeout_sec" on gate steps;
- "sandbox_policy" on gate steps;
- "sandboy" execution path for at least one Linux policy;
- policy artifacts in run record;
- one real Own.NET run record committed or attached as fixture/docs.

Acceptance criteria:

- forbidden path edit fails before gates;
- gate timeout kills the step and records "ERROR";
- sandbox policy produces "gate/<name>.sandbox.json";
- run record is complete enough for postmortem without rerunning anything.

Phase 2: trace and repair

Goal: make agent behavior observable enough for replay/judging.

Deliverables:

- switch Claude run to stream/verbose output where possible;
- persist "agent.trace.jsonl";
- define trace schema;
- add "repair.max_attempts";
- feed bounded gate-log summaries back into repair;
- record each repair attempt separately.

Acceptance criteria:

- "o7 run" can show what tool calls happened;
- failed gate logs are sliced, not dumped whole;
- repair cannot violate original "TaskContract";
- final run record distinguishes first patch from repair patches.

Phase 3: judge-run and replay

Goal: make run quality reviewable.

Deliverables:

- "o7 judge-run <run-dir>";
- check task adherence;
- check diff scope;
- check test/gate reflection;
- check suspicious tool behavior from trace;
- "o7 replay <run-dir>" dry reconstruction.

Acceptance criteria:

- judge-run emits machine-readable verdict;
- replay can summarize task, diff, gates, policy, and agent trace;
- failed runs are useful training/debug artifacts, not just piles of logs.

Phase 4: context and memory economics

Goal: borrow the useful part of "koma" without turning into "koma".

Deliverables:

- non-destructive summary of previous run records;
- project memory split:
  - committed ".007/memory.md";
  - private "~/.007/memories/<repo-hash>/learned.md";
- stable/volatile prompt split;
- prompt-cache-friendly stable head;
- recall from prior failed gates/diffs.

Rules:

- never rewrite original run records;
- generated memory must be distinguishable from human-authored policy;
- memory cannot override "TaskContract";
- memory cannot weaken sandbox/diff policy.

Phase 5: stronger isolation backends

Goal: support higher trust levels.

Deliverables:

- container/gVisor backend;
- microVM backend if justified;
- auth broker pattern for model calls from isolated execution;
- CUE-based composable policies.

Trust levels:

trusted-local:
  local repo, developer-owned, normal sandboy policy

semi-trusted:
  stricter filesystem, no network by default

untrusted:
  no host secrets, no ambient credentials, strong isolation required

Do not build microVM first. Start with policy and sandboy. MicroVM before task contract is just expensive cosplay.

Relationship to Koma

Ideas to borrow:

- dual rail context: full stored history + shaped API payload;
- stable/volatile prompt split;
- rolling summaries;
- project-scoped memory;
- background job observation;
- resumable project sessions.

Ideas not to borrow now:

- TUI-first UX;
- MCP-first extensibility;
- web browser scraping;
- model marketplace UX;
- broad interactive agent tool surface.

"koma" optimizes the agent conversation.

"007" should optimize the agent experiment.

That distinction matters.

Concrete next PR sequence

Recommended order:

1. Docs consolidation PR
   
   - link current roadmap docs;
   - mark overlaps;
   - define Phase 1 source of truth.

2. TaskContract PR
   
   - add "task.o7.toml";
   - parse/validate;
   - copy into run record.

3. DiffPolicy PR
   
   - path allow/deny;
   - file/diff limits;
   - dependency-file guard;
   - "policy/diff-policy.json".

4. Gate timeout PR
   
   - "timeout_sec";
   - timeout verdict;
   - log truncation rules.

5. Sandboy gate PR
   
   - "sandbox_policy";
   - run gate step through sandboy;
   - emit sandbox report.

6. Real Own.NET run PR
   
   - one actual constrained task;
   - include sanitized run record or docs summary;
   - document what broke.

7. Trace PR
   
   - "agent.trace.jsonl";
   - minimal event schema.

8. Repair loop PR
   
   - one bounded repair attempt;
   - policy enforced between attempts.

This order keeps the project honest. It forces "007" to become useful before it becomes grand.

Non-goals

This proposal explicitly does not recommend:

- replacing Claude Code or Codex interactive workflows;
- building a general terminal AI client;
- adding a TUI before run records are strong;
- letting workflow scripts execute arbitrary shell directly;
- treating classifier output as a security boundary;
- treating worktree isolation as a sandbox;
- adding microVMs before task contracts and diff policy;
- adding behavior profiling before trace events exist.

Final position

"007" should become the harness that answers:

What exactly did the agent try?
Was it allowed?
What did it change?
Did it pass gates?
Was it inside the sandbox?
Can we replay and judge it later?

If "koma" is “budget models, real work”, then "007" should be:

agent runs, real evidence

Not another shiny assistant shell. A controlled execution rig.

Less vibe. More receipts.