# Capability FD transport (stage SB-B) — accepted direction, not yet implemented

Status: **accepted (direction), contract-first, RED until implemented.**
Stage identifiers: SB-A0 → SB-A1 → SB-A2 → **SB-B** (this note) → MG-C
(the `SB-` prefix avoids collision with the unrelated Q-Deck A0/A1).
Prerequisite: Sandboy Vertical B GREEN (SB-A1,
`docs/tasks/sandboy-a1-vertical-b.md`).
This note fixes the load-bearing decisions so the implementing PR does not
improvise architecture. Nothing here is Vertical B scope.

## Motivation

A sandboxed agent must be able to hold a *capability* — a pre-opened, policy-bound
connection to a trusted broker — without ever holding a secret, a bearer token, or
the ability to open the connection itself. First consumer (stage MG-C):
`o7-model-gate`, brokering ArliAI model invocation. The transport below is
generic; Sandboy learns nothing about models, profiles, or upstreams.

## Decision 1 — declarative capability manifest, bound to the launch

`LaunchRequest` gains a capability manifest. It declares *names and target fd
numbers*, never the parent's live descriptor numbers:

```json
{
  "capabilities": [
    { "name": "model_invoke", "target_fd": 3, "kind": "unix_stream" }
  ]
}
```

- The manifest is part of `launch_spec_digest` — a report cannot bind to a launch
  whose grants differ from what the parent declared.
- Validation (fail closed, whole session): `target_fd` unique, ∉ {0,1,2}, below
  `CAP_TARGET_MAX`, disjoint from every internal/reserved descriptor range;
  `kind` from a closed enum; unknown fields rejected.
- Every declared capability is mandatory — the grant is atomic all-or-nothing
  (Decision 4), so a `required` flag would be meaningless today and is
  deliberately absent. Optional capabilities, if a real consumer ever needs
  them, arrive as a future manifest version — not as improvised semantics
  during implementation.
- **Entry↔descriptor correspondence is canonical, never positional guesswork.**
  `name` is unique (as is `target_fd`), bounded in length and charset
  (`[a-z0-9_.-]`, ≤ 64 bytes). The manifest is a set; its canonical order —
  the order that enters the digest, the CAP_GRANT frame, and the SCM_RIGHTS
  array — is **by `target_fd`, then by `name`**. The CAP_GRANT frame carries
  the canonical ordered entry list; the *n*-th received fd corresponds to the
  *n*-th entry. The receiver verifies the frame's entry list equals the
  canonical manifest exactly — any deviation (order, count, names) fails
  closed. Without this, two correct components can map `[fdA, fdB]` onto two
  capabilities differently and grant the right descriptor to the wrong name.
- An empty manifest selects the capability-free choreography (see Decision 3).

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
child   validates, stages, maps, scrubs, proves survivor set, WAITS
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
report. Capability-bearing launches therefore use **protocol v2**: typed,
length-prefixed frames in both directions, no half-close, explicit terminal
states replacing the EOF-proof.

Version selection is determined by the manifest, and downgrade is prohibited:

```
empty capability manifest      → protocol v1 (unchanged)
non-empty capability manifest  → protocol v2 REQUIRED
backend does not support v2    → fail closed BEFORE any target launch
automatic retry/downgrade v2→v1 → prohibited
```

There is no "fallback" for a capability-bearing launch: a launch that declared
capabilities either runs under v2 with every barrier intact, or does not run.

**Version selection happens out-of-band, before the backend reads anything.**
The socketpair's type must be chosen before the backend can parse a
LaunchRequest, so the version cannot be discovered from the manifest alone.
The binding is fixed as follows — one decision, not an implementation choice:

```
o7-worker:  selects the version from its local launch spec
            creates the socketpair of the matching type
            launches the backend with --protocol-version 1|2 on argv

backend,    argv protocol version
pre-fork,   SO_TYPE of the stdin socket (STREAM for v1, SEQPACKET for v2)
verifies:   the version field of the first frame
            manifest emptiness (empty ⇔ v1)

any mismatch among the four → fail closed before fork
```

(A version number on argv does not violate Decision 3 of
`sandboy-boundary.md` — that decision keeps *descriptor numbers and target
argv/env* off `/proc`-visible argv; a public protocol version is neither.)

