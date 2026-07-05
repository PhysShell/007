# Agent behavior profiling — assessment (007-specific)

**Conclusion up front:** the proposal (borrow Act·onomy-style behavioral
observability and add an `o7 profile` sibling command over run records) is
**architecturally sound and factually well-grounded** — but it rests on one
false premise, and it is **off the current critical path**. Ship the missing
foundation (a real trace) first, or the whole layer is a PDF.

Source of the idea: *How to Interpret Agent Behavior* (arXiv:2605.13625),
which introduces **Act·onomy** — a three-level hierarchy of 10 actions, 46
subactions, 120 leaf categories for runtime agent behavior. The paper is real
and the numbers are as quoted.

## What checks out (grounding review)

The proposal was checked against the tree, not taken on faith. Almost every
concrete claim holds:

| Claim | Verdict | Evidence |
| --- | --- | --- |
| Paper exists; taxonomy = 10 / 46 / 120 | ✅ | arXiv:2605.13625 |
| `agent.rs` runs claude `--output-format json`; TODO to parse `session_id`/`total_cost_usd` | ✅ | `src/agent.rs:77`, `src/agent.rs:92` |
| Record layout: `task.md`, `meta.json`, `agent.stdout`, `diff.patch`, `gate/*.log`, `verdict.json` | ✅ | `README.md`, `src/record.rs` |
| `RunMeta` is serde-versioned; optional/Phase-2 fields skip when empty | ✅ | `src/record.rs:26-34` |
| judge is the right pattern template: counts `files_skipped`/`findings_malformed`, PARTIAL overlay exits non-zero, raw output kept | ✅ | `src/judge.rs:110`, `src/judge.rs:231-236` |
| Verdict reduces to PASS/FAIL/ERROR | ✅ | `src/verdict.rs` |
| consensus / memory / policy are deferred | ✅ | `README.md:58`, `TODO.md:52-55` |

Architectural instincts are also right and should be kept:

- **Do not** stuff behavior labels into `meta.json` — keep `behavior.json` /
  `behavior.md` as separate record files; `meta.json` gets at most a summary +
  flag list.
- Build it as a **read-only, post-processing sibling command** modeled on
  `judge` (raw artifact → classify → structured JSON → markdown report), not a
  new framework.
- Keep a small **007-specific** taxonomy: top levels close to Act·onomy,
  leaf categories domain-specific to a coding-agent harness.
- Require **evidence spans** so a profile can't be an LLM fabrication.
- Do **not** resurrect the deferred memory layer through a side door.

## The load-bearing correction

**`agent.stdout` under `--output-format json` is NOT a turn-by-turn trace.**

`claude -p --output-format json` emits a **single final result object**
(result text + `session_id` + `total_cost_usd` + usage) — not the stream of
thoughts and tool calls. That is exactly why the TODO at `src/agent.rs:92`
only mentions `session_id` + `total_cost_usd`: that is all the final JSON
carries.

Consequences the proposal missed:

- The proposed **Phase-1 "deterministic trace extraction from `agent.stdout`"**
  — a `TraceEvent { turn, Thought/ToolCall/ToolResult/Final }` stream — **cannot
  be built from the current output.** There are no turns and no tool calls in a
  final result object.
- To get a real trace, `agent.rs` must first switch to
  `--output-format stream-json --verbose` and persist the message stream
  (e.g. `agent.trace.jsonl` alongside `agent.stdout`). **This is the actual
  prerequisite for everything else.**

### What works today vs what needs the trace

| Signal / detector | Buildable now? | Needs |
| --- | --- | --- |
| `GATE_FAILED`, gate-log presence | ✅ | `gate/*.log`, `verdict.json` |
| diff size / over- or under-diff | ✅ | `diff.patch` |
| final cost / session id | ✅ (after json parse) | final result JSON |
| `AGENT_LOOP` (same file searched > N times) | ❌ | tool-call stream |
| `PATCH_WITHOUT_LOCALIZATION` (edit before retrieval) | ❌ | tool-call stream |
| `NO_GATE_REFLECTION` (never inspected gate log) | ❌ | tool-call stream |
| behavior distribution (Retrieval/Reasoning/Executing %) | ❌ | tool-call stream |

The most valuable failure-mode detectors — the whole reason the feature is
interesting — are in the ❌ rows. Without `stream-json` the layer degrades to a
few crude diff/gate heuristics.

## Off the critical path

Even with the trace fixed, this is backlog, not next:

- **There are zero real run records to profile.** `TODO.md:53` is explicit:
  `o7 run` has "not yet exercised on a real coding task." Tuning behavioral
  heuristics against synthetic data is premature.
- The documented next step (`TODO.md` "▶ RESUME HERE") is the **FP-direction
  gate** and the **STS-156 judge run** — a different lane entirely.
- The headline `o7 compare` (claude vs codex) is **dead on arrival**: the codex
  engine `bail!`s "Phase 2 — not wired yet" (`src/agent.rs:61`). Nothing to
  compare until a second engine exists.

## Recommended sequencing

1. **Prerequisite (small, do this regardless):** switch `agent.rs` to
   `--output-format stream-json --verbose`; persist `agent.trace.jsonl`; parse
   the final result for `session_id` + `cost_usd` into `RunMeta` (closes the
   `src/agent.rs:92` TODO). This is the missing foundation and is independently
   useful.
2. **Critical path first:** finish the FP-direction gate and the STS run per
   `TODO.md`; get at least one real `o7 run` record on the ground.
3. **Then Phase 1 (deterministic):** `o7 profile --run <dir> --no-llm` over the
   real trace + `diff.patch` + gate logs → `behavior.json` with the
   trace-derived stats and the crude flags.
4. **Phase 2 (heuristic labels):** map tool calls → 007-local taxonomy leaves;
   the trace-dependent detectors become buildable here.
5. **Phase 3 (one LLM summarizer):** a single per-run call, reusing judge's
   discipline — raw output saved, malformed counted, partial never passed off
   as complete, evidence spans mandatory.
6. **Later:** `o7 compare` once codex is wired; HTML report; automated codebook
   extension; anything memory-flavored.

## Verdict scorecard

| Axis | Score | Note |
| --- | --- | --- |
| Factual grounding | 9/10 | Actually read the repo; paper is real |
| Architectural fit | 8/10 | Respects invariants; sibling-command shape is right |
| Technical accuracy | 5/10 | `stdout` ≠ trace collapses Phase 1 + best detectors |
| Timing / priority | 3/10 | Backlog over nonexistent data; compare unbuildable |

**Bottom line:** good idea, honest architecture, one wrong assumption at the
base, and the wrong week to build it. Land the `stream-json` trace as
groundwork; keep the profiling layer parked behind the FP gate and the first
real run records.

## References

- Paper: *How to Interpret Agent Behavior*, arXiv:2605.13625 (Act·onomy;
  tools MIT, codebook/datasets CC BY 4.0).
- Code touchpoints: `src/agent.rs` (engine + output format), `src/record.rs`
  (`RunMeta`, record files), `src/judge.rs` (the pattern to mirror),
  `src/verdict.rs` (reduce), `TODO.md` (current critical path).
