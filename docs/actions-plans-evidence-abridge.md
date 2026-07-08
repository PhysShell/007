# Action Plan & Evidence Bridge (007 / Own.NET / OwnAudit)

- **Status:** proposal
- **Scope:** 007, Own.NET, OwnAudit, Sandboy integration
- **Intent:** связать agent task contract, runtime policy, diff policy, trace/evidence и OwnAudit triage в один проверяемый контур

## 1. Summary

В 007 уже формируется правильная архитектурная линия:

```text
task contract
  → isolated agent run
  → diff policy
  → gated execution
  → run record
  → judge / eval / replay
  → OwnAudit evidence sink
```

Но между слоями всё ещё не хватает одного явного артефакта: машиночитаемого плана намерений агента.

Сейчас "TaskContract" может ограничивать общую рамку задачи, а "DiffPolicy" может проверять итоговый diff. Но система всё ещё не видит, что агент собирался сделать до изменения кода:

- какие findings он чинит;
- какие файлы собирается читать;
- какие файлы собирается менять;
- какие эффекты ожидает произвести;
- какие проверки должны подтвердить результат;
- какие действия считаются запрещёнными именно для этого fix-а.

Предлагается добавить bridge layer:

```text
Prompt / TaskSpec
  → O7ActionPlan
  → Policy validation
  → Agent run
  → DiffPolicy validation
  → Gate execution
  → Run evidence
  → OwnAudit ingest
```

Ключевая идея: 007 должен проверять не только “разрешена ли задача” и “прошли ли тесты”, но и совпало ли фактическое поведение агента с его заявленным планом.

## 2. Problem

Открытые proposal-и уже покрывают крупные блоки:

- PromptOps / AgentOps;
- task contracts;
- plan-then-build discipline;
- diff policy gates;
- CUE policy authoring;
- fail-closed gates;
- run-record hash chain;
- sandboy-backed isolation;
- trace-driven verifier;
- OwnAudit as evidence sink.

Но без промежуточного "ActionPlan" остаётся дыра:

```text
TaskContract говорит:
  “агенту разрешено работать в этих рамках”.

DiffPolicy говорит:
  “вот что агент фактически поменял”.

Gate говорит:
  “команды завершились так-то”.

OwnAudit говорит:
  “вот evidence и risk после выполнения”.

Но никто явно не фиксирует:
  “вот что агент обещал сделать до изменения кода”.
```

Это создаёт несколько failure modes.

### 2.1. Fix без проверяемого намерения

Агент может сказать:

“Починил OWN001 leak”.

Но фактически:

- удалил подписку вместо корректного release;
- подавил warning;
- поменял unrelated code;
- добавил dependency;
- изменил gate config;
- изменил тесты так, чтобы они проходили.

И если смотреть только на итоговый diff + green tests, часть таких случаев будет выглядеть приемлемо.

Это не engineering. Это “поверь брат” с CI.

### 2.2. DiffPolicy без semantic intent

DiffPolicy может проверить:

- path allowed;
- file count under limit;
- no forbidden paths;
- no dependency files;
- no binary files;
- diff size within budget.

Но она не знает:

- какие findings должны исчезнуть;
- какие findings не должны появиться;
- какие resource obligations должны быть восстановлены;
- какой expected effect должен быть у изменения.

То есть policy видит форму diff-а, но не знает его обещанного смысла.

### 2.3. OwnAudit получает evidence без declared baseline

OwnAudit может ingest-ить run record, policy violations, gate verdicts, sandbox reports и trace events. Но для полноценного triage ему нужно сравнивать:

```text
declared intent
  vs
actual diff
  vs
actual runtime behavior
  vs
actual analyzer/audit result
```

Без declared intent OwnAudit видит только последствия, но не может точно сказать:

- агент сделал то, что обещал;
- агент сделал больше, чем обещал;
- агент сделал другое;
- агент не подтвердил результат;
- агент прошёл gate, но не решил target finding.

## 3. Goals

Добавить минимальный bridge layer, который:

1. Фиксирует agent intent до изменения кода.
2. Проверяется policy-слоем до запуска agent patching.
3. Сверяется с итоговым diff.
4. Сохраняется в run record.
5. Становится входом для OwnAudit ingest.
6. Связывает Own.NET deterministic findings с agent repair workflow.

Система должна быть boring, local, deterministic и reviewable.

Никакого нового “языка программирования” для агента на v1. Только schema-first contract.

## 4. Non-goals

Не предлагается:

