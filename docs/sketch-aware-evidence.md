Proposal: Sketch-Aware Agent Execution, Context Selection, and Risk Control

Target repository

"PhysShell/007"

Suggested file:

"docs/proposals/P-030-sketch-aware-agent-execution.md"

Summary

007 should use compact data structures and sketch-style summaries to make agentic coding runs safer, more explainable, and less wasteful.

This proposal fits the existing 007 direction:

- strict agent task contracts;
- "task.yaml" / task rules;
- "plan.o7plan";
- verification artifacts;
- effect ledgers;
- declared intent versus actual diff;
- integration with OwnAudit as the gate/report layer;
- integration with Own.NET as a source of analyzer facts and repair constraints.

The core idea:

Every agent run should leave behind compact evidence about:
- what it touched;
- what it repeatedly failed at;
- what context it used;
- what files/symbols became hotspots;
- how risky the diff was;
- how similar the run was to previous failed runs.

Without this, agentic coding becomes “trust me bro, I changed only what mattered”. That sentence should be illegal in CI.

Problem

Agent runs can fail in boring but dangerous ways:

- touching too many files;
- revisiting the same wrong fix repeatedly;
- missing relevant context;
- overfitting to noisy diagnostics;
- changing architecture-sensitive files accidentally;
- producing broad diffs for narrow tasks;
- repeating previous failed patterns;
- hiding risk behind a plausible summary.

Traditional logs are too verbose. Final summaries are too flattering. Humans are bad at both, a stunning double feature.

007 needs small, machine-checkable summaries that can guide execution and later feed OwnAudit.

Proposed solution

Add a sketch-aware evidence layer to 007.

Suggested module:

src/o7/evidence/
  context_index.*
  effect_sketch.*
  risk_sketch.*
  similarity.*
  run_summary.*

The exact language/module layout can follow the existing 007 implementation. The important part is the contract.

Evidence produced per run

Each 007 run should produce:

artifacts/o7/runs/<run-id>/
  action-plan.json
  effect-ledger.json
  touched-files.bitmap.json
  symbol-hotspots.json
  diagnostic-heavy-hitters.json
  latency-summary.json
  context-selection.json
  similarity-groups.json
  risk-summary.json

This is not “more logs”. Logs are where signals go to die. This is structured evidence.

1. Bitmap indexes for touched files and symbols

Assign stable ids during a run:

- file id;
- symbol id;
- diagnostic id;
- test id;
- rule id.

Use bitmap-style sets for:

- files declared in plan;
- files actually touched;
- files touched by generated patch;
- files covered by tests;
- files flagged by analyzers;
- files considered architecture-sensitive;
- files outside allowed task scope.

Then compute:

UnexpectedTouches =
    ActuallyTouchedFiles
    AND NOT AllowedFiles

SensitiveTouches =
    ActuallyTouchedFiles
    AND ArchitectureSensitiveFiles

UntestedTouches =
    ActuallyTouchedFiles
    AND NOT FilesCoveredByExecutedTests

This gives 007 a hard mechanism for detecting scope drift.

2. Top-K / Count-Min for repeated failure patterns

Use heavy-hitter summaries for:

- most repeated diagnostics during a run;
- files repeatedly modified and reverted;
- tests repeatedly failing;
- analyzer rules repeatedly triggered;
- commands repeatedly failing;
- prompt/task categories that produce bad diffs.

Example:

{
  "schema": "o7.heavy_hitters.v1",
  "kind": "failed_commands",
  "top": [
    {
      "key": "dotnet test",
      "count": 4,
      "lastExitCode": 1
    }
  ]
}

This helps distinguish:

The agent is making progress

from:

The agent is headbutting the same wall with improved formatting

Important distinction. Civilization depends on it.

3. t-digest/DDSketch-style summaries for execution timing

Track p50/p95/p99 for:

- context retrieval;
- build/test commands;
- analyzer runs;
- patch generation;
- verification gates;
- report generation.

This helps 007 understand where runs become expensive and where caching/context pruning matters.

4. SimHash/MinHash for similar failures and repeated bad patches

Fingerprint:

- failed diffs;
- diagnostic clusters;
- test failure messages;
- stack traces;
- review comments;
- rejected plans.

