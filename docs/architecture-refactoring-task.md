# 007 Architecture Refactor Task Contract

- **Status:** draft

## Summary

Introduce a strict 007 task contract for architecture refactoring driven by Own.NET/OwnAudit architecture findings.

007 should not detect architecture violations. Own.NET detects. OwnAudit reports/gates. 007 executes constrained refactoring attempts against specific findings, with typed tasks, policy checks, run artifacts, and verification gates.

This proposal defines the 007-side execution loop for architecture fixes:

```text
OwnAudit architecture finding
        ↓
task.yaml
        ↓
rendered task.md
        ↓
agent run in isolated worktree
        ↓
plan.o7plan + diff.patch
        ↓
policy check
        ↓
architecture gates
        ↓
run record
```

The goal is not “let AI redesign the system”. That is how repositories turn into haunted houses with build scripts. The goal is narrow, evidence-bound, policy-gated refactoring.

## Existing groundwork

007 already has [docs/agentic-coding-discipline-proposal.md](agentic-coding-discipline-proposal.md) for disciplined agentic coding. It describes task contracts, negative prompts, plan-then-build, diff policy gates, judge-run, trust levels, and notes that 007 already has an isolate → run → gate → harvest shape.

[docs/agentops-promptops.md](agentops-promptops.md) proposes a PromptOps/AgentOps layer across Own.NET, OwnAudit, and 007. It says Own.NET is the source of analysis truth, OwnAudit is the audit/fix orchestration layer, and 007 is the private agent harness using worktree isolation, gates, and run artifacts.

That document also states the right trust boundary: AI is not the source of truth for diagnostics; analyzer output, tests, SARIF, and gates are the source of truth.

[docs/agent-language.md](agent-language.md) proposes a strict agent language: `task.yaml`, optional `task.rules`, `plan.o7plan`, `verify.json`, and `summary.md`. It also states the key rule that the LLM proposes a structured plan, 007 parses/type-checks/policy-checks it, and only authorized actions are accepted.

Related documents:

- [agentic-coding-discipline-proposal.md](agentic-coding-discipline-proposal.md)
- [agentops-promptops.md](agentops-promptops.md)
- [agent-language.md](agent-language.md)

## Problem

Architecture findings are only useful if there is a disciplined way to act on them.

A human can read:

ARCH001: Presentation depends on Infrastructure directly.

But an agent needs a much stricter contract:

- which finding is selected;
- where the evidence lives;
- which files may be edited;
- which files are forbidden;
- whether baseline changes are allowed;
- which gates must run;
- how many files may change;
- whether public API changes are allowed;
- what counts as a valid fix;
- what counts as cheating.

Without this, “AI architecture refactoring” becomes a charming little bug farm.

## Non-goals

007 should not:

- infer architecture style;
- detect architecture violations;
- own architecture rule definitions;
- update architecture baselines automatically;
- suppress findings as a fix;
- make broad architectural rewrites without a selected finding;
- treat markdown summaries as machine-verifiable output.

## Proposed 007 architecture task

Add a task type:

`ownarch.fix`

Canonical file:

`task.yaml`

Example:

```yaml
schema: o7.task/v1
task_id: ownarch.fix.arch001.presentation-infrastructure
kind: ownarch.fix

target:
  # the audited C# solution (the codebase under repair), not the Own.NET
  # analyzer repo itself
  repo: <audited-csharp-repo>
  base_ref: main

input:
  findings_file: artifacts/architecture-findings.json
  drift_file: artifacts/architecture-drift.json
  selected_finding:
    rule: ARCH001
    fingerprint: "sha256:..."
    severity: error

evidence:
  required:
    - finding_exists
    - source_span_exists
    - from_component_exists
    - to_component_exists

permissions:
  allow_read:
    - "**/*"
  allow_edit:
    - "src/**/*.cs"
    - "tests/**/*.cs"
    - "docs/**/*.md"
  deny_edit:
    - "artifacts/**"
    - ".github/**"
    - "**/architecture-baseline.json"
    - "**/arch-snapshot*.json"

constraints:
  max_files_changed: 5
  require_tests: true
  require_reaudit: true
  forbid_baseline_update: true
  forbid_suppression_only_fix: true
  forbid_public_api_break_without_note: true

required_outputs:
  - plan.o7plan
  - diff.patch
  - verify.json
  - summary.md

gates:
  required:
    - build
    - tests
    - own-arch-evaluate
    - no-new-arch-findings
```

