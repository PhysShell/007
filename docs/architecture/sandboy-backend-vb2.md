# Sandboy VB-2 — Landlock filesystem confinement, self-checked

VB-2 is the third Vertical B slice, stacked on the accepted+frozen VB-1 monitor
(`e7dc936579a4e0a61b650073fbb4b082fcddb618`). Scope, strictly: real **Landlock filesystem**
confinement — writes confined to the worktree, read+execute confined to `allow_exec` — plus an
effect-based self-check, reporting the `filesystem` dimension honestly. **No seccomp, network, or env
(VB-3); no `confinement_backend()` switch; no RED-matrix flip (VB-4).**

## Decision 1 — `unsafe` via raw `libc` syscalls, contained to one audited module

VB-2 is the slice that first needs kernel syscalls, so it is where `unsafe` enters — and nowhere
else. The three Landlock syscalls (`create_ruleset`, `add_rule`, `restrict_self`) plus
`prctl(PR_SET_NO_NEW_PRIVS)` are issued through a direct, pinned `libc` dependency (its `SYS_*`
constants + `syscall`/`prctl` only — no wrapper crate hiding the real implementation, no inline asm
that would make us the ABI maintainer for several architectures). The **entire** `unsafe` surface —
six documented blocks — lives in `crates/sandboy/src/landlock/sys.rs`; the gate greps to prove
containment, and clippy's `undocumented_unsafe_blocks` + `multiple_unsafe_ops_per_block` keep each
block small and commented. The kernel UAPI structs are `#[repr(C)]` (the path-beneath attr is
`#[repr(C, packed)]` — the kernel struct is `__packed`, so 12 bytes, not 16) with compile-time
size/alignment assertions, so a layout drift is a build error, not a mis-sized syscall. Policy
construction (ruleset building, rule rights, enforcement decision) is entirely SAFE code in
`landlock/mod.rs`.

Production stays forbid-unsafe. `main.rs` carries
`#![cfg_attr(not(feature = "test-harness"), forbid(unsafe_code))]`: a production build (feature off)
compiles NEITHER the Landlock module NOR `libc` NOR any `unsafe`, so the VB-0 empty-inventory
guarantee is byte-for-byte intact. The `test-harness` feature — the same gate as the VB-1 cgroup
monitor — unlocks the capability. VB-4 wires it into the live `run` path and lifts the forbid.

## Decision 2 — enforcement is PROVEN, never assumed; the exact minimum ABI

A ruleset fd or an ABI number restricts nothing on its own — the kernel API separates *creating* a
ruleset, *adding* rules, and *applying* the restriction. So VB-2 considers the filesystem dimension
`enforced` **only** after the full ordered sequence succeeds — `create_ruleset` → `add_rule`* →
`prctl(PR_SET_NO_NEW_PRIVS)` → `restrict_self` — **and** an effect-based self-check observes that an
outside-worktree write is DENIED (specifically EACCES/EPERM) while an inside-worktree write is
ALLOWED. `PR_SET_NO_NEW_PRIVS` is mandatory: without it (or `CAP_SYS_ADMIN`) `restrict_self` returns
`EPERM`.

The runtime ABI is probed FIRST, and the exact minimum the frozen filesystem oracle needs is
required: **ABI 3**, for `LANDLOCK_ACCESS_FS_TRUNCATE` (the oracle denies truncation of a pre-existing
outside file as a right distinct from create/write). An absent (`ENOSYS`), disabled (`EOPNOTSUPP`),
or insufficient (`< 3`) Landlock reports `filesystem=not_enforced` and NEVER runs the target. Every
install stage returns a typed `InstallError` mapped to a distinct exit code (80–89) and stage label;
each path-fd is opened via SAFE `std` with `O_PATH|O_CLOEXEC` so its `File` closes on every return.

## Decision 3 — a tested CAPABILITY, gated; production `run` is unchanged

Like VB-1, the monitor is delivered `test-harness`-gated (`landlock/`) plus a `sandboy __landlock-run`
harness entry that installs the policy, runs the effect-based self-check, and — only if fully
enforced — performs the probe op (`fs` writes or a non-allowed `exec`) inside the confinement. At
VB-2 the backend still cannot report `FullyEnforced` (network/env absent), so production `run` is the
untouched VB-0 honest bootstrap.

## Tests & gates

- `crates/sandboy/tests/landlock_confinement.rs` — `#[ignore]`d, real Landlock ABI ≥ 3 required.
  Mirrors the frozen oracles (`writes_are_confined_to_the_worktree`,
  `exec_of_a_non_allowed_binary_is_denied_by_the_kernel`) and adds the adversarial fail-closed matrix:
  unsupported/disabled (ENOSYS/EOPNOTSUPP), insufficient ABI, and a failure at each install stage
  (create_ruleset, add_rule, partial ruleset, no_new_privs, restrict_self) each report
  `not_enforced` and never run the op. A capability guard SKIPS when Landlock is absent, so
  `--include-ignored` is safe anywhere. A TEST-ONLY `O7_LL_FAULT` knob forces each stage to fail.
- Hosted `sandboy backend gate` COMPILES + lints the module (`--features test-harness`) and proves
  the `unsafe` surface is contained to `landlock/sys.rs` (production stays forbid-unsafe); it does
  not run the ignored tests (hosted runners can't guarantee Landlock).
- `sandbox-confinement.yml` (self-hosted, `workflow_dispatch`-only — trigger UNCHANGED) gains one
  step running the VB-2 Landlock acceptance with `--include-ignored` after the existing
  Landlock+seccomp preflight.

`libc` is the one new dependency (pinned, already in the workspace tree, MIT/Apache-2.0). No seccomp,
network, or env; no `confinement_backend()` switch; no RED-matrix flip. One layer; stop for re-gate;
do not start VB-3.
