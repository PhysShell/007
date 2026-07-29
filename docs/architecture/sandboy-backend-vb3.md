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

**Network deny-all is REAL, not a single-door lock.** `socket(2)` is not the only way to create an
INET socket: io_uring's `IORING_OP_SOCKET` does the equivalent. So the filter ALSO denies
`io_uring_setup`/`io_uring_enter`/`io_uring_register` (no ring → no `IORING_OP_SOCKET`; and `enter`/
`register` deny + the fd scrub close the inherited-ring path). An oracle proves `io_uring_setup`
succeeds unconfined and is `EPERM` after — one green `socket(AF_INET)` would only prove one locked door
in a building with a known service entrance.

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

Enforcement is never assumed, and the self-check is DIFFERENTIAL and AUTHORITATIVE — the backend's own
verdict, not just the acceptance tests. Before install, every intended-denied op is proven permitted
unconfined (`socket(AF_INET/INET6/UNIX)` and `io_uring_setup` succeed; `setsid`/`setpgid` succeed in
disposable children), and **those baseline results participate in the verdict**: a pre-existing denial
is a typed `BaselineDenied`, never re-attributed to this filter. After install: `socket(AF_INET/INET6)`,
the x32 socket, and `io_uring_setup` are **exactly `EPERM`** (not "any error" — an incidental `ENOSYS`
would not prove a deny rule; x32/io_uring checks are conditional on the op being available at baseline);
`AF_UNIX` still succeeds; `setsid`/`setpgid` are `EPERM` probed in **fresh children identical to the
baseline** (seccomp denies before the group-leader check, so the denial is unambiguous); a forked child
sees the same deny; a child inherits **zero socket descriptors**; and a child's environment is
**byte-exactly** the allowlisted key/value map (no forbidden name, no lost allowed name, values intact).

The inherited-fd scrub is FAIL-CLOSED: enumeration (`/proc/self/fd`) propagates every dir-open,
readdir, and non-numeric-entry error; `close` returns a result; and an **independent post-scrub
re-enumeration** proves only keep-fds survive (any survivor → `FdScrubIncomplete`). Multiple planted
descriptors (a socket AND a regular file) are the oracle.

Any mismatch is typed → `not_enforced`; the target never launches on failure. Every stage has a
distinct exit code (93–100), and a TEST-ONLY `O7_SC_FAULT` knob forces each — `no_new_privs` (95),
`apply` (96), fd enumeration failure (97), incomplete scrub (98), each omitted rule → `EffectMismatch`
(99), and baseline-denied (100) — so the whole matrix is proven RED, not decorative.

## Tests & gates

- `crates/sandboy/tests/seccomp_confinement.rs` — `#[ignore]`d, real seccomp-BPF required. The
  capability guard is an INDEPENDENT `prctl(PR_GET_SECCOMP)` probe (not the SUT), so a broken backend
  fails RED; `O7_REQUIRE_SECCOMP=1` makes an incapable host a FAILURE, never a skip. Covers: IPv4/IPv6
  deny with AF_UNIX preserved; the io_uring socket-creation deny; setsid/setpgid deny (exact EPERM in
  fresh children); the x32 anti-bypass oracle (exact EPERM); the fail-closed scrub of multiple planted
  descriptors + child-side zero-inherited-sockets; the BYTE-EXACT env allowlist (kept sentinel, dropped
  sentinel, and empty-allowlist cases); the frozen compiled-BPF digest; and the full typed failure
  matrix (95–100). A TEST-ONLY `O7_SC_FAULT` knob forces each stage.
- Hosted `sandboy backend gate` COMPILES + lints the module (`--features test-harness`) and proves the
  `unsafe` surface stays contained to `landlock/sys.rs` + `seccomp/sys.rs` (production forbid-unsafe).
- `sandbox-confinement.yml` (self-hosted, `workflow_dispatch`-only — trigger UNCHANGED) runs the VB-3
  acceptance with `--include-ignored --nocapture` and `O7_REQUIRE_SECCOMP=1` after the seccomp
  preflight.

One new dependency (`seccompiler`, pinned, Apache-2.0/BSD-3-Clause, deps: `libc` only). No Landlock/
cgroup change; no `confinement_backend()` switch; no RED-matrix flip. One layer; stop for re-gate; do
not start VB-4.
