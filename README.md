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
2. **run** — `claude` full-auto in the worktree (`bypassPermissions` + a
   deny-list on irreversible ops), inside an OS sandbox (bubblewrap): read-only
   root, `$HOME` blanked (secrets invisible), env cleared, worktree the only
   writable surface, network shared for the API only. No nagging; the
   *namespace* is the guardrail, the worktree is the blast radius.
3. **gate** — run `<repo>/.007/gate.toml` steps (`bash -lc`, in order) in the
   same sandbox **minus network** (a hostile manifest can run code, not
   exfiltrate or read secrets) → reduce to a verdict (`PASS`/`FAIL`/`ERROR`).
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

Sandbox knobs: `--sandbox auto|bwrap|none` (default `auto` — **hard error** if
`bwrap` is missing; `none` is explicit and loudly warned, never a silent
fallback), `--sandbox-ro <path>` / `--sandbox-rw <path>` (repeatable) to bind a
toolchain prefix or cache the `/home` blanket hides. Policy + residual-risk
ledger: `docs/opencode-postmortem.md`.

## Setup

- Runs in **WSL2** (agents + gates execute there; worktrees on ext4).
- Target **Own.NET first** — cross-platform. OwnAudit's Windows-bound gates
  (FlaUI/ClrMD/Roslyn/VS2022) are Phase 2, tagged `env = "windows"` in the manifest.
- Each target repo carries its own `.007/gate.toml` — see `examples/gate.own.net.toml`.
- Dev env = nix flake (crane + rust-overlay) + direnv; **no system-wide Rust**. `claude`/`codex` are external (npm + subscription).
- Requires `git` + `bwrap` (both in the devShell) and `claude` on PATH (Pro/Max,
  logged in). `bwrap` needs unprivileged user namespaces — standard on WSL2.

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

Security layers (what's real, what's absent, and the triggers for
Cedar/Verus/Kani/fuzz; the `run`/gate sandbox slot is now filled by
`src/sandbox.rs`): `docs/security-layers.md`. Why the sandbox is a hard
boundary and not string filters — the OpenCode CVE postmortem mapped onto this
stack: `docs/opencode-postmortem.md`.
Verification harnesses (proptest/fuzz/Kani) + lints: `docs/verification.md`.
Performance (007 is subprocess-bound — the only lever is parallel judge calls):
`docs/performance.md`.

Sibling project (parked, separate): **sandboy** — WASM/WIT plugin surface, lives
in Own.NET. Not part of `007`.

## P.S.
o7 is for 🫡 mirrored :)
