# PromptOps / AgentOps слой для Own.NET, OwnAudit и 007

## 1. Кратко

Предлагается добавить общий инженерный слой для промптов, AI-задач, evals, схем ответов и agent-run артефактов вокруг трёх проектов:

- Own.NET — источник доменной логики: ownership checker, OwnIR, WPF lifetime/resource diagnostics, Roslyn extractor, static analysis core.
- OwnAudit — audit/fix оболочка: запуск анализаторов, нормализация в SARIF, scoring, health report, будущий fix arm.
- 007 — приватный agent harness: изолированные запуски "claude"/"codex" над Own.NET и OwnAudit через CLI, worktree isolation, gates, run artifacts.

Цель: перестать использовать промпты как магические строки и оформить их как версионируемые, тестируемые, ревьюируемые модули продукта.

## 2. Текущее положение

### Own.NET

Own.NET сейчас является PoC OwnLang: маленький ownership language с Rust-style ownership discipline, который компилируется в C# и включает lexer/parser, AST, resolver, signatures, CFG, flow-sensitive dataflow, diagnostics и codegen.

Важный для proposal момент: Own.NET уже не просто toy language. В README описан WPF lifetime/module slice, где подписка на событие моделируется как "acquire" token, отписка как "release", а утечки превращаются в OWN001/OWN014 diagnostics. Это идеально ложится на audit/triage/fix pipeline.

### OwnAudit

OwnAudit уже определён как lift-out home для audit pipeline Own.NET. Каноническая реализация пока живёт в "Own.NET/audit/": static aggregation, taxonomy, scoring, reporters, runtime LeakHarness, DuplicateDetector и storm profiler. OwnAudit держит boundary, STS runner, artifacts и будущую C# skeleton-зону.

OwnAudit также уже имеет валидированный STS-прогон: 380 findings за 49 секунд, с категоризацией, heatmap и spot-check true positives. Это значит, что у нас есть не игрушечная база для eval-кейсов и regression tests.

### 007

007 уже описан как приватный harness, который гоняет "claude"/"codex" по публичным Own.NET и OwnAudit через CLI, без API keys, через subscription auth. README прямо говорит держать репозиторий приватным, потому что agent-routing/subscription-auth не должны попадать в public tree.

MVP 007 уже имеет правильную базовую форму: isolate через "git worktree", run agent, gate через ".007/gate.toml", harvest canonical record с "task.md", "meta.json", "agent.stdout", "diff.patch", gate logs и verdict.

Это почти готовая AgentOps-платформа. Просто пока без нормального PromptOps-контракта. То есть двигатель есть, но руль пока из фанеры. Классика.

## 3. Проблема

Сейчас AI-задачи рискуют расползтись в три плохих формы:

1. Промпты как одноразовые task.md
   
   - невозможно сравнивать версии;
   - невозможно понять, почему агент начал вести себя иначе;
   - невозможно прогонять regression evals.

2. Агент как “умный shell script”
   
   - результат зависит от phrasing;
   - слабая воспроизводимость;
   - трудно отличить улучшение от случайной удачи.

3. AI как параллельный анализатор без доверия
   
   - если AI выдаёт finding без привязки к deterministic analyzer / SARIF / source span / gate, это просто уверенный попугай в каске инженера.

## 4. Предлагаемая архитектура

Ввести общий слой:

```text
PromptOps = prompt modules + typed inputs + output schemas + evals + versioning
AgentOps  = task specs + isolated runs + gates + artifacts + regression history
```

### 4.1. Own.NET: source of analysis truth

Own.NET остаётся местом, где живут:

- ownership model
- OwnIR
- diagnostics OWN001/OWN014/OWN050/...
- Roslyn extractor
- WPF lifetime profile
- audit static core, пока lift-out не завершён

AI-слой в Own.NET не должен заменять checker. Он должен помогать вокруг него:

- объяснять diagnostics человеку;
- генерировать минимальные repro cases;
- предлагать новые gallery/corpus examples;
- находить подозрительные code patterns для будущих deterministic rules;
- ревьюить изменения checker-а по golden cases;
- писать draft docs для diagnostic catalog.

Правило: AI не является источником истины для diagnostics. Истина — analyzer output, tests, SARIF, gates. AI может быть scout, narrator, fixer candidate, но не судья.

### 4.2. OwnAudit: audit/fix orchestration

