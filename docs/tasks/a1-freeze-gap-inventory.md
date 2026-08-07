# A1 contract freeze — gap inventory (адьюдицировано)

**Статус: ADJUDICATED (ревью maintainer'а, 2026-08-07).** Исходная редакция
этого документа была review input; интерактивный разбор maintainer'а
адьюдицировал C1–C13 и E1–E8 и добавил два пропущенных блокера E9/E10 —
решения **ратифицированы** (rule 3 carve-out: приняты в интерактивной
сессии). Ниже — запись диспозиций; **нормативный текст решений живёт в
`docs/q-deck/a1-contracts.md`** (NORMATIVE DRAFT до freeze), не здесь.
После freeze этот документ — historical record адьюдикации. Реализация A1
до принятого freeze запрещена.

Ключевая поправка всей адьюдикации: **campaign — отдельная logical
authority, а не новое имя для R1 conversation**; логическая и физическая
lineage соединяются каноническим binding-receipt, не отождествлением.

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

## C. Открытые выборы — АДЬЮДИЦИРОВАНО (нормативный текст в `a1-contracts.md`)

C1. **Canonical bytes — ACCEPT WITH REVISION.** Не один универсальный
ответ, а ДВА вида digest: `BlobDigest = SHA-256(exact stored bytes)` для
CAS/refs (без нормализации и повторной сериализации) и protocol/semantic
digests (`MessageBindingDigest`, `ContractDigest`, `RegistryDigest`,
`PolicyDigest`) по явно фреймированным типизированным полям с domain
separation. Canonical-артефакты: UTF-8 JSON, `deny_unknown_fields`,
duplicate fields rejected, без Unicode-нормализации, без float-полей;
OS-байты (`RepoPathBytes` и т.п.) — замороженный `ByteStringV1` =
base64url без padding (гарантия A0 на не-UTF-8 не отменяется удобным
`String`).

C2. **Domain separation — ACCEPT.** Compile-time registry контекстов в
протокольном крейте, форма `o7-a1\0<purpose>\0v1\0`; только константы,
purpose не переиспользуется, uniqueness-test + known-answer test на каждый
контекст. Generic `BlobDigest` не domain-separated (адрес содержимого;
типовая защита — в typed ref).

C3. **Limits — ACCEPT, v1-профиль заморожен сейчас** (не ждать
telemetry). Таблица per-kind в контракте; превышение = reject, не
truncation; потоковый reader останавливается на cap+1.

C4. **Message identity — REVISE.** Три сущности: `payload_digest`
(exact stored payload bytes, envelope excluded), `blob_digest` (exact
complete stored artifact bytes), `message_binding_digest`
(domain-separated framed semantic bindings). `message_id` =
idempotency key, его request digest = `message_binding_digest`;
`message_id` в binding digest НЕ входит. `created_at` УДАЛЁН
(двусмысленный): есть недоверенный `producer_observed_at` внутри provider
receipt и `accepted_at` от controller/ledger; ни один не задаёт порядок;
replay возвращает сохранённые байты. `correlation_id` УДАЛЁН из canonical
envelope (нет закрытой семантики; поле «на всякий случай» станет будущей
authority).

C5. **Сшивка identity — REJECT рекомендации campaign 1:1 conversation.**
1:1 ломается об reviewer independence (reviewer либо продолжит coder
transcript, либо станет новым tail). Заморожен явный bridge
`CampaignRunBindingV1` (campaign_id, round_id, role,
provider_execution_id, conversation_id, command_id: Option, run_id,
attempt_id): `campaign_id != conversation_id`; одна campaign может
связывать несколько conversations; coder-lane может продолжать одну R1
conversation; каждый reviewer execution — fresh session, отдельный
binding, v1-default отдельная conversation; `producer_run_id` выводится
controller'ом из binding, никогда из входящего envelope. Две оси
(logical: root_goal→task→campaign→round; physical:
round→binding→conversation/command/run/attempt), соединённые каноническим
receipt.

C6. **Storage до A2 — REJECT «CAS + authoritative acceptance rows в
ledger».** CAS принят; shadow campaign authority в ledger — нет (это
урезанный A2 под видом хранилища). Граница: A1 владеет schemas, canonical
writer, typed refs, classifiers, CAS-интерфейсом, acceptance
preconditions, RED matrix, test-only append sink; A2 владеет production
authority (canonical campaign event append, atomic acceptance recording,
campaign reducer, replay/resume). A1 реализуем как protocol/library
layer и не заявляет работающий durable campaign runtime.

C7. **Execution/dispatch — ACCEPT WITH CLOSED VOCABULARY.**
`ProviderExecutionId` (одно bounded role execution, весь tool loop) /
`ProviderDispatchId` (один внешний запрос). Generic
`retry_of_invocation_id` УДАЛЁН; вместо него закрытые `ExecutionCauseV1`
(Initial / CorrectiveRound / SafeRedrive+evidence) и `DispatchCauseV1`
(Initial / ToolContinuation / SafeRedrive+evidence). ToolContinuation,
новая session и corrective round — не retry. Полная таксономия — A2.

C8. **Acyclicity — ACCEPT forward-only, направление формулировки
исправлено.** Ребро A→B = «A непосредственно содержит digest-ref на B»;
canonical refs идут от производного к antecedent evidence; заморожен
rank (accepted derived > accepted raw report > receipt/manifest > raw
blobs), каждая embedded reference — строго на меньший rank; back-links
только в проекциях. `ArtifactAcceptance` не вводится (триггер сохранён).

C9. **Transition tables — REVISE постановку.** FSM/таблиц семь: campaign
phase FSM; round FSM; provider execution FSM; provider dispatch FSM;
human-attention lifecycle; cancellation/supersede control barrier;
budget/ambiguity policy table. Три acceptance (CoderReport /
ReviewerReport / HumanCommand) — НЕ машины, а чистые authority-specific
classifiers `raw + context → Accepted | Rejected(reason)`. Спорные рёбра
решены: CANCEL принимается из любого non-terminal немедленно, CANCELLED
показывается только после dispatch-barrier + revocation + классификации
side effects + forensic capture; `dispatch_ambiguous → HUMAN_REQUIRED`
всегда (исключение — уже выданный CANCEL: ambiguity сохраняется как
evidence, output не принимается); budget проверяется ДО ограничиваемого
side effect, post-hoc overshoot — receipt как evidence, следующий
progress-переход запрещён, safety-операции разрешены.

C10. **Human-lane auth — ACCEPT single-principal v1, не «trust
localhost».** Один configured maintainer principal; installation-scoped
control capability ≥256 random bits; secret никогда в артефактах;
credential_epoch (revoke/rotate); confidential+authenticated transport;
caller не выбирает principal_id; authn до idempotency mutation и
conditional consume. Controller выводит `AuthenticatedActorV1` в
HumanDecision; `actor_identity`/`authorization_context` как
authoritative-поля запроса не существуют.

C11. **Gate registry — ACCEPT typed ID + digest-bound registry.**
`GateRequirementV1 {gate_id, gate_contract_digest}`;
`GateRegistryRefV1 {registry_artifact_ref, registry_digest}`; gate result
связывает candidate_state_ref + gate_id + contract/policy digests +
outcome + evidence; unknown/duplicate/mismatch fail closed; никаких shell
strings.

C12. **model_identity — ACCEPT split, никакой «нормализации family».**
`LogicalModelRouteV1` (provider_id, route_id, requested_model,
routing_config_digest) отдельно от `ModelResolutionEvidenceV1`
(ProviderReported / FingerprintOnly / ProviderReportedWithFingerprint /
Unavailable). Alias остаётся requested_model, никогда provider_model_id.
У controller/human артефактов model-полей нет вовсе (tagged producer
binding, E10).

C13. **Types vs RED — ACCEPT, три уровня.** (1) Непредставимо wire-типом
(unknown variant, invalid digest/ID form, provider artifact без provider
binding и наоборот, alias как resolved identity, outcome-поля вне
variant, raw report со статусом accepted, path/URL вместо typed ref,
invalid producer binding); (2) непредставимо через construction API
(accepted без classifier, verdict без resolved report, decision без
authenticated actor, admission receipt без verified A0 receipt, receipt
до terminal/ambiguous outcome) — закрытые поля, checked constructors;
(3) RED-тесты (stale bindings, lineage mismatch, duplicate ID + другой
digest, unknown registry IDs, scope escalation, resolver escape, reviewer
mutation creds, claim mismatch, superseded question, unauthorized
command, retry без non-dispatch proof, replay-вызывает-provider, digest
cycle, budget/cancel спорные переходы). «Newtype вокруг String» — не
доказательство семантики.

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

## E. Противоречия — АДЬЮДИЦИРОВАНО (E1–E8) + два новых блокера (E9–E10)

E1. **Near-duplicate candidate identity — ACCEPT + переименование.**
Тип называется **`CandidateAdmissionReceiptV1`** (не CandidateReceipt —
слишком похоже на A0 candidate-state receipt): `candidate_state_ref` +
round binding + coder_report_ref + observed_change_set + admission
(profile, gate_requirements, classification_policy_digest) + optional
claim_check. НЕ содержит candidate_head / candidate_tree_identity /
base_ancestry / repository_identity / base_commit / candidate_tree_oid —
всё это разрешается через accepted A0 receipt.

E2. **Opaque refs — ACCEPT с точными wrappers, не голый ArtifactRef.**
`CandidateStateReceiptRefV1 {source_run_id, run_artifact_ref}` (kind
обязан быть `ArtifactKind::CandidateState`);
`CandidateMaterializationRefV1 {child_run_id, materialization_event_id,
materialization_event_digest}`; WorkOrder получает
`InputCandidateBindingV1 {candidate_state_ref, materialization_ref}` —
доказательство, что конкретный run материализовал конкретный accepted
candidate state до dispatch.

E3. **Identity stitching — ACCEPT, закрыт C5** (`CampaignRunBindingV1`;
никакого 1:1 campaign/conversation).

E4. **Head-поля — ACCEPT, усилено: слова `head` в canonical A1 нет
вовсе.** Удалены `expected_input_head`, `WorkOrder.input.base_sha`,
`ReviewRequest.base_sha`/`candidate_head`, `HumanCommand.expected_head`;
замены — candidate-state refs. Git tree OID и внешний commit SHA — разные
сущности; `head` зарезервировано для будущего `external_head_sha` (A3).
Provider-facing prompt может показывать base/tree как помеченную
projection из A0 receipt.

E5. **#94 — ACCEPT.** В контракт внесены как собственные нормы:
controller-derived dedupe; EXTERNAL_DRIFT как attention reason code
(detection — A3); autonomous mutation ≥ STRICT; controller-derived risk
classification; ambiguity → строгий профиль; schema versioning
артефактов. НЕ внесены раньше времени: reducer version, campaign replay
semantics, полная profile-таксономия, level-triggered reconciler (A2/A3).

E6. **REVISE_CONTRACT — ACCEPT new-campaign rule.**
`SupersedeRequested → block dispatches → quiesce/classify →
CampaignSuperseded → atomically mint replacement` (новый campaign_id,
новый contract_digest/version, explicit supersedes_campaign_id, тот же
root_goal_id, обычно тот же task_id, новая round-sequence). Новая
campaign не минтится до revocation capabilities старой и классификации
её side effects.

E7. **`claimed_state_digest` — DELETE.** Вместо него
`claimed_candidate_tree_oid: Option<GitTreeOid>` только при
candidate_produced; отсутствие допустимо; mismatch с controller-derived
tree = rejection fail closed; никаких claims о repository/base identity.

E8. **`round_id` — DEFINE NOW.** Opaque controller-minted `RoundId` +
монотонный `round_ordinal` (с 0); round создаётся до первого coder
dispatch, связывает campaign_id + contract_digest + input_candidate_ref +
work-order/directive ref; несколько execution IDs только при доказанном
safe pre-dispatch redrive; максимум один accepted
CandidateAdmissionReceipt; закрытый набор исходов (ACCEPTED /
CHANGES_REQUESTED / BLOCKED / HUMAN_REQUIRED / BUDGET_EXHAUSTED /
CANCELLED / SUPERSEDED / FAILED); CHANGES_REQUESTED минтит новый round.

E9. **НОВЫЙ БЛОКЕР: два разных `ArtifactRef`.** A0 уже владеет
`o7_run::ArtifactRef {kind, locator, digest}` (run-relative); draft A1
использовал то же имя для `(digest, media_type, size)` — это другая
address model (global CAS object). Имена разведены:
`o7_run::ArtifactRef` не переименовывается; A1 вводит
`CasObjectRefV1 {digest, size, media_type, content_kind}`. Никаких
typedef'ов одного в другое; импорт run-артефакта в CAS — только через
явный bridge `ArtifactImportedV1 {source_run_id, source_run_artifact_ref,
cas_object_ref}` с доказанным равенством байтов/digest. (Ирония
зафиксирована: draft, поймавший near-duplicate CandidateReceipt, сам
почти создал near-duplicate ArtifactRef.)

E10. **НОВЫЙ БЛОКЕР: envelope не был common — мешок условных полей.**
`producer_adapter_version` / `model_identity` / `prompt_digest` /
`tool_policy_digest` / `provider_invocation_receipt_ref` бессмысленны для
HumanDecision и controller-derived артефактов; `authorization_context` —
для model reports. Вместо nullable soup — **tagged `ProducerBindingV1`**:
`Controller {component_version, policy_digest}` / `Provider {role,
campaign_run_binding_ref, provider_execution_id, invocation_receipt_ref,
adapter_version, model_route_ref, prompt_digest, tool_policy_digest}` /
`Human {authenticated_actor_ref}`. Envelope core маленький
(schema/kind/version, message_id, logical lineage, causation, producer
binding, payload digest, artifact refs, recorded metadata);
contract_digest / candidate preconditions / action bindings уходят в
typed payload.

---

## Порядок freeze (ратифицирован)

1. Inventory обновлён решениями C1–C13 — сделано (этот документ).
2. E9/E10 добавлены — сделано.
3. Нормативный A1-контракт (`docs/q-deck/a1-contracts.md`): compact
   envelope core; tagged producer bindings; exact A0 wrappers;
   campaign/run bridge; digest registry; limits table;
   transition/classifier tables.
4. Отдельный review контракта: нет повторно объявленных A0/R1 identities;
   нет слова `head` без квалификатора `candidate_tree`/`external_head`;
   нет generic `ArtifactRef`; нет generic `retry_of`; нет authoritative
   caller-supplied actor/model/lineage.
5. Types + construction API.
6. RED matrix.
7. Contract freeze (issue #95 из DRAFT; supersede-path — единственный
   способ изменений).
8. Реализация protocol/library layer A1.
9. Production acceptance authority — только вместе с A2 canonical
   campaign log, не через временный reducer в SQLite.

Границы этапа (повторение мандата): никакого coder/reviewer runtime,
provider adapters, action broker, MG-C, изменений A0, A2-инкарнаций,
общего planner'а в этой ветке.

---

## Review round: A1 contract @ `f65b21f`

**VERDICT: CHANGES_REQUESTED** (maintainer, 2026-08-07). Шесть
P1-блокеров — все на связях между объектами, вне досягаемости
лексического чек-листа («компилятор grep не доказывает протокол»):

```text
P1-1  initial/exact input-state binding incomplete
P1-2  evidence DAG rank cannot represent legal controller→controller refs
P1-3  HumanAttention lifecycle mutates an immutable canonical artifact
P1-4  HumanCommand has duplicate idempotency identities
P1-5  replay/accepted_at ordering permits canonical-blob fork
P1-6  provider normalized-output provenance is not explicitly bound
```

Исправления внесены в `a1-contracts.md`: закрытый `InputStateBindingV1`
(InitialMaterialization | ContinuedCandidate) с явным cross-object
verification rule, всегда в `message_binding_digest` (§7.1a); rank
заменён frozen per-kind allowed-edge matrix с machine-checked
ацикличностью, rank — следствие топологической сортировки (§11);
`HumanAttentionRequestV1` — immutable OPEN + frozen transition kinds
(Acknowledged/Resolved/Superseded), production append — A2 (§8.7); одна
idempotency-поверхность `message_id`, payload `command_id`/
`idempotency_key` удалены (§8.8); заморожен acceptance construction
ordering — только winner назначает `accepted_at`, replay возвращает
существующие байты verbatim, race/crash-оракул в RED (§4.6);
`normalized_output_ref` (pre-envelope blob) обязателен в receipt,
classifier доказывает `report payload_digest == normalized_output_ref
digest` (§8.6). В §18 добавлены семантические проверки 6–8 (binding ⇒
cross-check rule; каждый digest-ref разрешён frozen DAG; никаких
in-place lifecycle).

**Provenance-пересверка ревью**: ветка отстаёт от main (~53 коммита), но
governing inputs (A0, R1, autonomy-controller, decision-and-admission)
имеют те же blob SHA на ветке и в текущем main;
`evidence-and-decision-discipline.md` изменился аддитивно (внешний
failure case; четыре правила и classifier-constraint не изменились).
Rebase для ревью не требуется; **перед FREEZE обязательна пересверка и
запись актуального main/blob identity** (rule 4), включая честную связку
frozen invariants с новым ratified invariant registry, если он к тому
моменту станет governing input.

Следующий шаг: повторный contract-only review. Types + construction API
не начинаются до его прохождения.

---

## Review round #2: A1 contract @ `a5615b8`

**VERDICT: CHANGES_REQUESTED** (maintainer, 2026-08-07). Предыдущие
шесть P1 закрыты по существу; второй проход вскрыл девять P1 на стыках
между секциями:

```text
P1-7   happy-path round нарушал собственную execution cardinality
P1-8   opaque refs у InitialMaterialization + reviewer execution
       не привязан к материализованному exact candidate
P1-9   round_binding_ref не определён вовсе
P1-10  DAG не был закрытым universe (открытые классы узлов,
       неперечисленные producer/cause-рёбра, нет per-kind
       producer mapping)
P1-11  crash-окно между idempotency claim и blob store
P1-12  transport session внутри semantic identity (дважды)
P1-13  HumanDecision source binding противоречил §11
P1-14  денормализованные antecedent-identity без equality proof
P1-15  три открытые семантики: attention transitions,
       budget predicate, per-outcome normalized_output
+      envelope artifact_refs как вторая writer-supplied
       reference surface без canonical order
```

Исправления в `a1-contracts.md`: cardinality per role chain, ровно один
usable terminal result на роль (§2.3); `RunContractCandidateStateRefV1`
/ `WorktreeMaterializationRefV1` как точные wire-типы (в принятом A0
obligation живёт внутри `RunStarted.contract.candidate_state`, evidence
— через `WorktreeCreated`), `CampaignRunBindingV1.input_state_binding` c
equality-проверкой против dispatching artifact для ОБЕИХ ролей
(§2.1/§7.1/§8.4); `coder_run_binding_ref: CampaignRunBindingRefV1` с
обязательным равенством producer binding'у CoderReport, логический
round без нового authority artifact (§8.3); закрытый universe из пяти
классов узлов + frozen per-kind producer mapping (receipt —
Controller-produced, что растворяет self-reference) + исчерпывающие
edge-sets с классами `intra`/`causal` и честной формулировкой
ацикличности — intra-подграф сортируется топологически на kind-уровне,
causal-рёбра instance-ацикличны по construction (create-before-reference
+ strictly-lower round_ordinal) (§11.1–11.4); двухфазный
ABSENT→RESERVED→COMMITTED протокол с durable `accepted_at` до
построения blob, fenced recovery и typed IN_PROGRESS для duplicate на
RESERVED, граница C6 сохранена (§4.6);
`AuthenticatedPrincipalV1`/`DeliveryObservationV1` split,
`control_session_id` удалён из canonical payload и principal-записи,
`message_id` переименован в logical/idempotency identity (§8.8);
`HumanCommandRequestRefV1 {message_id, binding_digest, blob_ref}`
(§8.8); удалены вторая coder_report-ссылка из ReviewRequest и
денормализованный reviewer-блок из ReviewVerdict, общее правило
«antecedent ref ИЛИ доказанное равенство» (§8.4); закрытые attention
transitions с terminal monotonicity и reject-on-terminal (§8.7);
детерминированный exhaustion predicate без «or» (§12.4); закрытый
per-outcome маппинг `normalized_output_ref` (§8.6); `ref_manifest` —
controller-derived exact manifest с defined dedupe/sort вместо
writer-supplied `artifact_refs` (§3). RED-матрица расширена
соответствующими оракулами.

Следующий шаг: **третий contract-only review** — по оценке maintainer'а,
после этих правок должна появиться реальная возможность дать APPROVED
FOR TYPES. Types + construction API не начинаются до вердикта.

---

## Review round #3: A1 contract @ `4f51457`

**VERDICT: CHANGES_REQUESTED** (maintainer, 2026-08-07). Оба
сознательных решения round #2 (intra/causal split; Controller-produced
receipt) ратифицированы и не откатываются. Шесть P1 — «последние места,
где ref притворяется proof»:

```text
P1-16  round_ordinal без replayable canonical authority
P1-17  ref_manifest / §11 всё ещё не exhaustive (causation не
       определён; producer/cause-рёбра пропущены; [producer]-метка
       на Controller-produced receipt; SafeRedrive evidence без типа;
       edge tags не заморожены)
P1-18  ArtifactImported противоречил §6/§11/checklist #3
P1-19  RESERVED без durable construction seed; byte contract
       (writer/recorded) не определён
P1-20  ProviderBinding допускал provenance-splice с чужим receipt
P1-21  InitialMaterialization доказывал сосуществование, не
       contract↔worktree correspondence
```

Исправления в `a1-contracts.md`: inline-пара `round_id + round_ordinal`
во всех round-scoped объектах и в binding digest, без RoundBinding
artifact (§2.3, §2.1, §3); `CausationV1 = Artifact{kind, message_id,
blob_ref} | CampaignGenesis` — causal-ребро в ref_manifest/DAG (§3);
матрица §11.3 переписана с producer/cause/ext-рёбрами, `[payload]`
вместо `[producer]` у receipt, глобальное causation-правило; закрытый
реестр stable edge tags (дисциплина §4.2) — wire semantics; 
`EstablishedNonDispatchEvidenceRefV1 {run_id, classification ∈ {absent,
valid_unsealed_pre_dispatch}, classifier_version,
classification_record_ref}` + новый CAS-kind (§11.1);
`RunArtifactSourceRefV1` как class-3 wrapper + constructor-цепочка
byte-equality для `ArtifactImportedV1` (§6); `CanonicalConstructionSeedV1`
durable до/атомарно с RESERVED (kind/lineage/causation/producer/payload
ref/ref_manifest/bindings/writer_version), seed — запись idempotency
store, не canonical artifact (C6 сохранён); `CanonicalJsonV1` (sorted
keys, no whitespace, fixed escaping, integers, ByteStringV1) +
`RecordedMetadataV1` (accepted_at = ns since epoch UTC) +
writer_version + known-answer corpus, recovery строит под writer_version
из reservation (§4.5/§4.6); `ProviderBindingV1::Provider` схлопнут до
`{invocation_receipt_ref}` — role/execution/binding/model/adapter/
prompt/tool-policy разрешаются ЧЕРЕЗ receipt; `adapter_version`
перенесён В receipt (adapter произвёл normalized bytes — provenance
больше не задом наперёд) (§3/§8.6/§8.4); `correspondence_ref` —
`worktree-correspondence-evidence-blob` от именованного verifier'а
(rule-4 запись: принятая o7-worktree attestation доказывает filesystem
identity/ownership — `attest.rs`, — а не семантическое соответствие;
поэтому bridge, вариант 2 ревью) с equality-вердиктами repo/base против
obligation (§7.1a). RED-матрица дополнена оракулами по всем шести.

Следующий шаг: **четвёртый, узко-скоуповый review** — только эти швы +
§18-чеклист, без полного архитектурного раскопа. Types + construction
API не начинаются до APPROVED FOR TYPES.