- писать полноценный workflow language;
- заменять CUE policy authoring;
- заменять TaskContract;
- заменять DiffPolicy;
- заменять Sandboy isolation;
- заменять OwnAudit triage;
- делать AI источником истины для diagnostics;
- позволять агенту auto-promote memory/policy без PR review;
- строить multi-agent colony / self-improving loop / автономного PR-робота.

AI не становится судьёй. Истина остаётся в deterministic layers:

- Own.NET diagnostics;
- OwnAudit normalized findings;
- 007 policy/gates;
- Sandboy reports;
- schema validation;
- run evidence.

## 5. Proposed architecture

Новый контур:

```text
TaskSpec
  ↓
Prompt module render
  ↓
Agent produces O7ActionPlan
  ↓
007 validates ActionPlan against TaskContract + policy
  ↓
Agent applies patch
  ↓
007 validates diff against TaskContract + ActionPlan
  ↓
Gates run under Sandboy / declared sandbox policy
  ↓
AOB reducers emit structured command evidence
  ↓
Run record stores plan, diff, gates, trace, evidence
  ↓
OwnAudit ingests run record
  ↓
OwnAudit emits SARIF/risk/triage report
```

## 6. New artifact: "O7ActionPlan.v1"

"O7ActionPlan" is a machine-checkable declaration of what the agent intends to do.

It is not a free-form markdown plan. It is not a prompt. It is not a gate manifest. It is not a policy language.

It is the agent’s declared contract for a single run.

### 6.1. Example

```yaml
schema: o7.action_plan.v1
task_id: ownaudit.fix.own001.subscription-token
intent: fix_existing_finding

target_repo: OwnAudit
target_base_ref: main

target_findings:
  - id: OWN001
    source: ownaudit.sarif
    # OWN001 = "owned resource not released on all paths" (Own.NET
    # spec/Diagnostics.md); event_subscription_leak is the OwnAudit/SARIF
    # category layered on top, not the analyzer rule name
    category_name: event_subscription_leak
    finding_id: "OWN001:src/Foo/ViewModel.cs:142"

proposed_changes:
  - file: src/Foo/ViewModel.cs
    operation: edit
    reason: add deterministic unsubscribe path
    expected_effect: remove OWN001 finding for event subscription token

declared_effects:
  reads:
    - audit/findings.sarif
    - src/Foo/ViewModel.cs

  writes:
    - src/Foo/ViewModel.cs

  forbidden:
    - "**/*.csproj"
    - "Directory.Build.props"
    - "global.json"
    - ".007/**"
    - ".agents/**"
    - "docs/**"

verification:
  required_gates:
    - build
    - test
    - ownaudit-reaudit

  expected_result:
    removed_findings:
      - OWN001

    no_new_findings: true
    no_public_api_change: true
    no_policy_violation: true

risk:
  level: medium
  reasons:
    - touches resource lifetime logic
    - fix must preserve existing subscription behavior
```

### 6.2. Required fields

```text
schema
task_id
intent
target_repo
target_findings or target_objective
proposed_changes
declared_effects
verification
risk
```

A run with no "O7ActionPlan" may still exist in legacy mode, but protected mode should require it.

### 6.3. Intent classes

Initial enum:

```text
fix_existing_finding
add_analyzer_rule
add_test_case
refactor_internal_only
update_docs
triage_report
investigate_only
```

Each intent class can have additional constraints.

Example:

```text
fix_existing_finding:
  requires target_findings
  requires re-audit
  forbids diagnostic suppression unless explicitly allowed

investigate_only:
  forbids source writes
  requires summary artifact
```

## 7. ActionPlan policy validation

Before the agent is allowed to patch, 007 validates the plan against:

- TaskContract;
- CUE-rendered policy;
- target repository policy;
- allowed/forbidden paths;
- declared effect vocabulary;
- prompt module output schema;
- per-intent required fields.

### 7.1. Validation output

```text
runs/<target>/<run-id>/
  policy/
    action-plan.normalized.json
    action-plan.verdict.json
```

Example verdict:

```json
{
  "schema": "o7.action_plan_verdict.v1",
  "verdict": "PASS",
  "checks": [
    {
      "id": "O7PLAN001",
      "name": "schema-valid",
      "verdict": "PASS"
    },
    {
      "id": "O7PLAN002",
      "name": "writes-within-task-scope",
      "verdict": "PASS"
    },
    {
      "id": "O7PLAN003",
      "name": "forbidden-paths-not-declared",
      "verdict": "PASS"
    },
    {
      "id": "O7PLAN004",
      "name": "required-reaudit-present",
      "verdict": "PASS"
    }
  ]
}
```

