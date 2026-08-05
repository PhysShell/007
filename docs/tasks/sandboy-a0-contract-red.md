# Задача SB-A0: коррекция контракта Sandboy Vertical B + RED-оракулы (read confidentiality)

> Идентификаторы этапов: **SB-A0** (этот документ) → **SB-A1** (реальный backend,
> `sandboy-a1-vertical-b.md`) → **SB-A2** (retirement Own.NET) → **SB-B**
> (capability transport, `capability-fd-transport.md`) → **MG-C** (o7-model-gate).
> Префикс SB- обязателен: в проекте уже существуют Q-Deck A0/A1
> (`docs/q-deck/a0-candidate-state.md`), голые «A0»/«A1» двусмысленны.

## Роль

Ты работаешь как senior systems/security engineer в проекте 007. Это **маленький,
строго ограниченный contract-first PR**. Никакого kernel-enforcement кода, никакого
backend'а, никакого capability transport. Только правка контракта, RED-тесты и
документация решений.

Контекст, который ОБЯЗАТЕЛЬНО прочитать до изменений:

- `docs/architecture/sandboy-boundary.md` — замороженный Vertical A и RED-контракт
  Vertical B;
- `crates/o7-sandbox-protocol/` — wire-протокол (policy, report, frame, request);
- `crates/o7-worker/tests/sandbox_confinement.rs` — существующая RED-матрица;
- `crates/o7-worker/src/bin/fd_probe.rs` и остальные harness-бинари;
- `docs/architecture/capability-fd-transport.md` — принятое направление этапа
  SB-B (в этом PR НЕ реализуется);
- `Own.NET/sandboy/` (sibling-репозиторий) — только для понимания; см. запрет ниже.

## Зафиксированные решения (не пересматривать)

1. **Топология**: monitor-backend (Decision 2 в `sandboy-boundary.md`). Монитор —
   неограниченный доверенный владелец lifecycle: cgroup, timeout, teardown, report
   relay. Confinement (Landlock + seccomp) устанавливает **confined child между
   fork и exec**, сам делает self-check и отдаёт результат монитору. Монитор
   никогда не попадает под target policy.
2. **Capability FD не входит в Vertical B.** Направление зафиксировано в
   `docs/architecture/capability-fd-transport.md`; в SB-A0/SB-A1 — ни строчки
   реализации.
3. **Судьба standalone CLI**: новый backend (этап SB-A1) никогда не имеет
   production-режима `run --policy <toml>`. Диагностический `probe` сохраняется.
   Ручная отладка — только через будущий dev-harness, говорящий на настоящем
   `o7-sandbox-protocol`. Второго policy-формата не существует.
4. **Own.NET/sandboy неприкосновенен** до этапа SB-A2:
   - не изменять код спайка;
   - не чинить его `PartiallyEnforced → warn → continue`;
   - не переносить и не «улучшать» его TOML CLI;
   - не удалять и не править `sandboy-feasibility-gate.yml`;
   - не менять accepted S0 identity.

## Техническое задание

### 1. `SandboxPolicy`: разделить read и execute authority

Добавить в `o7-sandbox-protocol` поле `allow_read`:

```rust
pub struct SandboxPolicy {
    pub worktree: PathBuf,          // read + write; execute ТОЛЬКО если путь явно в allow_exec
    pub allow_read: Vec<PathBuf>,   // read-only, БЕЗ execute
    pub allow_exec: Vec<PathBuf>,   // read + execute, БЕЗ write
    pub network: NetworkPolicy,
    pub env_allowlist: Vec<OsString>,
    pub timeout: Duration,
}
```

Требования:

- валидация как у существующих полей: абсолютные пути, duplicates — ошибка
  (set-семантика, та же причина: одинаковый смысл ⇒ одинаковый digest);
- **каноническая нормализация и порядок**: путь с `.`/`..`-компонентами или
  trailing slash — ошибка валидации (лексическая нормализация ОТВЕРГАЕТСЯ, не
  выполняется: разрешение symlink/`..` на этапе digest создало бы TOCTOU);
  внутри digest-формулы v2 элементы `allow_read` и `allow_exec` входят в
  байтово-лексикографическом порядке, чтобы одинаковые множества в разном
  порядке давали одинаковый digest;
