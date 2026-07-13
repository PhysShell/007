# 007 (`o7`)

Private, personal harness that drives `claude`/`codex` (subscription auth, no API
keys) over the public repos **Own.NET** and **OwnAudit** — from the outside, via
their CLIs. Keep this repo private: subscription-auth/agent-routing code must not
land in a public tree.

## MVP — one isolated, gated run (the "unit")

```bash
o7 run --repo <path> --base <ref> --task ./task.md [--gate <toml>]
```

The loop:

1. **isolate** — `git worktree` of `<repo>` at `<base>` on a throwaway branch.
2. **run** — `claude` full-auto in the worktree (`bypassPermissions` + a hard
   deny-list on irreversible ops). No nagging; the worktree is the guardrail.
3. **gate** — run `<repo>/.007/gate.toml` steps (`bash -lc`, in order) → reduce
   to a verdict (`PASS`/`FAIL`/`ERROR`).
4. **harvest** — write the canonical record into the private store:

```text
runs/<target>/<run-id>/
  task.md        # the task given
  meta.json      # engine, model, base_commit, verdict, per-step results
  agent.stdout   # raw agent output
  diff.patch     # staged diff vs base
  gate/
    <name>.log   # each step's output
    verdict.json # per-step verdicts
```

Exit code is `0` on `PASS`, `1` otherwise — so callers/CI can gate on it.

## Setup

- Runs in **WSL2** (agents + gates execute there; worktrees on ext4).
- Target **Own.NET first** — cross-platform. OwnAudit's Windows-bound gates
  (FlaUI/ClrMD/Roslyn/VS2022) are Phase 2, tagged `env = "windows"` in the manifest.
- Each target repo carries its own `.007/gate.toml` — see `examples/gate.own.net.toml`.
- Dev env = nix flake (crane + rust-overlay) + direnv; **no system-wide Rust**. `claude`/`codex` are external (npm + subscription).
- Requires `git` (in the devShell) and `claude` on PATH (Pro/Max, logged in).

```bash
direnv allow                 # enter the nix devShell (flake.nix)
cargo generate-lockfile      # once — crane/nix build needs a committed Cargo.lock
cargo build --release        # binary: target/release/o7
cp examples/gate.own.net.toml ../Own.NET/.007/gate.toml
# nix build .#o7             # reproducible build (after Cargo.lock exists)
# nix flake check            # fmt + clippy(-Dwarnings) + build
```

## Design

Full decision record: see `../.claude` memory (`007-harness-design`). Locked MVP;
deferred (design with real run records): consensus (claude+codex race + cross-family
judge), memory layer, policy/ignore engine, container egress hardening.

Loop design (`o7 run` mapped to the nine-field loop-engineering canvas, and where
the deferred loop parts — control loop, ledger, sandbox slot — attach):
`docs/loop-canvas.md`.

Security layers (what's real, what's absent, and the triggers for
Cedar/Verus/Kani/fuzz plus the `run`/gate sandbox slot): `docs/security-layers.md`.
Zero Trust roadmap (phased plan to close that gap, cross-repo division of labor
with Own.NET/Sandboy/OwnAudit, the CUE policy-authoring decision):
`docs/zero-trust-framework.md`.
Verification harnesses (proptest/fuzz/Kani) + lints: `docs/verification.md`.
Performance (007 is subprocess-bound — the only lever is parallel judge calls):
`docs/performance.md`.
Workflow scripting (what to take from CoStrict-style strict workflows, what to
defer, and the v1 scope — flat `workflow.toml`, no DAG/skills/multi-provider
yet): `docs/workflow-scripting.md`.

Which agent-research papers are worth transplanting here vs. Own.NET (and which
are already spiked / in flight): `docs/paper-transplant-map.md`.

Imported design proposals (normalized from design discussions; all draft):
`docs/microvm-isolation.md` (microVM isolation assessment for `run`/gate),
`docs/agentic-coding-discipline-proposal.md` (pointer to the canonical Own.NET
doc), `docs/agent-memory-layer.md` (`o7 memory` / `o7 context`),
`docs/task-aware-context-generator.md` (deterministic reverse source generator for
task-specific, evidence-backed agent context),
`docs/agent-language.md` (strict TaskSpec/O7Plan contract),
`docs/agentops-promptops.md` (PromptOps/AgentOps layer),
`docs/actions-plans-evidence-abridge.md` (action-plan & evidence bridge),
`docs/architecture-refactoring-task.md` (typed arch-refactor task contract),
`docs/agents-outputs-budgeter.md` (agent output budgeter),
`docs/koma-agent-inspiration.md` (verifiable-harness positioning),
`docs/sketch-aware-evidence.md` (sketch-aware run evidence),
`docs/CFR.md` (CFR/game-theoretic scheduling survey),
`docs/fastcontext.md`, `docs/omnigraph.md`.

Working experiment: **`qodec/`** — token-aware lossless codec lab (measured
context encoding for agent payloads; design record and bench numbers in
`docs/token-codec.md`).

Sibling project (separate, in Own.NET): **sandboy** — a Landlock + seccomp
*wrap-the-child* confinement (`sandboy run --policy step.toml -- <cmd>`), the
least-privilege-per-command layer for the `run`/gate sandbox slot that
`docs/security-layers.md` marks as missing. Not yet wired into `o7`; not part of
the `007` binary.

## P.S.
o7 is for 🫡 mirrored :)
