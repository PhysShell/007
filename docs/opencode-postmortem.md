# ADR: the OpenCode postmortem, applied to 007

Status: accepted · Scope: whole harness (007 `run`/`judge`, Own.NET, OwnAudit) ·
Companion: `docs/security-layers.md`

OpenCode shipped CVE-2026-22812 (GitHub CNA 8.8 High): before 1.0.216 it
auto-started a **local HTTP server with no authentication**, and permissive CORS
let a local process or a website execute shell commands as the user — the
advisory lists endpoints for running shell commands, creating terminal sessions,
and reading files. The CWE classes: *missing auth*, *exposed dangerous
method/function*, *permissive cross-domain policy*. The public criticism around
it also attacked the string-based permission model, file-permission guessing,
context/cache handling, and "just use Docker" as a security answer.

This note maps every claim class onto this stack, records what was verified,
and pins the invariants that keep us out of the same hole. It exists because
"we would never do that" is exactly what OpenCode thought.

## Invariants (the non-negotiables)

1. **No listening sockets, ever, without auth.** Nothing in 007, Own.NET, or
   OwnAudit binds a port today (verified — see below). If a component ever
   needs an IPC/server surface, it must be: loopback-only **and**
   authenticated (per-session token minimum) **and** CORS-less by default —
   and it lands only with an ADR. "It's only localhost" is the exact
   pre-CVE-2026-22812 reasoning; localhost is reachable by every local process
   and, via CORS/DNS-rebinding, by the browser.
2. **String filters are advisory, never load-bearing.** Shell is a language
   (aliases, env lookup, absolute paths, command substitution, heredocs,
   redirections, pipes, interpreters, indirect execution) — a pattern
   deny-list cannot bound it. `agent.rs::DENY` stays as defense-in-depth that
   spares a *well-behaved* run a destructive turn; the boundary is the OS.
3. **The boundary is the process, not the prompt.** `o7 run` executes the
   agent and every gate step inside bubblewrap (`src/sandbox.rs`): read-only
   root; tmpfs over `/home`/`/root`/`/tmp` (secrets invisible, not merely
   write-protected); worktree + its shared `.git` as the only default rw
   surface; `--clearenv` + allowlist; `--unshare-all`; network only for the
   agent profile; gate steps fully offline.
4. **No silent security downgrade.** `--sandbox auto` (default) hard-errors
   when bwrap is missing; running unconfined requires the explicit, loudly
   warned `--sandbox none`. Logs never claim confinement that isn't on
   (`Sandbox::label()`).
5. **Secrets don't enter the workspace.** Worktrees are throwaway clones of
   public repos; agent auth lives in `~/.claude*`, bound only into the agent
   process, never into gate steps; ambient env tokens die at `--clearenv`.

## The claim-by-claim map

| # | OpenCode claim class | Applies here? | Our position |
| --- | --- | --- | --- |
| 1 | prompt-cache broken by unstable context | marginal | 007 composes per-call prompts itself (`judge`), stateless and byte-stable per file; no date stamping, no mid-session context surgery. Cache economics of the `claude`/`codex` CLIs are upstream's problem, not reproduced here. |
| 2 | over-eager context pruning | no | 007 never prunes a live session; each judge call carries the whole source file it is judging. `run` hands the agent one task file and lets the CLI manage its own window. |
| 3 | compaction as fake infinite context | no — by design | The article's prescription ("write an on-disk handoff, start fresh") **is** 007's architecture: every run harvests `task.md` / `meta.json` / `diff.patch` / `agent.stdout` / `gate/*` into the private store; the next session reads records, not a summary of a summary. |
| 4 | bloated, unstable system prompts | partial | `judge/prompt.template.md` is short, versioned in-repo, and identical across calls; rubric is the domain's file. No per-model prompt forks. Keep it that way. |
| 5 | permission prompts / string filters as the security model | **was the gap** | Closed for `run`: OS boundary via bwrap (invariants 2–4). `DENY` demoted to convenience. `judge` was already closed-world (`--tools ""`, `--strict-mcp-config`; codex `--sandbox read-only` with the documented network caveat). |
| 6 | file "permissions" by guessing which commands touch files | **was the gap** | Same closure. We never parse commands to guess file access; the mount namespace decides: nothing outside the worktree is writable, `$HOME` isn't even readable. |
| 7 | unauthenticated local HTTP server (the CVE) | no | Verified 2026-07-04 by sweep over all three repos: no `TcpListener`/`HttpListener`/bind/listen/CORS surface anywhere (the only grep hits are Own.NET *analyzer corpus samples* — code under test, not code that runs). 007 is a short-lived CLI: subprocess spawns + file ops, zero server code, zero CORS. Invariant 1 keeps it so. |
| 8 | "just use Docker" as the answer | agreed | We don't punt to Docker. Confinement is native (namespaces via bwrap), per-subprocess, with nothing secret mounted in — so there is nothing inside worth exfiltrating even when the agent profile has its API network. A container that mounts `$HOME` and tokens is the same fire in a different room. |

## Residual risks (honest ledger)

- **Agent-profile network.** The agent needs the subscription API, so a
  prompt-injected agent could exfiltrate *worktree contents* (public-repo code
  + the task text — no secrets by invariant 5). Tightening = egress allowlist
  to the API host only; tracked as the deferred "container egress hardening".
- **Shared `.git` is rw.** Git-in-worktree requires the common dir (objects,
  refs, `worktrees/<n>`); a hostile run could bend refs of the *target* repo
  (recoverable via reflog; the main working tree stays untouchable).
  Tightening = bind only `objects/`, `refs/heads/o7/`, `logs/`,
  `worktrees/<n>` — do it when untrusted target repos become real.
- **Read-only root is still readable.** `/etc` and system state are visible
  (needed for toolchains/DNS). User-level secrets are blanked; system-level
  hardening (Landlock read scoping) is a later ratchet, not an MVP need.
- **`bwrap` needs user namespaces.** Fine on WSL2/modern kernels; on hosts
  where it isn't, the failure is a hard error, never a silent downgrade
  (invariant 4).
- **`judge` subprocesses are flag-confined, not namespace-confined.** Closed
  world by CLI flags today; if that ever weakens upstream, move `judge` calls
  behind the same `Sandbox` (Gate-like profile + net).

## Verification

- `src/sandbox.rs` unit tests pin the policy: gate profile has no
  `--share-net` and no agent state; tmpfs blankets precede re-binds (mount
  order is load-bearing); `--clearenv` present; worktree is the sole default
  `--bind`.
- E2E (2026-07-04, adversarial probes from *inside* the sandbox): host
  `~/.ssh` invisible; writes outside the worktree fail; `ANTHROPIC_API_KEY`
  seeded in the parent env absent inside; gate-step network dial to
  `1.1.1.1:443` fails; `git status`/`log` work in the worktree; the diff and
  gate verdicts harvest normally.
