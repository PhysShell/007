# Task-Aware Reverse Source Generator for Agent Context

- **Status:** draft
- **Primary home:** `007` (`o7 context` orchestration and context budgeting)
- **Static-analysis provider:** Own.NET
- **Audit/risk-profile provider:** OwnAudit
- **Related docs:** [agent memory layer](agent-memory-layer.md), [FastContext](fastcontext.md), [Omnigraph](omnigraph.md), [agent output budgeter](agents-outputs-budgeter.md)

## Summary

Build a deterministic, task-aware context generator for coding agents.

The working analogy is a **source generator in reverse**:

```text
normal source generator:
  semantic program facts -> generated source

reverse source generator:
  repository source -> compact semantic facts -> agent context
```

The output is not a concatenated repository dump and not an LLM-authored summary. It is a provenance-backed context pack built from repository structure, symbols, dependencies, analyzer findings, task-specific risk rules, and an explicit token budget.

```text
repo + task + profile + budget
  -> deterministic extractors
  -> Context IR
  -> task-aware ranking
  -> context.md + context.json + context.meta.json
  -> Claude/Codex run
```

The product niche is deliberately narrow:

> **Task-aware context generation for legacy .NET/WPF analysis and repair.**

This is not another generic “put the repo in a prompt” tool. The agent should receive an inventory, route map, evidence trail, and risk register, not the whole warehouse.

## Problem

Coding agents repeatedly rediscover repository structure through `grep`, file reads, build-file inspection, and speculative navigation.

That causes several failures:

- token waste from rereading stable facts;
- tool-call waste from repeated discovery;
- weak architectural awareness;
- noisy prompts containing large irrelevant files;
- inconsistent context between runs;
- no reproducible explanation for why a file or symbol was selected;
- no domain awareness for WPF, MVVM, DevExpress, .NET Framework, or OwnAudit findings;
- poor separation between authoritative analyzer facts and agent guesses.

A large context window does not solve this. It merely permits a larger landfill.

## Prior-art landscape

Existing tools cover useful pieces of the problem, but usually stop at one layer.

