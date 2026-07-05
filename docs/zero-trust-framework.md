# Zero Trust framework for 007 — roadmap and cross-repo methodology

Status: design note / roadmap (not an implementation commitment) · Companion to
[`security-layers.md`](security-layers.md) (accepted ADR: what's real today) and
[`../TODO.md`](../TODO.md) (backlog). This note answers a different question than
the ADR: not "what does 007 have today" but "what does 007 need to grow into, and
in what order" — using Anthropic's Zero Trust framework for agentic AI as the
external reference model, and recording two follow-on decisions (policy authoring
language, WIT/WASM scope) that came out of applying it.

## 0. Why this applies to 007 specifically

Anthropic's framework treats agents as a different threat class from ordinary
software because they choose their own steps, call tools/APIs/filesystems, hold
context across a run, and can chain with other agents — so "it's sandboxed inside
our project" stops being sufficient. 007 is exactly that shape: `o7 run` drives
`claude`/`codex` full-auto over a target repo, and `security-layers.md` already
says plainly that the one present-day gap is real — `current_dir(worktree)` is
not confinement, and `.007/gate.toml` executes arbitrary `bash -lc` from the
target repo. That ADR is the honest diagnosis. This note is the roadmap for
closing the gap, in the order the framework argues actually matters.

## 1. The framework's core claim, applied

Controls should make an attack **impossible**, not merely **tedious**. A
deny-list (`agent.rs::DENY`) or a string-prefix check makes bad things annoying;
only a capability that does not exist (no network stack in the sandbox, no route
to a credential) makes them impossible. `security-layers.md` already draws this
line for 007 correctly — the priority order is:

1. deny-by-default execution for gate steps
2. network off by default
3. write confined to worktree/output dirs
4. read allowlist
5. structured audit of every process spawn
6. **then** policy engines (Cedar/ABAC) — otherwise policy is bureaucratic
   theatre on top of a floor with holes in it

This note doesn't change that order. It turns it into phases with concrete
artifacts, and answers the two design questions the ADR left open (policy
authoring format, WIT/WASM's actual scope).

## 2. Cross-repo division of responsibility

None of this is 007-only. The pieces already exist, split across three repos —
this table is the map that should stop each repo from quietly re-inventing the
others' half:

| Concern | Owner | Artifact |
|---|---|---|
| Declares the capability/policy vocabulary, verifies static contracts | **Own.NET** | [`docs/notes/agent-capability-layer.md`](../../own.net/docs/notes/agent-capability-layer.md) ("Owen Gate") |
| Process/syscall isolation boundary (the actual sandbox) | **Own.NET** (`sandboy/`, sibling project, not core) | [`sandboy/README.md`](../../own.net/sandboy/README.md), [`docs/notes/sandboy-isolation-adr.md`](../../own.net/docs/notes/sandboy-isolation-adr.md) |
| Orchestrates the run, enforces policy at runtime, emits the audit trail | **007** (this repo) | `gate.rs`, `record.rs`, this doc |
| Ingests the audit trail as evidence, triages, reports | **OwnAudit** | [`docs/agent-run-triage.md`](https://github.com/PhysShell/OwnAudit/blob/main/docs/agent-run-triage.md) |

Own.NET verifies contracts · 007 enforces them at runtime · Sandboy provides the
isolation boundary · OwnAudit consumes the resulting evidence. None of these
should grow a second copy of another's job — that discipline already exists in
this branch (`AGENTS.execution-surfaces.md` in Own.NET draws the same line for
rule engines) and applies here too.

## 3. Phased roadmap (foundation tier — not the enterprise version)

Scaled to what this actually is: a personal harness, N=1 user, no compliance
audience. No HSM/attestation, no full ABAC engine, no SIEM, no governance
committee — see §6. What's worth building, in order:

### Phase 1 — 007 hardening (this repo)

| Item | Shape | Status |
|---|---|---|
| Run identity | `run_id`, `agent_id`, `target_repo`, `base_commit`, `policy_id` as first-class fields, not just fields inside `meta.json` | partially there (`meta.json` exists; not yet a named, referenced identity) |
| Capability manifest | `.007/gate.toml` gains a `[permissions]` block (network/read/write/process-spawn/destructive-git), not just step commands | not built — §4 |
| Sandbox backend | wrap every gate step through Sandboy instead of bare `bash -lc` | **spiked, not wired** — `sandboy run --policy step.toml -- <cmd>` exists in Own.NET; `gate.rs` still calls `Command::new("bash")` directly |
| Egress off by default | no network unless a step's policy allows it | depends on the Sandboy wiring above |
| Output filtering | scan `agent.stdout`/`diff.patch`/gate logs for tokens/keys/`.env`/PATs before they land in the run record | not built |
| Config integrity | `.007/gate.toml` (or its successor) is versioned, reviewable, and the runtime fails closed on an unknown/malformed policy field | partial — `GateManifest` currently *tolerates* unknown fields on purpose (forward-compat for step commands); a `[permissions]` block should **not** inherit that leniency, since silently ignoring an unrecognized permission key is a fail-open bug, not a compat feature |

### Phase 2 — Own.NET contract layer

Already scoped in `agent-capability-layer.md` (§3, "Owen Gate"): `owen.policy`
as the canonical policy source, capability vocabulary (`repo.read`, `exec`,
`network`), `owen policy check/explain/gen-ignore`. This note's contribution is
narrow: the authoring *language* for that policy (§4 below).

### Phase 3 — sandboy

Already an accepted ADR in Own.NET (`sandboy-isolation-adr.md`): Landlock +
seccomp wrap-the-child (Layer 2, spiked), Firecracker/gVisor as Layer 1 gated on
an untrusted target repo entering scope, netns + blanket-UDP-block as Layer 3.
Nothing new to decide here — 007's job is to actually call it from `gate.rs`.

### Phase 4 — OwnAudit as the evidence sink (agentic triage, not a SOC)

Not a real SOAR, not Splunk-for-one-person. A `007 run failed suspiciously` event
becomes a structured incident note an agent can draft (read-only: logs, diff,
verdict) — but containment/rerun/escalate decisions stay a human call, per the
framework's own division between automatable evidence-collection and
non-automatable disposition. Detailed in OwnAudit's `agent-run-triage.md`; 007's
only obligation is to emit the structured events that make it possible.

## 4. Capability manifest — sketch

Extends `GateManifest` (`src/gate.rs`) with a permissions block per gate file,
not per step (a step-level override would recreate the copy-paste risk §5 exists
to prevent):

```toml
# .007/gate.toml — illustrative; the authored source of truth is CUE (§5)
schema = 2

[permissions]
network = "deny"
write   = ["$WORKTREE", "$RUN_DIR"]
read    = ["$WORKTREE"]
process_spawn   = ["python", "dotnet", "cargo", "git"]
destructive_git = "deny"

[[gate]]
name = "regression"
cmd  = "python tests/run_tests.py"
required = true
```

The runtime consumer (`GateManifest::parse`) should treat `[permissions]` with
`deny_unknown_fields` — unlike the step list, an unrecognized permission key must
be a hard parse error, not a silently-ignored field. A policy engine is only as
strong as its failure mode on the field it doesn't recognize yet.

## 5. Policy authoring language: CUE, not TOML

TOML is fine for "three fields and a list" (today's `[[gate]]` steps). It stops
being fine once the manifest needs `no-net`, `worktree-only`, `windows-gate`,
`trusted-repo` / `untrusted-repo` profiles that **compose** — "inherit the base,
add these steps" — because TOML has no merge semantics: composing configs means
hand-copying, and a copy that forgets `network = "deny"` is the whole class of
bug this framework exists to prevent.

**Decision: author policy in [CUE](https://cuelang.org), compile to a flat JSON
artifact for the runtime.**

CUE's unification model is the reason it wins over plain "config with
inheritance": in CUE, a child and a parent don't override each other, they
*unify* — and unification fails loudly if the child disagrees with the parent
(`network: "deny"` in the base + `network: "allow"` in a leaf is a **compile
error**, not a silent merge). That is exactly the property a security floor
needs: `no-net` should be concrete, not a default someone can quietly shadow.

```text
.007/
  policies/
    no-net.cue           # network: "deny" — the floor
    worktree-only.cue    # read/write confined to $WORKTREE, $RUN_DIR
    default-processes.cue
  gates/
    own-net.cue           # target: "Own.NET"; unifies the policies above + steps
    own-net.windows.cue   # env: "windows" — must explicitly opt into a different
                           # process policy (e.g. allow powershell); can't silently
                           # inherit a denylist that forgot it exists
  gate.lock.json           # compiled artifact — what `o7 run` actually reads
```

Authoring pipeline: `o7 policy compile .007/gates/own-net.cue > .007/gate.lock.json`,
committed like a lockfile. **The runtime parser stays dumb on purpose** — a
`serde_json` + strict Rust schema over the compiled JSON, no CUE evaluation at
run time. Scarcity of moving parts at the enforcement point is the point: a
security-critical parser should be boring, not clever.

Runner-up: [Nickel](https://nickel-lang.org) — has `import` + record merge (`&`)
and typed contracts, closer to "config as a real language" if the project ever
wants functions/generated policy. Reasonable second choice; picked CUE first
because a security floor benefits more from "conflicts are errors" than from
programmability.

**Considered and rejected for policy authoring** (not for other uses — these are
all fine tools, wrong fit here):

- **Jsonnet** — object composition via `+`/`super` is a *generative* tool (stamp
  out N Kubernetes manifests); for a security source of truth, "generate me an
  object" is the wrong posture. A silent-override bug in Jsonnet is exactly as
  easy as in TOML, just with fancier syntax.
- **Dhall** — safe and total, but its ergonomics are more academic rigor than
  this scale warrants; CUE gets the same "conflicts are errors" property with
  less ceremony.
- **HCL/Terragrunt** — `include` + `merge_strategy` gives structural inheritance,
  but importing HCL means importing Terraform's whole mental model and tooling
  for a harness that has nothing to do with infrastructure deployment.

## 6. WIT/WASM: an execution/plugin boundary, not a config language

A natural follow-on question once WASI is on the table for Sandboy: could
WIT/WASI also *be* the policy format, as a language-independent config? **No —
that would be solving the wrong layer with the right technology.** WIT
(WebAssembly Interface Types) is an ABI/contract description for components —
what a plugin may import and must export — not a general-purpose language, and
it has no merge/inheritance model for data the way CUE does.

The right split, already converging in Own.NET's design notes independently of
this one:

- **CUE/Nickel** — policy *authoring* (§5): human-facing, composable, has a
  conflict model.
- **`gate.lock.json`** — the compiled, boring runtime artifact.
- **Sandboy (Landlock/seccomp, later Firecracker/gVisor)** — the actual process
  isolation boundary. Already an accepted ADR in Own.NET.
- **WIT + Wasmtime** — a typed plugin ABI for components that parse **untrusted
  input**, not a cage for the agent itself (native `bash`/`git`/compilers can't
  be caged by WASM — `agent-capability-layer.md` §0 already draws this line, and
  the concrete instance is already spiked: `audit/adapters/` in Own.NET, raw→SARIF
  adapters as capability-free WASM components).

For 007 today: gate steps stay ordinary CLI invocations wrapped by Sandboy. WIT
componentization is worth doing only where a step's *input* is untrusted and
parsed by code you'd rather contain — e.g. a future gate step that ingests a
target repo's own findings/report file, not `python tests/run_tests.py`. Forcing
today's MVP steps (which are just "run this trusted toolchain command") through
a WASM component boundary would be effort spent on the wrong risk.

## 7. Non-goals (foundation tier — revisit only on a real trigger)

Matches `security-layers.md`'s existing discipline of "trigger, not vibes":

| Deferred | Trigger to revisit |
|---|---|
| HSM/TPM attestation | never, for a personal single-host harness |
| Full ABAC/Cedar policy engine | >1 permission profile that changes without a rebuild — `security-layers.md` already tracks this |
| SIEM/SOAR | there is something to centrally monitor (multi-host, multi-user) |
| ML anomaly detection | structured logs exist and are boring/complete first |
| Certificate lifecycle management | a multi-node runner exists |

## 8. Bottom line

The gap `security-layers.md` names — worktree isn't a boundary, `bash -lc` from
an untrusted target is arbitrary code execution — has a concrete, already
partially-built fix: wire `gate.rs` through Sandboy (Phase 1), express the
permission floor in CUE so a leaf config can't silently drop `network = "deny"`
(§5), and keep WIT scoped to untrusted-input parsers, not the agent's cage (§6).
Nothing here invents new machinery Own.NET hasn't already sketched or spiked —
this note's job was to sequence it and settle the two open format questions.
