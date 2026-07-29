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

## Decision 2 — enforcement is PROVEN against an unconfined baseline

A ruleset fd or an ABI number restricts nothing on its own — the kernel API separates *creating* a
ruleset, *adding* rules, and *applying* the restriction. So VB-2 considers the filesystem dimension
`enforced` **only** after the full ordered sequence succeeds — `create_ruleset` → `add_rule`* →
`prctl(PR_SET_NO_NEW_PRIVS)` → `restrict_self` — **and** a DIFFERENTIAL self-check.

The self-check is differential because a bare EACCES proves nothing: Landlock composes with DAC and
other LSMs, so a denial could come from anywhere. VB-2 therefore performs the exact outside-worktree
write **before** `restrict_self` and requires it to SUCCEED (establishing that, unconfined, the op is
allowed), then repeats it **after** `restrict_self` and requires EACCES/EPERM — so the post-restrict
denial is attributable to Landlock alone. An inside-worktree write must succeed both times. A baseline
that fails unconfined (read-only dir, DAC, another LSM) is its own `not_enforced` verdict
(`baseline_outside` / `baseline_inside`), never a false "confined". `PR_SET_NO_NEW_PRIVS` is mandatory:
without it (or `CAP_SYS_ADMIN`) `restrict_self` returns `EPERM`.

The runtime ABI is probed FIRST, and the exact minimum the frozen oracle needs is required: **ABI 3**,
for `LANDLOCK_ACCESS_FS_TRUNCATE`. Absent (`ENOSYS`), disabled (`EOPNOTSUPP`), or insufficient (`< 3`)
reports `not_enforced` and NEVER runs. Every stage is a typed `InstallError` → distinct exit code
(80–92) + label; each path-fd is opened via SAFE `std` `O_PATH|O_CLOEXEC` and RAII-closed on every path.

## Decision 2b — the writable worktree is NOT an executable root

The frozen policy splits two powers: the worktree is the single **writable** root; `allow_exec` names
the **read+execute** roots. So the worktree rule grants `WORKTREE_RIGHTS` = every handled FS right
**except `EXECUTE`** — otherwise, since a directory `path_beneath` rule applies to everything beneath
it, a process could drop or copy an executable into the writable worktree and run it despite the
worktree being absent from `allow_exec`. `EXECUTE` stays in the ruleset's HANDLED set, so it is denied
wherever it is not granted.

Rule installation is **TOCTOU-safe**: every rule object (the worktree and each `allow_exec` entry) is
opened **exactly once** via `O_PATH`, and rules attach to those SAME fds — no pathname is re-resolved
between any decision and the rule attachment, so a concurrent rename/symlink swap cannot redirect a
rule to a different object, and an open/identity failure fails closed (no lexical guessing). A
descendant of the worktree that is also allow-listed needs **no** explicit union: the worktree
ancestor rule grants write and the nested `allow_exec` rule grants execute/read, and Landlock's
documented same-layer hierarchy semantics accumulate them along the path. The only combined rule is
for the **exact same object** — an `allow_exec` entry whose opened identity (`dev`,`ino`) equals the
worktree's — folded into a single deliberate rule rather than a duplicate. A deterministic race oracle
(a test-only barrier between open and attach) proves a mid-setup pathname swap grants no outside
authority.

## Decision 2c — object-type-correct rules and fd-based execution

`allow_exec` rules are masked by the OPENED object's type (`fstat` of the same `O_PATH` fd, no TOCTOU):
a **directory** may hold `EXECUTE|READ_FILE|READ_DIR`; a **regular file** only the file-applicable
subset (`READ_DIR` on a file makes `add_rule` return EINVAL); any other type is rejected fail-closed.

Execution goes through the fd, not the path: the target is opened BEFORE `restrict_self` and run with
`execveat(fd, "", AT_EMPTY_PATH)` (fexecve) AFTER. For a real file the kernel still enforces the
Landlock EXECUTE right at `execveat` (a non-allowed binary is denied EACCES); a sealed memfd — an
anonymous inode with no filesystem path, so un-nameable by any `path_beneath` rule — executes because
no path rule can name it, while the surrounding filesystem stays confined. This mirrors the frozen
`a_sealed_proc_fd_target_executes_under_confinement` contract.

## Decision 3 — a tested CAPABILITY, gated; production `run` is unchanged

Like VB-1, the monitor is delivered `test-harness`-gated (`landlock/`) plus a `sandboy __landlock-run`
harness entry that installs the policy, runs the effect-based self-check, and — only if fully
enforced — performs the probe op (`fs` writes or a non-allowed `exec`) inside the confinement. At
VB-2 the backend still cannot report `FullyEnforced` (network/env absent), so production `run` is the
untouched VB-0 honest bootstrap.

## Tests & gates

- `crates/sandboy/tests/landlock_confinement.rs` — `#[ignore]`d, real Landlock ABI ≥ 3 required. The
  capability guard is an INDEPENDENT raw-syscall ABI probe (NOT the system-under-test), so a broken
  backend fails RED rather than skipping green; with `O7_REQUIRE_LANDLOCK=1` an incapable host is a
  TEST FAILURE, never a skip. Covers: the frozen filesystem oracle; BOTH exec directions (allowed
  directory executable, allowed exact-file rule, sealed `/proc/<pid>/fd/<n>` memfd, and a non-allowed
  binary denied); and the full fail-closed matrix — unsupported/disabled, insufficient ABI, each
  install stage, the four self-check verdicts (outside-allowed/outside-inconclusive/inside-denied and
  both baselines), and an unsupported object type. A TEST-ONLY `O7_LL_FAULT` knob forces the
  hard-to-stage stages; the baselines and the object-type case are real conditions.
- Hosted `sandboy backend gate` COMPILES + lints the module (`--features test-harness`) and proves
  the `unsafe` surface is contained to `landlock/sys.rs` (production stays forbid-unsafe); it does
  not run the ignored tests (hosted runners can't guarantee Landlock).
- `sandbox-confinement.yml` (self-hosted, `workflow_dispatch`-only — trigger UNCHANGED) runs the VB-2
  Landlock acceptance with `--include-ignored --nocapture` and `O7_REQUIRE_LANDLOCK=1` after the
  existing Landlock+seccomp preflight.

`libc` is the one new dependency (pinned, already in the workspace tree, MIT/Apache-2.0). No seccomp,
network, or env; no `confinement_backend()` switch; no RED-matrix flip. One layer; stop for re-gate;
do not start VB-3.
