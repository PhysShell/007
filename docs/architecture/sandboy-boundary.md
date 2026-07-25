# Sandboy process boundary (PR 4.5)

The `UnconfinedHostBoundary` (PR 2) gives lifecycle control and attests
`EnforcementLevel::None`. Sandboy is the *mandatory* boundary that must land before any live
provider run: it attests `Sandboy` / `FullyEnforced`, backed by real Linux confinement
(Landlock + seccomp), and it must PROVE what it enforced, not merely claim it. This note
records the load-bearing decisions so they can be gated before the mechanism is built.

## Decision 1 — the confinement runs in an EXTERNAL `sandboy` backend, not in-crate

Every 007 crate sets `unsafe_code = "forbid"`. Applying Landlock/seccomp to the child of a
`tokio`/`std` `Command` requires a `pre_exec` hook (`unsafe fn`) between `fork` and `exec`.
We will not add `unsafe` to `o7-worker`, and we will not smuggle a closure across the seam.

So the confinement is applied by an external `sandboy` executable. `SandboyBoundary` launches
it through the spawn seam — `executable = <sandboy>` (ABSOLUTE; never a PATH search),
`arguments = [<report/nonce/policy flags>, "--", <target>, <target args…>]`. All `unsafe`
lives in the backend binary and its dependencies; the 007 crates stay `unsafe`-free.

## Decision 2 — MONITOR topology (not exec-in-place), same PID namespace

An earlier draft had the backend `exec` the target *in place*. That is unsound: after `exec`
the backend no longer exists, so it cannot enforce a wall-clock timeout, kill a runaway
process tree, or observe escaped descendants — `timeout: enforced` and `process_tree:
enforced` would be wishes typed into a JSON file.

Instead the backend is a **monitor**:

1. it installs the Landlock ruleset + seccomp filter and creates a cgroup it owns;
2. it forks a confined child that `exec`s the real target;
3. it stays alive as the cgroup-owning monitor — enforcing the deadline (kill the cgroup on
   timeout) and the process-tree teardown, and reporting on them honestly.

Crucially it does **not** unshare a new PID namespace: the confined child shares the
verifier's PID namespace, so PR 3's sealed-memfd exec via `/proc/<verifier_pid>/fd/<n>` still
resolves. Landlock is a filesystem *restriction*, so the policy MUST grant read+execute on the
sealed-memfd exec target, or the `exec` is denied and the run fails closed.

The `BoundaryProcess` the worker returns wraps the **monitor** as leader, so the supervisor's
stop/wait/membership act on the cgroup owner. The wall-clock deadline lives in the
`SandboxPolicy` (enforced by the monitor), so `WorkerSpec` needs no separate deadline. On the
supported Landlock ABI the confined child must also be prevented from leaving the owned group
(seccomp on `setsid`/`setpgid`) as defense in depth.

`SandboyBoundary::spawn` validates the exec target BEFORE launch: if `spec.executable` is not
covered by the policy's exec allowances it refuses, rather than launch a backend that will
only fail to `exec`. This is the PR-3 compatibility invariant, made testable without a live
launch — but it is a LEXICAL check; the live sealed-memfd-under-Landlock probe is what proves
the kernel actually grants the sealed object (and, if it cannot on the supported ABI, forces
evolving `BoundarySpawnSpec` to carry the descriptor — never a path-copy fallback).

## Decision 3 — the report is a bounded, versioned frame on a dedicated descriptor

The backend does not write its report to stdout/stderr (which belong to the target's
streams). It writes ONE frame to a dedicated inherited descriptor passed as `--report-fd <n>`:

- a 4-byte big-endian length prefix + JSON body, body `<= MAX_REPORT_BYTES` (64 KiB);
- exactly one frame, with a defined end — malformed, oversized, truncated, or trailing bytes
  fail closed (`o7_sandbox_protocol::frame`);
- the write end is the backend's; it is closed / set `CLOEXEC` before the target `exec`s, so
  the confined target cannot write or append to the channel;
- the parent reads the single frame to completion BEFORE `spawn` returns; a dropped spawn
  future kills and reaps the already-spawned backend (cancel-safety contract);
- the backend installs confinement and self-checks full enforcement BEFORE it `exec`s the
  target, so a `Partial` result never means "the target already ran unconfined while the
  parent was reading JSON".

## Decision 4 — the report is BOUND to policy, launch, and target; no boolean `secure`

