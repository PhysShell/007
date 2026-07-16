# ADR: security layers — what 007 has, what it doesn't, and the triggers

Status: accepted (design note) · Scope: `o7 run` + `o7 judge` · Last verified against `29f6191`

Agent security is not one mechanism — it's layers, and each layer answers a
*different* attack. The failure mode is treating "what to deny" as the whole
story. This note grounds that model in what 007 **actually is today** (a thin
orchestration binary: subprocess spawns + config parsing + file ops; deps are
`clap/serde/serde_json/toml/anyhow/sha1/sha2`; zero `unsafe`), separates real
layers from aspirational ones, and records the trigger that would make each
missing layer worth building.

## The layers, mapped to this codebase

| Layer | Answers | In 007 today |
| --- | --- | --- |
| **allowlist / closed world** | *what can be invoked at all* | ✅ `judge --provider claude` (default) — `call_claude` uses `--tools ""` + `--strict-mcp-config` (closed world; no built-in tool, no ambient MCP, no read/network/exfil path). ⚠️ `judge --provider codex` — `call_codex` runs `--sandbox read-only`: writes are denied, but **network is not** (codex has no one-flag equivalent), so a prompt-injection payload can't write yet still has a network path — the closed-world guarantee holds for the claude backend only; prefer it on untrusted source. ⚠️ `run` — `agent.rs::DENY` is still a **deny-list** (`Bash(rm -rf*)`, `git reset --hard`, …), which the code itself flags as "not a sandbox — command obfuscation can slip it." ✅ `invoke --engine claude` — same closed-world flags as `judge`'s claude path, live-verified. ⚠️ `invoke --engine codex` — adds `-c features.shell_tool=false` on top of `--sandbox read-only` (neither `judge`'s codex path nor Demand Radar's prior `codex_cli.py` combined both), but **neither flag has been exercised against a real `codex` binary** — not installed anywhere this was built. An earlier draft of `invoke.rs` described this as equivalent to claude's structural "no shell" guarantee; it was not, and callers processing untrusted content (Demand Radar's `cli.py::run`) now refuse `--engine codex` until a live install confirms the flag does what it claims — see `docs/o7-invoke.md`. |
| **path confinement + `canonicalize`** | confused-deputy via *arguments* (`../../../etc/passwd`, symlinks) | ✅ `judge` — `repo.join(file).canonicalize()` then `starts_with(&repo)`, skip-with-warning. Canonicalize **before** the prefix check is load-bearing (else symlink bypass). Residual **TOCTOU** between canonicalize and open remains — only a real sandbox layer closes it, not a string check. |
| **stdin instead of argv** | argv is world-readable (`/proc/*/cmdline`, `ps`), hits shell history/logs, has a size limit | ✅ `judge` — the prompt (whole source file) goes via piped stdin, not `-p <arg>`. |
| **worktree isolation** | cleanup + *convention*, **not** a boundary | ⚠️ `run` — `worktree.rs` runs the agent/gate with `current_dir(worktree)`. That sets the process **cwd**, which is *not* confinement: an unsandboxed command can still write outside the tree via absolute or `..` paths, read anything the user can, and reach the network. It helps well-behaved tools stay put and makes teardown a `git worktree remove` — it does **not** make the main checkout untouchable against untrusted code. |
| **syscall sandbox (Landlock+seccomp; container/VM)** | the only true deny-by-default *boundary* (capability I/O) | ❌ **absent from 007 today**, but the enforcement tool now exists: the sibling `sandboy` (Own.NET) is a Landlock + seccomp *wrap-the-child* confinement — `sandboy run --policy step.toml -- bash -lc '<step>'`, no root, no daemon, both survive `execve` so the whole process tree inherits the cage — built for exactly this `run`/gate slot. Not yet wired (the hook is a per-step `sandbox_policy` on `GateStep`). Host-*escape* resistance is still a VM (Firecracker, Layer 1); Landlock+seccomp is defense-in-depth inside it. (Earlier notes framed this row as WASI/Wasmtime — that direction was dropped; `sandboy` is Landlock+seccomp.) |
| **typed tool surface (MCP schemas)** | arg validation before execution | ❌ N/A — 007 consumes external CLIs (`claude`, `git`, `bash`); it does not publish its own MCP tools. |
| **structured audit log** | forensics / replay (not defense) | ✅ `record.rs` → `meta.json`, `diff.patch`, `agent.stdout`, `gate/*.log`. |