**v2 transport is `AF_UNIX + SOCK_SEQPACKET + SOCK_CLOEXEC`** — on both the
parent↔monitor control socket and the monitor↔child rendezvous socketpair.
SOCK_STREAM preserves no message boundaries, so "a frame without its expected
ancillary payload fails closed" would be unenforceable without a hand-defined
sendmsg/recvmsg envelope and partial-read handling. SEQPACKET keeps boundaries:
each typed frame and its SCM_RIGHTS payload arrive in one `recvmsg`, surplus or
truncated datagrams are directly detectable (`MSG_TRUNC`/`MSG_CTRUNC` fail
closed), and the length prefix is retained as an internal size bound. v1 stays
on the existing stream socketpair unchanged.

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
4. every received/staging descriptor is **explicitly closed** after its
   successful `dup3` — not left to die at exec. CLOEXEC remains the safety
   net, not the mechanism;
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
  fd_classification      (pre-EXEC survivor-set proof, Decision 6)
  scrub_status
```

Conflating "filesystem: enforced" with "fd 3 was granted correctly" would weld
two trust statements into one model; verifiers need to reject them independently.

## Decision 6 — two fd-table proofs: survivor set before EXEC, exact set inside the target

A single "exact set == {0,1,2} ∪ targets" proof *before* EXEC is physically
impossible: at that moment the child must still hold the rendezvous fd (to
receive EXEC) and is mid-conversation with the monitor. The proof is therefore
split into two statements, each honest about what it can see:

**Pre-EXEC (inside CapabilityEvidence)** — a *classification* proof over
`/proc/self/fd` (excluding the enumeration descriptor by identity, as the
existing `fd_probe` already does). Every open descriptor falls into exactly one
class, and the classes are exhaustive:

```
survivor_fds     = all fds with FD_CLOEXEC cleared
                 == {0, 1, 2} ∪ declared_target_fds
internal_fds     = the exact known rendezvous/control descriptors,
                   every one with FD_CLOEXEC set
unclassified_fds = ∅
```

Received/staging descriptors do not appear here at all — Decision 4 closes
them explicitly after mapping.

**Post-exec (acceptance test, target side)** — the target-side `fd_probe`
proves the actual table the target woke up with:

```
actual_fds == {0, 1, 2} ∪ declared_target_fds
```

Additionally: stdio present with expected types; every capability fd has
FD_CLOEXEC **cleared** and the expected socket domain/type; nothing else
exists. Extend `fd_probe` — do not write a third probe.

The two proofs answer different questions — "will exactly the declared set
survive exec?" (runtime evidence, verifiable by the parent before EXEC) and
"did exactly the declared set survive exec?" (acceptance oracle). Both are
required; neither substitutes for the other.

## Adversarial matrix (the implementing PR turns these into tests)

- fd-number permutation 3↔4; `received == target`; several fds in reverse order;
- duplicate `target_fd`; target_fd ∈ {0,1,2}; target_fd colliding with an
  internal descriptor; descriptor number above the ordinary range;
- surplus SCM_RIGHTS fds; missing fds; SCM_RIGHTS without frame; frame without
  SCM_RIGHTS; malformed ancillary data; surplus or truncated SEQPACKET
  datagrams (`MSG_TRUNC`/`MSG_CTRUNC`);
- a non-empty manifest against a v1-only backend fails closed before launch;
  no v2→v1 downgrade path exists to exercise;
- version-binding mismatches: argv `--protocol-version` vs stdin `SO_TYPE` vs
  first-frame version vs manifest emptiness — each pairwise mismatch fails
  closed before fork;
- a CAP_GRANT entry list deviating from the canonical manifest order (or
  count, or names) fails closed — no positional reinterpretation;
- CAP_GRANT before sandbox authorization; duplicate CAP_GRANT; EXEC before
  CapabilityReady; duplicate EXEC; EOF at each barrier;
- failure on the N-th mapping leaves no earlier capability reachable;
- child death after grant: parent observes, broker sees session fd close;
- sibling/monitor processes never expose the capability fd to anything but the
  target;
- control socket unreachable from inside the sandbox (Landlock path denial —
  the broker's socket path must not appear in any target policy root).

## Non-goals

No ArliAI, model profiles, queueing, or credential handling (stage MG-C —
`o7-model-gate`). No bearer tokens or secrets over this transport, ever. No TCP
fallback. No generic RPC framework: this note defines descriptor *transfer*;
what flows over a granted socket is the consumer's contract.
