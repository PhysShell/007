# Capability FD transport (stage B) — accepted direction, not yet implemented

Status: **accepted (direction), contract-first, RED until implemented.**
Prerequisite: Sandboy Vertical B GREEN (`docs/tasks/sandboy-a1-vertical-b.md`).
This note fixes the load-bearing decisions so the implementing PR does not
improvise architecture. Nothing here is Vertical B scope.

## Motivation

A sandboxed agent must be able to hold a *capability* — a pre-opened, policy-bound
connection to a trusted broker — without ever holding a secret, a bearer token, or
the ability to open the connection itself. First consumer (stage C):
`o7-model-gate`, brokering ArliAI model invocation. The transport below is
generic; Sandboy learns nothing about models, profiles, or upstreams.

## Decision 1 — declarative capability manifest, bound to the launch

`LaunchRequest` gains a capability manifest. It declares *names and target fd
numbers*, never the parent's live descriptor numbers:

```json
{
  "capabilities": [
    { "name": "model_invoke", "target_fd": 3, "kind": "unix_stream", "required": true }
  ]
}
```

- The manifest is part of `launch_spec_digest` — a report cannot bind to a launch
  whose grants differ from what the parent declared.
- Validation (fail closed, whole session): `target_fd` unique, ∉ {0,1,2}, below
  `CAP_TARGET_MAX`, disjoint from every internal/reserved descriptor range;
  `kind` from a closed enum; unknown fields rejected.
- An empty manifest degrades to the capability-free choreography (see Decision 3).

## Decision 2 — two SCM_RIGHTS hops; descriptors exist in the child only after verification

The control socket belongs to the monitor; the confined child is forked (and
confined) *before* authorization. Therefore descriptors travel in two hops:

```
parent ──(control socket, SCM_RIGHTS)──▶ monitor ──(pre-fork private socketpair, SCM_RIGHTS)──▶ child
```

- The monitor↔child socketpair is created before fork, CLOEXEC on the monitor
  side after forwarding, and is the same channel the child's confinement
  self-check already travels on (Vertical B topology: child installs Landlock +
  seccomp between fork and exec, self-checks, reports to the monitor).
- The monitor validates each hop: received fd count exactly matches the manifest,
  no extra descriptors, socket domain/type checked via `getsockopt(SO_DOMAIN,
  SO_TYPE)` as far as the declared `kind` allows. Any mismatch tears the session
  down; partial grants are unrepresentable.
- The target never sees the control socket or the rendezvous socketpair.

## Decision 3 — two authorization barriers, explicit state machine

Sandbox enforcement and capability provisioning are separately proven, each
before the parent commits further:

```
parent  ── LaunchRequest (manifest, no fds) ──▶ monitor ── fork ──▶ child
child   installs confinement, self-checks
monitor ── SandboxReport ──▶ parent            parent verifies      [barrier 1]
parent  ── CAP_GRANT + SCM_RIGHTS ──▶ monitor ── forward ──▶ child
child   validates, stages, maps, scrubs, proves exact fd table, WAITS
monitor ── CapabilityEvidence ──▶ parent       parent verifies      [barrier 2]
parent  ── EXEC ──▶ monitor ── forward ──▶ child ── closes rendezvous, execs target
```

Backend states (invalid transitions fail closed):

```
AwaitLaunchRequest → InstallingConfinement → AwaitSandboxAuthorization
  → AwaitCapabilityGrant → AwaitCapabilityAuthorization → Executing → Completed
```

Fail-closed events: CAP_GRANT before barrier 1; EXEC before CapabilityReady;
duplicate CAP_GRANT or EXEC; a frame without its expected ancillary payload; an
ancillary payload without its frame; surplus ancillary messages; EOF at any
barrier; child death after grant (the parent must observe it, and the broker
must see its session fd close).

