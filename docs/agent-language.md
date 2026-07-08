Proposal: Strict Agent Language for Own.NET, OwnAudit and 007

Status

Draft

Target repositories

- "PhysShell/007"
- "PhysShell/OwnAudit"
- "PhysShell/Own.NET"

Summary

Introduce a strict, parseable and verifiable agent task/action layer for 007-driven work over Own.NET and OwnAudit.

The current "task.md" model is useful for human-readable instructions, but it is not strong enough as an execution contract. Markdown should remain documentation. Machine-relevant behavior should move into structured specs, typed agent plans, policy checks and gates.

This proposal introduces:

task.yaml     — canonical machine task spec
task.rules    — optional controlled-English task/policy DSL
plan.o7plan   — strict agent-produced action plan
verify.json   — machine-readable verification result
summary.md    — human-readable explanation only

The key design rule:

LLM does not directly execute trusted actions.
LLM proposes a structured plan.
007 parses the plan.
007 type-checks and policy-checks the plan.
007 accepts only authorized actions.
Own.NET / OwnAudit gates verify the result.

Motivation

007 already has the right execution shape:

isolate -> run agent -> gate -> harvest artifacts

However, the current task surface is still too close to free-form text. A Markdown instruction can say:

Do not edit artifacts.
Do not suppress findings.
Run the gates.

But this is not a contract. It is a request.

For agentic work on static analysis and audit/fix pipelines, this is too weak. An agent can:

- edit files outside the intended scope;
- claim to fix a finding that does not exist;
- suppress a finding instead of fixing it;
- skip required verification;
- produce a patch that passes local formatting but worsens the audit result;
- return a plausible explanation without machine-checkable evidence.

The desired model is closer to a compiler/verifier pipeline:

parse(AST)
&& typecheck(AST)
&& policy_check(AST)
&& execute_in_sandbox(AST)
&& verify(result)

The goal is not to make the model “smarter”. The goal is to reduce the number of invalid states the model can express and to reject unsafe or unverifiable outputs before they matter.

Prior art

Inform 7

Inform 7 is the most relevant design inspiration.

It uses English-like syntax for interactive fiction, but the text is not free prose. It is compiled into a formal world model with objects, relations, actions, rules and deterministic execution.

The useful idea is not to depend on Inform 7. The useful idea is:

controlled natural language
  -> parser/compiler
  -> world model
  -> rulebooks
  -> deterministic execution

For 007, the analogous shape is:

controlled task language
  -> parser/typechecker
  -> task/action AST
  -> policy checker
  -> gates
  -> deterministic verdict

BAML / TypeChat / LMQL / Guidance / Outlines / Guardrails

Other useful references:

BAML:
  typed prompt/task modules;
  typed outputs;
  invalid output as validation failure.

TypeChat:
  natural language -> typed intent -> validated action -> executor.

LMQL:
  LLM calls as programming constructs;
  output constraints;
  control flow around generation.

Guidance / Outlines:
  constrained generation;
  regex/CFG/schema-bound outputs.

Guardrails / NeMo Guardrails:
  validation and policy layer around LLM input/output.

These are useful as prior art, not as mandatory dependencies. 007 should not become dependent on a large external LLM DSL/runtime unless there is a very specific reason.

Decision

Build a small project-specific strict language layer:

O7Rules  — optional controlled-English policy/task DSL.
O7Plan   — required strict agent action language.
TaskSpec — canonical YAML/JSON machine task contract.

Markdown remains allowed only for human-facing explanation.

Design overview

Layer split

Human-readable docs:
  summary.md
  rendered task.md

Machine task contract:
  task.yaml
  task.rules

Agent output:
  plan.o7plan
  diff.patch
  verify.json

Execution:
  007 parser
  007 typechecker
  007 policy checker
  007 gate runner
  007 artifact harvester

Data flow

task.yaml / task.rules
  ↓ parse
Task AST
  ↓ typecheck
Typed Task
  ↓ compile policy
Permissions + constraints + required gates
  ↓ render
task.md for the agent
  ↓ run agent in isolated worktree
plan.o7plan + diff.patch + summary.md
  ↓ parse plan
Plan AST
  ↓ typecheck plan
Typed Plan
  ↓ policy check
Authorized Plan
  ↓ inspect/apply patch
Patch
  ↓ gates
VerificationResult
  ↓ harvest
run record

O7Rules

"O7Rules" is a controlled-English DSL for task and policy definition.

It is intended for humans to read and write, but it must compile into a strict task model.

Example:

The target repository is OwnAudit.
The base branch is main.

The task is to fix the highest ranked OWN001 finding.

