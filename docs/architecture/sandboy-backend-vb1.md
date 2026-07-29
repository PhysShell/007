# Sandboy VB-1 — monitor-owned cgroup v2 leaf, timeout, cgroup.kill teardown

VB-1 is the second Vertical B slice on top of the accepted VB-0 authority
(`e0b3b757c812bb3b5fab3ad9c08280feef67b3f3`). Scope, strictly: a dedicated non-root cgroup v2
leaf owned by the monitor, wall-clock timeout, and `cgroup.kill` teardown. **No Landlock,
seccomp, or env** (VB-2/VB-3); no `confinement_backend()` switch, no RED-matrix flip (VB-4).

## Decision 1 — placement by INHERITANCE, so the monitor stays `unsafe`-free

Putting the target in the leaf via a `pre_exec` hook or `clone3(CLONE_INTO_CGROUP)` is `unsafe`.
Instead the monitor **moves ITSELF into the leaf, then spawns the target**: cgroup membership is
inherited across `fork`/`exec`, so the target — and every descendant, including a double-forked
one reparented to `init` — is in the leaf without any syscall crate. Teardown moves the monitor
**back out** to the parent cgroup and writes `cgroup.kill`, which kills the whole tree at once
without killing the monitor; the monitor then proves the drain (`cgroup.procs` empties) and
`rmdir`s the leaf. Everything is safe `std::fs` writes + `std::process`. **VB-1 adds no `unsafe`;
the crate's `#![forbid(unsafe_code)]` stays** (Landlock/seccomp remove it at VB-2/VB-3). Proven on
a delegated host: monitor+target+child+double-fork all in one leaf, `cgroup.kill` drains it, dir
removed.

## Decision 2 — a tested CAPABILITY, gated; production `run` is unchanged

At VB-1 the backend still cannot report `FullyEnforced` (filesystem/network/env are absent), so
the production `run` path still refuses to execute any target (the VB-0 honest bootstrap is
untouched). The cgroup monitor is therefore delivered as a **`test-harness`-gated** module
(`crates/sandboy/src/cgroup.rs`) plus a `sandboy __cgroup-run …` harness entry — the same pattern
as o7-worker's harness probes. A production build (no feature) has neither the module nor the
subcommand. **VB-4** promotes the monitor into the live `run` path and flips the `process_tree` /
`timeout` dimensions once the full self-check passes.

## Decision 3 — teardown ownership and the absolute window

The MONITOR owns teardown of the tree it created: on the target's exit, on the deadline, or on any
setup failure it moves out, `cgroup.kill`s, drains, and removes the leaf. The wall-clock timeout is
enforced against an ABSOLUTE window measured from spawn — a target outliving the deadline is killed
with its descendants and the whole kill+drain+rmdir completes inside `deadline + grace`. A setup
failure (leaf join or target spawn) tears the partial leaf down, so **no cgroup ever leaks**. The
PARENT-side `force_stop` teardown path (the `SandboyMonitorProcess` integration in the frozen
matrix oracle) is a VB-4 wiring concern; VB-1 provides the monitor's self-teardown machinery.

## Tests & gates

- `crates/sandboy/tests/cgroup_confinement.rs` — `#[ignore]`d, real delegated cgroup v2 required.
  Mirrors the frozen oracles: owned-leaf membership (monitor+target+ordinary-child+double-fork),
  `cgroup.kill` drain + directory removal, timeout kill within the absolute window (the `survived`
  marker absent), and setup-failure-leaves-no-cgroup. A capability guard SKIPS (never fails) when
  delegation is absent, so `--include-ignored` is safe anywhere.
- Hosted `sandboy backend gate` and `o7-worker gate` COMPILE + lint the harness (`--features
  test-harness`) but do not run the ignored tests (hosted runners cannot delegate cgroups).
- `sandbox-confinement.yml` (self-hosted, `workflow_dispatch`-only, unchanged trigger) gains one
  step running the cgroup acceptance with `--include-ignored` after the preflight proves the
  capability.

No new external dependency. No Landlock/seccomp/cgroup-controllers config. One layer; stop for
re-gate; do not start VB-2.