### 7.2. Fail-closed rules

Protected agent runs should fail before patching if:

- ActionPlan is missing;
- ActionPlan is invalid;
- ActionPlan declares writes outside TaskContract;
- ActionPlan targets forbidden paths;
- ActionPlan omits required verification;
- ActionPlan requests unknown effects;
- ActionPlan requests diagnostic suppression without explicit permission;
- ActionPlan attempts to edit .007/, .agents/, gate policy, or prompt registry without human-approved task scope.

## 8. DiffPolicy binding

After the agent produces a diff, 007 compares actual diff against both:

```text
TaskContract
ActionPlan
```

This creates three levels:

```text
TaskContract:
  broad allowed scope

ActionPlan:
  declared intended scope

Diff:
  actual behavior
```

### 8.1. Diff vs ActionPlan checks

Initial checks:

```text
O7DIFF001 changed path not declared in ActionPlan
O7DIFF002 forbidden path touched
O7DIFF003 dependency file changed without permission
O7DIFF004 test changed without fix/test intent
O7DIFF005 generated file changed without permission
O7DIFF006 public API changed without permission
O7DIFF007 .007 or gate config changed
O7DIFF008 .agents memory/policy changed
O7DIFF009 target finding not referenced by diff
O7DIFF010 diff exceeds declared risk/size budget
```

### 8.2. Verdict classes

```text
PASS
FAIL
ERROR
NOT_APPLICABLE
```

### 8.3. Output

```text
runs/<target>/<run-id>/
  policy/
    diff-policy.json
    diff-vs-action-plan.json
```

Example:

```json
{
  "schema": "o7.diff_vs_action_plan.v1",
  "verdict": "FAIL",
  "violations": [
    {
      "rule": "O7DIFF001",
      "severity": "error",
      "message": "Changed file was not declared in ActionPlan.",
      "path": "Directory.Build.props"
    }
  ]
}
```

## 9. Shared Effect Ledger

A shared vocabulary is needed so 007, Own.NET and OwnAudit do not invent three names for the same thing.

Initial file:

`Own.NET spec/EffectsVocabulary.md` (Own.NET owns the vocabulary — its `spec/`
is the normative layer; 007 and OwnAudit consume it)

Possible schema:

```yaml
schema: owen.effects.v1

effects:
  fs.read:
    scope: runtime
    evidence_required: true

  fs.write:
    scope: runtime
    evidence_required: true

  net.egress:
    scope: runtime
    default: deny

  proc.exec:
    scope: runtime
    sandbox_required: true

  source.edit:
    scope: repo
    diff_policy_required: true

  source.public_api_change:
    scope: repo
    human_review_required: true

  own.resource.acquire:
    scope: static
    provider: Own.NET

  own.resource.release:
    scope: static
    provider: Own.NET

  audit.finding.suppress:
    scope: audit
    human_review_required: true

  audit.finding.fix:
    scope: audit
    evidence_required: true
```

This vocabulary should be used by:

- O7ActionPlan;
- TaskContract;
- CUE policy;
- Sandboy reports;
- AOB command summaries;
- OwnAudit ingest rules;
- Own.NET resource model exports.

## 10. Own.NET binding: diagnostics as repair constraints

Own.NET remains the deterministic source of truth for diagnostics and resource/lifetime models.

For agent repair, Own.NET should expose repair constraints around findings.

Example:

```yaml
schema: own.repair_constraints.v1

diagnostic: OWN001
category_name: event_subscription_leak

resource:
  kind: EventSubscriptionToken
  acquire_effect: own.resource.acquire
  release_effect: own.resource.release

preferred_fix_patterns:
  - store subscription token in owner field
  - release token in Dispose
  - preserve existing event behavior

forbidden_fix_patterns:
  - remove subscription without justification
  - suppress diagnostic
  - add broad try/catch
  - change public behavior silently
  - replace deterministic cleanup with finalizer-only cleanup

required_verification:
  - ownnet-check
  - ownaudit-reaudit
  - no-new-findings
```

This should integrate with project-local resource model files such as "own.models.yaml".

### 10.1. Why this matters

Without repair constraints, an agent can “fix” a leak by removing behavior.

That is not a fix. That is a software amputation.

For "OWN001" and "OWN014", the agent should receive structured constraints:

- what resource was acquired;
- what release obligation is missing;
- which fix patterns are acceptable;
- which “fixes” are forbidden;
- what verification must prove.