A finding is eligible if:
    the diagnostic code is "OWN001";
    the category is "subscription-token";
    the confidence is not "low";
    the source span exists.

The agent may read any file.
The agent may edit files under "src/".
The agent may edit files under "tests/".
The agent must not edit files under "artifacts/".
The agent must not edit workflow files.

Instead of suppressing a finding:
    reject the plan with reason "suppression is not a fix".

After the patch is produced:
    run "dotnet-test";
    run "ownaudit-smoke";
    run "no-new-findings".

A run passes if:
    every required gate passes;
    no new finding appears;
    the patch changes at most 3 files.

This compiles into a machine model similar to:

TargetRepo("OwnAudit")
BaseBranch("main")

Rule EligibleFinding:
  diagnostic == OWN001
  category == subscription-token
  confidence != low
  source_span exists

Permission:
  read any
  edit src/**
  edit tests/**
  deny artifacts/**
  deny .github/**

Instead SuppressFinding:
  Reject("suppression is not a fix")

After Patch:
  RunGate("dotnet-test")
  RunGate("ownaudit-smoke")
  RunGate("no-new-findings")

PassCondition:
  all_required_gates_pass
  no_new_findings
  changed_files <= 3

Why not only YAML?

YAML is acceptable for the first implementation. It is easier to parse and validate.

However, YAML is poor as a long-term human policy surface. It becomes nested configuration instead of readable rules.

Recommended approach:

Phase 1:
  task.yaml is canonical.

Phase 2:
  task.rules compiles to task.yaml / Task AST.

Phase 3:
  task.rules becomes the preferred authoring surface for policies.

O7Plan

"O7Plan" is the strict action language the agent must return.

The agent should not return vague prose like:

I think we should add unsubscribe in Dispose.

The agent must return a parseable action plan:

plan v1

claim finding_exists {
  diagnostic = "OWN001"
  finding_id = "STS:BrokerDataClasses/KTSGoods2:customerChanged"
  evidence = "artifacts/findings.json#finding[42]"
}

edit add_unsubscribe {
  file = "src/BrokerDataClasses/KTSGoods2.cs"
  region = "Dispose"
  reason = "subscription token acquired but not released"
  fixes = ["STS:BrokerDataClasses/KTSGoods2:customerChanged"]
}

test add_regression {
  file = "tests/SubscriptionLeakTests.cs"
  scenario = "disposes_customerChanged_subscription"
}

verify {
  run = ["dotnet-test", "ownaudit-smoke", "no-new-findings"]
}

007 then verifies:

- plan syntax is valid;
- finding_id exists;
- evidence reference exists;
- diagnostic code matches the selected finding;
- edited file is allowed;
- edited file is not denied;
- action is permitted by task spec;
- required gates are present;
- suppress-only fix is not used;
- changed file count is within constraints.

If any check fails:

REJECTED: invalid plan

The run may still preserve "agent.stdout", "plan.o7plan", "diff.patch" and gate logs, but the verdict must not be "PASS".

Minimal O7Plan grammar

MVP should stay intentionally small.

Supported statements:

claim
read
edit
test
verify
refuse

Draft grammar:

plan        := "plan" version statement*
statement   := claim | read | edit | test | verify | refuse

claim       := "claim" ident "{" field* "}"
read        := "read" ident "{" field* "}"
edit        := "edit" ident "{" field* "}"
test        := "test" ident "{" field* "}"
verify      := "verify" "{" field* "}"
refuse      := "refuse" "{" field* "}"

field       := ident "=" value
value       := string | number | bool | array
array       := "[" value* "]"

Initial domain types:

FindingId
DiagnosticCode
RepoPath
GateName
EvidenceRef
PatchIntent
RiskLevel

Important invariant:

Every claim must have an EvidenceRef.

A claim without evidence is rejected.

TaskSpec YAML

Before "O7Rules" exists, "task.yaml" should be the canonical task contract.

Example:

task_id: ownaudit.fix.own001.top
version: 1

target:
  repo: OwnAudit
  base: main

input:
  artifact: artifacts/findings.json
  selector:
    diagnostic: OWN001
    category: subscription-token
    rank: 1

permissions:
  allow_read:
    - "**/*"
  allow_edit:
    - "src/**/*.cs"
    - "tests/**/*.cs"
  deny_edit:
    - "artifacts/**"
    - ".github/**"
    - "Run-Audit.ps1"

constraints:
  max_files_changed: 3
  require_tests: true
  require_reaudit: true
  forbid_suppression_only_fix: true
  forbid_public_api_break: true

required_outputs:
  - plan.o7plan
  - diff.patch
  - verification.json
  - summary.md

