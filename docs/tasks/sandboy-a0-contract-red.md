# Задача A0: коррекция контракта Sandboy Vertical B + RED-оракулы (read confidentiality)

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
- `docs/architecture/capability-fd-transport.md` — принятое направление этапа B
  (в этом PR НЕ реализуется);
- `Own.NET/sandboy/` (sibling-репозиторий) — только для понимания; см. запрет ниже.

## Зафиксированные решения (не пересматривать)

1. **Топология**: monitor-backend (Decision 2 в `sandboy-boundary.md`). Монитор —
   неограниченный доверенный владелец lifecycle: cgroup, timeout, teardown, report
   relay. Confinement (Landlock + seccomp) устанавливает **confined child между
   fork и exec**, сам делает self-check и отдаёт результат монитору. Монитор
   никогда не попадает под target policy.
2. **Capability FD не входит в Vertical B.** Направление зафиксировано в
   `docs/architecture/capability-fd-transport.md`; в A0/A1 — ни строчки реализации.
3. **Судьба standalone CLI**: новый backend (этап A1) никогда не имеет
   production-режима `run --policy <toml>`. Диагностический `probe` сохраняется.
   Ручная отладка — только через будущий dev-harness, говорящий на настоящем
   `o7-sandbox-protocol`. Второго policy-формата не существует.
4. **Own.NET/sandboy неприкосновенен** до этапа A2:
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
- `allow_read` входит в канонический `SandboxPolicy::digest()`;
- known-answer fixtures digest'а обновляются осознанно (это ожидаемое изменение,
  зафиксировать в комментарии фикстуры);
- существующие смысловые oracle не ослаблять; механические правки конструкторов
  policy из-за нового поля допустимы. Compatibility-конструкторы «чтобы diff
  выглядел нетронутым» запрещены.

### 2. Три независимых RED-оракула read/exec в `sandbox_confinement.rs`

По конвенции существующей матрицы (конкретный errno, конкретный эффект, никакого
`is_err()`), против замороженного fake backend (RED = наблюдаемый escape):

1. **Секрет вне allow_read**: файл с уникальным canary-содержимым вне
   `worktree ∪ allow_read ∪ allow_exec`; probe пытается `read()` →
   ожидание `EACCES`/`EPERM`; байты canary НЕ появляются в stdout, stderr,
   marker artifact, report и evidence (универсальный canary-скан, по образцу
   env-canary Own.NET S0);
2. **Read без execute**: файл внутри `allow_read`, не в `allow_exec`:
   `read()` → OK; `execve()` → `EACCES`/`EPERM`;
3. **Execute разрешён**: бинарь внутри `allow_exec`: `execve()` → OK.

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

- создание Landlock ruleset со ВСЕМИ access-битами, которые нужны Vertical B
  (включая `TRUNCATE`, ABI ≥ 3, и net-биты ABI 4);
- установку seccomp-фильтра;
- cgroup v2: create / move / `cgroup.kill` / drain / remove с delegated
  ownership;
- доступность `/proc` в форме, нужной fd_probe;
- IPv4/IPv6 prerequisites для будущих network-оракулов.

Каждая проверка fail-closed: недоступная возможность = красный check, не warning.
Прогон confinement-матрицы в VM — этап A1, сюда не тащить. Переиспользуй
проверки из `.github/workflows/sandbox-confinement.yml` (self-hosted preflight) —
семантика должна совпадать.

## Обязательные тесты

- unit: валидация `allow_read` (относительный путь, дубликат), digest меняется
  при изменении `allow_read`, digest-фикстуры;
- unit: сериализация policy round-trip с новым полем; `deny_unknown_fields`
  поведение сохранено;
- RED-оракулы №1–3 (designated-runner suite, `#[ignore]` по существующей
  конвенции);
- canary-скан как переиспользуемый helper, не копипаста по тестам;
- `nix flake check` зелёный, включая новый preflight.

## Definition of Done

- `allow_read` в policy + digest + валидация + fixtures;
- три RED-оракула соответствуют конвенции матрицы (конкретный errno + эффект);
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

Backend-реализация (A1), capability transport (B), o7-model-gate (C), любые
изменения Own.NET (A2), ArliAI, очереди, credentials, изменение wire-формата
протокола.
