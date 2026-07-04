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
| **allowlist / closed world** | *what can be invoked at all* | ✅ `judge --provider claude` (default) — `call_claude` uses `--tools ""` + `--strict-mcp-config` (closed world; no built-in tool, no ambient MCP, no read/network/exfil path). ⚠️ `judge --provider codex` — `call_codex` runs `--sandbox read-only`: writes are denied, but **network is not** (codex has no one-flag equivalent), so a prompt-injection payload can't write yet still has a network path — the closed-world guarantee holds for the claude backend only; prefer it on untrusted source. ✅ `run` — `agent.rs::DENY` remains a **deny-list** ("not a sandbox — command obfuscation can slip it") but is now defense-in-depth *behind* the OS boundary in `sandbox.rs`, not the boundary itself. |
| **path confinement + `canonicalize`** | confused-deputy via *arguments* (`../../../etc/passwd`, symlinks) | ✅ `judge` — `repo.join(file).canonicalize()` then `starts_with(&repo)`, skip-with-warning. Canonicalize **before** the prefix check is load-bearing (else symlink bypass). Residual **TOCTOU** between canonicalize and open remains — only a real sandbox layer closes it, not a string check. |
| **stdin instead of argv** | argv is world-readable (`/proc/*/cmdline`, `ps`), hits shell history/logs, has a size limit | ✅ `judge` — the prompt (whole source file) goes via piped stdin, not `-p <arg>`. |
| **worktree isolation** | cleanup + *convention*, **not** a boundary | ⚠️ `run` — `worktree.rs` runs the agent/gate with `current_dir(worktree)`. That sets the process **cwd**, which is *not* confinement: an unsandboxed command can still write outside the tree via absolute or `..` paths, read anything the user can, and reach the network. It helps well-behaved tools stay put and makes teardown a `git worktree remove` — it does **not** make the main checkout untouchable against untrusted code. The property it *implies* is delivered by the sandbox layer below, which mounts the worktree as the only writable surface. |
| **syscall sandbox (namespaces / WASI / container)** | the only true deny-by-default *boundary* (capability I/O) | ✅ `run` — `sandbox.rs` wraps the agent **and every gate step** in bubblewrap: read-only root, tmpfs over `/home`/`/root`/`/tmp` (secrets invisible), worktree + shared `.git` as the only rw surface, `--clearenv` + env allowlist, `--unshare-all` (network re-shared for the agent profile only; gate steps offline). Default `--sandbox auto` **hard-errors** without bwrap — opting out is explicit (`--sandbox none`) and loudly warned. Residual ledger: `docs/opencode-postmortem.md`. Wasmtime still lives in the **parked, separate** sibling `sandboy` (Own.NET), not here. |
| **typed tool surface (MCP schemas)** | arg validation before execution | ❌ N/A — 007 consumes external CLIs (`claude`, `git`, `bash`); it does not publish its own MCP tools. |
| **structured audit log** | forensics / replay (not defense) | ✅ `record.rs` → `meta.json`, `diff.patch`, `agent.stdout`, `gate/*.log`. |

## The gap the layer list understated — now closed

`o7 run` executes **arbitrary `bash -lc <cmd>` from the target repo's
`.007/gate.toml`** (`gate.rs`). Historically that ran with only
`current_dir(worktree)` — which is *not* a confinement boundary — so an
untrusted target repo meant attacker-controlled code execution with the user's
full read/write/network reach. That was 007's sharpest trust boundary, and the
OpenCode CVE fallout (`docs/opencode-postmortem.md`) is what pulled the fix
forward: the "sandbox slot" is now **filled** by `sandbox.rs`. Gate steps run
offline in the bwrap boundary (code execution ≠ exfiltration: no `$HOME`, no
env tokens, no network); the agent runs in the same boundary with only the API
network re-shared. `current_dir(worktree)` is back to being what it always
was — a convention, now backed by a namespace that makes it true.

Still deferred, with triggers recorded in the postmortem's residual ledger:
egress allowlisting for the agent profile (container egress hardening) and
fine-grained `.git` binds once untrusted target repos are actually in scope.

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
  parsers and pure functions is the whole ROI. The sandbox layer is **built**
  where it belongs — `run`/gate (`sandbox.rs`, bubblewrap) — while `judge` stays
  closed-world by construction; what remains open is the residual ledger in
  `docs/opencode-postmortem.md` (agent egress allowlist, fine-grained `.git`
  binds), each with its trigger recorded.
