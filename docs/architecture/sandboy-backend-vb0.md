# The real Sandboy backend — VB-0 authority & fail-closed bootstrap

Milestone VB (Vertical B) replaces the frozen *fake* enforcer with a REAL external `sandboy`
backend that installs Landlock + seccomp + a monitor-owned cgroup v2 and reports each of the
five dimensions honestly after a self-check. **VB-0 is the first slice: it establishes the
real backend as an honest production AUTHORITY that later slices fill with enforcement.** It
adds NO kernel enforcement — none, not half. See `docs/architecture/sandboy-boundary.md` for
the frozen Vertical A protocol/lifecycle and the Vertical B RED acceptance matrix.

This note FIXES the load-bearing VB-0 decisions before any code (the design the milestone
directive requires be pinned first).

## 1. Backend source / build / package location

- The backend lives in a **new workspace member crate `crates/sandboy`** — a binary crate
  whose only target is the `sandboy` executable.
- It depends on exactly one thing: the pure `o7-sandbox-protocol` crate (a path dependency).
  At VB-0 it needs NOTHING else — the control transport is the fd-0 socket obtained through a
  SAFE owned clone of stdin, framing/decoding/digests come from the protocol crate, and the
  backend digest is `std::fs` reading `/proc/self/exe`. **Zero new external dependencies**, so
  `cargo deny` (bans/licenses/sources/advisories) is untouched.
- Build/artifact: it is built by the ordinary workspace build (`cargo build -p sandboy`), so
  the hosted gates compile and test it with no special toolchain. The exact artifact the
  parent trusts is obtained by `BackendImage::acquire(path, digest, identity)`, which stages
  the bytes into a SEALED memfd and execs `/proc/<owner_pid>/fd/<n>` — never a PATH search.
  Tests locate the built binary as the sibling of the test binary in the workspace target
  dir, building it on demand if absent.

## 2. Separate Cargo / toolchain / `unsafe` boundary

- Every EXISTING 007 crate sets `[lints.rust] unsafe_code = "forbid"` in its OWN manifest
  (there is no `[workspace.lints]` inheritance). `crates/sandboy` is the ONE crate that does
  **not** forbid `unsafe`: it is the designated external confinement trust boundary, exactly
  as `docs/architecture/sandboy-boundary.md` Decision 1 requires (Landlock/seccomp/`pre_exec`
  are `unsafe`). No existing crate's `forbid` is relaxed.
- **At VB-0 the backend contains ZERO `unsafe`, compiler-enforced.** The manifest establishes
  the boundary (it does not forbid `unsafe`), but `crates/sandboy/src/main.rs` additionally
  pins a crate-level `#![forbid(unsafe_code)]` so this slice's inventory is provably empty. The
  slice that first installs Landlock/seccomp (VB-2/VB-3) removes that ONE line to unlock
  `unsafe` HERE — and nowhere in the forbid-unsafe 007 crates.
- Toolchain: VB-0 builds on the repo's floating `stable`. If a later slice needs a specific
  toolchain for a syscall surface, it may add `crates/sandboy/rust-toolchain.toml` without
  perturbing the workspace — the boundary is per-crate.

## 3. Future syscall / `unsafe` surfaces (fixed now, added later — NOT in VB-0)

| Slice | Surface | `unsafe`/syscall |
|-------|---------|------------------|
| VB-1  | monitor-owned cgroup v2 leaf, `cgroup.kill` teardown, timeout | cgroup fs writes (safe fs); `fork`/`clone` monitor topology (`unsafe`) |
| VB-2  | Landlock ruleset (ABI ≥ 3: `MAKE_REG`/`WRITE_FILE`/`TRUNCATE`, exec) | `landlock_create_ruleset`/`add_rule`/`restrict_self` (`unsafe`) |
| VB-3  | seccomp filter (IPv4/IPv6 socket deny, `setsid`/`setpgid` deny), fd scrub, env allowlist | `seccomp`/`prctl` BPF install, `pre_exec` (`unsafe`) |
| VB-4  | flip dimensions to `enforced` after self-check; enable the on-GO monitor→target spawn | wiring only |