## plan.o7plan

The agent must produce a strict plan.

Example:

```text
plan v1

claim selected_finding {
  rule = "ARCH001"
  fingerprint = "sha256:..."
  evidence = "artifacts/architecture-findings.json#finding[0]"
}

claim violation_shape {
  from = "Broker.Presentation"
  to = "Broker.Infrastructure"
  boundary = "Presentation must not depend on Infrastructure directly"
  evidence = "artifacts/architecture-findings.json#finding[0].evidence[0]"
}

edit introduce_application_port {
  file = "src/Broker.Application/Ports/ICustomerLookup.cs"
  reason = "Presentation should depend on Application abstraction instead of Infrastructure implementation"
  fixes = ["sha256:..."]
}

edit move_call_behind_application_service {
  file = "src/Broker.Presentation/ViewModels/CustomerViewModel.cs"
  reason = "Replace direct Infrastructure dependency with Application service"
  fixes = ["sha256:..."]
}

test add_arch_regression {
  file = "tests/Architecture/LayeringTests.cs"
  scenario = "presentation_does_not_depend_on_infrastructure"
}

verify {
  run = ["build", "tests", "own-arch-evaluate", "no-new-arch-findings"]
}
```

## Policy checks

007 must reject the run before accepting the patch if:

- selected finding does not exist;
- finding fingerprint does not match;
- plan has no evidence reference;
- edited file is denied;
- changed file count exceeds task limit;
- plan updates architecture baseline when forbidden;
- plan only suppresses/removes the finding without structural fix;
- required gates are missing;
- verify.json is missing or invalid;
- diff.patch is empty when a fix was required;
- agent claims success while gates fail.

This is the difference between an agent and a shell script wearing a fake moustache.

## verify.json

Example:

```json
{
  "schema": "o7.verify/v1",
  "task_id": "ownarch.fix.arch001.presentation-infrastructure",
  "verdict": "FAIL",
  "policy": {
    "status": "PASS",
    "checked": [
      "selected_finding_exists",
      "evidence_refs",
      "allowed_files",
      "denied_files",
      "max_files_changed",
      "baseline_not_modified"
    ]
  },
  "gates": [
    {
      "name": "build",
      "status": "PASS",
      "log": "gate/build.log"
    },
    {
      "name": "tests",
      "status": "PASS",
      "log": "gate/tests.log"
    },
    {
      "name": "own-arch-evaluate",
      "status": "PASS",
      "log": "gate/own-arch-evaluate.log"
    },
    {
      "name": "no-new-arch-findings",
      "status": "FAIL",
      "log": "gate/no-new-arch-findings.log",
      "reason": "1 new ARCH011 finding introduced"
    }
  ],
  "changed_files": [
    "src/Broker.Application/Ports/ICustomerLookup.cs",
    "src/Broker.Presentation/ViewModels/CustomerViewModel.cs",
    "tests/Architecture/LayeringTests.cs"
  ]
}
```

## Run record additions

Extend 007 run records with architecture-specific metadata:

```json
{
  "schema": "o7.run-record/v1",
  "task_id": "ownarch.fix.arch001.presentation-infrastructure",
  "task_kind": "ownarch.fix",
  "target_repo": "Own.NET",
  "base_commit": "...",
  "selected_finding": {
    "rule": "ARCH001",
    "fingerprint": "sha256:..."
  },
  "prompt_module": "ownarch.fix-layer-violation",
  "prompt_version": "0.1.0",
  "policy_verdict": "PASS",
  "gate_verdict": "FAIL",
  "artifacts": {
    "task": "task.yaml",
    "rendered_task": "task.md",
    "plan": "plan.o7plan",
    "diff": "diff.patch",
    "verify": "verify.json",
    "summary": "summary.md"
  }
}
```

