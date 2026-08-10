# A1-V0 шаг 2: Owned CAS + resolver

> Подзадача **шага 2** из `docs/tasks/a1-v0.md` («Owned CAS + resolver —
> `immediate_refs`, проверки рангов, границы замыкания, типизированное
> разрешение ссылок»). Не новый контракт: нормативный источник прежний.

## Status

**IMPLEMENTATION TASK / CONTRACT-PRESERVING.** Как и шаг 1.

## Authority

```text
нормативный контракт   docs/q-deck/a1-authority-contracts.md
contract blob  B1      3b26849cc39a3391aaed46cca56be3b6715afabb
                       sha256:1a0739752a5a2f7b34bcbc8f2d600615f823c76ad8c3a91d603c4921c848175d

шаг 1 incorporated     a6625bc6473e3029a3309ddd7f2795ce57516a60   (PR #124)
```

Релевантные разделы: **FD-1.5** (замыкание и его границы, алгоритм заморожен как
алгоритм), **FD-1.8** (что именно идентифицирует ссылка и как проверяются обе
половины), **FD-2.1** (таблица рангов), **FD-2.4** (импортированные корни),
**FD-2.5** (обязанности resolver'а).

Контракт не выводится из кода. Если реализация упирается в контракт — §7, а не
правка по ходу.

## Четыре свойства, которые шаг 2 обязан удерживать

Это не пожелания к стилю. Каждое куплено конкретным дефектом шага 1, где
восемь раундов ревью ушли на один и тот же класс: **правило держалось на одном
маршруте и не держалось на другом.**

```text
1. Admission equivalence
   all sanctioned construction / resolution routes accept the same semantic set.

2. Equivalent representation is not equivalent provenance
   ArtifactRef is not ResolvedArtifact.

3. Proof-bearing state is minted only by the proof boundary
   tests may forge hostile inputs and hostile storage, never resolved state.

4. Accounting authority belongs to the resolver session
   resolver owns accounting state
   resolved value owns resolution evidence
   resolved value is bound to the session that paid for that evidence
   caller owns neither proof nor accounting
```

Третья строка четвёртого свойства — не педантизм. Без неё остаётся дыра, в
которой ничего не подделано:

```text
session A:  resolve(ref) -> ResolvedArtifact ; closure charged
caller:     keeps the value
session B:  reuse it — no resolution, no charge
```

Digest настоящий, байты настоящие, resolver действительно всё проверил. Ложь
появляется только в контексте использования: доказательство оторвалось от
accounting authority, при которой было получено. Отсюда нормативная форма:

> Resolution evidence must not be transferable across accounting contexts unless
> the transfer itself re-establishes and re-charges every invariant that matters.

Эта оговорка — и место, где следующий дефект попробует поселиться: «мы это уже
проверили, зачем платить дважды» звучит совершенно разумно. Поэтому она стоит
первым пунктом falsification-списка, а не сноской в конце.

## Build prerequisite

```text
Before InteractionManifestV1 can implement WireArtifact at 64 MiB,
replace the materialising document parser with bounded-during-parse admission.
The existing const gate must remain red until that is true.
```

Это уже не памятка. `WireArtifact::CEILING_IS_PARSEABLE` — const-assert,
вычисляемый внутри `parse_artifact`, а `MATERIALISING_PARSER_SAFE_MAX` равен
control-artifact максимуму. Тип с потолком выше **не компилируется**. Шаг 2 не
может забыть это условие; он может только выполнить его или быть им
заблокирован.

## Порядок срезов

Снизу вверх, как и весь A1-V0. Первый PR заканчивается там, где proof boundary
существует и доказуемо является единственным.

```text
2A  bounded JSON admission          снимает compile-time блок для 64 MiB манифеста
2B  owned CAS primitive             production write/read; hostile raw store — только в тестах
2C  resolver proof boundary         ArtifactRef -> ResolvedArtifact<'session>
2D  accounting session              bytes / objects / dedup — внутри сессии
2E  immediate_refs + rank rules     FD-1.5 union, FD-2.1 таблица
2F  closure traversal               FD-1.5 алгоритм целиком, все агрегатные границы
2G  adversarial qualification       см. ниже
```

Порядок — маршрут, а не контракт. Но 2C не может идти после 2E: если traversal
появится раньше границы, он какое-то время будет возвращать непроверенные
значения, и первый же тест зафиксирует это как норму.

## Falsification-список шага 2

Перед тем как считать срез готовым, попытаться опровергнуть его:

1. Можно ли переиспользовать `ResolvedArtifact` в другой accounting-сессии —
   через lifetime, через клон, через сериализацию, через контейнер, переживший
   сессию?
2. Существует ли способ получить proof-bearing тип, не пройдя resolver:
   `From<ArtifactRef>`, публичный конструктор, `Deserialize`, `Default`,
   test-only lever, поле, которое можно выставить?
3. Может ли caller сообщить состояние бюджета — прямо параметром или косвенно,
   через уже заряженную сессию?
4. Дедуплицируется ли замыкание по `(kind, digest)`, а не по одному digest
   (FD-2.5: одни и те же байты через два типизированных слота — два узла)?
5. Проверяется ли объект против ожидания **слота**, а не против `kind`,
   заявленного отправителем (FD-2.5)?
6. Заряжается ли *объявленный* `size` до чтения, и отвергается ли объект,
   реальный размер которого расходится с объявленным (FD-1.5, FD-1.8)?
7. Возможен ли частично принятый closure — «принято, но доказательства не
   хватает»?

**`From<ArtifactRef> for ResolvedArtifact` вынесен в список отдельно.** Это будет
самый соблазнительный API на всём шаге: выглядит естественно, компилятор
доволен, семантически — `EnvelopeV1 { producer_role: Coder }` в новом костюме.

## Тестовая дисциплина

Правило шага 1 переносится дословно и получает вторую половину:

> To test layer X, do not reach the state before X by a route that makes fewer
> guarantees than the production boundary X must uphold. Vary the input; never
> relax the bound.
>
> Forge the input, never the verdict.

Конкретно для CAS это даёт границу, которая **разрешает** отрицательные тесты
вместо того, чтобы делать их невозможными:

```text
тест МОЖЕТ подделать:        тест НЕ МОЖЕТ подделать:
  hostile raw bytes            ResolvedArtifact
  отсутствующий объект         уже заряженную сессию
  digest mismatch              pre-deduplicated closure state
  size mismatch                accounting authority
  wrong kind / media type
  запрещённое рёбро ранга
  ArtifactRef любой формы
```

Враждебное хранилище — это **другая реализация read-трейта**, а не обходной путь
записи. Она возвращает untrusted-тип и не имеет способа вернуть proof-bearing.
Поэтому resolver можно фальсифицировать по-настоящему, не заводя параллельный
production API — та самая дилемма, из-за которой шаг 1 шесть раз подряд
доказывал тестом соседнюю проверку вместо нужной.