VB-0 adds NONE of these. It must not add Landlock/seccomp/cgroup even half-way.

## 4. Protocol state machine (VB-0)

The backend speaks the frozen protocol verbatim (`docs/architecture/sandboy-boundary.md`
Decisions 3–4). Argv: `run --launch-nonce <n> [--deny-net] --worktree <p> --timeout-ms <ms>
[--allow-exec <p>]… [--allow-env <name>]… -- <sealed target descriptor>`. The control transport
is the single bidirectional socket on **fd 0**.

```
parse argv ──▶ acquire fd-0 control socket
   │ (bad argv / no socket)                 └─▶ EXIT non-zero, no report, no target
   ▼
read ONE length-prefixed LaunchRequest frame  (bounded by MAX_REQUEST_BYTES)
   │ (truncated / oversized / malformed / dup-env / bad version)
   │                                         └─▶ EXIT non-zero (fail closed), no report, no target
   ▼
reconstruct policy from argv → policy_digest;  launch_spec_digest = request.spec_digest();
backend_digest = sha256(/proc/self/exe);  identity = sandboy-linux / 0.1.0
   ▼
INSTALL CONFINEMENT  ── VB-0: a no-op stub that installs nothing ──▶ self-check = NOT enforced
   ▼
emit ONE bound report frame: correct bindings, EVERY dimension = not_enforced (HONEST downgrade)
   ▼
HALF-CLOSE the write side  (parent proves end-of-message by reading EOF)
   ▼
read the parent's GO byte
   │ EOF / NACK ──────────────▶ EXIT, target NEVER runs
   │ 'G' (GO) ── VB-0: self-check FAILED, so the target is STILL refused ──▶ EXIT, no target
```

**Invariant:** the target NEVER runs at VB-0 — the self-check is never satisfied, so even a
(spurious) GO does not start it. "install confinement → self-check → refuse on failure"
happens BEFORE any target could run, so a downgraded result can never mean "the target
already ran unconfined." The on-GO monitor→target spawn is added only by the slice that first
achieves full enforcement (VB-4).

## 5. Cleanup ownership

- At VB-0 the backend starts NO monitor and NO target, so it owns no process set to tear
  down; on any failure it simply EXITS non-zero.
- The BACKEND process itself is owned by the PARENT: `SandboyBoundary::spawn` runs it in its
  own process group and, on a rejected/mis-bound/downgraded report or any error, `killpg`s +
  reaps the whole group and PROVES it drained (`kill_and_reap`), with cancel-safety via
  `SpawnGroupGuard`. That ownership is frozen Vertical A and unchanged here.
- Group teardown of a backend that forked descendants is already covered by the frozen fake's
  modes; VB-0 does not re-test it. VB-0's "no surviving monitor/descendant after rejection"
  holds because the real backend spawns nothing.

## 6. Confinement runner availability

- No self-hosted `[self-hosted, linux, x64, confinement]` runner is provisioned. The
  `sandbox-confinement.yml` workflow stays `workflow_dispatch`-only and NON-required, and the
  RED matrix (`sandbox_confinement.rs`, `confinement_backend()` → the fake) is UNCHANGED.
- **VB-0 needs no kernel capabilities** (no Landlock/seccomp/cgroup), so its tests are fully
  PORTABLE and run in the hosted `o7-worker gate` and a dedicated `sandboy` gate. The
  confinement runner is required only from VB-1 onward.

## 7. What VB-0 does NOT do

No Landlock, seccomp, or cgroup (not even half). No switch of `confinement_backend()` to the
real backend (that is VB-4). No `pull_request` trigger on the confinement workflow. No target
execution. No o7d / Tandem / Cockpit. One meaningful layer; stop for re-gate.
