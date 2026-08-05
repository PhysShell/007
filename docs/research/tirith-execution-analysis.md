# Tirith — execution-path analysis (выверенная редакция)

**Статус: зафиксированные выводы приняты оператором; нормативные следствия для
007 вынесены в [`docs/tasks/ra0-remote-admission-contract.md`](../tasks/ra0-remote-admission-contract.md).**

Происхождение утверждений: код Tirith проверен оператором на ревизии
`1bf050fefa09d8aa4ed986e3006c0097d8d0b24b`. Репозиторий Tirith не входит в
sources этой сессии, поэтому утверждения о его коде здесь **записаны со слов
проверявшего**, не перепроверены независимо; каждое утверждение о 007
ссылается на код и документы этого репозитория. Первая (чатовая) редакция
разбора содержала четыре фактические ошибки — они исправлены в §10 и не
воспроизводятся в основном тексте.

---

## 1. Executive verdict

Tirith — не поделка. Это цельный, практичный terminal/agent guard: rich command
analysis, policy engine, shell mediation, materialize-once `run`, taint
tracking, receipts, checkpoints, command cards, cloaking diagnostics. Его
threat model честно ограничена, и сильнейшая для 007 часть — **workflow вокруг
remote execution**: materialize → inspect → authorize → execute.

Но execution в Tirith остаётся обычным пользовательским процессом без runtime
confinement, identity артефакта не доводится до момента `exec`, а receipt
фиксирует acquisition/analysis, не факт исполнения. 007 не должен ни
копировать Tirith, ни превращаться в ещё один сигнатурный pre-execution
scanner: его вклад — довести workflow-свойство Tirith до **enforcement
contract** (§9, §11).

## 2. Верифицированный путь `tirith run`

```text
validate URL and redirects
→ полностью скачать тело, максимум 10 MiB
→ SHA-256
→ optional expected-SHA check
→ записать обычный файл cache/<sha256>
→ статически проанализировать исходные bytes
→ определить interpreter
→ запросить подтверждение
→ сохранить receipt
→ Command::new(interpreter).arg(cache/<sha256>)
```

**Что действительно сильно.** Remote content загружается один раз целиком;
анализируются те же bytes, которые записаны в cache; повторного HTTP-запроса
перед execution нет. Классическая схема «preview request → benign A, execution
request → malicious B» для `tirith run` **структурно уничтожена**: сервер
получает один запрос, Tirith — одно тело. Streaming remote execution превращён
в staged local execution. Это настоящее архитектурное улучшение, а не просто
«анализ и предупреждение».

**Что слабее, чем кажется.** Полного обязательного просмотра тела скрипта в
этом пути нет: показываются SHA, interpreter и несколько предупреждений
(sudo, eval, base64), затем `Execute this script?`. Пользователь может
подтвердить исполнение, не увидев содержимого.

## 3. Identity: участвует в acquisition, но не доводится до execution

SHA в Tirith — не приписка в журнале: он вычисляется по полностью загруженному
телу, сравнивается с явным `expected_sha256` и служит именем cache-объекта.
Identity уже участвует в acquisition path.

Но `cache/<sha256>` — обычный **изменяемый** файл, и после анализа и
подтверждения Tirith снова открывает его **по pathname** через interpreter.
Повторного хеширования перед запуском нет, descriptor не удерживается, файл
не sealed:

```text
hash(bytes A) → write cache/<hash-A> → analyze A → human confirmation
→ reopen cache/<hash-A> → interpreter reads whatever is there now
```

Имя выглядит content-addressed, но filesystem не обязан соблюдать семантику
этого имени: same-UID процесс может переписать файл, оставив pathname прежним.

Второй незакрытый объект — **interpreter**: Tirith разрешает набор имён и
передаёт строку в `Command::new`; для относительного имени (`bash`) разрешение
идёт через PATH, и точные bytes interpreter не связаны ни с SHA скрипта, ни с
receipt. Сам проект в модуле executable provenance признаёт этот TOCTOU.

**Почему `execute(hash)` — не решение.** Hash — имя/утверждение об identity,
а не объект исполнения и не полномочие. `execute(hash) → resolve in mutable
cache → reopen pathname → execute substituted bytes` оставляет ту же дыру,
украшенную криптографией. Несущее свойство — **object continuity**: analyzer и
executor обязаны потреблять один и тот же immutable object, а авторизация —
связывать digest артефакта с полной launch-спецификацией (interpreter
identity, argv, cwd, env, policy digest) и выдаваться как one-shot grant.

В 007 это свойство уже реализовано для локальных объектов: acquisition в
sealed memfd с digest, вычисленным по sealed bytes
(`crates/o7-worker/src/sealed.rs`), exec через `/proc/<owner_pid>/fd/<n>`,
`launch_spec_digest` поверх точных sealed bytes + argv + cwd + env + stdin
(`docs/architecture/sandboy-boundary.md`, Decision 6).

## 4. Receipt — это acquisition-and-analysis receipt, не execution receipt

