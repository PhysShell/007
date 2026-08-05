# Задача RA-0: Remote Admission — контракт (DESIGN DRAFT, не frozen)

> Идентификаторы этапов: **RA-0D** (этот документ — design/contract draft) →
> **RA-0T** (протокольные типы + RED acceptance matrix; только после него
> контракт FROZEN) → **RA-1** (реализация: networked acquirer + admission
> state machine + execution receipt). Префикс RA- обязателен: в проекте уже
> существуют Q-Deck A0/A1 и Sandboy SB-A0/SB-A1.
>
> **Статус: RA-0 DESIGN DRAFT.** Контракт и эскизы RED-оракулов написаны;
> типы не landed, RED-тесты не landed, контракт НЕ заморожен. Freeze
> наступает только когда RA-0T положит типы в протокольный слой и красную
> матрицу в тестовый — «замороженный» контракт без RED-формализации
> размораживается через пятнадцать минут после первого компилятора.

## Происхождение

Контракт выведен из выверенного анализа Tirith
([`docs/research/tirith-execution-analysis.md`](../research/tirith-execution-analysis.md)):
Tirith доказал ценность materialize-once workflow для remote execution, но
оставил три дыры — mutable cache между анализом и exec, несвязанный
interpreter, receipt до факта исполнения. 007 закрывает их, опираясь на
существующие примитивы; RA формализует это как enforcement contract и ЧЕСТНО
разделяет, что уже enforced, а что этот этап только добавляет.

## Несущий инвариант (нормативный)

> **Remote bytes may become executable input only after complete
> materialization, identity establishment and one-shot authorization;
> analysis and execution must consume the same immutable object, and the
> execution phase must have no ambient route to reacquire or substitute
> executable bytes.**

Разложение по обязанностям:

1. **Complete materialization** — тело материализуется целиком, под size cap,
   до какого-либо анализа; никакого streaming в interpreter. Семантика — «один
   admitted materialized object», НЕ «ровно один HTTP request» (см. §
   «Materialize-once»).
2. **Identity establishment** — digest вычисляется по **sealed** bytes,
   артефакт дальше существует только как sealed object (`SealedObject`,
   memfd + `F_SEAL_WRITE|GROW|SHRINK|SEAL`).
3. **Object continuity** — analyzers, admission decision и executor потребляют
   **тот же** sealed object через `descriptor_path()`; pathname исходника и
   remote locator после identity establishment не участвуют ни в одном шаге.
4. **One-shot authorization** — admission decision связывается ровно с одним
   launch и потребляется атомарно существующим GO-переходом (см. §
   «Admission state machine»).
5. **No ambient reacquisition route** — execution phase физически не может
   перекачать или подменить executable input: inet-сокеты запрещены seccomp
   (SB-A1 §4), подмена source/inode не меняет исполняемые bytes (sealed
   memfd), interpreter — тоже sealed object, не PATH-строка.

## Что уже enforced, а что RA добавляет

Vertical A (GREEN, `docs/architecture/sandboy-boundary.md` Decision 6) даёт:
sealed acquisition, descriptor execution, source-swap resistance — **для
одного primary target**. Текущий `LaunchRequest::spec_digest()`
(`crates/o7-sandbox-protocol/src/request.rs`) связывает:

```text
schema_version, target_digest (ОДИН sealed объект), argv, cwd,
env (set), stdin kind (сегодня только Null), launch_nonce
```

Он НЕ содержит: отдельного artifact digest, отдельного interpreter digest,
policy digest, admission decision, capability manifest. `policy_digest`
связывается report-протоколом отдельным полем, не внутри
`launch_spec_digest`. Любая формулировка, приравнивающая расширенный
admission binding к нынешнему `launch_spec_digest`, — ложное равенство.

Нормативная композиция RA (названия вторичны; несводимость трёх ролей —
нет):

```text
execution_inputs_digest =
    H(interpreter_digest, artifact_digest, declared auxiliary objects)

launch_spec_digest =
    H(execution_inputs_digest, argv, cwd, env, stdin, launch_nonce)

admission_binding =
    H(launch_spec_digest, policy_digest, decision_digest)
```

Один digest не должен изображать одновременно (а) стабильную спецификацию
входов, (б) одноразовый launch instance и (в) policy decision. Policy
binding остаётся отдельным полем report/authorization-слоя.

## Multi-object binding: interpreter + payload — это RA-модификация

Существующий протокол умеет связать ОДИН target. Для interpreted remote
payload это даёт ровно два неполных варианта:

- **Вариант A: target = script.** Kernel читает sealed script, но interpreter
  из shebang разрешается отдельно; его bytes не связаны ничем.
- **Вариант B: target = sealed interpreter.** Script уходит в argv как
  pathname; digest его bytes в текущий `LaunchRequest` не входит.