- **overlap-семантика**: один и тот же путь одновременно в `allow_read` и
  `allow_exec` — ошибка валидации (`allow_exec` уже включает read; дубль —
  ошибка автора policy). Вложенные пути (родитель в одном списке, потомок в
  другом, или пересечение с `worktree`) допустимы, и эффективные права —
  union по Landlock-семантике (права по вложенным правилам складываются);
  это документируется как контракт, а не оставляется поведению ядра «как
  получится»;
- `allow_read` входит в канонический `SandboxPolicy::digest()`, и формула
  получает **version bump domain-separation строки**:
  `o7-sandbox-policy\0v1\0` → `o7-sandbox-policy\0v2\0` (policy.rs:278).
  Просто обновить ожидаемый hash в фикстуре при старой строке v1 запрещено —
  новая семантика не должна притворяться формулой v1;
- known-answer fixtures digest'а обновляются осознанно под v2 (это ожидаемое
  изменение, зафиксировать в комментарии фикстуры);
- **argv-контракт policy обновляется механически** (это НЕ kernel-enforcement
  и НЕ нарушение scope — без этого fake backend начнёт выдавать другой
  `policy_digest` и Vertical A развалится):
  - `SandboyBoundary::policy_flags()` добавляет `--allow-read` — **грамматика
    идентична существующему `--allow-exec`**: флаг повторяется для каждого
    пути (`--allow-read <path> --allow-read <path> …`), пустой `allow_read`
    ⇒ флаг отсутствует, порядок флагов — канонический (лексикографический,
    как в digest), значения передаются как `OsString` (argv на Unix — байты,
    не-UTF-8 пути не ломаются и не перекодируются);
  - `sandboy_fake` парсит `--allow-read` той же грамматикой;
  - `reconstructed_policy()` включает `allow_read`;
  - contract-тесты проверяют argv и совпадение digest
    (`original.digest == reconstructed.digest`), включая случай не-UTF-8 пути;
- существующие смысловые oracle не ослаблять; механические правки конструкторов
  policy из-за нового поля допустимы. Compatibility-конструкторы «чтобы diff
  выглядел нетронутым» запрещены.

### 2. Оракулы read/exec в `sandbox_confinement.rs`: два RED + один GREEN control

По конвенции существующей матрицы (конкретный errno, конкретный эффект, никакого
`is_err()`). **Каждый оракул обязан быть non-vacuous**: до confinement тест
делает baseline-проверку вне sandbox — секрет-фикстура читается, исполняемая
фикстура успешно запускается (статический ELF или фикстура с гарантированно
доступными loader-зависимостями; permissions выставлены явно). Иначе `EACCES`
может прийти от обычных file permissions, а `execve`-отказ — от
`ENOEXEC`/`ENOENT`, и оракул «зеленеет» по причинам, не связанным с Landlock.
Точный статус каждого теста против замороженного fake backend:

1. **Секрет вне allow_read** — **RED** (fake backend не конфайнит, escape
   наблюдаем): файл с уникальным canary-содержимым вне
   `worktree ∪ allow_read ∪ allow_exec`; probe пытается `read()` →
   ожидание `EACCES`/`EPERM`; байты canary НЕ появляются в stdout, stderr,
   marker artifact, report и evidence (универсальный canary-скан, по образцу
   env-canary Own.NET S0);
2. **Read без execute** — **RED**: файл внутри `allow_read`, не в `allow_exec`:
   `read()` → OK; `execve()` → `EACCES`/`EPERM`;
3. **Execute разрешён** — **GREEN positive control** (non-vacuity): бинарь
   внутри `allow_exec`: `execve()` → OK. Этот тест зелёный уже на fake backend
   и обязан ОСТАТЬСЯ зелёным на реальном — он доказывает, что оракулы 1–2
   зелены не потому, что сломано всё подряд. НЕ пытаться искусственно сделать
   его красным.

