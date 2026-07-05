# Reflect: a learning layer over run records — assessment + design (007-specific)

**Conclusion up front:** borrowing the idea behind
[`claude-reflect`](https://github.com/BayramAnnakov/claude-reflect) — turn
corrections/preferences into reviewed, persistent agent memory — is
**architecturally sound for 007**, but 007 should **not** port its mechanism.
claude-reflect's core trick is a `UserPromptSubmit` hook that regexes *live
chat text* ("no, use X not Y") to guess that a correction happened. 007 already
has something strictly better sitting in `runs/<target>/<run-id>/`: a
**structured, durable outcome record** — `task.md`, `agent.stdout`,
`diff.patch`, and machine-checked `gate/*.log` + `verdict.json` — for every
run. Guessing intent from a sentence is a worse signal than reading whether the
gate actually failed and why. `o7 reflect` should be a read-only miner over
that existing store, in the same shape as `judge`: raw artifact in, structured
JSON out, nothing written to a target repo without a human in the loop.

This is a **design proposal, not next up**. Per `TODO.md`, `o7 run` has "not
yet [been] exercised on a real coding task" and the memory layer is explicitly
on the deferred backlog behind the FP-direction gate and the STS-156 judge run.
This doc exists so the design isn't re-derived later, and so `reflect` can be
picked up as soon as (a) real `runs/` records exist and (b) the current
critical path clears — not before.

## What checks out (grounding review of the imported idea)

| Claim (from claude-reflect) | Verdict for 007 | Why |
| --- | --- | --- |
| Corrections/preferences are worth capturing as reusable memory | ✅ keep the goal | Same motivation applies to any repeat coding-agent harness. |
| A human must review before anything is written to a memory file | ✅ keep, non-negotiable | See "no silent write path" below — 007's own trust-boundary posture (`docs/security-layers.md`) already treats unreviewed writes as the sharp edge. |
| Destination can be `AGENTS.md` / `CLAUDE.md` / per-tool rule files | ✅ applicable | Own.NET already has `AGENTS.md`; the promotion target is the target repo's own convention, not 007's. |
| Warn when a memory file gets too large (~150 lines) to fight instruction bloat | ✅ good heuristic, reuse it | Same failure mode 007 already avoids by splitting `docs/security-layers.md` / `docs/performance.md` / this file instead of one giant `NOTES.md`. |
| Live `UserPromptSubmit` hook + regex correction/guardrail/positive-feedback detection on chat text | ❌ **wrong mechanism for 007** | 007 is a CLI harness (`o7 run`), not an interactive chat surface — there is no live prompt stream to hook. What 007 has instead is *post-hoc, ground-truth outcome data* (gate PASS/FAIL, a real diff, full stdout) for every run, which is a stronger signal than parsing a human's live scolding. |
| Queue is project-scoped, not global, to avoid cross-project bleed | ✅ directly reusable | Maps onto `runs/<target>/` — the store is already keyed by `target`, so scoping is free. |
| Русские паттерны live-capture are incomplete (regexes are mostly English/CJK) | N/A | Irrelevant here — 007 doesn't do live prompt capture at all (previous row). |

## The load-bearing correction

**claude-reflect's hardest problem — "did a correction actually happen?" — is
already solved for 007, for free, by the gate.** A live-prompt regex has to
guess at intent from a sentence fragment. 007 instead has:

- a **binary, machine-checked outcome** per run (`Verdict::{Pass,Fail,Error}`,
  `src/verdict.rs`),
- **which files were touched** (`diff.patch`),
- **what was asked** (`task.md`),
- **what the agent said it did** (`agent.stdout`),
- **why a gate step failed, verbatim** (`gate/<name>.log`).

So `reflect`'s extraction problem is not "detect a correction in a sentence",
it's "given a FAIL verdict + a diff + a gate log, what rule would have
prevented it, or what would a human need to have told the agent up front?" —
a strictly easier, better-grounded question than the one claude-reflect
answers. This is the same shape of correction the profiling assessment made
about `agent.stdout` not being a trace (`docs/agent-behavior-profiling.md`):
import the goal, replace the mechanism with what 007's own artifacts actually
support.

## Design

### Learning categories (with provenance, not vibes)

Every candidate carries the evidence it was mined from — no candidate without
a `run_id` and a byte range/log name to point at:

```jsonc
{
  "id": "ownnet-2026-07-05-001",
  "target": "own.net",
  "kind": "gate_rule",
  "status": "pending",           // pending | accepted | edited | rejected
  "confidence": 0.9,
  "evidence": {
    "run_id": "1720000000-12345",
    "files": ["gate/regression.log", "diff.patch"]
  },
  "proposal": "When touching ownlang/codegen.py, run tests/test_codegen_props.py 3000 1234 before finalizing.",
  "destination_hint": "AGENTS.md"
}
```

Kinds, each with a distinct, mechanical extraction rule over the record:

| Kind | Extracted from | Example |
| --- | --- | --- |
| `gate_rule` | FAIL verdict + which files the diff touched that a specific `required` gate step then failed on | "when changing X, run Y first" |
| `anti_pattern` | Repeated FAIL on the same gate step across runs for the same target | "codegen changes without golden update fail regression — see gate log pattern" |
| `workflow` | Repeated structural similarity across multiple `task.md` for one target (same intent, different day) | candidate for a reusable task template — **not** a claude-reflect-style auto-generated slash command; 007 only surfaces the pattern, the target repo's own tooling decides what a command means there |
| `domain_fact` | Stable facts pulled from `agent.stdout` that later gate runs corroborate (e.g. a path, a command invocation that consistently works) | "regression entrypoint is `python tests/run_tests.py`" |
| `security_policy` | A `DENY`-adjacent near-miss: agent attempted (and was blocked by) a listed-dangerous op, or a gate step needed a policy not yet in `.007/gate.toml`/sandboy policy | candidate for `agent.rs::DENY` or a sandboy `policy.toml` tightening |

`workflow` and `domain_fact` are the two kinds most likely to be **noise** —
flag them lower-confidence by construction (fewer runs corroborating = lower
score) rather than trying to be clever about NLP-classifying `task.md` prose.

### Storage: private, queued, never auto-applied

```text
runs/<target>/<run-id>/              # unchanged — the source of truth
runs/<target>/reflect/
  queue.jsonl                        # append-only candidates, one per line
  promotions/<id>/promotion.patch    # a reviewed candidate turned into a diff
```

`queue.jsonl` and `runs/` stay in 007's private store — same rule 007 already
applies to itself (`README.md`: "subscription-auth/agent-routing code must not
land in a public tree"). The parallel here: 007's *reasoning about* a target
repo (which runs, which failures, which candidates were rejected and why)
never lands in the target repo either. Only a reviewed, human-approved
**promotion patch** does — and it lands as an ordinary diff a human applies or
opens as a PR themselves, through the target repo's normal review, not a
direct write from `o7`.

### No silent write path (the non-negotiable)

Three separate, explicit steps, matching how `judge` already separates
"produce a verdict" from "a human/report decides what to do with it":

```bash
o7 reflect --target own.net --dry-run          # mine runs/, print candidates, write nothing
o7 reflect --target own.net --review           # interactive accept/edit/skip over queue.jsonl
o7 reflect --target own.net --promote <id> \
           --dest AGENTS.md --out promotion.patch   # turns one *accepted* candidate into a patch
```

`--promote` never writes inside the target repo's working tree directly —
same posture as `run`'s worktree: it emits `diff.patch`-shaped output for a
human (or the target repo's own PR flow) to apply. This is the answer to the
"self-learning system writes its own hallucination into granite" failure mode
the source review calls out: the write boundary is a patch file, reviewed like
any other diff, not a live filesystem mutation from inside `o7`.

### Explicitly not building

- **No live prompt-capture hook.** 007 is not a chat surface; there is nothing
  to hook. (See "load-bearing correction" above.)
- **No auto-write to any target repo's memory file.** Promotion always stops
  at a patch; applying it is a separate, human act.
- **No cross-target bleed.** `runs/<target>/reflect/` is scoped exactly like
  `runs/<target>/` already is.
- **No NLP correction-classifier.** Extraction rules are mechanical
  (verdict + touched-files + gate-log grep), not sentiment/regex guessing —
  deliberately narrower than claude-reflect's `detect_patterns`, because 007
  has better signal available and shouldn't reinvent a worse one.
- **No slash-command/skill generation.** `workflow` candidates are surfaced as
  data; turning a recurring task into a reusable command is the target repo's
  (or the user's own tooling's) call, not 007's.

## Sequencing

1. **Prerequisite (blocking, not optional):** at least one real `o7 run`
   record against Own.NET must exist (`TODO.md`'s own next milestone). Mining
   synthetic/example runs would tune extraction rules against fiction.
2. **Critical path first:** FP-direction gate + the STS-156 judge run stay
   ahead of this, per `TODO.md`. Reflect is backlog, not next.
3. **Then, MVP:** `o7 reflect --dry-run` — read `runs/<target>/*/meta.json` +
   `gate/*.log` + `diff.patch`, emit `gate_rule`/`anti_pattern` candidates only
   (the two kinds with mechanical, verdict-driven extraction and no
   cross-run corroboration needed).
4. **Phase 2:** `--review` interactive queue + `queue.jsonl` persistence;
   `domain_fact`/`workflow` kinds, which need corroboration across ≥N runs to
   avoid one-off noise.
5. **Phase 3:** `--promote` → `promotion.patch`. Wire into Own.NET's memory
   layer once that side exists (see the companion proposal in
   `PhysShell/Own.NET`, `docs/proposals/P-026-agent-memory-layer.md`, which
   defines the destination shape — `.agents/*.md` + `AGENTS.md` as index —
   this doc's `promotion.patch` targets).
6. **Later:** `security_policy` kind feeding `agent.rs::DENY` / sandboy
   `policy.toml` tightening — gate behind sandboy actually being wired into
   `run` (still a spike per `sandboy/README.md` in Own.NET).

## Verdict scorecard

| Axis | Score | Note |
| --- | --- | --- |
| Factual grounding (source idea) | 9/10 | claude-reflect's README/hooks were read directly; the queue/review/dedupe shape is real and worth borrowing. |
| Architectural fit for 007 | 8/10 | `runs/` already gives reflect its raw material for free; the `judge`-shaped read-only-miner pattern fits cleanly. |
| Mechanism accuracy | 4/10 → corrected | The imported live-hook + regex-correction mechanism does not fit 007 at all; replaced with verdict/diff/gate-log mining, which is strictly stronger signal for this harness. |
| Timing / priority | 2/10 | Explicitly backlog: gated behind a first real `o7 run` and the current FP-gate/STS critical path. |

**Bottom line:** good idea, wrong imported mechanism, right one is already
implied by `record.rs`. Land it after real run records exist and the current
critical path clears — this doc is the design so that's a start, not a
re-derivation.

## References

- Source idea: [`claude-reflect`](https://github.com/BayramAnnakov/claude-reflect)
  (hooks: `capture_learning.py`, `check_learnings.py`; commands: `/reflect`,
  `/reflect-skills`).
- Code touchpoints: `src/record.rs` (`RunRecord`/`RunMeta` — the mining
  source), `src/judge.rs` (the read-only-miner pattern to mirror), `src/gate.rs`
  (per-step logs `reflect` reads), `src/agent.rs::DENY` (a `security_policy`
  promotion target), `TODO.md` (current critical path — this is behind it),
  `README.md` (memory layer already listed as deferred).
- Companion doc: `PhysShell/Own.NET` — `docs/proposals/P-026-agent-memory-layer.md`
  (the public, reviewed destination this reflect design promotes into).