OwnAudit должен стать местом, где AI применяется после deterministic aggregation:

```text
OwnSharp / CodeQL / Roslyn / runtime harness
        ↓
SARIF normalization
        ↓
taxonomy + scoring + cross-tool agreement
        ↓
AI triage / explanation / fix proposal
        ↓
dry-run patch
        ↓
re-audit
        ↓
gate verdict
```

AI-задачи для OwnAudit:

- explain top findings by pain score;
- cluster repeated leaks into root causes;
- generate candidate fix plans;
- draft minimal patches for OWN001/OWN014;
- summarize audit report for PR / issue;
- compare before/after health reports;
- detect “new findings introduced by fix”.

Особенно важно для fix arm: уже в PLAN описана идея dry-run → diff → re-audit no-new-findings → tier gate.  AI должен встраиваться именно туда, а не “я поправил, поверь брат”.

### 4.3. 007: private execution harness

007 становится исполнителем AI-задач:

```text
o7 run --repo <path> --base <ref> --task ./task.md --gate <toml>
```

Но "task.md" должен стать не ручной простынёй, а render output из typed task spec.

Новая форма:

```text
task spec
  ↓
prompt module + typed args
  ↓
rendered task.md
  ↓
007 isolated run
  ↓
gate
  ↓
run record
  ↓
eval/regression store
```

007 должен хранить не только "task.md", "agent.stdout", "diff.patch" и gate logs, но и:

```json
{
  "task_id": "ownaudit.fix.own001.subscription-token.v1",
  "prompt_module": "ownaudit.fix-own001",
  "prompt_version": "0.1.0",
  "input_schema_version": "1",
  "output_schema_version": "1",
  "target_repo": "OwnAudit",
  "base_commit": "...",
  "engine": "claude-cli",
  "gate_verdict": "PASS",
  "eval_suite": "ownaudit.fix-arm.smoke",
  "created_at": "..."
}
```

Без этого через месяц будет невозможно понять, что именно сработало: промпт, модель, конкретный diff, gate, удача или случайный демон в WSL.

## 5. Предлагаемая структура файлов

### 5.1. В Own.NET

```text
Own.NET/
  ai/
    prompts/
      ownnet.diagnostic-explainer.prompt.md
      ownnet.gallery-case-generator.prompt.md
      ownnet.checker-review.prompt.md

    schemas/
      diagnostic-explanation.schema.json
      gallery-case-proposal.schema.json
      checker-review.schema.json

    evals/
      diagnostic-explainer.cases.jsonl
      gallery-case-generator.cases.jsonl
      checker-review.cases.jsonl

    tasks/
      explain-own001.example.task.yaml
      generate-wpf-lifetime-case.example.task.yaml

  .007/
    gate.toml
```

### 5.2. В OwnAudit

```text
OwnAudit/
  ai/
    prompts/
      ownaudit.report-triage.prompt.md
      ownaudit.finding-cluster.prompt.md
      ownaudit.fix-plan.prompt.md
      ownaudit.fix-own001.prompt.md
      ownaudit.fix-own014.prompt.md
      ownaudit.audit-diff-review.prompt.md

    schemas/
      audit-triage.schema.json
      finding-cluster.schema.json
      fix-plan.schema.json
      patch-review.schema.json

    evals/
      report-triage.cases.jsonl
      finding-cluster.cases.jsonl
      fix-own001.cases.jsonl
      fix-own014.cases.jsonl
      audit-diff-review.cases.jsonl

    tasks/
      triage-sts-health-report.task.yaml
      fix-top-subscription-leak.task.yaml
      compare-health-report-before-after.task.yaml

  .007/
    gate.toml
```

### 5.3. В 007

```text
007/
  src/
    prompt_registry/
    task_renderer/
    run_store/
    gate_runner/

  prompts/
    internal/
      o7.agent-system.prompt.md
      o7.diff-summarizer.prompt.md
      o7.gate-failure-classifier.prompt.md

  schemas/
    run-record.schema.json
    gate-verdict.schema.json
    task-spec.schema.json

  examples/
    tasks/
      ownnet.checker-review.yaml
      ownaudit.fix-own001.yaml

  runs/
    <target>/
      <run-id>/
        task.md
        task.yaml
        meta.json
        prompt.rendered.md
        prompt.meta.json
        agent.stdout
        diff.patch
        gate/
          verdict.json
          *.log
```

