Это бывает нужно, когда обычный RAG уже начинает пахнуть как помойка из Markdown-файлов, embeddings, SQLite-табличек, “memory.json” и надежды на лучшее.

omnigraph по README позиционируется как lakehouse graph database для context assembly и multi-agent coordination: графовая БД, где агенты могут читать/писать состояние, делать это в ветках, мержить изменения, искать по графу + векторному поиску + full-text в одном query runtime.

Зачем оно вообще надо

Главная идея: хранить не просто документы, а связи между фактами, решениями, задачами, агентами, файлами, findings, runs, diff-ами и результатами проверок.

Не:

"вот тебе куча markdown-файлов, модель, пожалуйста, догадайся"

А:

Task -> touched File -> Symbol -> Finding -> Verdict -> Run -> Gate result
     -> related Decision -> previous Failure -> known Fix pattern

То есть контекст становится не свалкой, а операционным графом знаний. Почти цивилизация, только без совещаний в 9 утра.

Что конкретно даёт Omnigraph

1. Агентная память с ветками

В README прямо указано: агенты могут обогащать граф на parallel isolated branches, а изменения потом review/merge Git-style.

Это сильно отличается от “одна общая память, куда каждый агент пишет что попало”.
Без веток агентная память быстро превращается в:

fact: "это точно false positive"
source: "модель так почувствовала"
confidence: "trust me bro"

С ветками можно делать:

agent/run-123:
  - добавил гипотезу
  - связал finding с прошлым решением
  - предложил паттерн фикса

main:
  - попадает только после gate/review

2. Context assembly для LLM

У него заявлен multimodal retrieval: graph traversal + vector ANN + full-text + Reciprocal Rank Fusion в одном runtime.

Это полезно, когда “найди релевантный контекст” нельзя решить одним embeddings search.

Пример для Own.NET:

Задача: исправить утечку памяти в справочнике TNVED.

Обычный vector search:
  "вот 20 похожих файлов, удачи, кожаный мешок"

Graph + search:
  - этот Presenter трогает TNVED cache
  - этот cache уже ломался в run-42
  - этот gate падал из-за lazy projection
  - это решение было принято в ADR-007
  - вот тест, который защищает инвариант

Вот это уже контекст. Не магия, но хотя бы не лотерея.

3. Dev graph для coding agents

README прямо перечисляет use case Dev graph: issues & dependency model that coding agents read and write.

Для твоего стека это может быть:

Issue
Finding
File
Symbol
Project
Test
Gate
Run
Decision
Hypothesis
Regression
Patch

И связи:

Run PRODUCED Finding
Finding POINTS_TO File
Patch TOUCHES Symbol
Gate FAILED_ON Test
Decision JUSTIFIES Suppression
Finding DUP_OF Finding
Fix INTRODUCED Regression

Это особенно полезно для Own.NET/OwnAudit/007, потому что у тебя там не “один маленький pet project”, а уже натуральный зоопарк: агенты, анализаторы, gates, ложноположительные срабатывания, legacy .NET, будущий memory layer. Да, всё как любят люди: сначала сделать сложность, потом сделать систему для управления сложностью.

4. Безопасность и governance

У Omnigraph заявлена Cedar policy enforcement server-side on every mutation, bearer auth, actor/audit tracking.  Также README отдельно говорит, что every write path идёт через Cedar gate, actor определяется server-side, а клиент не может сам себя подделать.

Для multi-agent системы это важно. Иначе агент может писать:

{
  "actor": "senior_architect",
  "decision": "delete all tests, vibes are green"
}

WHERE'S THE ERROR HANDLING?!
Правильнее: сервер сам знает actor/token/policy, а не верит клиенту на слово.

5. Инфраструктурно: хранение не только локально

Omnigraph рассчитан на S3-compatible object store: RustFS/MinIO on-prem, AWS S3, R2, GCS; данные остаются в твоём store.  Для локального dev есть embedded file-backed graph без сервера.