Значит «interpreter — sealed object с digest в launch binding» — свойство,
которое RA **добавляет**, а не существующее свойство Vertical A. Механизм
выбирает RA-0T из кандидатов (выбор — часть freeze):

1. `ExecutionObjectManifest` в `LaunchRequest` с ролями
   `interpreter` / `program` / `auxiliary` (digest на каждый объект);
2. несколько pre-opened sealed object descriptors, передаваемых backend'у;
3. sealed payload через расширенный stdin-протокол (`StdinKind` сегодня —
   только `Null`; это тоже расширение);
4. trusted launcher, принимающий interpreter FD + payload FD.

Требование к любому варианту: каждый executable input перечислен, каждый —
sealed object, каждый digest входит в `execution_inputs_digest`, и backend
исполняет только перечисленное.

## Admission state machine: nonce + GO ≠ one-shot admission

Существующий GO означает: parent проверил sandbox report конкретного backend
instance и разрешает старт target. Nonce связывает request ↔ report ↔ launch
и убивает replay отчёта. Но сам по себе он не запрещает сценарий:

```text
admission decision D → spawn with nonce N1 → GO
→ spawn again with nonce N2 → тот же D употреблён второй раз
```

RA расширяет существующую parent-authorization state machine — не вводит
второй механизм авторизации: admission decision связывается ровно с одним
launch nonce и потребляется существующим GO-переходом:

```text
PendingAdmission → BoundToLaunchNonce → ConsumedByGo
```

Повторный переход `BoundToLaunchNonce` для уже связанного/потреблённого
decision непредставим по типам (по образцу `BoundaryEvidence`). Второй launch
требует нового admission decision (или явно выданного multi-use grant — вне
scope RA; reusable approval identity остаётся признанным отсутствием).

## Evidence-модель: четыре смысловые записи

Acquisition, analysis, decision и execution — разные события с разными
жизненными циклами: один sealed artifact может анализироваться несколькими
версиями analyzer'ов, получить отказ по policy P1 и позже допуск по P2,
участвовать в нескольких launch attempts. Запись acquisition при этом не
мутирует и не превращается в дневник всей последующей жизни артефакта.

```text
AcquisitionEvidence
  source, redirects, body size, sealed digest,
  expected digest + its provenance, acquisition timestamps

AnalysisEvidence
  acquisition digest, analyzer identities/versions,
  executed checks, findings digests

AdmissionDecision
  acquisition + selected analysis evidence,
  policy digest, approver identity,
  decision, scope, expiry / nonce binding

ExecutionReceipt
  consumed admission decision,
  launch bindings (launch_spec_digest, nonce, backend_digest,
  report digest), sandbox evidence, outcome, timestamps
```

`AnalysisEvidence` допустимо вложить в admission record, если разводить типы
не хочется; но decision НЕ поле acquisition evidence. `AcquisitionEvidence`
существует для каждой материализации (включая отказ и `--no-exec`) и не
утверждает исполнения; `ExecutionReceipt` существует только для launch.

## Durable lifecycle: «receipt после outcome» без crash-gap

«Receipt до outcome непредставим» — правильно, но недостаточно: после GO
parent может упасть, target мог исполняться, а terminal receipt отсутствует.
Отсутствие receipt НЕЛЬЗЯ читать как отсутствие исполнения. Поэтому ещё
важнее второй инвариант: **исполнение без какой-либо durable записи
непредставимо**. Хореография (write-ahead, теперь с агентами и красивыми
digest):

```text
persist admission-bound launch intent   (LaunchAuthorized /
                                         ExecutionPossiblyStarted)
→ fsync / commit event
→ GO
→ observe outcome
→ persist terminal receipt
```

Terminal-множество закрыто и включает потерю наблюдения:
`ExecutionCompleted` / `ExecutionSignaled` / `ExecutionTimedOut` /
`ExecutionObservationLost`. Эквивалентная типовая форма — fail-closed outcome
в самом receipt: `Known(...)` | `Indeterminate { last_observed_state,
reason }`. Recovery после crash обязана доводить каждый durable intent до
terminal записи (хотя бы `ObservationLost`).

## Materialize-once: один admitted object, не «один HTTP request»

Redirect chain — уже несколько HTTP requests; безопасный retry до завершения
materialization инвариант тоже не нарушает. Защитное свойство:

- ровно один полностью материализованный объект выбран как admitted
  identity;
- после identity establishment ни анализ, ни execution не обращаются к
  remote locator;
- никакой второй response body не может заменить admitted object.

`request_count == 1` — допустимый оракул только в простом fixture без
redirects/retries, и он не является определением сетевой семантики.

## Analyzer = classifier, не authority

