# Задача SB-A1: Sandboy Vertical B — настоящий kernel-confinement backend в 007

> Идентификаторы этапов: **SB-A0** (контракт + RED) → **SB-A1** (этот документ) →
> **SB-A2** (retirement Own.NET) → **SB-B** (capability transport) → **MG-C**
> (o7-model-gate). Префикс SB- обязателен: голые «A0»/«A1» конфликтуют с
> Q-Deck A0/A1 (`docs/q-deck/`).

## Роль

Ты работаешь как senior systems/security engineer в проекте 007. Задача — один
вертикальный PR: настоящий внешний Sandboy-backend (monitor topology, Landlock +
seccomp + cgroup v2), который переводит существующую RED-матрицу Vertical B в
GREEN. Не платформа, не gateway, не capability runtime.

Пререквизит: этап SB-A0 (`docs/tasks/sandboy-a0-contract-red.md`) принят — policy
содержит `allow_read`, read/exec-оракулы существуют (два RED negative + один
GREEN positive control), VM preflight зелёный.

Контекст, который ОБЯЗАТЕЛЬНО прочитать до изменений:

- `docs/architecture/sandboy-boundary.md` — все шесть замороженных Decision +
  дописанные в SB-A0 решения; RED-контракт Vertical B;
- `crates/o7-sandbox-protocol/` — протокол, который backend обязан говорить;
- `crates/o7-worker/` — seam, control-socket хореография, verify_report,
  fake backend (`sandboy_fake`), harness-бинари;
- `crates/o7-worker/tests/sandbox_confinement.rs` — acceptance-матрица;
- `Own.NET/sandboy/src/{main.rs,policy.rs}` — enforcement-примитивы для переноса
  (см. §2); сам репозиторий Own.NET НЕ изменять;
- `docs/architecture/capability-fd-transport.md` — что НЕ входит в этот PR.

## Зафиксированные решения (не пересматривать)

1. **Место**: `tools/sandboy-backend/` — отдельный workspace-package, НЕ модуль
   o7-worker. Визуальная граница доверия: `crates/*` = unsafe forbidden,
   `tools/sandboy-backend` = audited syscall boundary, узко-скоуповый unsafe
   разрешён собственными `[lints]`, каждый unsafe-блок с SAFETY-комментарием
   (по образцу спайка Own.NET).
2. **Топология**: монитор — неограниченный владелец lifecycle (cgroup owner,
   timeout, teardown, report relay). Confinement устанавливает **child между
   fork и exec**, с **cgroup placement barrier** — членство в cgroup
   устанавливает и доказывает МОНИТОР, child никогда сам не пишет в
   `cgroup.procs`, и confinement не начинается до доказанного членства:

   ```
   fork
   → child немедленно ждёт на приватном pre-fork socketpair
   → monitor пишет PID child в cgroup.procs своего dedicated cgroup
   → monitor перечитывает cgroup.procs и доказывает membership
   → monitor посылает CHILD_CONTINUE
   → child ставит no_new_privs → Landlock → seccomp
   → child self-check → результат монитору по socketpair
   → child ждёт авторизации → exec target
   ```

   Монитор никогда не под target policy.
3. **Протокол**: только существующий `o7-sandbox-protocol` (LaunchRequest →
   report → EOF-proof → GO по socket на stdin). Никакого второго формата policy,
   никакого TOML CLI.
4. **Отчёт честный**: по каждому измерению `enforced`/`partial`/`not_enforced`.
   Backend НИКОГДА не «предупреждает и продолжает»: он сообщает факт, решение
   fail-closed принимает родитель (`verify_report` уже отказывает без полного
   enforcement). Дублирующего `strict`-рубильника в backend'е нет.
5. **Own.NET/sandboy неприкосновенен** (переносим код копированием с указанием
   происхождения в commit message, не git-зависимостью; спайк, его CLI, его
   workflow и S0 identity не трогаем — это этап SB-A2).
6. **Capability FD transport — вне scope** (этап SB-B).

## Техническое задание

### 1. `tools/sandboy-backend`

Бинарь с двумя подкомандами:

- **основной режим** (запускается только o7-worker'ом через sealed backend
  descriptor): читает LaunchRequest с socket на stdin, исполняет хореографию
  Decision 3;
- **`probe`**: диагностический host-capability отчёт (JSON, всегда exit 0),
  перенос и адаптация `probe` из спайка; тот же enforcement-код, что и боевой
  путь, чтобы отчёт не мог разойтись с реальностью.

Зависимости: `o7-sandbox-protocol`, `landlock`, `seccompiler`, `libc`/`nix`,
serde/serde_json. Без tokio (backend — маленький синхронный процесс), без
внешних сервисов.

### 2. Перенос enforcement-примитивов из Own.NET

Переиспользовать (адаптируя к новой топологии и policy):

- построение Landlock ruleset + маппинг статусов;
- seccomp-денилист + таблица имя→номер;
- `no_new_privs`;
- `close_range(CLOEXEC)` scrub;
- подход к env allowlisting (полная очистка + явные имена из LaunchRequest);
- каркас `probe`.

Выбросить: exec-in-place топологию, TOML policy, standalone `run` CLI,
`PartiallyEnforced → warn → continue`.

