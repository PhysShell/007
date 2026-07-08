# Agentic Coding Discipline — Proposal (pointer)

> Статус: **pointer**. Канонический текст этого предложения (task contracts,
> negative prompts, plan-then-build, diff policy gates, judge-run, trust levels
> — для Own.NET/OwnAudit/007) живёт в публичном репо Own.NET:
> `Own.NET/docs/agentic-coding-discipline-proposal.md`
> (https://github.com/PhysShell/Own.NET/blob/main/docs/agentic-coding-discipline-proposal.md).
>
> Раньше идентичная копия лежала в каждом из трёх репозиториев; копии
> неизбежно расходятся, поэтому здесь оставлен только указатель.

Что из этого предложения уже реализовано в 007: цикл `isolate → run → gate → harvest`
(`o7 run`), `o7 judge` с machine-readable verdict (`judge/fp-verdicts.schema.json`).
Открытые пункты для 007: машиночитаемый `task.o7.toml` / `TaskContract` + `DiffPolicy`
gate, `o7 plan` / `o7 judge-run` / `o7 replay`, `trust_level`.