То есть можно начать с:

./graph.omni

А потом переехать в:

s3://company/clusters/company-brain

без полной смены концепции.

Где это может лечь на 007

В 007 сейчас есть простой run lifecycle: worktree → agent → gate → record. README 007 прямо говорит, что memory layer отложен на потом.

Вот Omnigraph как раз может стать этим memory layer, но не вместо runs/, а поверх/рядом.

Минимальная схема

Node Run {
  id
  target
  base_commit
  engine
  model
  verdict
  started_at
}

Node Task {
  sha256
  text
}

Node File {
  path
  repo
}

Node Finding {
  id
  tool
  rule
  message
  line
}

Node GateStep {
  name
  status
  duration
}

Node Decision {
  title
  body
  confidence
  source
}

Edge Run_GIVEN_Task: Run -> Task
Edge Run_TOUCHED_File: Run -> File
Edge Run_PRODUCED_Finding: Run -> Finding
Edge Run_RAN_GateStep: Run -> GateStep
Edge Finding_IN_File: Finding -> File
Edge Decision_EXPLAINS_Finding: Decision -> Finding
Edge Decision_SUPERSEDES_Decision: Decision -> Decision

Практическая польза:

"Покажи все прошлые runs, где этот analyzer rule оказался false positive"
"Какие файлы часто ломают gate после фикса graph 47?"
"Какие решения уже принимали по SQLite/SQL Server compatibility?"
"Какие findings связаны с WPF memory leak?"
"Какие patches трогали TNVED cache и чем закончились?"

Вот тут Omnigraph становится не модной графовой игрушкой, а нормальным журналом инженерной памяти.

Где можно связать с FastContext

Три слоя:

Omnigraph = долговременная структурная память
FastContext = быстрый локальный summarizer/router
Claude/Codex = дорогой исполнитель/судья
007 = оркестратор run/gate/record

Pipeline:

task.md
  ↓
007 asks Omnigraph:
  related runs, files, failures, decisions, suppressions
  ↓
FastContext compresses that into context brief
  ↓
Claude/Codex gets clean task + context
  ↓
007 gates result
  ↓
007 writes new facts back to Omnigraph branch
  ↓
merge only if reviewed/gated

Вот это выглядит сильно. Не потому что “AI memory”, а потому что есть provenance, branches, merge, policies и typed graph.

Когда оно не надо

Не надо тащить Omnigraph, если задача:

- один агент
- один repo
- маленькая история runs
- достаточно grep + markdown + SQLite
- нет параллельных агентов
- нет необходимости review/merge памяти
- нет governance/policy

Иначе получится: “нам нужен был список заметок, поэтому мы подняли lakehouse graph database”. This is so tightly coupled it makes conjoined twins look independent!

Для 007 прямо сейчас я бы не начинал с “полной интеграции”. Я бы сделал тонкий эксперимент:

Phase 1:
  after o7 run:
    write Run/Task/File/Gate/Finding nodes into local ./graph.omni

Phase 2:
  before o7 run:
    query related context from Omnigraph
    produce context.md

Phase 3:
  branch per agent run:
    agent/run-id
    merge only if gates pass

Phase 4:
  FastContext summarizes Omnigraph query result for Claude/Codex

Коротко

omnigraph нужен, когда тебе нужна не “память агента”, а версионируемая, policy-controlled, queryable инженерная память для флота агентов.

Для твоих проектов самый вкусный use case:

007 run records + Own.NET architecture decisions + OwnAudit findings
→ в один dev/context graph
→ оттуда собирать контекст перед задачей
→ туда писать результаты после gates

И вот тогда 007 перестаёт быть просто harness-ом для одного isolated run и становится системой, которая накапливает опыт. Не “модель вспомнила”, а “система знает, откуда это взялось”.

Да, через MCP можно, но я бы разделил:

Claude Code / Codex / agent напрямую читает Omnigraph → MCP

007 сам собирает контекст, пишет run records, делает gates → лучше HTTP/CLI/SDK, не MCP