### 3. Landlock-маппинг policy

- `worktree` → read + write (execute НЕ выдавать; если exec из worktree нужен —
  путь обязан быть явно в `allow_exec`);
- `allow_read` → read-only биты, без execute;
- `allow_exec` → read + execute, без write;
- `TRUNCATE` (ABI ≥ 3) обязателен — существующий оракул это проверяет;
- отсутствующий на хосте путь из policy — ошибка запуска (fail closed), не
  skip-with-warn: у backend'а, в отличие от спайка, нет «портабельности policy»
  как задачи.

### 4. seccomp

- курируемый денилист спайка (ptrace, mount, bpf, kexec, module-loading, …) —
  перенести;
- **добавить argument-level правила**:
  - `socket(AF_INET, …)` и `socket(AF_INET6, …)` → `EPERM` (доказывает
    «новые inet-сокеты запрещены» независимо от Landlock-net и закрывает
    UDP/QUIC-дыру, которую Landlock ABI 4 не покрывает);
  - `setsid`, `setpgid` → `EPERM` (defense in depth к cgroup-ownership,
    уже заявлено в ADR);
- `AF_UNIX` socket() остаётся разрешён;
- неизвестное имя syscall в конфигурации — ошибка, не skip (конвенция спайка
  для explicit-списков);
- **Landlock network (ABI 4) не используется и не требуется**: seccomp
  argument filter — единственный нормативный механизм запрета inet-сокетов
  в SB-A1. Landlock-net как второй слой defense-in-depth — возможное будущее
  расширение, не часть этого этапа.

### 5. cgroup v2 + lifecycle

- монитор создаёт собственный cgroup, помещает child по placement barrier из
  «Зафиксированных решений» п.2 (child ждёт CHILD_CONTINUE, монитор доказывает
  membership перечитыванием `cgroup.procs`), следит за wall-clock timeout
  (`cgroup.kill` по дедлайну), доказывает teardown (drain до пустого
  `cgroup.procs`, потом remove);
- double-fork/descendant escape закрывается членством в cgroup — существующие
  оракулы процесс-дерева должны позеленеть без ослабления;
- монитор, умирающий раньше живого owned target, — fail closed (конвенция ADR).

### 6. Report

- собирается монитором из self-check child'а; привязки `policy_digest`,
  `launch_nonce`, `backend_digest`, `launch_spec_digest` — по существующим
  типам протокола;
- каждое измерение отражает ФАКТ (например, Landlock `PartiallyEnforced` на
  старом ядре → `partial`, и родитель не даст GO);
- никаких новых полей протокола в этом PR (если self-check требует
  внутреннего формата child→monitor — это приватный формат socketpair,
  не публичный протокол).

### 7. Nix + generic Linux

- `packages.<system>.sandboy-backend`;
- NixOS VM-тест, прогоняющий ПОЛНУЮ confinement-матрицу (A0-preflight — его
  предусловие) — воспроизводимый основной environment;
- self-hosted workflow `sandbox-confinement.yml` обновить на реальный backend —
  второе подтверждение на реальном хосте;
- документация generic-сборки: `cargo build --release -p sandboy-backend`,
  требования к ядру (Landlock ABI, cgroup v2 delegation), запуск матрицы.

## Обязательные тесты

- вся существующая RED-матрица `sandbox_confinement.rs` → GREEN. Смысловые
  oracle не ослаблять и не переписывать ради GREEN; допустимы механические
  изменения из-за расширения policy (формулировка SB-A0); RED-оракулы SB-A0
  переходят RED → GREEN, positive control (`execve` в `allow_exec`) остаётся
  GREEN;
- unit-тесты backend'а: маппинг policy → Landlock rights; argument-фильтры
  seccomp (socket AF_INET/AF_INET6 denied, AF_UNIX allowed, setsid/setpgid
  denied); отказ на отсутствующий путь policy; honest partial report на
  урезанном ABI (мокается статусом, не требует старого ядра);
- fd scrub: после exec таблица дескрипторов target'а — ровно `{0,1,2}`
  (расширить существующий `fd_probe`, не создавать новый);
- canary-скан SB-A0 зелёный на реальном backend'е;
- `nix flake check` (включая VM-матрицу), `cargo test` workspace, clippy чистый.

## Definition of Done

- `tools/sandboy-backend` существует, собирается, говорит на
  `o7-sandbox-protocol`;
- монитор unrestricted, confinement в child, self-check в child;
- Landlock read/write/exec-разделение соответствует §3;
- seccomp включает argument-level socket/setsid/setpgid фильтры;
- cgroup lifecycle с proven teardown;
- отчёт честный по измерениям, fail-closed решение у родителя;
- RED-матрица GREEN без ослабления оракулов;
- Nix package + VM-матрица + обновлённый self-hosted workflow + generic docs;
- Own.NET не тронут; capability transport отсутствует; model-gate отсутствует;
- ни одного unsafe вне `tools/sandboy-backend`, каждый unsafe — с SAFETY.

## Non-goals

Capability FD transport (SB-B), o7-model-gate/ArliAI (MG-C), чистка Own.NET
(SB-A2), Firecracker/microVM (Layer 1), address/domain egress (Layer 3),
Agent Vault, любой multi-provider/gateway код.