| Category | Examples | Useful idea | Missing for this proposal |
|---|---|---|---|
| Repository packers | [Repomix](https://github.com/yamadashy/repomix), [Gitingest](https://github.com/coderamp-labs/gitingest), [code2prompt](https://github.com/mufeedvh/code2prompt) | deterministic file collection and LLM-friendly serialization | semantic structure, task-aware selection, domain risk profiles |
| Repository maps | [Aider repo map](https://aider.chat/docs/repomap.html), [RepoMapper](https://github.com/pdavis68/RepoMapper) | compact symbol maps under a context budget | legacy-.NET-specific facts, audit evidence, strict provenance contract |
| Persistent code graph / MCP | [codebase-memory-mcp](https://github.com/DeusData/codebase-memory-mcp), [RepoGraph](https://github.com/ozyyshr/RepoGraph) | structural graph, targeted traversal, persistent indexing | deterministic task pack contract owned by `007` |
| Semantic code search | [claude-context](https://github.com/zilliztech/claude-context) | retrieve relevant code without brute-force browsing | authoritative architecture and analyzer facts |
| Deterministic repository intelligence | [Repository Intelligence Graph](https://arxiv.org/abs/2601.10112) | evidence-backed architectural map exposed as LLM-friendly JSON | .NET/WPF task profiles and integration with `o7 run` / gates |
| Context compression | FastContext and generic compression middleware | reduce already-selected material | correct selection and evidence boundaries before compression |
| Agent memory | [`o7 memory`](agent-memory-layer.md), Omnigraph-style graphs | reuse previous runs, failures, decisions, and fixes | current-repository semantic extraction |

The conclusion is not “the idea already exists.” The conclusion is that the ecosystem has separately built packers, maps, graphs, search, memory, and compression. The missing product layer is the contract that composes them for a specific engineering task.

## Decision

Implement the context generator as a deterministic pipeline owned by `007`, with repository intelligence supplied by Own.NET and audit-specific evidence supplied by OwnAudit.

Optional LLM compression may run **after** deterministic selection. It must never be the source of authoritative facts.

```text
Own.NET   -> code/semantic/lifetime facts
OwnAudit  -> findings/taxonomy/risk profiles
007       -> task interpretation, selection, budgets, rendering, provenance
FastContext (optional) -> final compression or explanation only
Omnigraph / o7 memory  -> historical run evidence only
Claude/Codex -> executor, not repository cartographer
```

## Cross-repository responsibilities

### Own.NET

Own.NET should expose deterministic repository facts derived from Roslyn and its existing ownership/lifetime analysis.

Candidate facts:

- solutions, projects, target frameworks, project references;
- files, namespaces, types, members, signatures, source spans;
- callers, callees, overrides, interface implementations;
- event subscriptions and matching/unmatched releases;
- `IDisposable` ownership and disposal paths;
- DI registrations and lifetime relationships;
- WPF views, view models, bindings, commands, resources, and event handlers;
- `INotifyPropertyChanged` dependency edges;
- async/sync boundaries and blocking calls;
- SQL provider abstractions and provider-specific branches;
- DevExpress control usage and high-risk UI materialization paths;
- analyzer findings with rule, severity, confidence, and exact evidence.

Own.NET should not decide which facts fit a particular agent task. It produces the evidence-backed semantic inventory.

### OwnAudit

OwnAudit should define risk-oriented profiles and provide normalized findings.

Candidate profiles:

```text
wpf-memory-leaks
async-over-sync
sql-provider-compat
propertychanged-hell
devexpress-grid-performance
resource-lifetime
di-captive-dependencies
large-object-materialization
```

A profile may define:

- seed rules and diagnostics;
- important symbol/file kinds;
- graph edge weights;
- expansion depth;
- required tests and gates;
- known false-positive patterns;
- forbidden low-value paths;
- required evidence categories.

OwnAudit owns “what hurts for this audit task,” not generic code indexing.

### 007

`007` owns context assembly because it already owns the run contract, task, gates, and canonical run artifacts.

Responsibilities:

- parse TaskSpec / task Markdown;
- select a profile;
- request fresh repository facts;
- combine current facts with historical memory;
- rank candidates under explicit budgets;
- render human-readable and machine-readable outputs;
- record selection reasons and hashes;
- inject context into the isolated run;
- preserve the exact context pack in the run record;
- keep gates authoritative after the agent edits code.

## Architecture

```text
                         +------------------+
                         | TaskSpec / task  |
                         +--------+---------+
                                  |
                                  v
                         +------------------+
                         | Profile resolver |
                         +--------+---------+
                                  |
              +-------------------+-------------------+
              |                                       |
              v                                       v
   +----------------------+                 +----------------------+
   | Own.NET fact export  |                 | OwnAudit evidence    |
   | Roslyn + analyzers    |                 | findings + profiles  |
   +----------+-----------+                 +----------+-----------+
              |                                       |
              +-------------------+-------------------+
                                  v
                         +------------------+
                         |   Context IR     |
                         | evidence-backed  |
                         +--------+---------+
                                  |
                  +---------------+----------------+
                  |                                |
                  v                                v
         +------------------+             +----------------------+
         | current repo map |             | historical memory    |
         | symbols / edges  |             | runs / gates / ADRs  |
         +---------+--------+             +----------+-----------+
                   +----------------+-----------------+
                                    v
                         +------------------------+
                         | rank + expand + budget |
                         +-----------+------------+
                                     |
                                     v
                     +--------------------------------+
                     | context.md                     |
                     | context.json                   |
                     | context.meta.json              |
                     +---------------+----------------+
                                     |
                          optional deterministic-safe
                          final summarization layer
                                     |
                                     v
                            +------------------+
                            | Claude / Codex   |
                            +--------+---------+
                                     |
                                     v
                            +------------------+
                            | gates + harvest  |
                            +------------------+
```

## Context IR

The core artifact must be a stable, versioned IR rather than Markdown assembled directly from ad hoc queries.

Illustrative shape:

```json
{
  "schema": "o7.context-ir/1",
  "repository": {
    "name": "PhysShell/Own.NET",
    "commit": "abc123",
    "solution": "Own.NET.sln"
  },
  "task": {
    "id": "audit.wpf-memory-leaks.customer-window",
    "profile": "wpf-memory-leaks"
  },
  "entities": [
    {
      "id": "symbol:OwnSharp.Lifetimes.SubscriptionAnalyzer.Analyze",
      "kind": "method",
      "display": "SubscriptionAnalyzer.Analyze",
      "location": {
        "path": "frontend/roslyn/OwnSharp/SubscriptionAnalyzer.cs",
        "start_line": 41,
        "end_line": 118
      },
      "facts": [
        "reads event subscription operations",
        "emits OWN001"
      ],
      "evidence": [
        {
          "kind": "roslyn-symbol",
          "extractor": "Own.NET",
          "extractor_version": "<commit>",
          "source_hash": "sha256:..."
        }
      ]
    }
  ],
  "edges": [
    {
      "from": "symbol:CustomerViewModel..ctor",
      "type": "SUBSCRIBES_TO",
      "to": "symbol:EventBus.CustomerChanged",
      "evidence": ["source:src/CustomerViewModel.cs:37"]
    }
  ],
  "findings": [
    {
      "id": "OWN001:src/CustomerViewModel.cs:37",
      "rule": "OWN001",
      "confidence": "high",
      "evidence": ["sarif:artifacts/own.sarif#result-18"]
    }
  ],
  "selection": {
    "score": 0.93,
    "reasons": [
      "direct diagnostic match",
      "one hop from task seed symbol",
      "owns required regression test"
    ]
  }
}
```

### IR requirements

Every selected item must have:

- a stable identity;
- repository commit;
- source location or artifact pointer;
- producing extractor and version;
- selection score;
- plain-language selection reasons;
- trust level;
- content hash where practical.

The IR must distinguish:

```text
source-derived
analyzer-derived
gate-derived
memory-derived
human-confirmed
agent-claimed
```

`agent-claimed` material is never authoritative.

## Selection pipeline

### 1. Resolve task seeds

Seeds may come from:

- explicit paths or symbols in TaskSpec;
- diagnostic IDs such as `OWN001`;
- stack traces or build errors;
- issue text and domain terms;
- selected OwnAudit profile;
- changed files when repairing an existing patch.

### 2. Retrieve exact matches

Retrieve:

- matching symbols and source spans;
- findings for the requested rule/category;
- direct callers/callees;
- owning project and tests;
- related XAML/code-behind/view-model files;
- relevant configuration and registration sites.

### 3. Expand structurally

Use typed edges rather than raw text similarity:

```text
CALLS
REFERENCES
IMPLEMENTS
OVERRIDES
SUBSCRIBES_TO
RELEASES
BINDS_TO
REGISTERED_AS
DEPENDS_ON_PROJECT
COVERED_BY_TEST
PRODUCES_FINDING
TOUCHED_BY_RUN
FAILED_GATE
```

Expansion depth and edge weights are profile-specific.

### 4. Add historical evidence

Query `o7 memory` / Omnigraph for:

- previous successful and failed runs touching selected entities;
- known regressions;
- rejected fixes;
- accepted decisions;
- suppressions and false-positive rulings;
- gates that previously failed.

Historical evidence supplements the current repository map. It does not replace it.

### 5. Budget

Apply separate budgets:

```text
map budget       symbols, relationships, architecture
source budget    exact snippets or small files
finding budget   diagnostics and evidence
history budget   runs, decisions, failures
instruction budget task constraints and required gates
```

A single total-token limit is insufficient because one noisy category can consume everything.

### 6. Render

Emit three artifacts:

```text
context.md        concise agent-readable brief
context.json      selected entities, edges, findings, snippets
context.meta.json reproducibility metadata, budgets, hashes, queries
```

## Proposed CLI

```sh
o7 context build \
  --repo ../Own.NET \
  --base main \
  --task tasks/fix-own001.task.yaml \
  --profile wpf-memory-leaks \
  --budget 12000 \
  --out runs/_context/fix-own001
```

Generated files:

```text
runs/_context/fix-own001/
  context.md
  context.json
  context.meta.json
```

Then:

```sh
o7 run \
  --repo ../Own.NET \
  --base main \
  --task tasks/fix-own001.task.yaml \
  --context runs/_context/fix-own001 \
  --engine claude
```

Convenience mode may come later:

```sh
o7 run \
  --repo ../Own.NET \
  --base main \
  --task tasks/fix-own001.task.yaml \
  --context-profile wpf-memory-leaks \
  --context-budget 12000 \
  --engine claude
```

The explicit two-command flow should exist first. Hidden context generation makes failed runs harder to reproduce, and agent systems already contain enough invisible machinery to qualify as minor weather phenomena.

## Context brief format

```markdown
# 007 Task Context

## Task interpretation

Fix the high-confidence `OWN001` event-subscription leak in `CustomerViewModel` without suppressing the diagnostic.

## Authoritative findings

- `OWN001` at `src/CustomerViewModel.cs:37`
  - source: `artifacts/own.sarif#result-18`
  - source lifetime: `App`
  - captured object lifetime: `ViewModel`

## Relevant architecture

- `CustomerViewModel` is constructed by `CustomerPresenter`.
- `EventBus` is registered as a singleton.
- `CustomerWindow` owns the presenter and closes through `CloseCustomerCommand`.

## Files and spans to inspect

1. `src/CustomerViewModel.cs:24-91`
   - direct finding and lifecycle methods
2. `src/CustomerPresenter.cs:18-64`
   - owner and disposal path
3. `tests/CustomerWindowLifetimeTests.cs:1-143`
   - existing regression-test home

## Known failed approaches

- `run-42`: suppression-only patch; rejected by `no-new-findings` policy.
- `run-57`: unsubscribed from a different delegate instance; leak remained.

## Constraints

- do not edit `artifacts/**`;
- do not add a suppression;
- preserve .NET Framework 4.7.2 compatibility;
- add or update a regression test.

## Required gates

- `dotnet-test`
- `own-check`
- `no-new-findings`
```

## Determinism and reproducibility

The same inputs must produce the same Context IR and the same ranking unless an explicitly versioned ranking implementation changes.

Cache key:

```text
sha256(
  repository commit
  + task hash
  + profile version
  + extractor versions
  + ranking version
  + budget configuration
)
```

`context.meta.json` should record:

- repository and commit;
- dirty-worktree status;
- task hash;
- selected profile and version;
- extractor versions;
- ranking version;
- per-category budgets and actual use;
- omitted candidates and omission reasons;
- optional summarizer model/prompt/version;
- hashes of all emitted artifacts.

If optional LLM compression is used, both pre-compression and post-compression forms must be retained.

## Non-goals

This proposal does not attempt to:

- replace Roslyn or Own.NET analyzers;
- make an LLM the source of architectural truth;
- send the whole repository to a model;
- build a universal graph database in phase one;
- expose unrestricted write-capable MCP tools to full-auto agents;
- infer every runtime behavior statically;
- solve long-term memory inside the repository mapper;
- make context generation silently mutate source code;
- optimize for every language before the .NET/WPF path works.

## Implementation phases

### Phase 0: contract and fixtures

Deliver:

- `o7.context-ir/1` schema;
- one hand-authored golden fixture;
- one WPF leak task fixture;
- deterministic rendering tests;
- budget accounting tests.

Acceptance:

- identical inputs produce byte-identical JSON;
- every rendered claim points to evidence;
- no LLM dependency.

### Phase 1: structural repository map

Deliver:

- solution/project/file/symbol inventory;
- project-reference graph;
- symbol definitions and reference edges;
- test-project association;
- incremental cache by commit/file hash.

Acceptance:

- answer “where is this symbol, who references it, which tests cover its project?” without agent exploration;
- emit useful context for Own.NET itself.

### Phase 2: legacy .NET/WPF facts

Deliver:

- XAML/code-behind/view-model relationships;
- event subscription/release facts;
- `IDisposable` and lifetime facts;
- DI registration/lifetime facts;
- async-over-sync facts;
- provider-compatibility facts.

Acceptance:

- generate evidence-backed packs for at least three real STS tasks;
- include exact source spans and analyzer provenance.

### Phase 3: task profiles and ranking

Deliver:

- profile schema;
- initial profiles: `wpf-memory-leaks`, `async-over-sync`, `sql-provider-compat`;
- typed-edge expansion;
- per-category budgets;
- omission explanations.

Acceptance:

- selected context contains the known patch-relevant files for curated tasks;
- irrelevant-source ratio is measured, not admired from a distance;
- every selected item has a reason.

### Phase 4: `o7 run` integration

Deliver:

- `o7 context build`;
- `o7 run --context`;
- context artifacts copied into the canonical run record;
- gate results linked back to selected entities.

Acceptance:

- a run is reproducible from task, base commit, context metadata, and gate manifest;
- failed and successful runs preserve exactly what the agent saw.

### Phase 5: memory and graph integration

Deliver:

- query historical runs and decisions;
- export selected current-repo facts into the memory graph;
- keep current facts and historical facts as separate trust domains;
- optional read-only MCP projection.

Acceptance:

- previous failed approaches can be injected with provenance;
- stale/superseded decisions are excluded by default.

### Phase 6: optional compression

Deliver:

- FastContext or another local summarizer as a post-selection step;
- structured-output validation;
- pre/post compression retention;
- fallback to deterministic rendering.

Acceptance:

- compression never drops required constraints, findings, files, or gates in the golden corpus;
- context quality is measured against uncompressed deterministic packs.

## MVP acceptance contract

The first useful version is complete when it can take a real OwnAudit task and produce a reproducible context pack that:

1. identifies the direct finding and exact source span;
2. identifies the owning project and likely regression-test home;
3. includes one-hop structural dependencies;
4. includes profile-specific risks and constraints;
5. stays within a configured budget;
6. explains why each item was selected;
7. contains no unsupported architectural claims;
8. is archived with the `o7 run` record;
9. measurably reduces agent discovery reads/tool calls on the fixture corpus.

The comparison baseline is not “agent with no context.” It should include:

- raw task only;
- repository packer output;
- grep/file-read exploration;
- deterministic task-aware context pack.

Metrics:

```text
patch success / gate pass rate
discovery tool calls
tokens consumed before first edit
total input tokens
irrelevant selected bytes
required-file recall
unsupported-claim count
wall-clock time
```

## Product boundary

The defensible product is not a generic repository mapper.

It is:

> **A reverse source generator that turns legacy .NET/WPF repositories into task-specific, evidence-backed agent context.**

Packers can provide bytes. Search can provide candidates. Graphs can provide relationships. Memory can provide history. Compression can shorten output.

This layer decides what the agent needs for this task, why it needs it, what authority supports it, and what must remain outside the context budget.
