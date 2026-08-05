# Задача RA-0: Remote Admission — контракт (contract-first, RED)

> Идентификаторы этапов: **RA-0** (этот документ — контракт + RED-оракулы) →
> **RA-1** (реализация: networked acquirer + execution receipt). Префикс RA-
> обязателен: в проекте уже существуют Q-Deck A0/A1 и Sandboy SB-A0/SB-A1.
> Серия RA- независима от серии SB- как контракт, но RA-1 (реализация)
> требует SB-A1 GREEN — см. «Пререквизиты».

## Происхождение

Контракт выведен из выверенного анализа Tirith
([`docs/research/tirith-execution-analysis.md`](../research/tirith-execution-analysis.md)):
Tirith доказал ценность materialize-once workflow для remote execution, но
оставил три дыры — mutable cache между анализом и exec, несвязанный
interpreter, receipt до факта исполнения. 007 закрывает все три уже
существующими примитивами; RA формализует это как enforcement contract.

## Несущий инвариант (нормативный)

> **Remote bytes may become executable input only after complete
> materialization, identity establishment and one-shot authorization;
> analysis and execution must consume the same immutable object, and the
> execution phase must have no ambient route to reacquire or substitute
> executable bytes.**

Разложение по обязанностям:

1. **Complete materialization** — тело скачивается один раз, целиком, под
   size cap, до какого-либо анализа; никакого streaming в interpreter,
   никакого повторного fetch.
2. **Identity establishment** — digest вычисляется по **sealed** bytes
   (не по source до staging), артефакт существует дальше только как sealed
   object (`SealedObject`, memfd + `F_SEAL_WRITE|GROW|SHRINK|SEAL`).
3. **Object continuity** — analyzers, policy decision и executor потребляют
   **тот же** sealed object через `descriptor_path()`; pathname исходника
   после staging не участвует ни в одном шаге.
4. **One-shot authorization** — авторизация связывает artifact digest +
   interpreter digest + argv + cwd + allowlisted env + stdin + policy digest
   (= существующий `launch_spec_digest`) и потребляется атомарно существующей
   хореографией request → report → EOF-proof → GO с per-spawn nonce. Повторное
   употребление той же авторизации для второго launch невозможно.
5. **No ambient reacquisition route** — execution phase физически не может
   перекачать или подменить executable input: inet-сокеты запрещены seccomp
   (SB-A1 §4), подмена source/inode не меняет исполняемые bytes (sealed
   memfd), interpreter — тоже sealed object, не PATH-строка.

Пункты 2–4 уже enforced для локальных объектов (Vertical A, Decision 6 в
`docs/architecture/sandboy-boundary.md`). RA добавляет пункт 1 (networked
acquirer), доводит пункт 5 до kernel-enforcement (через SB-A1) и добавляет
execution receipt.

## Архитектура pipeline

```text
networked acquirer (single fetch, size cap, redirect record)
→ SealedObject (memfd, digest по sealed bytes) + AcquisitionEvidence
→ analyzers читают descriptor_path() ТОГО ЖЕ sealed object
→ decision (deterministic policy / human) поверх typed admission evidence
→ one-shot LaunchAuthorization (nonce + launch_spec_digest)
→ Sandboy исполняет ТОТ ЖЕ sealed object (interpreter — тоже sealed)
→ ExecutionReceipt после outcome
```

Маппинг на существующие примитивы:

- **Sealed artifact** — `SealedObject::stage_from_bytes`
  (`crates/o7-worker/src/sealed.rs`): acquirer скачивает тело в память и
  сразу stage'ит в sealed memfd; **unsealed bytes никогда не пишутся на диск
  как executable input**. Это и есть отличие от `cache/<sha256>`-файла Tirith.
- **Expected digest** — прецедент `stage_from_path(source, expected)`:
  optional внешний `--sha256` проверяется против digest sealed bytes;
  provenance ожидаемого digest (откуда взялся) — обязательное поле
  `AcquisitionEvidence`, а не голое число.
- **Interpreter identity** — для interpreted payload interpreter binary
  проходит ту же acquisition (sealed object, digest входит в
  `launch_spec_digest`) — так, как Vertical A уже делает для backend'а.
  PATH-разрешение строки имени в execution path запрещено.
- **One-shot** — существующие `launch_nonce` (CSPRNG, fail-closed) +
  GO barrier; RA не вводит второго механизма авторизации.
- **Receipt** — проекция поверх канонических событий o7-run
  (`SandboxEvidenceCaptured`, digest-chained `events.jsonl`), см. ниже.

## Два типа записей — различие несводимо

По образцу `BoundaryEvidence` (двухвариантный enum, Decision 5): состояние
«исполнено, но receipt acquisition-уровня» непредставимо в типах.

