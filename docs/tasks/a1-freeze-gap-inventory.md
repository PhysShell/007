# A1 contract freeze — gap inventory (review-документ, не нормативный)

**Статус: REVIEW INPUT для A1 contract freeze.** Этот документ — первый
артефакт фазы contract-review: перепись того, что в issue #95 уже решено,
что решено-но-не-закодировано, что обязан выбрать именно freeze, что явно
отложено, и где draft противоречит принятым входам. Он потребляется будущим
нормативным A1-контрактом и после freeze теряет авторитет (может быть
удалён или помечен historical). Реализация A1 до принятого freeze запрещена.

## Входы и их ревизии (evidence-discipline rule 4)

Проверено в этой ветке на `main = fe16c41` (merge PR #107):

- `AGENTS.md`, `TODO.md` (A-series status, reconciled 2026-08-04);
- `docs/q-deck/r1-command.md` — R1 принят/merged (PR #90), включая §11
  (dispatch boundary, `ValidUnsealedDispatchAmbiguous`);
- `docs/q-deck/a0-candidate-state.md` — A0 принят: contract-first `71800fc`,
  accepted head `52627c3`, merge `f1ac458` (PR #92, 8 corrective-раундов);
- `docs/autonomy-controller.md` — принят на `c5b3ae0`, merge `c5c51e06`
  (PR #93);
- issue **#95** — DRAFT / NOT FROZEN, updated 2026-08-04; A0.0 precondition
  SATISFIED; без комментариев;
- issue **#94** — **PLANNED / risk note, «no schema freeze here»** — НЕ
  нормативный источник (см. E5);
- `docs/decision-and-admission-protocol.md` §5 (diagnostic vs admission);
- `docs/evidence-and-decision-discipline.md` — ratified; rule 4 применяется
  немедленно;
- `crates/o7-run/src/candidate.rs`, `crates/o7-ledger` (command/идемпотентность),
  `crates/o7-sandbox-protocol/src/request.rs` (прецедент domain-separated
  digest `o7-launch-spec\0v1\0`).

---

## A. Accepted unchanged — потребляется, повторно не решается

Перечислено, чтобы freeze-review не тратил раунды на уже принятые решения.

1. **A0-типы и семантика** (normative source `docs/q-deck/a0-candidate-state.md`
   на accepted head): `CandidateStateReceiptV1`, `CandidateStateContractV1`,
   `RepositoryIdentity`, cumulative-patch-model относительно одного
   immutable base, наследование base от родительского receipt (никогда от
   `--base`), sealing boundary как capability (`seal_candidate`), порядок
   `RunStarted → CommandBindingCaptured → CandidateStateMaterialized →
   AgentStarted → … → RunSealed`. Issue #95 сам запрещает их переопределять.
2. **R1 dispatch-boundary семантика**: `AgentStarted` = durable dispatch
   boundary; safe redrive только при доказанном non-dispatch;
   post-dispatch неоднозначность fail closed
   (`ValidUnsealedDispatchAmbiguous`), никогда auto-redrive; at-most-once
   scoped к single-host/one-ledger model; sealed run никогда не
   переоткрывается; идемпотентность через существующий
   `idempotency_record` (scope + key + request digest).
3. **autonomy-controller**: четырёхслойное разделение властей
   (planner / campaign FSM / workflows / canonical log+reducer); минимальный
   phase-набор кампании; stop/escалation-состояния; transition-authority
   таблица; recovery = replay + reconcile + fail-closed; budgets как typed
   terminal/escalation, не само-поднимаемые лимиты; ReviewVerdict minimum.
4. **Шесть authority-границ** (в issue #95 уже закодированы прозой —
   переносятся в типы, не пересматриваются): model verdict ≠ controller
   decision; GitHub comment ≠ inter-agent authority; успешный provider
   response ≠ accepted report; совпадение двух claims ≠ доказательство
   lineage; timestamp ≠ порядок; retry не лечит ambiguous side effect.
5. **Provider invocation boundary, девять правил** (issue #95 §2) —
   генерализация R1 на все роли, без усиления до exactly-once.
6. **evidence-discipline**: rule 2 (atomic precondition consumption — форма
   для human-command TOCTOU и будущего merge), rule 4 (revision binding),
   constraint `raw → authority-specific classifier → typed fact →
   deterministic policy`.
7. **Non-goals issue #95** целиком (no planner в core, no model-to-model
   channel, no auto-merge default, no A5 runtime, no D3/D4 зависимостей).

---

## B. Уже решено — требует только кодирования во freeze

1. **Двухформенность каждого артефакта** (raw untrusted / controller-accepted):
   `CoderReport→CandidateReceipt`, `ReviewerReport→ReviewVerdict`,
   `HumanCommandRequest→HumanDecision` — типы + acceptance-валидаторы,
   оформленные как authority-specific classifiers (A.6).
2. **Envelope**: список полей зафиксирован; кодированию подлежат схема,
   обязательность per message_kind и какие поля входят в digest (см. C4).
3. **Lineage authority rule**: controller-side resolution от causation
   target + canonical campaign binding; mismatch fail closed; негативный
   тест «две совпадающие claims не доказательство».
4. **Artifact refs** `(digest, media_type, size)` только в 007-owned
   storage; резолвер с confinement-дисциплиной по прецеденту o7d round 6
   (no-follow descriptor-based bounded reads).
5. **ProviderInvocationReceiptV1**: sketch → frozen schema; `capture.status`
   / `model.resolution.status` словари; interaction manifest; правило
   «alias никогда не resolved identity».
6. **§5 no-model-supplied-executable-authority**: `required_evidence_gate_ids`
   только из controller-owned registry + `verifier_policy_digest`;
   `diagnostic_runs` forensics-only (протокол #93/`decision-and-admission`
   §5 уже решил семантику).
7. **Feed vs attention**: два объекта; `dedupe_key` только controller;
   ACK ≠ RESOLVED; lifecycle `OPEN → ACKNOWLEDGED → RESOLVED / SUPERSEDED`.
8. **Human command binding**: `expected_campaign_state_version` /
   `expected_contract_digest` / `expected_head` — stale-screen TOCTOU
   закрывается по форме rule 2 (bind → refresh → conditional atomic
   consume); кодировать как условные операции, не check-then-act.
9. **ANSWER_QUESTION** `declared_scope_effect` и REVISE_CONTRACT-supersede;
   **CANCEL** как процесс (`CancelRequested → … → CampaignCancelled`).
10. **Reviewer independence** — механический список (fresh session, no
    transcript, detached attested worktree, no mutation credentials,
    отдельные prompt/tool-policy digests) + порядок входов (нарратив
    кодера последним, помечен advisory).
11. **Негативная матрица §10** — кодируется как RED acceptance matrix
    этапа типов (аналог SB-A0: типы + красные оракулы до реализации).
12. **Маппинг opaque-имён на реальные принятые типы** (см. также E2):
    `CandidateRef` → `ArtifactRef` на `CandidateStateReceiptV1`;
    `MaterializationAttestationRef` → принятый A0/o7-worktree
    attestation-тип. Freeze обязан назвать точные имена, а не «resolve to
    accepted types».
13. **Delivery-tier честность** (§8): v1-lite / operational v1 — имена и
    границы уже даны, кодируется как явное поле контракта acceptance.
14. **D0–D4** — остаётся non-normative rationale; в контракт входит только
    «A1 correctness = D0 + D2».

---

## C. Открытые выборы — решает именно freeze (нужна адьюдикация)

C1. **Canonical bytes.** Выбор: canonical-JSON/CBOR-кодек vs
**digest-of-exact-stored-bytes**. Рекомендация: exact-stored-bytes —
это уже действующий прецедент всего проекта (digest-chain `events.jsonl`,
`ArtifactRef`, A0-receipts), не требует изобретать канонизацию и не
создаёт «Byzantine whitespace». Вместе с ним зафиксировать:
`deny_unknown_fields` для всех canonical-артефактов (прецедент A0);
UTF-8-политика — byte-exact без нормализации, байтовые поля кодируются
явно (прецедент `EnvEntry: Vec<u8>`); никаких «semantically equal»
сравнений вне парсированных типов.

C2. **Domain separation digests.** Схема префиксов для каждого
digest-контекста (прецедент `o7-launch-spec\0v1\0`). Открыто: единый
реестр контекст-строк в протокольном крейте.

C3. **Size/depth limits.** Константы per message_kind (прецеденты:
1 MiB request-frame, 64 KiB report, 8/64/128 MiB bounded reads). Открыт
только выбор чисел; принцип «cap + фактическое наблюдение сверх cap =
reject» уже прецедентен.

C4. **Идентичность сообщения и покрытие digest.** Что именно покрывает
`payload_digest`; входит ли envelope в digest; где живёт `created_at`
(рекомендация: вне идентичности — он «observation, not ordering», и не
должен делать два логически идентичных replay разными). Без этого правило
«same message_id + different payload digest → conflict» не имеет точного
предмета.

C5. **Сшивка identity-скелетов** (см. E3 — самое крупное). Envelope вводит
`root_goal_id → task_id → campaign_id → round_id`, но не определяет
отношение к УЖЕ существующим `conversation_id / CommandId / RunId /
run_attempt`, которыми владеют R1/A0 и которые A1-роли фактически
исполняют. Рекомендация к адьюдикации: v1 — campaign 1:1 привязан к одной
R1-conversation при минтинге; каждый round ссылается на конкретные
command/run canonical id; повторная реализация conversation-механики
запрещена. Любой другой выбор тоже допустим — но он должен быть СДЕЛАН.

C6. **Storage authority A1-артефактов до A2.** A2 (durable campaign
reducer) ещё не существует; A1-артефакты обязаны быть durable, typed,
replayable уже сейчас. Варианты: (a) content-addressed artifact store +
acceptance-записи в `o7-ledger` (расширение существующих таблиц);
(b) campaign-level canonical event stream (фактически ранний кусок A2);
(c) гибрид. Freeze обязан провести границу A1/A2 так, чтобы A1 не строил
campaign-reducer втихую. Рекомендация: (a), с явной пометкой, что
canonical campaign FSM приходит в A2 и НЕ будет выводиться из ledger-строк
(ledger = projection, A.3).

C7. **Грань execution/dispatch.** Issue фиксирует split
`provider_execution_id` vs `dispatch_id` как freeze-time check, но
таксономию инкарнаций откладывает в A2. Развязка для freeze: СЛОВАРЬ и
правило «каждый retry называет свою грань (whole execution / single
dispatch / tool-loop continuation / new session)» замораживаются сейчас
(иначе receipt-схему C5 нельзя закрыть); полная таксономия
`producer_run_id` — A2, как и записано. Это надо проговорить явно, чтобы
не читалось как противоречие.

C8. **Ацикличность evidence-графа.** Механизм: только forward-refs
(`CandidateReceipt.coder_report_ref`, `ReviewVerdict.reviewer_report_ref`)
vs выделенное событие `ArtifactAcceptance`. Рекомендация: forward-refs,
`ArtifactAcceptance` остаётся deferred с триггером «появился потребитель
acceptance-как-события» (D8). Плюс кодифицированное правило направления:
digest-ссылки текут только в направлении authority (evidence → report →
accepted artifact), back-links живут в проекциях.

C9. **Закрытые transition-таблицы** для семи машин: campaign; provider
invocation (outcome-словарь receipt); принятие CoderReport; принятие
ReviewerReport; corrective round; human attention/decision; cancellation +
budget + ambiguity. Проза и списки есть (A.3, B.7, B.9); freeze должен
дать закрытые множества переходов и явные guard-требования по каждому
ребру (форма — transition-authority таблица #93). Конкретные открытые
рёбра: может ли CANCEL прервать `CI_WAIT`/`REVIEWING` немедленно или
только на границе шага; provider-outcome ambiguity → `HUMAN_REQUIRED`
кампании (рекомендация: да, всегда, как R1 manual-resolution) или
допускает автономный переход; исчерпание бюджета в середине corrective
round.

C10. **Аутентификация human-lane в v1.** `actor_identity` /
`authorization_context` без выбранного механизма — пустые слова. Открыто:
минимальная v1-модель (single-operator, локальная authority o7d,
Q-Deck-сессия?) либо явно записанное ограничение «v1 доверяет локальному
оператору хоста; многопользовательская auth — post-v1 с триггером».
Молча оставить поля нетипизированными нельзя: на них висит HumanDecision
как canonical authority.

C11. **Gate/verifier registry.** Представление идентичности гейта
(строка? typed id + версия?), версия реестра, связь с
`verifier_policy_digest`, поведение на unknown gate id (fail closed —
уже решено §10-матрицей; кодировать).

C12. **`model_identity` normalization.** Схема нормализации
(provider+family+alias?) и её связь со статусами resolution в receipt;
запрет представлять alias как resolved identity уже принят (A.5).

C13. **Что из негативной матрицы — свойство типов, а что — тест.**
Рекомендация по прецеденту A0/`BoundaryEvidence`: непредставимость
(receipt-до-outcome-класс ошибок; «accepted» из уст модели; lineage из
envelope) — типами; остальное — RED-тесты.

---

## D. Явно отложено — записать в контракт с триггерами

1. Полная таксономия инкарнаций + `producer_run_id` семантика → **A2**
   (вместе с campaign reducer; #94 §5 versioning приходит туда же).
2. `AUTHORIZE_MERGE` + mutation-auth story → post-v1; форма уже
   предписана rule 2 (`merge(sha=accepted_head)` как conditional atomic
   mutation). Триггер: action broker / merge policy.
3. Push-tier («operational v1») → триггер: phone-интервенция становится
   acceptance-критерием продукта (§8 честно называет это product
   requirement).
4. Cross-family reviewer → существующий consensus-backlog.
5. Webhooks / level-triggered reconciler → **A3** (#94 §1 — vocabulary
   note, не schema).
6. Goal-graph runtime, root-goal budgeting → **A5**.
7. `accept_residual_risk`, `PAUSE`/`RESUME`, `REQUEST_MORE_EVIDENCE`
   standalone → post-v1 (v1 достигает через attention actions).
8. `ArtifactAcceptance` как событие → триггер: реальный потребитель
   (C8).
9. Admission-profiles полная типология (`LIGHTWEIGHT…CRITICAL`, #94 §4) →
   A2/A3; A1 замораживает только: `admission_profile` derived от
   controller-observed diff, автономная мутация кода ≥ STRICT-семантика,
   неоднозначность классификации fail closed к строгому. (См. E5 — из #94
   в A1-контракт эти три пункта надо ВНЕСТИ, а не сослаться.)

---

## E. Противоречия и устаревшие формулировки — исправить ДО freeze

E1. **Near-duplicate candidate identity — главный риск draft'а.**
`CandidateReceipt` (§3) декларирует `candidate_head`,
`candidate_tree_identity`, `base_ancestry`, `repository_identity` —
поля, почти совпадающие с принятыми A0 `candidate_tree_oid`,
`base_commit`, `repository_id` из `CandidateStateReceiptV1`. Два почти
одинаковых authority-типа отличаются местом будущей аварии. Freeze обязан
переопределить `CandidateReceipt` как **ссылку на A0-receipt (ArtifactRef)
+ строго controller-derived расширение** (changed_paths, file_modes,
diff_scope, admission_profile, claim_check, coder_report_ref) — без
повторного объявления ни одного identity-поля, которым владеет A0.

E2. **Opaque-имена не существуют в принятом A0.** `CandidateRef` и
`MaterializationAttestationRef` — имена draft'а; принятые типы называются
иначе (B.12). До freeze — точный маппинг, иначе реализация «уточнит» сама.

E3. **Identity-скелеты не сшиты.** Envelope (`campaign_id`/`round_id`/…)
нигде не упоминает `conversation_id`/`RunId`/`CommandId`, при этом §9
v1-lite прямо зеркалит R1 single-in-flight, а исполнение ролей физически
идёт через R1/A0 runs. Без явной сшивки (C5) появятся две параллельные
линии lineage — ровно то, что lineage authority rule запрещает.

E4. **Три «head-подобных» поля без иерархии authority.** Envelope
`expected_input_head`, WorkOrder `input.base_sha`, candidate ref. По A0
base наследуется от родительского receipt и никогда не re-derived; freeze
обязан объявить `base_sha`/`expected_input_head` проекциями inherited
obligation (проверяемыми, fail closed при расхождении), не независимыми
входами.

E5. **Нормативная зависимость от ненормативного #94.** #95 применяет #94
§1/§2/§4/§5, но #94 — «PLANNED / risk note … no schema freeze here».
Контракт не может цитировать его как authority (rule 4 + rule 3):
потребляемые фрагменты (dedupe-идемпотентность, external drift как
attention-класс, три admission-profile-инварианта из D9, версии в
envelope) должны быть ВНЕСЕНЫ в текст A1-контракта как его собственные
нормы со ссылкой «происхождение — #94».

E6. **REVISE_CONTRACT vs «replacement mints new campaign».** §2 требует:
замена исполнения минтит новый `campaign_id` + supersedes. §7 REVISE_CONTRACT
говорит «new contract version → revalidation / replanning», не говоря,
минтится ли новая кампания. Freeze должен закрыть: revised contract ⇒
терминализация текущей кампании (superseded) + новая кампания с новым
`contract_digest`, либо явно обоснованное исключение. Иначе «frozen
acceptance criteria» и exact-head дисциплина охраняют не тот контракт.

E7. **`claimed_state_digest` не определён.** Digest чего (A0 tree OID?
patch? state-снимка?) — определить или удалить; недоопределённое поле в
untrusted-отчёте станет свалкой.

E8. **`round_id` не определён.** Кто минтит, что связывает (corrective
round ↔ command/run?), терминальность round'а. Связано с C5/C9.

---

## Порядок закрытия (предложение, соответствует объявленной фазе)

1. Адьюдикация C1–C13 + исправления E1–E8 (интерактивно, порциями —
   решения с maintainer'ом ратифицированы по rule 3 carve-out).
2. Нормативный A1-контракт (dedicated contract commit, версионирован),
   внёсший категорию B и результаты п.1; D-пункты — с триггерами.
3. Типы + RED-матрица (§10 issue + C13-разбиение) — аналог SB-A0: красные
   оракулы падают по наблюдаемому эффекту, не заглушкой.
4. Review → FREEZE (issue #95 переводится из DRAFT; supersede-path §7
   становится единственным способом изменений).
5. Только затем — реализация A1.

Границы этапа (повторение мандата, не новое решение): никакого
coder/reviewer runtime, provider adapters, action broker, MG-C, изменений
A0, A2-инкарнаций, общего planner'а в этой ветке.