gates:
  required:
    - dotnet-test
    - ownaudit-smoke
    - no-new-findings

007 validates this before running an agent.

Verification result

"verification.json" should be machine-readable.

Example:

{
  "version": 1,
  "verdict": "FAIL",
  "target_repo": "OwnAudit",
  "base_commit": "abc123",
  "changed_files": [
    "src/BrokerDataClasses/KTSGoods2.cs",
    "tests/SubscriptionLeakTests.cs"
  ],
  "gates": [
    {
      "name": "dotnet-test",
      "status": "PASS",
      "log": "gate/dotnet-test.log"
    },
    {
      "name": "ownaudit-smoke",
      "status": "PASS",
      "log": "gate/ownaudit-smoke.log"
    },
    {
      "name": "no-new-findings",
      "status": "FAIL",
      "log": "gate/no-new-findings.log",
      "reason": "2 new OWN001 findings appeared"
    }
  ],
  "policy": {
    "status": "PASS",
    "checked": [
      "allowed_files",
      "denied_files",
      "max_files_changed",
      "required_gates",
      "evidence_refs"
    ]
  }
}

Repository layout

007

007/
  crates/
    o7-task-spec/
    o7-rules-parser/
    o7-plan-parser/
    o7-plan-checker/
    o7-policy/
    o7-runner/

  schemas/
    task-spec.v1.schema.json
    verification.v1.schema.json

  grammars/
    o7rules.v1.ebnf
    o7plan.v1.ebnf

  examples/
    tasks/
      ownaudit.fix-own001.task.yaml
      ownaudit.fix-own001.task.rules

    plans/
      ownaudit.fix-own001.valid.o7plan
      ownaudit.fix-own001.invalid-no-evidence.o7plan
      ownaudit.fix-own001.invalid-denied-file.o7plan

OwnAudit

OwnAudit/
  ai/
    tasks/
      fix-own001-top.task.yaml
      triage-health-report.task.yaml

    rules/
      ownaudit-fix-policy.rules

    evals/
      fix-own001.cases.jsonl
      report-triage.cases.jsonl

  .007/
    gate.toml

Own.NET

Own.NET/
  ai/
    tasks/
      explain-own-diagnostic.task.yaml
      generate-wpf-gallery-case.task.yaml

    rules/
      wpf-lifetimes.ownrules

    evals/
      diagnostic-explainer.cases.jsonl
      wpf-lifetime-rules.cases.jsonl

  .007/
    gate.toml

OwnAudit use case

OwnAudit is the first practical target for strict agent tasks.

Initial scenario:

Fix the highest-ranked OWN001 subscription-token finding.

Policy constraints:

- selected finding must exist in artifacts/findings.json;
- selected finding must match diagnostic OWN001;
- selected finding must have source span;
- patch must claim which finding it fixes;
- patch must not modify artifacts;
- patch must not modify workflows;
- patch must change at most 3 files;
- suppress-only fix is forbidden;
- required gates must run;
- no new findings may appear.

This gives a narrow, auditable, useful vertical slice.

Own.NET use case

Own.NET can use the same strict layer for diagnostic semantics, docs and test generation.

Example controlled rules:

A view model is a scoped object.
An event subscription is a resource.
A static event has process lifetime.

Subscribing to an event acquires a subscription token.
Unsubscribing from the event releases the subscription token.

Subscribing to a static event captures the subscriber.
A capture leaks if the source lifetime outlives the subscriber lifetime.

A matching unsubscribe clears the capture.

Possible location:

Own.NET/ai/rules/wpf-lifetimes.ownrules

Uses:

- diagnostic documentation;
- test case generation;
- checker expectation examples;
- agent task constraints;
- review prompts;
- future deterministic rule extraction.

The rule file is not the checker. It is a controlled specification surface that can be tested against the checker.

007 run artifacts

Extend 007 run records with strict-language artifacts:

runs/<target>/<run-id>/
  task.yaml
  task.rules
  task.md
  plan.o7plan
  plan.normalized.json
  prompt.meta.json
  agent.stdout
  diff.patch
  verification.json
  summary.md
  gate/
    verdict.json
    *.log

Required metadata:

{
  "task_id": "ownaudit.fix.own001.top",
  "task_version": 1,
  "target_repo": "OwnAudit",
  "base_commit": "...",
  "agent": "claude-cli",
  "prompt_module": "ownaudit.fix-own001",
  "prompt_version": "0.1.0",
  "plan_language": "o7plan.v1",
  "verdict": "FAIL"
}

Gates

OwnAudit gate example

