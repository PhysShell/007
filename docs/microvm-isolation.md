# ADR: microVM isolation for `o7 run`/gate — assessment and roadmap

Status: proposed (design note, not yet implemented) · Scope: `o7 run` +
`.007/gate.toml` · Builds on `docs/security-layers.md`

**Conclusion up front:** microVM (Firecracker-class) is the right eventual
boundary for `run`/gate, but "wrap it in a microVM and you're done" is wrong.
A microVM only isolates the host if nothing bridges guest and host back
together — and the bridge (rw mounts of the live repo, mounted CLI auth,
open network, arbitrary-bash gates) is exactly what makes a sandbox
convenient, which is exactly why people build it back in. The isolation
layer and the policy/mount/network/secrets/audit layer around it are two
separate deliverables; only both together close the gap this repo already
documented.

## Grounding: what this repo already established

`docs/security-layers.md` independently reached the same conclusion this
proposal starts from, and it is verified against the current tree:

| Claim | Verdict | Evidence |
| --- | --- | --- |
| `o7 run` executes arbitrary `bash -lc <cmd>` from the target repo's `.007/gate.toml` | ✅ | `src/gate.rs:78-81` |
| `current_dir(workdir)` is the *only* confinement — no write/read/egress boundary | ✅ | `src/gate.rs:80`, `src/worktree.rs` (git worktree add/remove only sets cwd) |
| the agent itself runs unsandboxed under `bypassPermissions` | ✅ | `src/agent.rs:66-75` — comment there even says "safe only because the worktree contains the blast radius", which `security-layers.md` already flags as false against adversarial paths |
| the deny-list (`DENY` in `agent.rs`) is defense-in-depth, not a boundary | ✅ | `src/agent.rs:33-44`, docstring says so explicitly |
| "the real sandbox slot is `run`/gate, not `judge`" | ✅ | `judge` is already closed-world (`--tools ""`, `--strict-mcp-config`, canonicalize+prefix check); `run` is the open door |