MCP внутри 007 full-auto run → осторожно, только read-only на старте

MCP тут нужен как адаптер для LLM-host-а: Claude/Codex видит Omnigraph как набор tools/resources и может сам спрашивать граф: “найди связанные решения”, “покажи прошлые runs”, “какие findings были false positive”. MCP в целом как раз стандартизирует подключение LLM к внешним tools/data sources. 

Что говорит сам Omnigraph

В README Omnigraph прямо перечислен MCP server как bridge к LLM hosts: Claude, Codex и прочим. Там же указан пакет @modernrelay/omnigraph-mcp.

А сам Omnigraph-сервер поднимает графы по /graphs/{id}/…, после cluster apply и запуска omnigraph-server.  Команды query/mutate/branch/merge тоже есть в CLI: omnigraph query, omnigraph mutate, omnigraph branch create, omnigraph branch merge.

То есть архитектурно оно выглядит так:

Claude Code / Codex
        ↓ MCP
@modernrelay/omnigraph-mcp
        ↓ HTTP / SDK
omnigraph-server
        ↓
dev graph / context graph / run graph

Как бы я подключал для твоего 007

Вариант 1: MCP для Claude/Codex, 007 только даёт доступ

Это самый естественный путь.

o7 run
  ↓
создаёт isolated worktree
  ↓
запускает claude/codex
  ↓
агент через MCP читает Omnigraph
  ↓
агент правит код
  ↓
o7 gate
  ↓
o7 harvest

Плюс: агент сам может искать контекст.
Минус: воспроизводимость хуже, потому что часть reasoning/tool-calls живёт внутри agent session. А у 007 смысл как раз в “запустил, записал, загейтил”, а не “модель что-то там спросила у магической трубы”.

Для начала я бы дал MCP только read-only tools:

omnigraph.search_context
omnigraph.query
omnigraph.get_node
omnigraph.related
omnigraph.schema

А вот это пока не давал бы агенту напрямую:

omnigraph.mutate
omnigraph.branch_merge
omnigraph.policy_apply
omnigraph.schema_change

Иначе получишь “агент сам себе память написал, сам себе поверил, сам себя замержил”. IT'S FUCKING BUGGY! Не баг в коде, баг в устройстве вселенной.

Вариант 2: 007 сам ходит в Omnigraph, без MCP

Для core 007 это лучше.

o7 context --source omnigraph
  ↓
HTTP-запрос к omnigraph-server
  ↓
context.md
  ↓
o7 run --task task.with-context.md

Почему лучше не MCP внутри 007:

MCP хорош для LLM-host-а.
HTTP/CLI хорош для deterministic harness-а.

007 не должен притворяться LLM-клиентом, если ему надо просто выполнить stored query и сохранить результат в run record. MCP тут будет лишней прослойкой, как JSON-RPC матрёшка ради того, чтобы вызвать query. Великолепное человеческое стремление усложнить шланг для воды до Kubernetes.

Практическая схема подключения

1. Поднимаешь Omnigraph cluster

У Omnigraph flow такой:

omnigraph cluster validate
omnigraph cluster plan
omnigraph cluster apply

omnigraph-server --cluster ./company-brain --bind 127.0.0.1:8080

Это прямо описано в README: validate → plan → apply → server.

2. Заводишь dev graph для 007

Примерно:

007-graph/
├── cluster.yaml
├── dev.pg
├── queries/
│   ├── context_for_task.gq
│   ├── related_runs.gq
│   ├── finding_history.gq
│   └── affected_files.gq
└── policy.yaml

Минимальная модель:

node Run
node Task
node Repo
node File
node Finding
node GateStep
node Decision
node Patch

edge Run_GIVEN_Task: Run -> Task
edge Run_TOUCHED_File: Run -> File
edge Run_PRODUCED_Finding: Run -> Finding
edge Finding_IN_File: Finding -> File
edge Run_RAN_GateStep: Run -> GateStep
edge Decision_EXPLAINS_Finding: Decision -> Finding
edge Patch_TOUCHES_File: Patch -> File

