# own-check false-positive triage — judge

You are a **read-only classifier** for `own-check`, a static ownership/lifetime
analyzer for .NET that flags possible resource leaks. For each finding below, decide
whether it is a **false positive**, a **real** issue, or **uncertain** — grounded in
the actual code. You do NOT edit anything; you only classify. Do not use any tools;
judge only from this message.

## Rubric (what counts as FP vs real)
# FP-judge rubric v0 (fills `{{rubric}}` in the `o7 judge` prompt)

Owner: **OwnAudit (domain) agent.** This is *what a verdict means*. 007 injects this
verbatim into the per-file judge prompt; it does not interpret it. Tuned in the
Phase-1 manual-proof loop.

## The judge's job
For each own-check finding in a file, given the **whole file**, classify it:
`real` (a genuine defect), `false_positive` (own-check is wrong here), or
`uncertain` (the file alone is insufficient). Always give a one-line `reason` and,
when possible, `evidence` (a line number or the specific fact).

## Standing rules (all classes)
1. **Judge only from the provided file.** Do not invent teardown you cannot see.
   *Exception:* if teardown would plausibly live in an unseen base class / partial,
   that is **`uncertain`**, not `false_positive`.
2. **Do not rubber-stamp.** own-check over-reports this residual, but a
   `false_positive` needs a concrete, citable reason. When in doubt → `uncertain`.
3. **Confidence:** high only when the deciding fact is explicit in the file; low when
   you infer object lifetime/ownership.
4. **Evidence:** for `false_positive`, cite the teardown site (`:line`) or the fact
   (static handler / owned source). For `real`, state what makes it a leak
   ("no teardown in type; instance handler roots the subscriber").

---

## Class: `subscription-leak` (OWN001) — the bulk (156)
own-check flags `event += handler` on a source of injected/unknown/static lifetime
with no matching release. After the delegate-normalization fix, the residual is
subscriptions with an **instance/capturing** handler and **no `-=` anywhere** the
checker could see.

**`real` when ALL hold:**
- the source can outlive the subscriber — an injected/parameter/DI dependency, a
  `static`/`App`-level event, or a shared bus/aggregator; AND
- the handler roots the subscriber — an **instance method**, or a lambda that
  captures `this`/an instance member/an enclosing local; AND
- **no teardown** in the type: no `-=` for this source+handler in any form, no
  `Dispose`/`Unloaded`/`Closed`/`OnClosed` that detaches, no `using`, no handoff.

**`false_positive` when ANY hold (name which):**
- **teardown exists, own-check missed the spelling** — a `-=` via an aliased/differently-
  spelled receiver, a detach inside `Closed`/`Unloaded`/a dispatcher callback, a detach
  folded into a base teardown you *can* see, source set to `null`, etc.
- **handler retains no instance** — a `static` method, or a non-capturing / static-call
  lambda (null target → roots nothing).
- **source is owned / short-lived** — constructed by this type (`_x = new …`), a local,
  or a child control the type disposes; a self-owned cycle is GC-collectable.
- **subscriber is process-lived** (the WPF `App` singleton) — the "escape" pins nothing new.

**`uncertain` when:**
- **rebinding setter** — the source is reassigned in a setter that detaches the *old*
  value each time, but the *last-assigned* source is never torn down; a leak only if
  that last source outlives the subscriber and is not owned — undecidable from the file.
  (This is the known non-flow-sensitive gap; the checker calls it released.)
- an injected source whose lifetime truly needs cross-file knowledge; or ambiguous capture.

---

## Class: `idisposable-leak` (OWN001, category 1) — 47
own-check flags a disposable field/local that is never disposed.

- **`real`:** a value of an `IDisposable` type created here (`new X()` / a factory)
  and not disposed on some path — no `Dispose`/`using`/`Close`, not handed off.
- **`false_positive`:** disposed via `using` / `Dispose()` / `Close()` / a teardown;
  **ownership transferred** (returned, added to a collection/owner that disposes it —
  e.g. `Controls.Add`, passed to a wrapper that owns it); or it wraps **managed-only
  memory** with no unmanaged handle (`MemoryStream`, `DataTable`, `Task`) where a
  missing `Dispose` is benign.
- **`uncertain`:** dispose via a helper/indirection you cannot confirm; conditional
  dispose on one path only.

---

## Class: `region-escape` (OWN014) — 7
own-check flags a subscription/capture escaping to a longer-lived (static/App) region.