## 11. AOB binding: output reduction as evidence, not just compression

The Agent Output Budgeter should not only shorten stdout. It should emit structured evidence.

Each wrapped command should produce:

```json
{
  "schema": "o7.tool_result_reduced.v1",
  "event": "tool_result_reduced",
  "command_kind": "msbuild",
  "command": "msbuild Broker.sln /bl",
  "exit_code": 1,
  "duration_ms": 134000,
  "raw_artifact": ".agent-artifacts/2026-07-06/183012-msbuild/stdout.log",
  "summary_artifact": ".agent-artifacts/2026-07-06/183012-msbuild/summary.json",
  "diagnostics": [
    {
      "kind": "compiler_error",
      "code": "CS1061",
      "file": "LoginViewModel.cs",
      "line": 142
    }
  ],
  "budget": {
    "raw_chars": 184200,
    "returned_chars": 6400,
    "omitted_chars": 177800
  }
}
```

This event can be appended to:

```text
runs/<target>/<run-id>/agent.trace.jsonl
```

or stored under:

```text
runs/<target>/<run-id>/evidence/commands/*.json
```

### 11.1. Initial reducers

Priority reducers:

```text
git status
git diff
ripgrep
dotnet build
dotnet test
msbuild
TRX
MSBuild binlog
generic logs
JSON/JSONL
SQL diagnostics
```

### 11.2. Why this matters

If AOB only shortens output, it is a convenience tool.

If AOB emits structured evidence, it becomes part of the trust pipeline:

```text
command output
  → reduced diagnostic summary
  → trace event
  → OwnAudit ingest
  → SARIF/risk report
```

## 12. OwnAudit ingest

OwnAudit should ingest completed 007 run records and emit a normalized report.

Command shape:

```text
ownaudit ingest-o7-run <run-dir>
```

Input:

```text
runs/<target>/<run-id>/
  meta.json
  task.o7.toml
  action-plan.yaml
  policy/action-plan.verdict.json
  policy/diff-policy.json
  policy/diff-vs-action-plan.json
  gate/verdict.json
  gate/*.sandbox.json
  agent.trace.jsonl
  evidence/**/*.json
  diff.patch
```

Output:

```text
runs/<target>/<run-id>/
  ownaudit/
    agent-run.sarif
    risk.json
    triage.md
    evidence.jsonl
```

### 12.1. Initial OwnAudit rules

Rule IDs are aligned with the numbering defined in sections 7.1 and 8.1.

```text
O7PLAN001 invalid or missing ActionPlan
O7PLAN002 undeclared write effect
O7PLAN004 verification missing
O7PLAN005 target finding missing

O7DIFF001 changed path not declared in ActionPlan
O7DIFF002 forbidden path touched
O7DIFF003 dependency file changed without permission
O7DIFF004 tests changed without test/fix intent
O7DIFF007 gate config changed

O7GATE001 required gate failed
O7GATE002 required gate missing
O7GATE003 gate ran without sandbox policy
O7GATE004 gate timeout
O7GATE005 sandbox violation

O7TRC001 command spam
O7TRC002 repeated failed repair attempts
O7TRC003 tool call outside declared plan
O7TRC004 no verification after source edit
O7TRC005 raw output exceeded budget without reducer

O7AUDIT001 target finding not removed
O7AUDIT002 new finding introduced
O7AUDIT003 diagnostic suppressed instead of fixed
O7AUDIT004 no re-audit evidence
```

### 12.2. Triage buckets

```text
confirmed_violation:
  policy fired and evidence confirms bad behavior

policy_gap:
  suspicious action succeeded because no rule covered it

runtime_only:
  runtime/sandbox/trace evidence shows issue not visible statically

static_only:
  ActionPlan/DiffPolicy indicates issue, but runtime evidence is absent

needs_human_review:
  sensitive action, unclear intent, or gate config/policy/memory touched
```

## 13. Run record changes

Target run structure:

```text
runs/<target>/<run-id>/
  task.md
  task.o7.toml
  action-plan.yaml
  meta.json

  prompt/
    prompt.rendered.md
    prompt.meta.json

  agent/
    stdout.log
    trace.jsonl

  diff/
    diff.patch
    diff.stats.json

  policy/
    task-contract.normalized.json
    action-plan.normalized.json
    action-plan.verdict.json
    diff-policy.json
    diff-vs-action-plan.json

  gate/
    verdict.json
    <step>.log
    <step>.sandbox.json

  evidence/
    commands/
      <command-id>.summary.json
      <command-id>.metadata.json

  ownaudit/
    agent-run.sarif
    risk.json
    triage.md
    evidence.jsonl
```