**Wire-contract consequence (this is a protocol version bump, not an extension
of the frozen v1 choreography).** Vertical A's choreography — one report frame,
backend half-closes its write side, parent proves EOF, one GO byte — cannot
carry a second phase: the backend's write side is already closed after the
report. Capability-bearing launches therefore negotiate `protocol_version` N+1:
typed, length-prefixed frames in both directions, no half-close, explicit
terminal states replacing the EOF-proof. Capability-free launches keep the v1
choreography unchanged; v1 remains the default and the fallback.

## Decision 4 — collision-safe descriptor mapping in the child

Naive sequential `dup3(received, target)` breaks on permutations
(received 3→target 4, received 4→target 3), on `received == target`
(`dup3` rejects `oldfd == newfd`), and silently depends on which numbers the
kernel assigned at SCM_RIGHTS receipt. The mapping is contractual:

1. **Stage**: every received descriptor is moved up via
   `fcntl(F_DUPFD_CLOEXEC, CAP_STAGING_BASE)` — out of the target range;
2. **Scrub**: `close_range(3, ~0, CLOSE_RANGE_CLOEXEC)` marks everything;
3. **Map**: `dup3(staged, target_fd, 0)` for each manifest entry — flags 0
   clears CLOEXEC on exactly the declared targets;
4. staging descriptors stay CLOEXEC and die at exec (or are closed explicitly);
5. `exec`.

Reserved ranges, disjoint by construction:

```
0..2                    stdio
3..CAP_TARGET_MAX       declared capability targets
CAP_STAGING_BASE..      staging (CLOEXEC, dead at exec)
INTERNAL_FD_BASE..      rendezvous / control descriptors (moved high at startup)
```

The grant is atomic in effect: either every declared capability is mapped and
proven, or the target never execs and the session is torn down. A failure on the
second mapping must not leave the first capability reachable by anything.

## Decision 5 — capability evidence is a separate plane from the sandbox report

The sandbox report keeps its five dimensions (filesystem, network, env,
process_tree, timeout). Capability provisioning is not a sixth dimension — it is
a distinct artifact with its own binding:

```
CapabilityEvidence:
  manifest_digest        (must equal the manifest inside launch_spec_digest)
  granted                (name, target_fd, kind — as mapped, per entry)
  fd_table_proof         (exact-set oracle, below)
  scrub_status
```

Conflating "filesystem: enforced" with "fd 3 was granted correctly" would weld
two trust statements into one model; verifiers need to reject them independently.

## Decision 6 — the fd-table oracle proves an exact set, not absence of known-bad

After mapping and scrub (probed via `/proc/self/fd`, excluding the enumeration
descriptor by identity, as the existing `fd_probe` already does):

```
actual_fds == {0, 1, 2} ∪ declared_target_fds
```

Additionally: stdio present with expected types; every capability fd has
FD_CLOEXEC **cleared** and the expected socket domain/type; nothing else exists.
Extend `fd_probe` — do not write a third probe.

## Adversarial matrix (the implementing PR turns these into tests)

- fd-number permutation 3↔4; `received == target`; several fds in reverse order;
- duplicate `target_fd`; target_fd ∈ {0,1,2}; target_fd colliding with an
  internal descriptor; descriptor number above the ordinary range;
- surplus SCM_RIGHTS fds; missing fds; SCM_RIGHTS without frame; frame without
  SCM_RIGHTS; malformed ancillary data;
- CAP_GRANT before sandbox authorization; duplicate CAP_GRANT; EXEC before
  CapabilityReady; duplicate EXEC; EOF at each barrier;
- failure on the N-th mapping leaves no earlier capability reachable;
- child death after grant: parent observes, broker sees session fd close;
- sibling/monitor processes never expose the capability fd to anything but the
  target;
- control socket unreachable from inside the sandbox (Landlock path denial —
  the broker's socket path must not appear in any target policy root).

## Non-goals

No ArliAI, model profiles, queueing, or credential handling (stage C —
`o7-model-gate`). No bearer tokens or secrets over this transport, ever. No TCP
fallback. No generic RPC framework: this note defines descriptor *transfer*;
what flows over a granted socket is the consumer's contract.