## Prompt module

Suggested prompt module:

`prompts/ownarch.fix-layer-violation.prompt.md`

Prompt contract:

```yaml
name: ownarch.fix-layer-violation
version: 0.1.0
purpose: >
  Generate a minimal refactoring plan and candidate patch for one selected
  deterministic architecture layer violation.

inputs:
  finding:
    schema: OwnArchitectureFinding.v1
    required: true
  architecture_intent:
    schema: OwnArchitectureIntent.v1
    required: true
  source_context:
    schema: SourceContext.v1
    required: true
  constraints:
    schema: O7TaskConstraints.v1
    required: true

output:
  required:
    - plan.o7plan
    - diff.patch
    - summary.md

forbidden_actions:
  - update_baseline
  - suppress_finding_without_fix
  - modify_unrelated_files
  - invent_findings
  - skip_required_gates
```

## Fix classes

Start with only three fix classes.

1. Layer dependency reroute

Example:

```text
Presentation -> Infrastructure
```

Preferred moves:

- introduce Application service;
- introduce port/interface;
- move concrete dependency behind Infrastructure adapter;
- update caller to depend inward.

2. Domain framework pollution

Example:

```text
Domain -> System.Windows
Domain -> DevExpress
```

Preferred moves:

- move UI formatting to Presentation;
- introduce domain-neutral value object;
- move framework-specific behavior to adapter/presenter;
- keep Domain framework-free.

3. New sensitive dependency

Example:

```text
Domain -> Microsoft.Data.SqlClient
Application -> DevExpress.Xpf
```

Preferred moves:

- isolate dependency behind existing boundary;
- introduce interface in allowed layer;
- move implementation to Infrastructure/Presentation.

Do not start with “fix cycles automatically”. Cycle refactors are harder and easier to botch. Let the first version fix direct forbidden edges before it tries architectural surgery with a butter knife.

## Gate integration

007 should call OwnAudit/Own.NET commands, not duplicate their logic.

Example `.007/gate.toml` entry (`arch.review_cli` is the command surface
proposed in `OwnAudit docs/architecture-review.md`; today the closest
implemented pieces are `arch.cli` + `report.diff_cli`):

```toml
[[gate]]
name = "own-arch-evaluate"
cmd = "python3 -m arch.review_cli --facts artifacts/arch-facts.json --findings artifacts/arch-findings.json --baseline artifacts/architecture-baseline.json --out-dir artifacts/arch-review --gate-level error"
required = true

[[gate]]
name = "no-new-arch-findings"
cmd = "python3 -m report.diff_cli --baseline artifacts/baseline.json --current artifacts/current-findings.json --gate-level error"
required = true
```

## Acceptance criteria

MVP is accepted when:

1. 007 can read `ownarch.fix` task specs.
2. 007 stores `task.yaml`, rendered `task.md`, `plan.o7plan`, `diff.patch`, `verify.json`, and `summary.md`.
3. 007 rejects plans without evidence references.
4. 007 rejects edits to denied paths.
5. 007 rejects architecture baseline modification unless explicitly allowed.
6. 007 checks required gate names appear in `verify`.
7. 007 records architecture finding fingerprint in run metadata.
8. A sample task exists for `ARCH001`.
9. Fixture tests cover:
   - valid plan;
   - missing evidence;
   - denied file edit;
   - baseline edit attempt;
   - missing required gate;
   - changed file count exceeded;
   - failing architecture gate.

## First implementation slice

Implement only:

- task kind: `ownarch.fix`
- `task.yaml` schema extension
- `plan.o7plan` validation for claim/edit/test/verify
- policy check for selected finding + file permissions
- run record metadata
- one sample `ARCH001` task
- tests

Do not implement autonomous multi-step architecture migration yet. First make one precise architecture violation fixable under policy and gates. Then grow the beast.