3. Подключаешь MCP к Claude/Codex

Так как в README основного repo я вижу только ссылку на пакет @modernrelay/omnigraph-mcp, но не вижу точного launch contract, я бы не стал высекать на камне конкретные флаги. Общая форма будет такая:

{
  "mcpServers": {
    "omnigraph": {
      "command": "npx",
      "args": [
        "-y",
        "@modernrelay/omnigraph-mcp"
      ],
      "env": {
        "OMNIGRAPH_SERVER": "http://127.0.0.1:8080",
        "OMNIGRAPH_GRAPH": "dev",
        "OMNIGRAPH_TOKEN": "dev-token"
      }
    }
  }
}

Или CLI-форма, если host поддерживает mcp add:

claude mcp add omnigraph -- npx -y @modernrelay/omnigraph-mcp

Но точные env/args надо сверить с README самого npm-пакета. В основном README подтверждён сам факт MCP bridge и имя пакета, не конкретные флаги запуска.

Как это должно выглядеть в prompt-е агента

В o7 run task можно добавлять:

Before editing, use the omnigraph MCP server read-only.

1. Query related prior runs for this task.
2. Query known decisions touching the same files/symbols.
3. Query finding history if analyzer warnings are involved.
4. Do not mutate or merge graph branches.
5. Include the Omnigraph context you used in the final summary.

Но лучше, чтобы 007 сам добавлял это в system/task wrapper, а не каждый раз руками. Ручные инструкции, как известно, живут ровно до первого “забыл вставить”.

Что я бы сделал в 007

MVP

Добавить в 007:

o7 context omnigraph \
  --server http://127.0.0.1:8080 \
  --graph dev \
  --query context_for_task \
  --task ./task.md \
  --repo ../Own.NET \
  --out ./context.md

Потом:

cat context.md task.md > task.with-context.md

o7 run \
  --repo ../Own.NET \
  --base HEAD \
  --task task.with-context.md \
  --engine claude \
  --model opus

То есть MCP не в 007, а рядом для Claude/Codex.

Следующий шаг

После run:

o7 record omnigraph \
  --run runs/Own.NET/<run-id> \
  --server http://127.0.0.1:8080 \
  --graph dev \
  --branch agent/<run-id>

И только если gate прошёл:

omnigraph branch merge agent/<run-id> --into main

Это хорошо ложится на Omnigraph, потому что у него branches/review/merge заявлены как core сценарий для агентов.

Security posture

Omnigraph уже заявляет Cedar policy server-side on every mutation и actor/audit tracking.  README также говорит, что write paths идут через Cedar gate, actor определяется server-side, а клиент не может сам выставить identity.

Для твоего случая политика должна быть примерно такая:

Claude/Codex MCP token:
  allow read/query on main
  allow write only on branch agent/*
  deny merge
  deny schema change
  deny policy change

007 token:
  allow create branch
  allow write run record
  allow merge agent/* into main only after gate pass

human/admin token:
  allow schema/policy/merge overrides

Вот это уже нормально. Не “агенту дали root потому что он вежливо попросил”.

Мой вердикт

Да, через MCP подключать стоит, но только как интерфейс для Claude/Codex.

Для 007 core лучше:

читать Omnigraph через HTTP/CLI/SDK
писать run records через HTTP/CLI/SDK
использовать MCP только для agent-facing read-only tools

Идеальная схема:

007 deterministic layer:
  worktree, gate, harvest, run metadata

Omnigraph durable memory:
  runs, decisions, findings, file/symbol graph

MCP:
  controlled read-only context access for Claude/Codex

FastContext:
  сжимает retrieved graph context в короткий context.md

То есть MCP тут не “главная шина всего”. MCP тут розетка для агента. А 007 должен оставаться оркестратором, а не становиться ещё одним LLM-host-ом с раздвоением личности.