### 3. Чистка протухших комментариев протокола

- `crates/o7-sandbox-protocol/src/frame.rs:2` — упоминание «report descriptor»;
- `crates/o7-sandbox-protocol/src/request.rs:2` — упоминание `--request-fd`.

Оба описывают отменённую трёхтрубную схему. Заменить описанием фактической
замороженной хореографии (один Unix socket на stdin backend'а). Только
комментарии/доки — wire-формат не трогать.

### 4. Документировать решения в `sandboy-boundary.md`

Дописать (не переписывая замороженные разделы):

- топологию из «Зафиксированных решений» п.1 (monitor unrestricted / child
  installs confinement / self-check в child);
- явное «capability FD — вне scope Vertical B, см.
  `capability-fd-transport.md`»;
- судьбу standalone CLI (п.3) и неприкосновенность Own.NET до A2 (п.4).

### 5. NixOS VM preflight (только preflight!)

`checks.<system>.sandbox-vm-preflight`: NixOS VM-тест, который **реально
упражняет**, а не читает конфиг:

- создание Landlock ruleset со всеми FILESYSTEM access-битами, которые нужны
  Vertical B (включая `TRUNCATE`, т.е. ABI ≥ 3). Landlock network (ABI 4)
  НЕ требуется и НЕ проверяется: нормативный механизм запрета IPv4/IPv6 в
  SB-A1 — seccomp argument filter на `socket()`, работающий независимо от
  Landlock net support;
- установку seccomp-фильтра;
- cgroup v2: create / move / `cgroup.kill` / drain / remove с delegated
  ownership;
- доступность `/proc` в форме, нужной fd_probe;
- IPv4/IPv6 prerequisites для будущих network-оракулов.

Каждая проверка fail-closed: недоступная возможность = красный check, не warning.
Прогон confinement-матрицы в VM — этап SB-A1, сюда не тащить. Переиспользуй
проверки из `.github/workflows/sandbox-confinement.yml` (self-hosted preflight) —
семантика должна совпадать.

## Обязательные тесты

- unit: валидация `allow_read` (относительный путь, дубликат), digest меняется
  при изменении `allow_read`, digest-фикстуры под v2-формулой;
- contract: `SandboxPolicy → policy_flags() → fake backend reconstructs →
  reconstructed.digest == original.digest` (policy — НЕ serde wire-тип, он
  передаётся argv-флагами; serde round-trip тут неприменим);
- оракулы №1–3 из §2 (designated-runner suite, `#[ignore]` по существующей
  конвенции), со статусами точно как заявлено: 1–2 RED, 3 GREEN;
- canary-скан как переиспользуемый helper, не копипаста по тестам; множество
  сканирования — stdout, stderr, marker artifacts, report, evidence И
  канонический run record (`runs/<target>/<run-id>/`, если прогон его
  создаёт) — canary не должен иметь места, куда можно «легально» утечь мимо
  скана; layout run record при этом не меняется (backward compatibility);
- `nix flake check` зелёный, включая новый preflight.

## Definition of Done

- `allow_read` в policy + digest + валидация + fixtures;
- оракулы §2 (два RED negative + один GREEN positive control) соответствуют
  конвенции матрицы (конкретный errno + эффект);
- canary не появляется ни в одном артефакте прогонов;
- комментарии frame.rs/request.rs соответствуют реальной хореографии;
- решения из §4 записаны в `sandboy-boundary.md`;
- VM preflight существует и fail-closed;
- Own.NET не тронут ни одним байтом;
- нет ни одной строки Landlock/seccomp/cgroup-реализации;
- нет capability transport;
- workspace собирается, `cargo test` (portable suite) зелёный,
  clippy `-D warnings` чистый.

## Non-goals

Backend-реализация (SB-A1), capability transport (SB-B), o7-model-gate (MG-C),
любые изменения Own.NET (SB-A2), ArliAI, очереди, credentials, изменение
wire-формата протокола.
