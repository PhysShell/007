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
| **allowlist / closed world** | *what can be invoked at all* | ✅ `judge` — `call_claude` uses `--tools ""` + `--strict-mcp-config` (closed world; no built-in tool, no ambient MCP). ⚠️ `run` — `agent.rs::DENY` is still a **deny-list** (`Bash(rm -rf*)`, `git reset --hard`, …), which the code itself flags as "not a sandbox — command obfuscation can slip it." |
| **path confinement + `canonicalize`** | confused-deputy via *arguments* (`../../../etc/passwd`, symlinks) | ✅ `judge` — `repo.join(file).canonicalize()` then `starts_with(&repo)`, skip-with-warning. Canonicalize **before** the prefix check is load-bearing (else symlink bypass). Residual **TOCTOU** between canonicalize and open remains — only a real sandbox layer closes it, not a string check. |
| **stdin instead of argv** | argv is world-readable (`/proc/*/cmdline`, `ps`), hits shell history/logs, has a size limit | ✅ `judge` — the prompt (whole source file) goes via piped stdin, not `-p <arg>`. |
| **worktree isolation** | blast radius for *writes* | ✅ `run` — `worktree.rs` runs the agent in a throwaway `git worktree`; the main checkout is untouchable. Note: this bounds **writes to the tree**, not network egress or reads elsewhere on the box. |
| **syscall sandbox (WASI/Wasmtime, container)** | the only true deny-by-default *boundary* (capability I/O) | ❌ **absent in 007.** The agent runs under `bypassPermissions` with no syscall confinement. Wasmtime lives in the **parked, separate** sibling `sandboy` (Own.NET), not here. |
| **typed tool surface (MCP schemas)** | arg validation before execution | ❌ N/A — 007 consumes external CLIs (`claude`, `git`, `bash`); it does not publish its own MCP tools. |
| **structured audit log** | forensics / replay (not defense) | ✅ `record.rs` → `meta.json`, `diff.patch`, `agent.stdout`, `gate/*.log`. |

## The gap the layer list understates

`o7 run` executes **arbitrary `bash -lc <cmd>` from the target repo's
`.007/gate.toml`** (`gate.rs`), in the worktree. If a target repo is not fully
trusted, gate steps are attacker-controlled code execution, bounded only by the
worktree — which does **not** contain network or filesystem reads. Combined with
the agent itself running unsandboxed under `bypassPermissions`, this — not policy
engines or proof assistants — is 007's sharpest present-day trust boundary. The
real "sandbox slot" is here in `run`/gate, **not** in `judge` (already closed-world).

Decision needed (deferred until untrusted target repos are in scope): trust model
for `.007/gate.toml` and the agent — container egress hardening (already on the
deferred list) or a WASI boundary (the sandboy direction) belong here.

## Verification: buy it, don't build it

007's critical code is **glue** — orchestration, parsing, file ops. The high-ROI
tools operate on the real untrusted-input surfaces that exist today, in ascending
effort:

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
  built** — its place is `run`/gate, not `judge`.
