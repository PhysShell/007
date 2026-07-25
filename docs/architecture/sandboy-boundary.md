# Sandboy process boundary (PR 4.5)

The `UnconfinedHostBoundary` (PR 2) gives lifecycle control and attests
`EnforcementLevel::None`. Sandboy is the *mandatory* boundary that must land before
any live provider run: it attests `Sandboy` / `FullyEnforced` and is backed by real
Linux confinement (Landlock + seccomp). This note records the load-bearing decisions
so they can be gated before the mechanism is built.

## Decision 1 — the confinement runs in an EXTERNAL `sandboy` backend, not in-crate

Every 007 crate sets `unsafe_code = "forbid"`. Applying Landlock/seccomp to the child
of a `tokio`/`std` `Command` requires a `pre_exec` hook (which is `unsafe fn`) to run
between `fork` and `exec` in the child. We will not add `unsafe` to `o7-worker`, and
we will not fork the frozen `ProcessBoundary` seam to smuggle a closure across it.

Therefore the confinement is applied by the child *itself*: an external `sandboy`
executable that (1) installs its own Landlock ruleset + seccomp filter, (2) emits a
machine-readable enforcement report, then (3) `exec`s the real target in place.
`SandboyBoundary` launches that backend through the **unchanged** frozen PATH-spawn
seam — `executable = <sandboy>`, `arguments = [<policy/report flags>, "--", <target>,
<target args…>]`. All `unsafe` lives in the backend binary and its dependencies; the
007 crates stay `unsafe`-free.

This matches issue #53's `sandboy run --report` shape and keeps the seam frozen.

## Decision 2 — same PID namespace, to preserve PR 3's sealed-memfd exec

PR 3 execs a sealed `memfd` via `/proc/<verifier_pid>/fd/<n>`. That path resolves the
*verifier's* fd table by absolute PID, so it only works while the child shares the
verifier's PID namespace. `sandboy` therefore does **not** unshare a new PID namespace;
it confines-then-`exec`s in place. Landlock is a filesystem *restriction*, so the
policy MUST explicitly grant read+execute on the sealed-memfd exec target, otherwise
the `exec` is denied and the run fails closed.

`SandboyBoundary::spawn` validates this BEFORE launch: if `spec.executable` (e.g.
`/proc/<verifier_pid>/fd/<n>`) is not covered by the policy's exec allowances, it
refuses with a boundary error rather than launching a backend that will only fail to
`exec`. This is the PR-3 compatibility invariant, made testable without a live launch.

## Decision 3 — a per-dimension report, never a boolean `secure`

The backend reports what it *actually* enforced, per dimension
(`filesystem`, `network`, `env`, `process_tree`, `timeout`), each one of
`enforced` / `partial` / `not_enforced`. There is no boolean `secure` field — issue
#53: "that field is how software lies to itself before a breach report." The report
type uses `deny_unknown_fields` (a stray `secure` fails to parse) and names every
dimension as a required field (a missing dimension is a parse error, never a silent
"enforced"). `FullyEnforced` requires *every* dimension `enforced`; any downgrade maps
to `Partial` (or `None` when nothing is enforced) and can never satisfy
`RequireFullyEnforced`.

This report is the run-time evidence that feeds `o7-run`'s
`SandboxEvidenceCaptured` (o7-run stores it as a content-addressed artifact, so the
structured type stays local to `o7-worker`; if o7-run ever needs the structure it is a
mechanical extraction into a shared pure crate).

## Decision 4 — fail closed at construction AND at spawn

- `SandboyBoundary::new(backend, policy)` rejects a policy that cannot fully confine
  (no writable-root, no deny-all-by-default network posture, a zero timeout). A
  constructed boundary is one that *intends* full enforcement.
- `attestation()` reflects that configured intent (`Sandboy` / `FullyEnforced`), so the
  supervisor's `RequireFullyEnforced` check passes only for a real backend + full
  policy — never a silent fallback to the host.
- At spawn, the run-time `SandboxReport` must independently re-prove full enforcement;
  a downgraded report is surfaced as evidence and fails the run closed rather than being
  reported as confined.

## What this first slice is (and is not)

This slice lands the **pure contract** — the policy/report/enforcement types, their
validation, and the honest report→attestation mapping — plus a `SandboyBoundary`
skeleton whose fail-closed pre-spawn checks are real. The confined **live launch** of
the backend (wiring the report fd, reading and mapping the report onto a live
`BoundaryProcess`) is deliberately left RED and lands in the follow-up GREEN commit.
The frozen `ProcessBoundary` / `BoundarySpawnSpec` / `BoundaryProcess` seam is not
touched.