Если статический анализ участвует в decision:

```text
raw findings → versioned classifier → typed admission evidence
→ deterministic policy → parent authorization
```

Ни `severity=…`, ни «analyzer says safe» не становятся GO напрямую.
Эпистемическая дисциплина меток обязательна (урок `No cloaking detected`):
отчёт перечисляет ВЫПОЛНЕННЫЕ проверки и их результат («no findings among
executed checks»), не вердикт «safe» — та же дисциплина, что per-dimension
`enforced`/`partial`/`not_enforced` вместо boolean `secure` (Decision 4).

## RED-оракулы (эскизы; landed — в RA-0T)

Каждый оракул наблюдает конкретный эффект, не `is_err()`:

1. **Substitution** — после анализа и до exec переписать исходный локальный
   путь и «сменить» remote ресурс; исполненные bytes обязаны совпасть с
   admitted digest (расширение существующего source-swap оракула на
   remote-acquired артефакт).
2. **No duplicate acquisition** — простой fixture без redirects/retries:
   ровно один запрос на управляемом тест-сервере.
3. **Redirect accounting** — каждый hop записан в `AcquisitionEvidence`;
   admitted identity соответствует финальному body.
4. **No reacquisition after materialization** — после sealing тест-сервер не
   получает ни одного нового запроса от pipeline (анализ, decision, launch).
5. **No execution egress** — из confined target попытка создать inet-сокет →
   точный `EPERM` (переиспользует network-оракул SB-A1; RED, пока SB-A1
   RED).
6. **Interpreter binding** — подмена interpreter-бинаря по исходному пути /
   в PATH после авторизации не меняет исполняемые interpreter bytes; оба
   объекта манифеста связаны в `execution_inputs_digest`.
7. **One-shot admission** — попытка употребить один `AdmissionDecision` для
   второго launch (новый nonce) непредставима/отвергается; второй launch
   требует нового decision.
8. **Receipt honesty + durable intent** — отказ / NACK / падение до GO не
   оставляют `ExecutionReceipt` (но оставляют `AcquisitionEvidence` и, если
   был intent, terminal запись); kill parent'а после GO оставляет durable
   intent, и recovery доводит его до terminal записи
   (`ObservationLost`-класс); «receipt до outcome» непредставим по типам.

## Пререквизиты и порядок

- **RA-0D / RA-0T** — не зависят от SB-A1.
- **RA-1** — final acceptance и production promotion требуют SB-A1 GREEN:
  без kernel-запрета inet-сокетов «no ambient reacquisition route» — 
  workflow-обещание уровня Tirith, и полный инвариант заявлять нельзя.
  Это **acceptance/promotion gate, не логическая зависимость всех частей**:
  non-network slices RA-1 (acquirer, evidence-типы, object manifest,
  analyzer continuity, admission state machine, execution receipt,
  swap/no-refetch тесты) реализуемы раньше — при честной отчётности
  `network_confinement = not_enforced`, `fully_enforced = false`,
  promotion запрещён.

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
- **Reusable approval identity / multi-use grants.** Признанное отсутствие;
  RA даёт только one-shot admission.

## Non-goals

Tirith rule corpus; универсальный policy engine; UA-cloaking detector;
pager/review UX; npm/cargo/docker recursion; plan signing / command cards;
checkpoints; multi-use grants; любой network egress control сверх SB-A1
seccomp-запрета.

## Definition of Done — по этапам

**RA-0D (этот документ):**

- контракт описан с пятью корректировками ревью: (1) честное описание
  текущего `spec_digest` и композиция `execution_inputs_digest` /
  `launch_spec_digest` / `admission_binding`; (2) multi-object binding
  заявлен как RA-модификация с кандидатами механизма; (3) one-shot
  admission определён как расширение существующей GO state machine;
  (4) evidence разделена на acquisition / analysis / decision / execution +
  durable intent против post-GO crash-gap; (5) materialize-once определён
  через admitted object, не через счётчик запросов;
- статус документа — DESIGN DRAFT; freeze здесь НЕ объявляется.

**RA-0T (следующий этап; после него контракт FROZEN):**

- типы `AcquisitionEvidence` / `AnalysisEvidence` / `AdmissionDecision` /
  `ExecutionReceipt` landed в протокольном слое; непредставимость
  «receipt до outcome» и «повторное употребление decision» — свойство типов;
- механизм multi-object binding ВЫБРАН и его типы landed;
- RED-матрица оракулов §выше landed и красная по правильной причине
  (наблюдаемый эффект, не заглушка);
- после этого контракт замораживается; изменения — только forward-only
  корректирующими раундами, как SB-A0.

**RA-1:** реализация; acceptance/promotion — по gate SB-A1 GREEN.