So this proposal is not introducing a new problem — it is proposing the next
layer for a gap 007 already named and deferred (`security-layers.md` §"gap
the layer list understates"; `TODO.md` backlog: "container egress
hardening").

## Why microVM over plain container/sandbox

A container still shares the host kernel. A microVM (Firecracker: a
KVM-based VMM built for lightweight, minimal-device-model, multi-tenant
workloads — this is literally what it was built for at AWS for
Lambda/Fargate) gives a separate guest kernel and a hardware-virtualization
boundary. Kata Containers gets the same VM boundary with container UX
(container-shaped outside, lightweight VM inside; backed by QEMU, Cloud
Hypervisor, or Firecracker).

This maps cleanly onto 007's actual gap: `gate.toml` running attacker-
controlled `bash` against an untrusted target repo, with `current_dir` as
the only containment. A microVM boundary would mean the gate step literally
cannot reach the host filesystem or network unless something explicitly
wires that up.

## Where the boundary alone stops mattering

A microVM protects the host only if the host isn't smuggled back in through
the setup around it. The failure mode to design against, concretely for
007's shape:

- the live target repo mounted **rw** into the guest (instead of a
  snapshot copied in)
- host `$HOME` mounted in
- Claude/Codex CLI auth mounted into the guest as a raw token
- network left open by default
- `gate.toml` steps still arbitrary `bash`, just relocated
- diff/logs collected back out through a shared writable folder

None of that is a sandbox; it's the same trust boundary with a slower
enter/exit. The correct shape is: fresh guest → copy a **repo snapshot** in
(tar/squashfs, not a live mount) → run agent + gates inside → collect
diff/logs/verdict through a narrow, defined channel → destroy the guest.
Firecracker's own model agrees with layering rather than trusting the VM
word alone — it runs the VMM itself inside a `jailer` as a declared "second
line of defense," not as a single silver bullet.

## What a microVM boundary would and wouldn't fix here

Would fix (maps directly to `security-layers.md`'s open gap):
- gate step can't write outside its guest — no `../`, no `~/.ssh`, no
  reaching the real checkout
- a malicious build/gate script is contained to a disposable guest
- network can be off at the VM level, not hoped-for at the app level
- CPU/RAM/disk/time become enforceable limits, not conventions
- guest state (and anything it downloaded/wrote) is discarded every run
- run artifacts become a reproducible, disposable evidence pack

Would **not** fix (still needs its own layer, independent of VM vs container
vs nothing):
- prompt injection *if the guest still has egress* — the VM boundary and
  the "network off by default" decision are separate switches
- Claude/Codex auth leaking *if it's mounted into the guest as a raw
  token* — needs a broker, not a mount (see below)
- supply-chain pulls via npm/cargo/nuget if the guest can still reach the
  internet unrestricted
- resource exhaustion without explicit CPU/RAM/disk/time caps
- a bad diff or a bad task without a diff/task-contract policy
- a judge that rubber-stamps `real`/`false_positive` regardless of input —
  orthogonal to isolation entirely

## Options considered for the backend

| Backend | Gives | Cost |
| --- | --- | --- |
| **Firecracker direct** | Strongest, smallest boundary; fast start; fits one-shot agent runs (`network=off`, readonly rootfs, disposable workspace) | Own rootfs/kernel build, own copy-in/copy-out, own vsock/network/log/teardown plumbing; needs `/dev/kvm` — a real requirement to check on the target host (WSL2 needs separate verification), not an assumption |
| **Kata Containers** | Same VM boundary, container-shaped UX, integrates via containerd, can run on Firecracker/Cloud Hypervisor/QEMU underneath | More infra to operate; another moving stack on top of a currently-thin binary |
| **gVisor** (not a microVM — userspace kernel intercepting syscalls, `runsc`) | Meaningfully stronger than a bare container with much less setup; fits Docker/Kubernetes/containerd already | No hardware VM boundary; syscall-heavy workloads pay a tax; compatibility gaps exist |

Read for 007: gVisor is a reasonable *semi-trusted* stepping stone;
Firecracker direct is the right target for "run an arbitrary target repo's
`gate.toml`" once that's actually in scope, matching the
`o7 run --isolation microvm` shape below.

## Proposed shape (not implemented — for review before any code lands)

An isolation mode selector, orthogonal to today's worktree-only path:

```rust
pub enum IsolationMode {
    None,
    WorktreeOnly, // today's behavior — cwd only, no boundary
    Container,
    GVisor,
    MicroVm,
}
```

Task-contract-level config (illustrative, not a committed schema):

```toml
[isolation]
mode = "microvm"
network = "off"
copy_in = "snapshot"          # never a live rw mount of the target repo
copy_out = ["diff.patch", "gate/", "agent.stdout", "meta.json"]

[limits]
timeout_seconds = 900
memory_mb = 4096
disk_mb = 8192
cpu_count = 2

[secrets]
mount_auth = false             # raw CLI auth never enters the guest
```

`WorktreeOnly` (today's default) stays correct for trusted-local repos
(Own.NET). `MicroVm` + `network = "off"` is the mode for an untrusted target
repo's `gate.toml`.

### The auth question this forces

If a run inside an isolated guest needs a live Claude/Codex call, the raw
subscription/API token must not enter a guest that's also executing an
untrusted target's build/gate scripts — that reintroduces the exact bridge
the isolation was meant to remove. The shape that avoids it:

```
guest agent request → vsock → host broker → claude/codex call (outside guest) → response back into guest
```

i.e. the guest asks for a completion, the host makes the actual call and
returns only the result. This is materially more plumbing than a mount and
should stay a distinct, later phase — it's only needed once isolated runs
also need live model calls from inside the guest.

## Roadmap (phased, cheapest-value-first)

1. **Policy, no VM** (cheap, useful immediately, no infra dependency): task
   contract, diff policy / forbidden paths, changed-files check, dependency
   manifest check, evidence pack. Independent of everything below.
2. **Container/gVisor prototype**: `o7 run --isolation container|gvisor`,
   prove out copy-in/copy-out, network-off, resource limits end to end
   before paying for a VM boundary.
3. **microVM backend**: `o7 run --isolation microvm` — fresh guest per run,
   no host rw mounts, repo copied in as a snapshot, network off by default,
   artifacts copied out, guest destroyed, diff computed inside the guest or
   from the copied-out tree.
4. **Auth broker**: only once an isolated run needs live model calls from
   inside the guest without the raw token ever entering it.

## Bottom line

- `security-layers.md` already identified `run`/gate + `current_dir` as
  007's sharpest present trust gap; this note names the layer that closes
  it (microVM) and, just as importantly, the failure mode that silently
  reopens it (mounting the host back in for convenience).
- Isolation backend (container / gVisor / microVM) and the policy layer
  around it (mounts, network, secrets, limits, audit) are two separate
  axes. Neither substitutes for the other.
- Sequencing matters more than the backend choice: Phase 1 (policy, no VM)
  is valuable today and blocks on nothing; the microVM backend is the right
  target once an untrusted target repo's `gate.toml` is actually in scope,
  not before.