## 6. Prompt module contract

Каждый prompt module должен иметь contract file:

```yaml
name: ownaudit.fix-own001
version: 0.1.0
owner: ownaudit
purpose: >
  Generate a minimal candidate patch for OWN001 subscription-token leaks
  using the audit finding, source snippet, and project constraints.

inputs:
  finding:
    schema: OwnAuditFinding.v1
    required: true
  source_context:
    schema: SourceContext.v1
    required: true
  project_constraints:
    schema: ProjectConstraints.v1
    required: true

output:
  schema: FixProposal.v1

allowed_actions:
  - propose_patch
  - explain_risk
  - request_skip_with_reason

forbidden_actions:
  - invent_source_files
  - suppress_finding_without_fix
  - modify_unrelated_code
  - change_public_behavior_without_callout

evals:
  - own001.event-subscription-basic
  - own001.static-event-region-escape
  - own001.false-positive-equal-lifetime
  - own001.dispose-existing-pattern
```

## 7. Task spec contract

007 должен принимать task spec, а не только свободный "task.md".

Пример:

```yaml
id: ownaudit.fix.top-subscription-leak
target_repo: OwnAudit
base_ref: main

prompt:
  module: ownaudit.fix-own001
  version: 0.1.0

inputs:
  finding_file: artifacts/findings.json
  finding_selector:
    diagnostic: OWN001
    category: subscription
    rank: 1

constraints:
  max_files_changed: 3
  require_tests: true
  require_reaudit: true
  no_public_api_break: true

gate:
  file: .007/gate.toml
  required_verdict: PASS

outputs:
  patch: diff.patch
  explanation: fix-explanation.md
  verdict: gate/verdict.json
```

Rendered "task.md" уже может быть человекочитаемым, но canonical source должен быть YAML/JSON task spec.

## 8. Output schemas

AI-ответы должны быть структурированными. Не “вот мои мысли”, не markdown-суп, не философия про качество кода.

Пример "FixProposal.v1":

```json
{
  "type": "object",
  "required": [
    "summary",
    "target_findings",
    "changes",
    "risk_level",
    "verification_plan"
  ],
  "properties": {
    "summary": { "type": "string" },
    "target_findings": {
      "type": "array",
      "items": { "type": "string" }
    },
    "changes": {
      "type": "array",
      "items": {
        "type": "object",
        "required": ["file", "reason", "expected_effect"],
        "properties": {
          "file": { "type": "string" },
          "reason": { "type": "string" },
          "expected_effect": { "type": "string" }
        }
      }
    },
    "risk_level": {
      "type": "string",
      "enum": ["low", "medium", "high"]
    },
    "verification_plan": {
      "type": "array",
      "items": { "type": "string" }
    }
  }
}
```

Если агент не может вернуть valid schema, run должен быть "ERROR", а не “ну вроде понятно”. WHERE'S THE ERROR HANDLING?! Вот оно.

## 9. Evals

### 9.1. Типы evals

Нужны четыре слоя:

1. Render tests
   Проверить, что prompt рендерится без незаполненных placeholders.

2. Schema tests
   Проверить, что output валиден по JSON Schema.

3. Golden behavior tests
   Проверить, что на известных кейсах агент делает ожидаемое.

4. Regression tests
   Каждый production failure превращается в eval case.

### 9.2. Own.NET eval cases

ownnet.diagnostic-explainer:
- OWN001 subscription token leak
- OWN002 use after release
- OWN014 region escape
- OWN050 coverage ledger, not fake clean

ownnet.gallery-case-generator:
- generate minimal WPF event leak case
- generate static event escape case
- generate ArrayPool borrow conflict case
- reject case that does not trigger intended diagnostic

ownnet.checker-review:
- detect change that weakens release-on-all-paths
- detect diagnostic code drift
- detect missing golden update

### 9.3. OwnAudit eval cases

ownaudit.report-triage:
- summarize STS health report top pain areas
- distinguish high-confidence from single-tool finding
- keep OWN050 as coverage ledger, not clean result

ownaudit.finding-cluster:
- cluster repeated subscription leaks by namespace/type
- group static event region escapes separately
- avoid merging IDisposable leaks with event leaks

ownaudit.fix-own001:
- propose unsubscribe in Dispose when pattern exists
- propose IDisposable implementation only when appropriate
- skip when lifetime/source cannot be proven
- never suppress without fix