## The gap the layer list understates

`o7 run` executes **arbitrary `bash -lc <cmd>` from the target repo's
`.007/gate.toml`** (`gate.rs`), with `current_dir` set to the worktree. If a
target repo is not fully trusted, gate steps are attacker-controlled code
execution — and `current_dir(worktree)` is **not** a confinement boundary: a step
can write outside the tree via absolute or `..` paths, read anything the user can,
and reach the network. So the worktree bounds **neither** writes (against
adversarial paths) nor reads nor egress — it is cleanup convenience, not a
sandbox. Combined with the agent itself running unsandboxed under
`bypassPermissions`, this — not policy engines or proof assistants — is 007's
sharpest present-day trust boundary. The real "sandbox slot" is here in
`run`/gate, **not** in `judge` (already closed-world).

Decision needed (deferred until untrusted target repos are in scope): trust model
for `.007/gate.toml` and the agent — container egress hardening (already on the
deferred list) or a syscall-confinement boundary (the `sandboy` direction —
Landlock+seccomp per step today, Firecracker for true escape resistance) belong
here. See `docs/microvm-isolation.md` for a fuller assessment and phased roadmap
(policy-only first, container/gVisor prototype, microVM backend, auth broker).
The nine-field framing of this slot — Actions/Limits/Observability at the gate,
and the `sandboy` wiring contract — is in `docs/loop-canvas.md`.

## Verification: buy it, don't build it

007's critical code is **glue** — orchestration, parsing, file ops. The high-ROI
tools operate on the real untrusted-input surfaces that exist today, in ascending
effort (now wired — see `docs/verification.md` for how to run each):

- **`proptest`** — pure-function invariants: `finding_id` stability + the
  different-message-same-tuple split, `Overlay` serialize→deserialize round-trip,
  dedup (N findings → unique ids). (Unit-test seeds already exist in `judge.rs`.)
- **`cargo-fuzz`** — the three parsers of untrusted / semi-trusted input:
  `extract_json_array` (parses the **model's** output — least trusted),
  `serde_json` on `findings.json`, `toml` on `gate.toml`.
- **`Kani`** — bounded "no panic / no overflow / no UB" on small pure functions
  (`extract_json_array`, `finding_id`'s `[..16]` slice, `sanitize`). Runs on
  near-ordinary Rust — no proof-dialect rewrite. Roughly 20% of Verus's effort
  for 60% of the value.
- **`miri`** — **N/A**: no `unsafe` in the tree.

### Candidates that have NOT triggered

- **Cedar** (policy engine — Rust crate, embeddable, default-deny, `forbid`
  beats `permit`, algorithm formally verified in Lean). Right tool *when* there
  is **>1 permission profile that changes without a rebuild**. Today there are two
  modes (`run`/`judge`) with hardcoded sets — that's an `if` in code, not a PDP.
  Caveat for when it lands: Cedar is a *decision* point; the *enforcement* point
  is still our code. A "deny" is decoration if we call the tool before asking.
- **Verus** (SMT verification of Rust). Not now — there is nothing here to verify
  with leverage: the security-critical properties (symlink/TOCTOU, egress) live
  **outside** the model Verus can prove, and the rest is glue. The one door: if a
  small, pure, I/O-free stack VM for generated bytecode is ever written (§ backlog),
  a verified interpreter loop is exactly Verus's sweet spot. Closed until that
  workload knocks.

## Bottom line

- The layered model and the prioritization (Cedar = trigger-gated, Verus = no,
  Kani + fuzz + proptest = yes) are correct for 007.
- The reason is **not** "Wasmtime + Cedar already secure the stack" — neither is
  in 007. It is that 007 is glue, so property/fuzz/bounded-model testing of the
  parsers and pure functions is the whole ROI, and the sandbox layer is **not yet
  built** — its place is `run`/gate, not `judge`. The enforcement tool for that
  slot (`sandboy`, Landlock+seccomp) now exists in Own.NET but is not yet wired;
  see `docs/loop-canvas.md`.
