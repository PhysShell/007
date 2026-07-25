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
`BoundaryEvidence { attestation, report }` is the run-time proof established by THIS launch
(distinct from the pre-spawn configured `attestation()`).

- `SandboyBoundary::new` rejects a relative backend or a policy that cannot fully confine.
- `attestation()` reflects the configured intent (`Sandboy` / `FullyEnforced`), so the
  supervisor's `RequireFullyEnforced` gate passes only for a real backend + full policy.
- The supervisor publishes `LaunchEvidence` BEFORE `Spawned`, and — defense in depth —
  tears the process down and fails closed if the run-time evidence does not satisfy
  `RequireFullyEnforced`. Missing evidence (an unconfined boundary) never satisfies it.
- The downstream adapter persists `evidence.report` as o7-run's `SandboxEvidenceCaptured`.

## What this slice is (and is not)

Landed here, PURE and GREEN: the `o7-sandbox-protocol` crate (policy digest, versioned
length-bounded report frame, identity binding, KATs); the evolved evidence seam and its
supervisor wiring; `SandboyBoundary`'s construction/argv/exec-gate and the report
**verification** matrix (`verify_report` fails closed on any downgrade or mis-binding).

Deliberately RED: the confined LIVE LAUNCH — wiring the report pipe, spawning + reaping the
monitor backend, reading the frame, and returning a live `BoundaryProcess` — plus the
sealed-memfd-under-Landlock probe. The launch-level negative matrix (backend exits before
exec, cancel-mid-report leaves nothing, write-end closed to the target) lands WITH the live
launch in the GREEN commit, where asserting those is no longer vacuous.