Per dimension (`filesystem`, `network`, `env`, `process_tree`, `timeout`), each one of
`enforced` / `partial` / `not_enforced` — never a boolean `secure` (issue #53: "how software
lies to itself before a breach report"). The report also carries `schema_version`, a typed
`backend` identity, and three binding fields:

- `policy_digest` — must equal the parent's canonical `SandboxPolicy::digest()`;
- `launch_nonce` — the per-spawn nonce the parent minted; rejects a stale/replayed report;
- `target_digest` — the digest of the exact executable confined (the sealed memfd bytes).

`SandboyBoundary::verify_report` fails closed unless all three bind AND every dimension is
`enforced`. `deny_unknown_fields` rejects a stray `secure`; every dimension is a required
field so a missing one is a parse error, not a silent "enforced". These types live in the
pure `o7-sandbox-protocol` crate so the external backend emits the exact same protocol without
depending on the worker runtime; o7-run stores the raw frame as an opaque content-addressed
artifact.

## Decision 5 — evidence flows through the seam; fail closed at construction AND launch

A frozen interface that could only return a `BoundaryProcess` could never carry the required
proof, so the seam evolved: `spawn` returns `BoundaryLaunch { process, evidence }`, where
`BoundaryEvidence` is the run-time proof established by THIS launch (distinct from the
pre-spawn configured `attestation()`).

`BoundaryEvidence` is a two-variant ENUM, not a struct with an `Option<report>`, so "fully
enforced but no report" is unrepresentable: a launch either established nothing
(`Unconfined`) or established confinement and carries the verified `report` (`Reported`).
There is no constructor that pairs `FullyEnforced` with an absent report.

- `SandboyBoundary::new` rejects a relative backend descriptor or a policy that cannot fully
  confine.
- `attestation()` reflects the configured intent (`Sandboy` / `FullyEnforced`).
- The supervisor publishes `LaunchEvidence` BEFORE `Spawned`, and — defense in depth — tears
  the process down and fails closed unless `BoundaryEvidence::satisfies` holds: under
  `RequireFullyEnforced` the evidence must be `Reported`, fully enforced, AND its
  implementation must equal the configured one. A missing report, a downgrade, or an
  implementation swap all fail closed.
- The downstream adapter persists the `Reported` report bytes as o7-run's
  `SandboxEvidenceCaptured`.

## Decision 6 — trust is bound to OBJECTS, authorized by a barrier

Gate-round-2 hardening, so a report can neither be forged by an unexpected backend nor
authorize a target before the parent has verified it:

- **Backend identity + object.** `SandboyBoundary` holds a `BackendImage { descriptor,
  digest, identity }`: a HELD immutable descriptor (execed directly, so a post-construction
  path swap cannot substitute a binary), its `digest` (re-checked against the held bytes
  before launch — `verify_backend_object`), and the expected `BackendIdentity` the report
  must echo (`verify_report` → `BackendMismatch`). A bare absolute path is an address, not an
  identity; producing the sealed/held object is the caller's acquisition step (PR 3's
  mechanism).
- **Exact target binding.** The launch derives `target_digest` from the exact held bytes it
  hands the backend; the report must echo it. The sealed-memfd probe builds a REAL sealed
  memfd (`memfd_create` + `F_ADD_SEALS` + `F_GET_SEALS`) held by the parent and addressed as
  `/proc/<parent_pid>/fd/<n>`.
- **Parent authorization barrier.** A second descriptor, `--control-fd`, makes the launch
  two-phase: the backend installs confinement, HOLDS the target before `exec`, emits the
  report, and waits for the parent's GO byte. The parent decodes + verifies the report, then
  sends GO — or, on any malformed / mis-bound / downgraded report, NACK / EOF / cancel, the
  backend kills its cgroup and reaps. The target never runs on an unverified report.
- **Report/protocol errors are typed.** `BoundaryError::Evidence` keeps launch report
  failures distinct from a generic `Spawn` I/O error.
- **Nonce from the OS CSPRNG.** The 128-bit launch nonce comes straight from `getrandom`
  (`OsNonceSource`), no timestamp/counter fallback; an RNG failure fails the launch BEFORE any
  backend spawn. The source is injectable for deterministic tests and a forced-failure test.
- **Policy allowances/env are SETS.** `SandboxPolicy::validate` rejects a duplicate exec
  allowance or env name loudly, rather than silently hashing a multiset (which would give two
  same-meaning policies different digests).

## What this slice is (and is not)

Landed here, PURE and GREEN: the `o7-sandbox-protocol` crate (policy digest, versioned
length-bounded report frame, identity binding, KATs); the evolved evidence seam
(`BoundaryEvidence` enum + `satisfies`) and its supervisor wiring; `SandboyBoundary`'s
construction / argv (report + control descriptors) / exec-gate / backend-object + report
**verification** matrix; the CSPRNG nonce source with its RNG-failure fail-closed; and the
policy set-validation. The controlled fake `sandboy` backend is in-tree NOW, so the GREEN
launch only has to make the existing acceptance tests pass — it cannot move the contract.

Deliberately RED: the confined LIVE LAUNCH itself — verifying the held backend object, opening
the report + control pipes, spawning + reaping the monitor backend, reading and verifying the
frame, sending GO, and returning a live `BoundaryProcess` with `Reported` evidence. The full
acceptance matrix against the fake backend (malformed / wrong-nonce / wrong-policy /
wrong-target / wrong-backend / partial / premature-exit, and the valid-report + GO positive)
and the real sealed-memfd probe are all RED today with SPECIFIC error/marker assertions — not
vacuous `is_err()` — so GREEN turns them green without rewriting them.