[[step]]
name = "dotnet-test"
command = "dotnet test"

[[step]]
name = "ownaudit-smoke"
command = "pwsh ./Run-Audit.ps1 -Smoke"

[[step]]
name = "no-new-findings"
command = "python tools/compare_audit_reports.py --baseline artifacts/health-report.json --current artifacts/current-health-report.json"

Own.NET gate example

[[step]]
name = "python-tests"
command = "python tests/run_tests.py"

[[step]]
name = "gallery-tests"
command = "python tests/test_gallery.py"

[[step]]
name = "wpf-tests"
command = "python tests/test_wpf.py"

[[step]]
name = "lifetime-tests"
command = "python tests/test_lifetimes.py"

MVP roadmap

Phase 0: schemas first

Deliver:

- task-spec.v1.schema.json
- verification.v1.schema.json
- draft o7plan.v1.ebnf
- sample OwnAudit OWN001 task
- valid and invalid O7Plan fixtures

Acceptance criteria:

- valid task spec passes validation;
- invalid task spec fails validation;
- valid plan parses;
- invalid plan fails;
- missing evidence ref fails;
- denied file edit fails.

Phase 1: task.yaml as canonical

Deliver:

- 007 validates task.yaml before agent run;
- 007 renders task.md from task.yaml;
- 007 stores task.yaml in run artifacts;
- 007 stores prompt/task metadata.

Acceptance criteria:

- agent cannot run without valid task.yaml;
- task.md is treated as rendered view, not source of truth;
- run metadata includes task_id, task_version, base commit and target repo.

Phase 2: O7Plan parser/checker

Deliver:

- parser for claim/edit/test/verify/refuse;
- normalizer to plan.normalized.json;
- policy checker against task.yaml;
- rejection reasons.

Acceptance criteria:

- syntax errors produce ERROR;
- missing evidence produces REJECTED;
- forbidden edits produce REJECTED;
- missing required gates produce REJECTED;
- suppress-only fix produces REJECTED.

Phase 3: OwnAudit OWN001 vertical slice

Deliver:

- task spec for highest-ranked OWN001 finding;
- 007 run against OwnAudit worktree;
- plan verification;
- patch inspection;
- gates;
- before/after audit diff.

Acceptance criteria:

- finding must exist;
- patch claims exact finding id;
- changed files are within allowlist;
- required gates run;
- no-new-findings gate determines final verdict.

Phase 4: O7Rules controlled English

Deliver:

- parser for minimal O7Rules;
- compiler from task.rules to Task AST / task.yaml;
- examples for OwnAudit fix policy;
- examples for Own.NET WPF lifetime rules.

Acceptance criteria:

- task.rules compiles deterministically;
- equivalent task.rules and task.yaml produce same Task AST;
- invalid controlled-English rule fails with useful diagnostic;
- generated policy matches expected fixtures.

Phase 5: Own.NET diagnostic rules

Deliver:

- wpf-lifetimes.ownrules;
- diagnostic explanation task;
- gallery case generation task;
- eval cases for OWN001 / OWN014.

Acceptance criteria:

- rules are readable by humans;
- rules compile to a structured model;
- examples map to expected diagnostics;
- generated test cases must be accepted only if checker output matches expected code.

Non-goals

This proposal does not attempt to:

- replace Own.NET checker logic with LLM reasoning;
- replace OwnAudit scoring with LLM judgment;
- auto-merge AI-generated patches;
- trust model output without evidence;
- build a general-purpose LLM programming language;
- depend on BAML/LMQL/Guidance/Outlines as required runtime;
- make Markdown a source of truth;
- treat suppressions as fixes.

Risks

Risk: building a second OwnLang

Mitigation:

Keep O7Plan tiny. It is an action protocol, not a general-purpose language.

Risk: controlled English becomes ambiguous

Mitigation:

Start with "task.yaml". Add "O7Rules" only after the Task AST is stable.

Risk: valid syntax but invalid behavior

Mitigation:

Use semantic checks, evidence refs, audit diff and gates. Syntax is only the first layer.

Risk: agent learns to satisfy the format while doing bad work

Mitigation:

Treat gates as the final authority. A valid plan with a bad patch still fails.

Risk: too much infrastructure before value

Mitigation:

First vertical slice is narrow: one OwnAudit OWN001 finding, max 3 changed files, required gates.

Final design principle

Markdown is documentation.

YAML/JSON is the task contract.

O7Rules is controlled human-readable policy.

O7Plan is the agent action language.

007 is the compiler/interpreter/verifier harness.

Own.NET and OwnAudit gates are proof obligations.

The model may propose. The system decides.