- **`real`:** the capture escapes to a `static`/`App`/process-lived region and pins
  the instance, with no teardown.
- **`false_positive`:** an **intended process-lifetime hook** — `AppDomain`
  `ProcessExit`/`DomainUnload`/`UnhandledException`/`FirstChanceException`; the captured
  target is itself process-lived; or it is torn down.
- **`uncertain`:** the escape target's lifetime is unclear from the file.

---

## Output (per finding)
`class`, `confidence` (0..1), `reason` (≤1 line), `evidence` (line/fact when available)
— exactly the fields in `verdict-contract.md`. Nothing else.


NOTE: the files below are encoded as a `%q1` container. Format: first line
`%q1 <codec> ...` (parameters), then legend lines `<alias>=<phrase>` (each alias is a
short stand-in for that exact phrase), then a `%q1 body` line, then the body.
Mentally decode the body via the legend before classifying; never emit alias
characters in your output.

%q1 mine n=18
码=## File: oracle/LeakyOracle/ViewModels/
引= <see cref="
路=string[] Vocabulary = { "ACTIVE", "HALTED", "CLOSED", "PENDING"
类=WatchlistViewModel
函=_timer = new Timer(OnTick, null, 300_000, 300_000);
值=TickerViewModel
错=var status = new string(Vocabulary[i % Vocabulary.Length].ToCharArray());
试=namespace LeakyOracle.ViewModels;
件=public List<QuoteRow> Rows { get; } = new();
组=private void OnQuoteReceived(object? sender, string symbol)
标=private void OnTick(object? state) => _ticks++;
记=_service.QuoteReceived += OnQuoteReceived;
链=public sealed class
节=MarketDataService
层=(var i = 0; i < 5000; i++)
块= view-model
表= the timer
行=Rows.Add(new QuoteRow(status));
%q1 body
码类.cs
```
using System.Collections.Generic;
using LeakyOracle.Services;

试

/// <summary>
/// A per-screen块. DELIBERATELY LEAKY — two intentional smells:
///
///  • <b>Subscription leak (OWN001).</b> It subscribes to引节.QuoteReceived"/>
///    in the constructor and never unsubscribes (no <c>IDisposable</c>, no <c>-=</c>). Because the
///    service is application-scoped, every 类 ever created stays rooted through the
///    service's delegate list — open-and-close a hundred screens and a hundred块s (and their
///    row graphs) survive every GC. This is the heap-confirmable event-lifetime leak.
///
///  • <b>Duplicated strings.</b> It fills its rows with freshly-allocated copies of a tiny status
///    vocabulary (<c>new string(...)</c> never interns), so the heap holds thousands of distinct
///   引string"/> instances with identical content — the string-canonicalization target
///    (docs/string-canonicalization.md). Combined with the subscription leak, those duplicates never
///    get collected either.
/// </summary>
链 类
{
    private static readonly 路 };

    private readonly 节 _service;

    件

    public 类(节 service)
    {
        _service = service;
        记   // LEAK: never removed -> roots `this` forever

        for 层
        {
            // new string each time: same bytes, distinct reference, all retained.
            错
            行
        }
    }

    组
    {
        // The handler body is irrelevant — its mere existence in the service's invocation list is
        // what keeps this块 (and its 5000 rows) alive.
    }
}

```

码值.cs
```
using System.Threading;

试

/// <summary>
/// A per-screen块 that drives a live "ticker" off a引Timer"/>. DELIBERATELY LEAKY:
/// it starts a recurring引System.Threading.Timer"/> in the constructor and never disposes it.
///
/// An active timer is registered in the runtime's (static) TimerQueue, which holds表's callback
/// delegate — and the delegate's target is this块. So every 值 ever created stays
/// rooted by表 queue until引Timer.Dispose()"/> is called:表-lifetime leak
/// (own-check small rule; docs/wpf-audit-coverage.md, "Timers": DispatcherTimer/Timers.Timer/
/// Threading.Timer with no Stop/Dispose). The framework-agnostic core: identical on WPF and Avalonia.
///
/// The `_timer` field is deliberate — without it the public Timer wrapper would be finalized and stop
///表; holding it keeps表 alive (and the leak real), exactly as leaky code does.
/// </summary>
链 值
{
    private readonly Timer _timer;
    private int _ticks;

    public 值()
    {
        // recurring + far-future due time: registered (so it roots `this`) but won't actually fire
        // during a sub-second scenario. Never disposed -> never unlinked from the TimerQueue.
        函
    }

    标
}

```

码Fixed类.cs
```
using System;
using System.Collections.Generic;
using LeakyOracle.Services;

试

/// <summary>
/// The corrected counterpart of引类"/> — what the OWN001 fix looks like:
/// it owns its subscription and detaches on引Dispose"/>. Once disposed it is no longer
/// rooted by the service and collects normally.
///
/// This exists so the leak proof is self-validating: the same WeakReference harness that shows the
/// leaky块 surviving GC shows this one being collected. If the harness were rigged, BOTH
/// would look alive. (It also gives the future fix-arm a concrete before/after target.)
/// </summary>
链 Fixed类 : IDisposable
{
    private static readonly 路 };

    private readonly 节 _service;

    件

    public Fixed类(节 service)
    {
        _service = service;
        记

        for 层
        {
            错
            行
        }
    }

    组
    {
    }

    public void Dispose() => _service.QuoteReceived -= OnQuoteReceived;   // the fix: detach
}

```

码Fixed值.cs
```
using System;
using System.Threading;

试

/// <summary>
/// The corrected counterpart of引值"/>: it owns the引Timer"/> and
/// disposes it. Disposing unlinks表 from the runtime's TimerQueue, so once disposed this
///块 is no longer rooted and collects normally — the control case that keeps表-leak
/// proof honest (same WeakReference harness; correct code collects, leaky code doesn't).
/// </summary>
链 Fixed值 : IDisposable
{
    private readonly Timer _timer;
    private int _ticks;

    public Fixed值()
    {
        函
    }

    标

    public void Dispose() => _timer.Dispose();   // the fix: unlink from the TimerQueue
}

```

## Findings (own-check output, JSON)
[
  {
    "tool": "own-check",
    "rule": "OWN001",
    "category_name": "subscription-leak",
    "resource": "QuoteReceived subscription",
    "path": "oracle/LeakyOracle/ViewModels/WatchlistViewModel.cs",
    "line": 32,
    "message": "subscribes to MarketDataService.QuoteReceived with no matching unsubscribe",
    "suppressed": false
  },
  {
    "tool": "own-check",
    "rule": "OWN-TIMER",
    "category_name": "idisposable-leak",
    "resource": "Timer never disposed",
    "path": "oracle/LeakyOracle/ViewModels/TickerViewModel.cs",
    "line": 27,
    "message": "System.Threading.Timer created but never disposed (Stop/Dispose missing)",
    "suppressed": false
  },
  {
    "tool": "own-check",
    "path": "oracle/LeakyOracle/ViewModels/FixedWatchlistViewModel.cs",
    "line": 27,
    "rule": "OWN001",
    "category": 2,
    "category_name": "subscription-leak",
    "resource": "subscription token",
    "suppressed": false,
    "suppress_reason": "",
    "message": "event '_service.QuoteReceived' is subscribed (handler 'OnQuoteReceived') but never unsubscribed; its source is an injected dependency whose lifetime is unknown, so it may outlive and keep 'FixedWatchlistViewModel' alive (possible leak) [resource: subscription token]"
  },
  {
    "tool": "own-check",
    "path": "oracle/LeakyOracle/ViewModels/FixedTickerViewModel.cs",
    "line": 19,
    "rule": "OWN-TIMER",
    "category": 1,
    "category_name": "idisposable-leak",
    "resource": "timer",
    "suppressed": false,
    "suppress_reason": "",
    "message": "timer '_timer' (System.Threading.Timer) is created but never Stop()/Dispose()d; it stays registered in the TimerQueue and its callback 'OnTick' roots 'FixedTickerViewModel' (possible leak) [resource: timer]"
  }
]

## Your output — STRICT JSON, nothing else
Emit ONE JSON array, one object per finding above, in the same order. No prose
outside the array. Ground every `reason` in the code (cite line numbers, and note
the presence or absence of an unsubscribe `-=`, `Dispose`/`Unloaded`/`Closed`, or a
`WeakEventManager`). Put the deciding fact in `evidence`.

[
  {
    "path": "<path>",
    "line": <int>,
    "rule": "<rule>",
    "class": "real" | "false_positive" | "uncertain",
    "confidence": 0.0,
    "reason": "<one line, code-grounded>",
    "evidence": "<teardown site :line / the specific fact, or empty>"
  }
]