Then 007 can detect:

This proposed fix resembles a previously failed fix.
This failure message belongs to an existing failure group.
This PR repeats a known risky pattern.

That should affect risk scoring and context selection.

5. Context selection using compact indexes

007 should maintain compact indexes over repository facts:

- file-to-symbol;
- symbol-to-test;
- rule-to-file;
- file-to-architecture-layer;
- file-to-recent-changes;
- file-to-known-findings;
- file-to-agent-failure-history.

For a new task, 007 can select context by set operations:

RelevantContext =
    FilesMentionedInTask
    OR FilesWithDiagnostics
    OR TestsCoveringChangedSymbols
    OR SimilarPreviousFailures

Then prune:

ContextToAvoid =
    LargeUnrelatedFiles
    OR KnownNoisyFindings
    OR OutOfScopeModules

This is better than “stuff another 80k tokens into the prompt and pray”. That strategy is not context engineering. It is token landfill.

Proposed task contract additions

Extend task metadata with explicit risk and evidence expectations.

Example:

schema: o7.task.v1
id: fix-own014-cloud-mapping
scope:
  allowed_paths:
    - src/Own.Net.Analyzers/**
    - tests/Own.Net.Analyzers.Tests/**
  forbidden_paths:
    - src/LegacyRuntime/**
risk_budget:
  max_touched_files: 8
  max_architecture_sensitive_files: 0
  require_tests_for_touched_files: true
evidence:
  require_effect_ledger: true
  require_touched_files_bitmap: true
  require_diagnostic_heavy_hitters: true
  require_similarity_check: true

Risk summary output

Each run should emit:

{
  "schema": "o7.risk_summary.v1",
  "runId": "2026-07-06-own014-fix",
  "declaredIntentMatchedDiff": true,
  "touchedFiles": 5,
  "unexpectedTouchedFiles": 0,
  "architectureSensitiveTouchedFiles": 0,
  "testsExecuted": ["Own.Net.Analyzers.Tests"],
  "repeatedFailureGroups": [],
  "riskLevel": "low"
}

OwnAudit can then ingest this directly.

MVP

The MVP should include:

1. touched-file set tracking;
2. allowed-vs-actual scope comparison;
3. Top-K failed commands / diagnostics;
4. simple latency summary for commands;
5. risk summary JSON;
6. OwnAudit-compatible export.

No LLM magic required. No vector database required. No heroic rewrite required. Just evidence. Brutally inconvenient for bad automation, which is exactly the point.

Phase 2

Add:

- SimHash for failed patches and diagnostics;
- context selection index;
- previous-run comparison;
- hot-file avoidance;
- Own.NET analyzer fact ingestion;
- policy-based abort or warning when risk budget is exceeded.

Phase 3

Add:

- run history;
- trend analysis through OwnAudit;
- task templates using known risk profiles;
- automatic “do not repeat this failed strategy” hints;
- support for architecture-sensitive repair plans.

Non-goals

007 should not:

- become a monitoring platform;
- store all raw logs forever;
- block every run because a sketch estimate is scary;
- pretend approximate evidence is exact;
- hide broad diffs behind nice summaries;
- let the agent mutate files outside declared scope without evidence.

The core rule:

Declared intent must be checked against actual effects.

Without that, 007 is just a code generator with a clipboard and delusions of governance.

Acceptance criteria

This proposal is successful when:

- every run can report declared scope versus actual touched files;
- repeated diagnostics and failing commands are visible;
- expensive steps are summarized by latency percentiles;
- risk summary can be consumed by OwnAudit;
- task contracts can declare risk budgets;
- out-of-scope changes are detected automatically;
- future context selection can use evidence from previous runs.

Expected benefit

007 becomes safer and more useful as an agent runner:

- smaller accidental blast radius;
- better PR hygiene;
- clearer failure diagnosis;
- less repeated bad work;
- stronger bridge to OwnAudit;
- better reuse of Own.NET analyzer facts;
- more trustworthy automation.

The target is not “AI writes code”. That part is already cheap. The target is “AI writes code inside a constrained, inspectable, evidence-producing system”. That is the difference between engineering and a stochastic raccoon with commit access.