"meta.json" should reference all major artifacts:

```json
{
  "schema": "o7.run_meta.v1",
  "run_id": "2026-07-06T12-00-00Z-ownaudit-fix-own001",
  "target_repo": "OwnAudit",
  "base_commit": "...",
  "head_commit": "...",
  "task_contract": "task.o7.toml",
  "action_plan": "action-plan.yaml",
  "policy_verdict": "PASS",
  "diff_policy_verdict": "PASS",
  "gate_verdict": "PASS",
  "ownaudit_ingest_verdict": "PASS",
  "risk_level": "medium"
}
```

## 14. CLI additions

### 14.1. 007

```text
o7 plan --task task.o7.toml --out action-plan.yaml
o7 validate-plan --task task.o7.toml --plan action-plan.yaml
o7 run --task task.o7.toml --plan action-plan.yaml
o7 diff-policy --run runs/<target>/<run-id>
o7 evidence --run runs/<target>/<run-id>
```

### 14.2. OwnAudit

```text
ownaudit ingest-o7-run runs/<target>/<run-id>
ownaudit summarize-o7-risk runs/<target>/<run-id>
```

### 14.3. Own.NET

```text
own check --emit-repair-constraints
own export-effects --format json
```

These are not all Phase 1 requirements. They define the direction.

## 15. Phased roadmap

Phase 0: proposal only

Deliverables:

- this document;
- schema sketches;
- example ActionPlan;
- example OwnAudit SARIF rule list.

Acceptance criteria:

- proposal does not duplicate existing PromptOps/Zero Trust/Workflow/Sandboy docs;
- proposal clearly defines the missing bridge layer.

Phase 1: ActionPlan MVP in 007

Deliverables:

- `schemas/o7.action-plan.schema.json`;
- `examples/action-plans/ownaudit-fix-own001.yaml`;
- `o7 validate-plan`;
- store ActionPlan in run record;
- emit `policy/action-plan.verdict.json`.

Acceptance criteria:

- invalid ActionPlan fails before agent patching;
- undeclared write path fails;
- forbidden path declaration fails;
- missing re-audit for `fix_existing_finding` fails.

Phase 2: Diff vs ActionPlan

Deliverables:

- compare changed files against ActionPlan;
- compare changed files against TaskContract;
- emit `policy/diff-vs-action-plan.json`;
- fail before expensive gates when diff already violates plan.

Acceptance criteria:

- changing undeclared file fails;
- changing `.007/**` fails unless explicitly allowed;
- changing dependency/build files fails unless task scope allows it;
- docs-only changes do not satisfy `fix_existing_finding`.

Phase 3: AOB evidence events

Deliverables:

- AOB emits `summary.json` for `git diff`, `rg`, `dotnet test`, `msbuild`;
- 007 stores AOB summaries under run evidence;
- command summaries can be appended to trace/evidence stream.

Acceptance criteria:

- raw output is preserved locally;
- reduced output is structured;
- OwnAudit can read command summaries without parsing raw logs.

Phase 4: OwnAudit ingest

Deliverables:

- `ownaudit ingest-o7-run <run-dir>`;
- initial O7PLAN/O7DIFF/O7GATE/O7TRC/O7AUDIT rules;
- emit `agent-run.sarif`;
- emit `risk.json`;
- emit `triage.md`.

Acceptance criteria:

- OwnAudit flags missing ActionPlan;
- OwnAudit flags diff outside ActionPlan;
- OwnAudit flags gate config edits;
- OwnAudit flags target finding not removed;
- OwnAudit distinguishes policy violation from policy gap.

Phase 5: Own.NET repair constraints

Deliverables:

- Own.NET exports repair constraints for OWN001/OWN014;
- ActionPlan can reference target diagnostic + repair constraints;
- agent fix prompts receive structured allowed/forbidden fix patterns.

Acceptance criteria:

- agent cannot claim OWN001 fix by suppressing diagnostic;
- agent cannot remove behavior without explicit high-risk declaration;
- re-audit proves target finding removed and no new findings introduced.

## 16. Security notes

This proposal does not replace sandboxing.

"ActionPlan" is not a security boundary. It is a declared-intent artifact.

Security enforcement still belongs to:

- CUE-rendered policy;
- 007 runtime checks;
- Sandboy process isolation;
- no-network policies;
- fail-closed gates;
- hash-chained run records;
- human review for sensitive actions.