- **`AcquisitionEvidence`** — существует для КАЖДОЙ материализации, включая
  отказ и `--no-exec`. Нормативные поля: URL, redirect chain, размер, digest
  sealed bytes, expected digest + его provenance, timestamps, digest
  analyzer-findings, decision (и кем принято). Не утверждает исполнения.
- **`ExecutionReceipt`** — существует ТОЛЬКО после завершившегося launch.
  Связывает: artifact digest, interpreter digest, `launch_spec_digest`,
  `launch_nonce`, `policy_digest`, `backend_digest`, digest verified
  report-frame, outcome (exit/сигнал/timeout), timestamps начала и конца.
  Записывается после outcome, никогда до; отказ / NACK / не-launch не
  порождают `ExecutionReceipt` вовсе.

Это прямой ответ на слабость Tirith: там receipt сохраняется до запуска и при
отказе, поэтому его наличие ничего не доказывает об исполнении.

## Analyzer = classifier, не authority

Если статический анализ (свой или заимствованный) участвует в decision:

```text
raw findings → versioned classifier → typed admission evidence
→ deterministic policy → parent authorization
```

Ни `severity=…`, ни «analyzer says safe» не становятся GO напрямую.
Эпистемическая дисциплина меток обязательна (урок `No cloaking detected`):
отчёт анализатора перечисляет ВЫПОЛНЕННЫЕ проверки и их результат
(«no findings among executed checks»), а не выносит вердикт «safe».
Та же дисциплина, что per-dimension `enforced`/`partial`/`not_enforced`
вместо boolean `secure` (Decision 4).

## RED-оракулы (эскизы для RA-0)

Каждый оракул наблюдает конкретный эффект, не `is_err()`:

1. **Substitution** — после анализа и до exec: переписать исходный локальный
   путь И «сменить» удалённый ресурс; исполненные bytes обязаны совпасть с
   проанализированным digest (существующий локальный оракул source-swap
   расширяется на remote-acquired артефакт).
2. **Second fetch** — из confined target попытка создать inet-сокет во время
   execution phase → точный `EPERM` (переиспользует network-оракул SB-A1;
   RED, пока SB-A1 RED).
3. **Receipt honesty** — отказ / NACK / упавший до GO launch не оставляет
   `ExecutionReceipt` (но оставляет `AcquisitionEvidence`); receipt
   завершившегося launch связан с фактическим verified report-frame; путь
   «receipt записан до outcome» непредставим по построению типов.
4. **Interpreter binding** — подмена interpreter-бинаря по его исходному пути
   / в PATH после авторизации не меняет исполняемые interpreter bytes
   (interpreter — sealed object).
5. **Materialize-once** — на протяжении всего pipeline к remote источнику
   уходит ровно один запрос (оракул считает запросы на управляемом
   тест-сервере); анализ и exec не порождают повторных fetch.

## Пререквизиты и порядок

- **RA-0 (контракт + RED)** — не зависит от SB-A1; может замораживаться
  сейчас, contract-first, по образцу SB-A0.
- **RA-1 (реализация)** — требует SB-A1 GREEN: без kernel-запрета inet-сокетов
  «no ambient reacquisition route» — workflow-обещание (уровень Tirith), а не
  enforced-свойство. RA-1 до SB-A1 не начинать.

## Зафиксированные триггеры (не строить сейчас)

- **Plan commitment (донор — command cards).** Триггер: появляется action
  broker с многошаговым утверждаемым планом и реальным потребителем
  plan-bound capabilities (mint capability на шаг ревизии P; изменённый план
  → P+1, invalidation остальных grants). До этого не строить.
- **Рекурсивные фабрики (npm/cargo/docker).** Триггер: transitive execution
  closure — отсутствие ambient network И ambient launcher у descendants,
  capability-mediated acquisition, recursive provenance порождаемых
  executable inputs. Инвариант рекурсивен по execution graph; одиночный
  artifact hash его не выражает. Вне scope серии RA.
- **Checkpoints / compensating recovery.** Отдельный recovery layer ПОСЛЕ
  admission/confinement; не security boundary, не участник RA.

## Non-goals

Tirith rule corpus; универсальный policy engine; UA-cloaking detector;
pager/review UX; npm/cargo/docker recursion; plan signing / command cards;
checkpoints; любой network egress control сверх SB-A1 seccomp-запрета.

## Definition of Done (RA-0)

- контракт этого документа заморожен (изменения — только forward-only
  корректирующими раундами, как SB-A0);
- типы `AcquisitionEvidence` / `ExecutionReceipt` заявлены в протокольном
  слое (без реализации acquirer'а), непредставимость «receipt до outcome» —
  свойство типов;
- RED-оракулы §выше существуют и красные по правильной причине (наблюдаемый
  эффект, не заглушка);
- никакой реализации networked acquirer в RA-0.