Receipt Tirith содержит URL, redirects, SHA, размер, анализ, timestamp, cwd,
git context. В нём **нет**: факта исполнения, exit status, времён начала и
конца, identity interpreter, launch identity, policy decision, доказательства,
что kernel открыл именно эти bytes. Receipt сохраняется при `--no-exec`, при
отказе пользователя и **перед** фактическим запуском — его наличие вообще не
означает, что скрипт исполнялся. `receipt verify` лишь перечитывает текущий
`cache/<sha>` и проверяет, что нынешние bytes всё ещё дают указанный SHA — это
не доказательство исторического исполнения.

Это не бесполезно — просто доказательство значительно слабее названия.
Execution receipt для 007 обязан появляться **после завершения** и связывать
как минимум: artifact digest, interpreter digest, launch-instance digest
(nonce), argv/cwd/env/stdin, policy digest, authorization, sandbox report
digest, outcome, timestamps. И даже это — доказательство того, что 007
осуществил и наблюдал такой launch, а не аппаратная аттестация.

## 5. Cloaking detector: слепота детектора ≠ уязвимость execution path

Детектор Tirith меняет только User-Agent (Chrome, ClaudeBot, ChatGPT,
Perplexity, Googlebot, curl) и сравнивает нормализованные ответы. TCP timing и
поведение получателя body не измеряются — same-UA timing cloaking он не видит.
При отсутствии различий CLI печатает `No cloaking detected`, хотя установлен
лишь узкий факт «среди выполненных UA-профилей нет значимой content
differential» — эпистемически завышенная надпись.

Но из слепоты детектора **не следует**, что `tirith run` уязвим к
benign-preview / malicious-execution swap: fetch один, исполняется локально
materialized тело. Timing-aware сервер может выдать Tirith malicious body — и
тогда анализируется именно malicious body; провал возможен как **analysis
completeness failure** (статический анализ не понял, пользователь нажал `y`,
interpreter запустил без sandbox), но не как подмена контента между approval и
execution через второй сетевой запрос — второго запроса нет.

Корректный вывод: детектор остаётся диагностикой; безопасность против
двухфазного swap даёт materialize-once workflow, а не детектор.

## 6. Checkpoints: compensating recovery, не rollback

Реализация не декоративная: file-level snapshots, blobs по SHA-256, manifest
path→digest, crash-atomic запись, capture root, retention limits. Но
checkpoint сохраняет только выбранные paths, не откатывает сеть, не отзывает
утёкшие credentials, не отменяет cloud-запросы, не восстанавливает package
manager / daemon state, не отменяет действия потомков вне сохранённых путей.
Threat model Tirith сам подчёркивает: даже temp-run — только file isolation.

```text
checkpoint ≠ transaction ≠ sandbox ≠ rollback guarantee
checkpoint = bounded compensating recovery for captured filesystem state
```

Полезный adjunct. Не security boundary.

## 7. Policy engine: настоящий PDP, но не универсальный PEP

Policy Tirith — не пять if: fail-open/fail-closed, interactive/noninteractive
bypass rules, allow/blocklist, approval rules, network allow/deny,
severity/action overrides, escalation, agent-specific rules,
repo/user/org/remote scopes. Особенно правильное решение: **repo policy
считается недоверенной и может только ужесточать** — автор понимает policy
provenance.

Но policy действует только там, где Tirith посредничает: shell hook должен
передать команду, локальный привилегированный пользователь обходит систему,
существует `TIRITH=0`, runtime sandboxing и post-execution network monitoring
вне модели. Процесс, вызывающий `bash`/`curl`/syscall напрямую вне hook,
политикой не ограничен:

```text
Tirith policy = серьёзный PDP внутри mediated workflow
Tirith policy ≠ непреодолимый universal PEP
```

Именно здесь пролегает реальное различие с направлением Sandboy.

## 8. Command cards: seed идеи plan commitment, не реализация

Card подписывает Ed25519: точную command string, expected domains, optional
script SHA, ожидаемые writes, необходимость sudo, expiry. Но v1 **проверяет**
только signature, trusted key, expiry и exact command string; `script_sha256`,
domains, writes, sudo подписаны, но не enforced, и валидная карточка не
снимает остальные warnings/blocks (проект говорит это прямо).
`command_card_mismatch` сейчас означает «signed command text ≠ observed
command text», а не «observed runtime behavior ≠ approved plan».

Настоящий approved agent plan потребовал бы связывать plan revision, DAG
шагов, tool identities, exact arguments, cwd/env, input artifact revisions,
capability grants, preconditions, допустимые effects, policy revision, условия
replanning и evidence каждого шага — и, главное, enforcement должен **не
выдавать one-shot capability на шаг, которого нет в утверждённой ревизии**:

```text
approved plan revision P
→ mint capability for step P.7
→ execution consumes that capability atomically
→ evidence binds observed launch to P.7
→ changed plan produces P+1 and invalidates remaining grants
```

Это сосед Evidence Budget / Q-Deck / action admission 007 — но у Tirith пока
seed, не реализация. Триггер для 007 зафиксирован в RA-0 (§ триггеры).

## 9. Рекурсивные фабрики исполнения: честное обобщение