The value of "ActionPlan" is that it makes deception, drift, and accidental overreach easier to detect.

## 17. Design principles

### 17.1. Agent declares, host enforces

The agent may propose:

- plan;
- patch;
- explanation;
- verification strategy.

The host enforces:

- schema;
- policy;
- path scope;
- effect scope;
- gate execution;
- artifact storage.

### 17.2. Evidence beats narration

A markdown explanation is not enough.

Every important claim should be backed by one of:

- analyzer output;
- audit finding;
- SARIF result;
- gate verdict;
- sandbox report;
- diff policy verdict;
- trace event;
- command summary;
- re-audit result.

### 17.3. Deterministic tools stay source of truth

AI can help:

- explain findings;
- draft patches;
- propose fix plans;
- summarize run evidence;
- cluster repeated issues.

AI must not become the source of truth for:

- diagnostics;
- policy verdicts;
- sandbox verdicts;
- gate pass/fail;
- release decisions.

## 18. Concrete MVP example

Task: fix top "OWN001" subscription leak in OwnAudit.

Expected flow:

1. `task.o7.toml` declares:
   - target repo: OwnAudit
   - intent: fix_existing_finding
   - allowed paths: selected source/test files
   - forbidden paths: `.007/**`, `.agents/**`, project files, build files
   - required gates: test + re-audit

2. Agent produces `action-plan.yaml`:
   - target finding: OWN001
   - proposed changed file: src/Foo/ViewModel.cs
   - expected effect: remove OWN001
   - required verification: ownaudit re-audit

3. 007 validates ActionPlan:
   - schema valid
   - writes allowed
   - re-audit present
   - no forbidden path declared

4. Agent patches code.

5. 007 validates diff:
   - only declared files changed
   - no gate config changed
   - no dependency files changed

6. Gates run:
   - tests
   - OwnAudit re-audit
   - no-new-findings check

7. Run record is stored:
   - task
   - plan
   - diff
   - policy verdicts
   - gate verdicts
   - trace/evidence

8. OwnAudit ingests run:
   - emits `agent-run.sarif`
   - emits `risk.json`
   - emits `triage.md`

Pass condition:

- target OWN001 finding removed;
- no new findings introduced;
- diff matches ActionPlan;
- all gates pass;
- sandbox reports contain no violations;
- OwnAudit ingest produces no high-risk O7* findings.

## 19. Open questions

1. Should `ActionPlan` be generated by the agent, by 007 from a task template, or both?
2. Should protected mode require human approval of `ActionPlan` before patching?
3. Should `ActionPlan` be YAML, JSON, or CUE-rendered JSON?
4. Should ActionPlan be hash-chained as part of the run record from Phase 1?
5. How strict should the first `fix_existing_finding` intent be?
6. Should OwnAudit ingest fail the run, or only report risk?
7. Should AOB summaries live under `agent.trace.jsonl` or separate `evidence/commands/*.json`?

Recommended v1 answers:

1. Agent may draft ActionPlan; 007 validates it.
2. Human approval optional for local runs, required for sensitive scopes.
3. YAML for authoring/examples, normalized JSON for storage.
4. Yes, include normalized ActionPlan in run-record hash chain.
5. Very strict: no suppression, no public API changes, re-audit required.
6. OwnAudit reports risk; 007 decides enforcement.
7. Store both: trace event points to evidence artifact.

## 20. Final recommendation

Implement this as a bridge proposal, not as another standalone subsystem.

Priority order:

```text
P0:
  - O7ActionPlan schema;
  - ActionPlan validation;
  - store ActionPlan in run record.

P1:
  - DiffPolicy vs ActionPlan;
  - Effect Ledger vocabulary;
  - initial OwnAudit O7PLAN/O7DIFF rules.

P2:
  - AOB summaries as evidence;
  - OwnAudit ingest-o7-run;
  - Own.NET repair constraints for OWN001/OWN014.
```

The highest-value slice is:

```text
O7ActionPlan
  + DiffPolicy binding
  + OwnAudit ingest
```

That gives the system what it currently lacks most:

```text
declared intent
  → actual diff
  → runtime evidence
  → audit verdict
```

Without this bridge, 007 can become a pile of isolated runs and nice logs.

With this bridge, 007 becomes a verifiable agent execution harness where Own.NET supplies deterministic analysis, 007 enforces runtime boundaries, Sandboy isolates process behavior, and OwnAudit turns the evidence into reviewable risk.
