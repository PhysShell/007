# ADR: workflow scripting — what to take from CoStrict, what to defer

Status: accepted (design note) · Scope: proposed `o7 workflow`/`o7 script` layer ·
Last verified against `05df049`

A proposal floated giving `o7` CoStrict-style workflow scripting: strict
multi-stage workflows, skills, custom modes, slash commands, a typed
TS SDK, all wired through a capability-policy layer so scripts can't touch
the filesystem/network/shell directly. This note records what's worth
keeping, what's ahead of where the repo actually is, and the resulting v1
scope.

## The one load-bearing idea to keep

**The script proposes, the Rust host enforces.** A workflow script (TOML now,
optionally a typed SDK later) never gets `exec`/`fs`/network directly — it
calls named steps (`agent.run`, `gate.run`, `judge.*`) and the host decides
whether to run them. This matches the posture this repo already has for
`o7 run`: the worktree + `agent::DENY` deny-list is the guardrail, not the
script. Everything else in the proposal is judged against whether it adds to
this or just re-skins CoStrict's UX.

## Mapping, and what each item actually costs here

| CoStrict concept | Proposed o7 analog | Verdict |
| --- | --- | --- |
| Strict Mode (requirements → design → plan → code → test) | workflow stages: `agent.plan` → `judge.plan` → `agent.run` → `policy.checkDiff` → `gate.run` → `judge.diff` | **Partially real, partially invented.** `agent.run` → `gate.run` is `o7 run` today (`main.rs::run`, `gate.rs::GateManifest::run`). `judge.plan`/`judge.diff` **do not exist** — `judge.rs` only does read-only FP-triage of analyzer findings against one rubric/schema (`judge/fp-verdicts.schema.json`). Generalizing "judge" to arbitrary plans/diffs is new prompt templates, new schemas, unproven approach — not a rename. |
| Skills / commands | `.o7/skills/*`, slash commands, a skill.toml package format | **Defer, likely reject.** README: *"Private, personal harness."* A signed skill registry / package manager solves a multi-user distribution problem this single-user two-repo harness doesn't have. This is the same "shiny because it's modern" trap the proposal calls out for TS, recreated one layer up. |
| Custom modes | agent profiles | **Defer.** No second profile exists to generalize from yet — `run` and `judge` are two hardcoded subcommands, not N modes. |
| Tool/mode restrictions (`disableSwitchMode`) | capability policy (`shell: gate-only`, `network: off`, ...) | **Adopt the intent, correct the claim.** See gap below — it constrains the *script*, not the *agent*. |
| DAG (`depends_on`) | workflow step dependencies | **Not needed yet.** Every example workflow in the proposal (`ownnet-analyzer`, `fp-control`) is a straight line, no fork/join. Cycle detection, partial-failure/retry, and resumability are the actually-hard parts of a workflow engine — building them for a need that hasn't appeared yet is speculative generality. |
| `provider: "claude" \| "codex"` in the IR | multi-provider step config | **Ahead of the backlog.** `TODO.md` already defers claude+codex consensus explicitly ("design with real run records"). `agent.rs::Engine::Codex` exists as an enum variant; the run path for it is Phase 2, unwired. Baking provider choice into a new IR now bakes in an abstraction with one real implementation. |
| TS SDK (Deno) compiling to a `plan.json` IR | Layer 2 in the proposal | **Right shape, wrong phase.** Correct that TS should be a client emitting IR, never an engine with `exec()`. But with no TS SDK yet, the "compile to plan.json" step has nothing to compile — irrelevant until Layer 2 actually exists. |

## The gap the proposal understates

`policy.capabilities({ shell: "gate-only", network: "off", filesystem:
"worktree-only" })` reads as if it closes the same hole `docs/security-layers.md`
already documents as open. It doesn't. That policy would constrain calls the
**workflow script** makes directly — trivial to guarantee, since the API
surface simply never exposes `exec()`/`fs` to script authors. It says nothing
about what `claude` itself does once `agent::run` launches it full-auto inside
the worktree with `bypassPermissions`. That boundary — `current_dir(worktree)`
is cwd, not confinement; no syscall sandbox exists — is `security-layers.md`'s
**"sharpest present-day trust boundary,"** and it is explicitly on the
deferred list ("container egress hardening") independent of any workflow
layer. A capability-policy TOML next to a workflow script must not be read as
having solved that; it solves a much easier, unrelated problem.

## Why sequencing matters here specifically

`TODO.md`'s own backlog says `o7 run` hasn't had its **first real exercise on
an Own.NET coding task** yet. Design a 6-step DAG (`plan → judge.plan → run →
policy.checkDiff → gate → judge.diff`) on top of a single-step primitive that
has never been run for real, and the workflow layer is shaped by
CoStrict's feature list instead of by an actual run record — the same mistake
`docs/performance.md` and `docs/security-layers.md` both avoid by grounding
every design decision in what the codebase measurably does today.

## v1 scope (what to actually build, when asked)

- A flat, linear `workflow.toml` — an ordered step list, no `depends_on`.
- Steps limited to what exists: `agent.run` (wraps today's `o7 run`),
  `gate.run` (wraps `GateManifest::run`), `judge` (wraps today's FP-triage,
  unchanged scope — no invented `judge.plan`/`judge.diff`).
- `policy.checkDiff` as a new, narrow step: scope/path allow-deny list +
  changed-file/line limits against `diff.patch`, evaluated by the Rust host
  before harvest. This is genuinely new and genuinely small.
- No TS, no skills, no custom modes, no multi-provider IR, no DAG. Each of
  those gets its own trigger-gated note (matching the pattern `Cedar`/`Verus`
  already follow in `docs/security-layers.md`) once a real run record shows
  the flat-list step is insufficient.

## Bottom line

- Keep: script-proposes/host-enforces, TOML-IR-before-TS, single-provider,
  linear-steps-before-DAG.
- Cut for v1: skills packaging, custom modes, slash-command registry,
  multi-provider config, `depends_on`, and any implication that a script-level
  capability policy substitutes for agent-level sandboxing — that remains a
  separate, unsolved, already-tracked problem.