Для одиночного remote script граф прост: Network → Materialized Artifact →
Digest → Admission → Execution. Но npm / Cargo / Docker — не «другие способы
скачать один скрипт»: registry metadata → dependency resolution → множество
архивов → git deps → postinstall/build scripts → native compilers → downloaded
binaries → subprocesses, снова ходящие в сеть; плюс manifests/layers/daemon
authority (Docker), build.rs/proc macros (Cargo), lifecycle hooks (npm).

Честное обобщение — не «все инструменты проходят через один artifact hash», а:

> всякий переход от remote/untrusted bytes к executable input должен быть
> представлен в execution graph и пройти identity-bound admission.

Это **рекурсивное свойство графа**. Чтобы сделать его обязательным, нужны
одновременно: отсутствие ambient network у execution, отсутствие ambient
bypass launcher, capability-mediated acquisition, recursive provenance для
порождаемых executable inputs, confinement всех descendants, контроль
interpreters/dynamic loads/package hooks. 007 этого пока тоже не умеет —
рисовать стрелку раньше времени нельзя, иначе npm тихо запустит postinstall в
подвале.

## 10. Четыре исправленных утверждения первой редакции

1. **Tirith не только анализирует**: `run` уже структурно устраняет streaming
   execution и повторный remote fetch (materialize-once).
2. **Identity не «отсутствует»**: SHA участвует в acquisition/cache; дыра в
   том, что execution не потребляет immutable object (§3).
3. **Receipt не доказывает execution**: он фиксирует acquisition/analysis и
   существует даже после отказа (§4).
4. **Timing cloaking не обнаруживается детектором, но** конкретный
   preview/execution swap в `tirith run` уже нейтрализован single-fetch
   workflow (§5).

Также первая редакция преувеличивала review ceremony (`pager` в `run`-пути
нет) и частично выдавала будущую архитектуру 007 за работающий нижний слой —
честное сравнение в §11.

## 11. Честное сравнение Tirith ↔ 007 (на сегодня)

**Tirith сегодня** — практически полезный terminal/agent guard: rich command
analysis, policy, shell mediation, materialize-once run, taint tracking,
receipts, checkpoints, command cards, cloaking diagnostics. Execution —
обычный пользовательский процесс без runtime confinement.

**007 сегодня** — GREEN на protocol/lifecycle уровне (Vertical A,
`docs/architecture/sandboy-boundary.md`): acquisition точного primary target,
sealed memfd, backend получает descriptor, а не source path; подмена source не
меняет исполняемые primary bytes; nonce-bound launch request; report binding;
parent verification + GO barrier. Но: настоящий kernel-confinement backend
(Vertical B, SB-A1) ещё RED; remote artifact materialization отсутствует;
interpreted remote payload как sealed object отсутствует; reusable approval
identity отсутствует; transitive execution closure отсутствует.

Фраза «Tirith проверяет команду, а 007 проверяет инвариант исполнения»
описывает направление 007, но не его текущий продукт. Честно:

```text
Tirith: шире пользовательская защита, больше готовых терминальных workflow,
        слабее execution guarantees
007:    уже сильнее primary-object/launch binding, строже evidence model,
        реальный confinement и remote admission ещё не завершены
```

Для личного терминала сейчас полезнее Tirith. Для доверенного автономного
execution substrate направление 007 сильнее, но менее полно реализовано.

## 12. Донорские решения для 007

Забираем **не** rule corpus и **не** policy engine целиком:

1. **UX materialize → inspect → authorize → execute** — главный донор; но
   исполнение обязано потреблять sealed object, а не pathname → контракт RA-0.
2. **Receipt как first-class output** — перепроектированный как execution
   receipt, связанный с launch evidence, никогда не записанный до запуска.
3. **Expected digest** (`--sha256`-эквивалент) — полезен сразу как optional
   external precondition; но у external digest обязана быть provenance
   (откуда взялся, почему ему доверяют).
4. **Command card как донор plan commitment** — не строить сейчас;
   зафиксированный триггер: появляется action broker с многошаговым
   утверждаемым планом и реальным потребителем plan-bound capabilities. До
   этого signed plan — документ о том, чего система не может enforce'ить.
5. **Checkpoints как recovery layer** — после confinement/admission, не
   вместо них.
6. **Tirith analyzer как classifier, не authority** — если когда-либо
   употребляется: raw findings → versioned classifier → typed admission
   evidence → deterministic policy → parent authorization. Ни `severity=high`,
   ни «tirith says safe» не становятся GO напрямую.

## 13. Несущий инвариант

Самый ценный вывод — не «нам нужен свой Tirith» и не «нам нужен
`execute(hash)`», а:

> **Remote bytes may become executable input only after complete
> materialization, identity establishment and one-shot authorization;
> analysis and execution must consume the same immutable object, and the
> execution phase must have no ambient route to reacquire or substitute
> executable bytes.**

Tirith реализует это наполовину — на уровне хорошего пользовательского
workflow. 007 доводит это до enforcement contract: нормативная фиксация — в
[`docs/tasks/ra0-remote-admission-contract.md`](../tasks/ra0-remote-admission-contract.md).
