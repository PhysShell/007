# Sandboy VB-3 — seccomp network/setsid/setpgid deny, fd scrub, env allowlist

VB-3 is the fourth Vertical B slice, stacked on the accepted+frozen VB-2 Landlock layer
(`73b69a304079d250e39d3e224beae3ed2588d5bf`). Scope, strictly: a real seccomp-BPF filter that denies
INET/INET6 socket creation and `setsid`/`setpgid`, an inherited-fd scrub, and an exact env allowlist —
each PROVEN by effect-based self-checks with typed fail-closed verdicts. **No Landlock/cgroup change;
no `confinement_backend()` switch; no RED-matrix flip; no VB-4 integration.**

## Decision 1 — the filter is BUILT by `seccompiler`, not hand-rolled

VB-2's raw-syscall approach was right for three fixed Landlock calls. A seccomp deny would otherwise
mean hand-writing a classic-BPF compiler: control flow, `seccomp_data` offsets, argument-width and
endianness handling, the architecture discriminator, and the x86_64 x32 namespace. That correctness
surface is not justified to avoid one audited pure-Rust dependency. So the policy is a typed
`seccompiler` `SeccompFilter` (pinned `=0.5.0`, default features OFF → only its typed API + `libc`, no
system libseccomp, no JSON), compiled to a `BpfProgram` and installed via `apply_filter` after
`prctl(PR_SET_NO_NEW_PRIVS)`. It is `test-harness`-gated like VB-1/VB-2: a production build compiles
neither the module nor `seccompiler`/`libc` nor any `unsafe`. The only new `unsafe` — the effect-probe
syscalls (`socket`/`setsid`/`setpgid`/x32/`fork`/fd ops) — lives in a new audited `seccomp/sys.rs`;
`seccompiler` itself needs none of ours.

## Decision 2 — the policy, and the x32 anti-bypass

Default action **Allow**; matched deny action **`Errno(EPERM)`**. Rules: `socket` denied ONLY when
arg0 ∈ {`AF_INET`, `AF_INET6`} (two OR-ed rules; `AF_UNIX` matches neither → Allow, so the ban is
family-scoped, not a blanket socket ban), and `setsid`/`setpgid` denied unconditionally (empty rule
vectors).

`seccompiler`'s compiled BPF already begins with an architecture gate that returns
`SECCOMP_RET_KILL_PROCESS` on an `AUDIT_ARCH` mismatch — fail-closed, never Allow. **But** x32 on
x86_64 reports the same `AUDIT_ARCH_X86_64` and issues syscalls as `native_nr | 0x40000000`; since
rules match exact numbers, a naive filter would let `socket` through the x32 number. So the deny rules
are ALSO installed for the x32 syscall numbers (`SYS_socket|X32`, `SYS_setsid|X32`, `SYS_setpgid|X32`),
and an adversarial oracle issues `socket(AF_INET)` through the x32 number and requires it be denied
(never a created socket). The x32-safe policy is expressed for x86_64; any other target fails the arch
gate (`UnsupportedArch`, not Allow). A digest of the compiled BPF is FROZEN in a test, so a
`seccompiler`/compiler change or a rule edit forces a deliberate review instead of silently altering
the policy.

## Decision 3 — install order, fd scrub, env; all effect-proven

The harness models the real launch order: **scrub inherited fds → construct env → install seccomp →
(child) run**. The fd scrub reads `/proc/self/fd`, then closes every fd not in the keep-set (stdio +
the result fd) — a planted non-CLOEXEC socket is the oracle. Env construction removes every variable
whose name is not in the allowlist. The filter is installed on the single launch thread and inherited
across fork/exec (proven by a forked child observing the same deny).

Enforcement is never assumed. Before install, the exact operations are proven permitted unconfined
(`socket(AF_INET/INET6/UNIX)` succeed; `setsid`/`setpgid` succeed in disposable children). After
install: `socket(AF_INET)` and `socket(AF_INET6)` are `EPERM`, `AF_UNIX` still succeeds, `setsid` and
`setpgid` are `EPERM` (seccomp denies before the kernel's group-leader check, so the denial is
unambiguous), the x32 socket is denied, a forked child sees the same deny, and a child's environment
holds only allowlisted names. Any mismatch is a typed `EffectMismatch` → `not_enforced`; every install
stage is a typed error with a distinct exit code (93–99), and the target is never launched on failure.

## Tests & gates

- `crates/sandboy/tests/seccomp_confinement.rs` — `#[ignore]`d, real seccomp-BPF required. The
  capability guard is an INDEPENDENT `prctl(PR_GET_SECCOMP)` probe (not the SUT), so a broken backend
  fails RED; `O7_REQUIRE_SECCOMP=1` makes an incapable host a FAILURE, never a skip. Covers: IPv4/IPv6
  deny with AF_UNIX preserved; setsid/setpgid deny; the x32 anti-bypass oracle; the planted
  non-CLOEXEC-socket scrub; the exact env allowlist + fork inheritance; the frozen compiled-BPF
  digest; and a forced install failure failing closed. A TEST-ONLY `O7_SC_FAULT` knob forces the
  apply stage.
- Hosted `sandboy backend gate` COMPILES + lints the module (`--features test-harness`) and proves the
  `unsafe` surface stays contained to `landlock/sys.rs` + `seccomp/sys.rs` (production forbid-unsafe).
- `sandbox-confinement.yml` (self-hosted, `workflow_dispatch`-only — trigger UNCHANGED) runs the VB-3
  acceptance with `--include-ignored --nocapture` and `O7_REQUIRE_SECCOMP=1` after the seccomp
  preflight.

One new dependency (`seccompiler`, pinned, Apache-2.0/BSD-3-Clause, deps: `libc` only). No Landlock/
cgroup change; no `confinement_backend()` switch; no RED-matrix flip. One layer; stop for re-gate; do
not start VB-4.