ownaudit.audit-diff-review:
- detect new findings after patch
- detect category movement
- compare pain score before/after

### 9.4. 007 eval cases

o7.task-renderer:
- task spec renders deterministic task.md
- prompt version recorded in meta.json
- missing input fails before agent run

o7.gate-runner:
- PASS exits 0
- FAIL exits 1
- ERROR exits 1 with structured reason

o7.harvest:
- run directory contains required files
- diff.patch saved even on FAIL if diff exists
- prompt.rendered.md and prompt.meta.json saved

## 10. Gates

Схема манифеста — как в `007/src/gate.rs` и `007/examples/gate.own.net.toml`: заголовок `schema = 1` и таблицы `[[gate]]` с ключами `name`/`cmd`/`required`/`env`.

### 10.1. Own.NET `.007/gate.toml`

Initial gate:

```toml
schema = 1

[[gate]]
name = "python-tests"
cmd = "python tests/run_tests.py"
required = true

[[gate]]
name = "gallery-tests"
cmd = "python tests/test_gallery.py"
required = true

[[gate]]
name = "wpf-tests"
cmd = "python tests/test_wpf.py"
required = true

[[gate]]
name = "lifetime-tests"
cmd = "python tests/test_lifetimes.py"
required = true

[[gate]]
name = "prompt-contracts"
cmd = "python ai/tools/check_prompt_contracts.py"
required = true
```

### 10.2. OwnAudit `.007/gate.toml`

Initial gate:

```toml
schema = 1

[[gate]]
name = "schema-check"
cmd = "dotnet test"
required = true
env = "windows"

[[gate]]
name = "prompt-contracts"
cmd = "python ai/tools/check_prompt_contracts.py"
required = true

[[gate]]
name = "audit-report-smoke"
cmd = "pwsh ./Run-Audit.ps1 -Smoke"
required = true
env = "windows"

[[gate]]
name = "no-new-findings"
cmd = "python ai/tools/compare_audit_reports.py --baseline artifacts/health-report.json --current artifacts/current-health-report.json"
required = true
```

Оговорка: у текущего `OwnAudit/Run-Audit.ps1` ключа `-Smoke` нет (его param-блок принимает только `-OwnNet`, `-Ref`, `-Target`, `-Worktree`, `-Out`, `-Codeql`, `-Strict`, `-CodeqlExe`, `-CodeqlDb`, `-RebuildCodeqlDb`, `-LineTol`) — режим `-Smoke` нужно сначала добавить в Run-Audit.ps1; это внесено в Phase 0 deliverables.

Windows-bound parts should be tagged explicitly. 007 README already says Own.NET is target first and OwnAudit Windows-bound gates are Phase 2, tagged `env = "windows"` in the manifest.

### 10.3. 007 internal gate

```toml
schema = 1

[[gate]]
name = "fmt"
cmd = "cargo fmt --check"
required = true

[[gate]]
name = "clippy"
cmd = "cargo clippy -- -D warnings"
required = true

[[gate]]
name = "tests"
cmd = "cargo test"
required = true

[[gate]]
name = "schema-fixtures"
cmd = "cargo test task_spec_schema"
required = true
```

## 11. Security and privacy constraints

1. 007 remains private.
   Subscription auth, agent routing and CLI orchestration must not land in public Own.NET/OwnAudit. This is already stated in the 007 README and should remain a hard boundary.

2. Public repos may contain prompt contracts, not private routing.
   Own.NET/OwnAudit can contain:
   
   - prompt templates;
   - eval fixtures;
   - schemas;
   - sample task specs;
   - gates.
   
   They should not contain:
   
   - subscription auth details;
   - machine-local paths;
   - private STS code snippets unless already allowed;
   - agent credentials;
   - hidden routing logic.

3. Agent writes only inside isolated worktree.
   007 already uses worktree isolation and gates. Keep this as the core safety model.

4. No irreversible ops.
   Deny-list stays mandatory. Agent output is patch proposal until gate passes.

5. No AI-only trust.
   AI-generated fix must pass:
   
   - schema validation;
   - repo tests;
   - audit diff check;
   - no-new-findings gate;
   - optional manual review for high-risk categories.

## 12. Roadmap

### Phase 0 — Contract skeleton

Deliverables:

- ai/prompts/ directories in Own.NET and OwnAudit
- ai/schemas/ directories
- ai/evals/ seed cases
- ai/tools/check_prompt_contracts.py
- ai/tools/compare_audit_reports.py (сравнение health-report'ов для gate `no-new-findings`; сегодня ближайший аналог — `report/diff_cli.py`)
- .007/gate.toml in Own.NET
- .007/gate.toml in OwnAudit
- `-Smoke` mode in OwnAudit Run-Audit.ps1 (required by the audit-report-smoke gate step)
- task spec schema in 007

Acceptance criteria:

- prompt templates render deterministically
- unresolved placeholders fail CI/gate
- schemas validate
- sample task spec renders into task.md
- 007 records prompt metadata in run artifacts

### Phase 1 — Own.NET diagnostic explainer

First useful vertical slice:

```text
Input:
  OWN diagnostic + source span + small context

Output:
  structured explanation:
    - what happened
    - why checker is right
    - likely C# analogue
    - suggested safe fix
    - confidence
```

Why first: low risk, no code modification, good docs payoff.

Acceptance criteria:

- supports OWN001, OWN002, OWN014, OWN050
- evals cover at least 10 cases
- invalid/missing diagnostic fails schema
- explanation does not invent source facts

### Phase 2 — OwnAudit report triage

Second vertical slice:

```text
Input:
  health-report.json / findings.json

Output:
  ranked triage:
    - top pain areas
    - repeated root causes
    - likely quick wins
    - what requires runtime proof
    - what should stay low-confidence
```

Acceptance criteria:

- preserves single-tool vs cross-tool confidence distinction
- does not convert OWN050 ledger entries into clean results
- links every recommendation to finding ids/categories
- produces markdown summary plus structured JSON

### Phase 3 — 007 prompt-aware run records

Extend 007 run artifacts:

```text
runs/<target>/<run-id>/
  task.yaml
  task.md
  prompt.rendered.md
  prompt.meta.json
  meta.json
  agent.stdout
  diff.patch
  gate/
```

Acceptance criteria:

- every run records prompt module/version
- every run records task spec hash
- every run records target repo/base commit
- prompt changes are diffable
- failed gates still preserve diagnostics

### Phase 4 — OwnAudit fix arm MVP

Scope: OWN001 subscription-token leaks only.

Flow:

```text
finding cluster
  ↓
AI fix plan
  ↓
agent patch in isolated worktree
  ↓
repo tests
  ↓
re-audit
  ↓
no-new-findings gate
  ↓
manual review
```

Acceptance criteria:

- modifies max 3 files per run
- no unrelated formatting churn
- no suppressions as "fix"
- before/after report generated
- patch rejected if new findings appear

### Phase 5 — OWN014 / region escape support

Scope: static event / longer-lived source escape.

Acceptance criteria:

- handles static event unsubscribe patterns
- distinguishes token leak vs region escape
- skips cases where lifetime source cannot be proven
- documents uncertainty rather than inventing ownership

## 13. Proposed initial issues

### Own.NET

1. Add ai/ prompt contract skeleton
2. Add diagnostic explanation schema
3. Add OWN001/OWN002/OWN014/OWN050 explanation eval cases
4. Add prompt contract checker
5. Add .007/gate.toml for Own.NET

### OwnAudit

1. Add ai/ prompt contract skeleton
2. Add audit triage schema
3. Add finding cluster schema
4. Add fix proposal schema
5. Add STS health-report triage seed evals
6. Add .007/gate.toml for OwnAudit

### 007

1. Add task spec schema
2. Add prompt module metadata to run record
3. Save prompt.rendered.md and prompt.meta.json
4. Validate task spec before agent run
5. Add run artifact schema tests
6. Add env-tagged gate steps

## 14. Non-goals

This proposal does not attempt to:

- replace Own.NET checker with LLM judgment;
- make OwnAudit a second implementation of Own.NET/audit;
- expose 007 internals publicly;
- solve model selection;
- build a SaaS-style prompt registry;
- auto-merge AI-generated fixes;
- trust AI findings without deterministic corroboration.

## 15. Design principle

Own.NET should prove or detect.

OwnAudit should aggregate, rank and verify.

007 should isolate, execute and record.

PromptOps should make every AI behavior change reviewable, testable and reversible.

Anything else is just technical debt dressed up as “agentic workflow”.
