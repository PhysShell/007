# Sandboy process boundary (PR 4.5)

The `UnconfinedHostBoundary` (PR 2) gives lifecycle control and attests
`EnforcementLevel::None`. Sandboy is the *mandatory* boundary that must land before any live
provider run: it attests `Sandboy` / `FullyEnforced`, backed by real Linux confinement
(Landlock + seccomp + cgroup v2), and it must PROVE what it enforced, not merely claim it.

This note records the load-bearing decisions. **Vertical A** (the authorized launch/monitor
lifecycle over a controlled fake backend) is landed and FROZEN; its mechanism is described
below in the present tense. **Vertical B** (the real kernel confinement backend) is the next
slice; its contract and its RED acceptance matrix are in the final section.

## Decision 1 — the confinement runs in an EXTERNAL `sandboy` backend, not in-crate

Every 007 crate sets `unsafe_code = "forbid"`. Applying Landlock/seccomp to the child of a
`tokio`/`std` `Command` requires a `pre_exec` hook (`unsafe fn`) between `fork` and `exec`, and
`seccomp`/`landlock` syscalls are `unsafe`. We will not add `unsafe` to `o7-worker`.

So the confinement is applied by an external `sandboy` executable — a SEPARATE binary that
lives outside the forbid-unsafe 007 crates (its own crate/toolchain may use `unsafe`).
`SandboyBoundary` launches it through the spawn seam: `executable = <sealed backend descriptor>`
(`/proc/<owner_pid>/fd/<n>` of the acquired, sealed backend object — never a PATH search),
`arguments = ["run", "--launch-nonce", <n>, <policy flags…>, "--", <sealed target descriptor>]`.
All `unsafe` lives in the backend binary and its dependencies; the 007 crates stay `unsafe`-free.

## Decision 2 — MONITOR topology (not exec-in-place), same PID namespace

An earlier draft had the backend `exec` the target *in place*. That is unsound: after `exec`
the backend no longer exists, so it cannot enforce a wall-clock timeout, kill a runaway process
tree, or observe escaped descendants — `timeout: enforced` and `process_tree: enforced` would be
wishes typed into a JSON file.

Instead the backend is a **monitor**:

1. it installs the Landlock ruleset + seccomp filter and creates a cgroup v2 it owns;
2. it forks a confined child (placed in the owned cgroup) that `exec`s the real target;
3. it stays alive as the cgroup-owning monitor — enforcing the deadline (kill the cgroup on
   timeout), the process-tree teardown, and reporting on them honestly.

The confined child INHERITS the monitor's process group and owned cgroup — it does NOT create
its own group. So `remaining_members`/`force_stop` on the returned leader cover the target and
every descendant; a `process_group(0)` on the target would orphan it from the owned set. `setsid`
/ `setpgid` are denied by seccomp as defense in depth, so the target cannot escape the owned set
even by starting a new session — and cgroup membership contains what process-group ownership
alone cannot.

Crucially the monitor does **not** unshare a new PID namespace: the confined child shares the
verifier's PID namespace, so PR 3's sealed-memfd exec via `/proc/<verifier_pid>/fd/<n>` still
resolves. Landlock is a filesystem *restriction*, so the policy MUST grant read+execute on the
sealed-memfd exec target, or the `exec` is denied and the run fails closed.

The `BoundaryProcess` the worker returns wraps the **monitor** as leader, so the supervisor's
stop/wait/membership act on the cgroup owner. The wall-clock deadline lives in the `SandboxPolicy`
(enforced by the monitor), so `WorkerSpec` needs no separate deadline.

## Decision 3 — a single bidirectional control socket on the backend's STDIN

The control transport is ONE `UnixStream` pair, created CLOEXEC from creation and mapped onto the
backend's STDIN by `Command` (an atomic dup in the forked child). There is therefore no
inheritable-descriptor window a concurrent sibling spawn could race, and NO control descriptor
number appears on the `/proc`-visible argv. (The earlier three-pipe design — `--report-fd` /
`--control-fd` / `--request-fd`, parent ends made CLOEXEC only *after* creation — left exactly
that window and is retired.)

Over the one socket, in order:

1. the parent writes ONE length-prefixed `LaunchRequest` frame — the exact execution (target
   argv, cwd, allowlisted env, stdin) that would otherwise leak onto the `/proc`-visible argv;
2. the backend writes ONE length-prefixed report frame (4-byte big-endian length + JSON body,
   body `<= MAX_REPORT_BYTES` = 64 KiB), then HALF-CLOSES its write side;
3. the parent reads the body then proves end-of-message: one more read must be EOF. A trailing
   byte is a protocol violation and fails closed. The whole read is wall-clock bounded;
4. the parent verifies the report and writes the GO byte; a NACK is simply the parent dropping
   its end (the backend's GO read sees EOF).

The backend installs confinement and self-checks full enforcement BEFORE it `exec`s the target,
so a `Partial` result never means "the target already ran unconfined while the parent was reading
JSON". On GO the backend closes the control socket and SCRUBS every inherited descriptor except
stdout/stderr (enumerating `/proc/self/fd`), and starts the target with a NULL stdin — so no
control channel, and no other inherited fd, reaches the confined target.

## Decision 4 — the report is BOUND to policy, launch, backend, and target; no boolean `secure`

Per dimension (`filesystem`, `network`, `env`, `process_tree`, `timeout`), each one of
`enforced` / `partial` / `not_enforced` — never a boolean `secure` (issue #53: "how software lies
to itself before a breach report"). The report also carries `schema_version`, a typed `backend`
identity, and binding fields:

- `policy_digest` — must equal the parent's canonical `SandboxPolicy::digest()`;
- `launch_nonce` — the per-spawn nonce the parent minted; rejects a stale/replayed report;
- `backend_digest` — the digest of the sealed backend object the report came from;
- `launch_spec_digest` — a digest over the exact INVOCATION (sealed target bytes + argv + cwd +
  allowlisted env + stdin), not merely which binary.

`SandboyBoundary::verify_report` fails closed unless all bindings match AND every dimension is
`enforced`. `deny_unknown_fields` rejects a stray `secure`; every dimension is a required field so
a missing one is a parse error, not a silent "enforced". These types live in the pure
`o7-sandbox-protocol` crate so the external backend emits the exact same protocol without
depending on the worker runtime; o7-run stores the raw frame as an opaque content-addressed
artifact.

## Decision 5 — evidence flows through the seam; fail closed at construction AND launch

`spawn` returns `BoundaryLaunch { process, evidence }`, where `BoundaryEvidence` is the run-time
proof established by THIS launch (distinct from the pre-spawn configured `attestation()`).
`BoundaryEvidence` is a two-variant ENUM, not a struct with an `Option<report>`, so "fully
enforced but no report" is unrepresentable: a launch either established nothing (`Unconfined`) or
established confinement and carries the verified `report` (`Reported`).

- `SandboyBoundary::new` rejects a policy that cannot fully confine.
- `attestation()` reflects the configured intent (`Sandboy` / `FullyEnforced`).
- The supervisor publishes `LaunchEvidence` BEFORE `Spawned`, and — defense in depth — tears the
  process down and fails closed unless `BoundaryEvidence::satisfies` holds: under
  `RequireFullyEnforced` the evidence must be `Reported`, fully enforced, AND its implementation
  must equal the configured one. A missing report, a downgrade, or an implementation swap all fail
  closed.
- The downstream adapter persists the `Reported` report bytes as o7-run's
  `SandboxEvidenceCaptured`.

## Decision 6 — trust bound to OBJECTS, authorized by a barrier, cancel-safe

- **Backend + target are SEALED objects, not paths.** `BackendImage` has private fields and is
  built only via `acquire(path, expected_digest, identity)`, which stages the bytes into an
  anonymous `memfd`, applies `WRITE|GROW|SHRINK|SEAL`, verifies the seals, reads the sealed bytes
  back to bind the digest, and retains the sealed descriptor (`SealedObject`, `sealed.rs`). The
  launch execs `descriptor_path()` (`/proc/<owner_pid>/fd/<n>`); a source-path swap or in-place
  inode rewrite cannot change what runs. The target is acquired the same way, and the launch binds
  `launch_spec_digest` to those exact sealed bytes.

- **Hardened acquisition.** An ORDINARY source path is opened `O_NOFOLLOW | O_NONBLOCK |
  O_CLOEXEC` and proven a regular file with a drift-checked read (a symlink final component and a
  blocking FIFO both fail closed). The frozen PR-3 executable — a `/proc/<pid>/fd/<n>` procfs
  MAGIC symlink — takes a separate path: it is FOLLOWED (a universal `O_NOFOLLOW` would break the
  one executable contract PR-3 requires), required to be a regular object AND a fully sealed
  memfd, then read. Acquisition runs off the runtime thread (`spawn_blocking`).

- **Plane separation.** The backend is a TRUSTED launcher: it runs from a fixed trusted cwd (`/`)
  with a trusted control-plane environment, NEVER the untrusted target's cwd/env (LD_PRELOAD,
  LD_LIBRARY_PATH, …). The target's argv/cwd/allowlisted-env/stdin travel out-of-band in the
  `LaunchRequest` over the control socket, never on the `/proc`-visible argv or the backend env.

- **Parent authorization barrier.** The two-phase handshake (Decision 3) holds the confined target
  before it starts until the parent has verified the report and sent GO. A malformed / mis-bound /
  downgraded report, a NACK, EOF, or a cancel makes the backend kill its cgroup and reap — the
  target never runs on an unverified report.

- **Cleanup is PROVEN, and cancel-safe.** On any error the backend GROUP is killed and the whole
  owned set is proven drained (an unproven reap dominates the triggering error). From the moment a
  valid monitor PID exists until ownership transfers to the returned `BoundaryLaunch`, dropping the
  `spawn` future `killpg`s the whole group — leader + target + any descendant — so a cancelled
  launch never leaks a partial process set. A `FullyEnforced` launch that cannot read the monitor's
  live process identity (start time — the PID-reuse guard) fails closed with cleanup.

- **Fail-closed inputs.** The 128-bit launch nonce comes straight from `getrandom`
  (`OsNonceSource`), no fallback; an RNG failure fails the launch before any backend spawn.
  `SandboxPolicy::validate` rejects duplicate exec/env allowances (sets, not multisets).

## Test-harness lock (production isolation)

The controlled fake backend, its config knob (`BackendConfig::fake_mode`), the cancellation
barrier (`SandboyBoundary::with_post_go_barrier`), and the harness binaries are gated behind a
NON-DEFAULT `test-harness` Cargo feature. Cargo features are not private, so the guarantee is
REPOSITORY POLICY, enforced fail-closed by `o7-harness-policy`: the feature may be enabled from
exactly one dependency edge — o7-worker's own dev-dependency self-edge (resolved even through a
`package = "..."` rename or an inherited `workspace = true` alias) — and nowhere else; every
harness bin (`sandboy_fake`, `spawn_probe`, `fd_probe`, `sandbox_probe`) is `required-features`-gated
with `autobins = false`; and a compile probe proves the default o7-worker API has no `fake_mode` /
`with_post_go_barrier`.

## Vertical A — landed and FROZEN

GREEN and enforced: the pure `o7-sandbox-protocol` crate; the evolved evidence seam and its
supervisor wiring; the LIVE authorized launch — hardened backend+target acquisition, the single
control socket with the request → report → EOF-proof → GO choreography, monitor OWNERSHIP of the
target's process group (a monitor that exits leaving a live owned target fails closed even on a
clean code), full inherited-fd scrub, proven group-draining cleanup, and cancel-safety on the
spawn-future drop path; the CSPRNG nonce with its RNG-failure fail-closed; policy set-validation;
and the whole test-harness lock. The fake backend does NOT confine — it is the protocol/lifecycle
stand-in, not a kernel enforcer.

## Vertical B — real kernel confinement (RED contract)

Vertical B replaces the fake enforcer with the real external backend that installs Landlock +
seccomp + a monitor-owned cgroup v2 and reports each of the five dimensions HONESTLY after a
self-check. The acceptance tests below are RED against a NON-confining stand-in (the frozen fake,
which reports `enforced` but installs nothing): a target launched through it ESCAPES, so each
assertion observes the escape with a CONCRETE oracle rather than an `is_err()`. GREEN turns them
green by making the real backend actually enforce — the tests are not rewritten. Every RED asserts
a specific effect, never a vacuous failure.

Each dimension names its concrete oracle (the acceptance tests live in
`crates/o7-worker/tests/sandbox_confinement.rs`):

- **Filesystem / Landlock.** A write inside the single writable worktree succeeds; OUTSIDE it, THREE
  distinct operations each get a concrete kernel denial captured as the EXACT errno
  (`EACCES`/`EPERM`), not "the command failed" — creating a new file (`MAKE_REG`), a NON-truncating
  write to a pre-existing file (`WRITE_FILE`), and truncating a pre-existing file (`TRUNCATE`, ABI 3)
  — because Landlock governs them with separate rights and a partial ruleset could deny one while
  leaking another. The pre-existing files' bytes and size are additionally asserted UNCHANGED, so a
  wrong errno cannot mask actual corruption. Execute is restricted KERNEL-side: the confined
  target's own `execve` of a binary outside
  `allow_exec` is denied (exact errno), proven by a secondary marker that must never be created — the
  *lexical* parent-gate refusal is a distinct, already-frozen Vertical A test
  (`sandbox_contract::wrapping_an_unpermitted_target_fails_closed`), not part of this matrix. A live
  sealed `/proc/<pid>/fd/<n>` source actually EXECUTES under Landlock with no path-copy fallback and
  is still confined. An unavailable Landlock ABI, or an inability to install the full ruleset, never
  becomes `enforced`.

- **Network / seccomp.** The target cannot create an IPv4 or IPv6 socket; the denial is the exact
  `EPERM`/`EACCES` — an `EAFNOSUPPORT` is NOT a denial. A MANDATORY unconfined baseline requires the
  designated host to support IPv4 AND IPv6; a runner missing IPv6 is an environment gate FAILURE, not
  a reason to drop the IPv6 leg. No network descriptor is inherited: a separate test PLANTS a real
  non-CLOEXEC socket and confirms the confined target still reports `inherited_sockets=0`, and the
  enumeration probe fails CLOSED (an enumeration/parse/readlink error exits non-zero with no success
  marker, never a silent `0`).

- **Environment.** The target receives exactly the allowlisted names; no backend/control
  environment reaches it. Inability to prove the exact env construction is not `env: enforced`.
  (Enforced today by plane separation — a GREEN carryover, not RED.)

- **Process tree / seccomp escape.** `setsid` and `setpgid` are each denied with the exact `EPERM`,
  in SEPARATE probe processes each starting from the target's initial state — so `setpgid` cannot
  pass merely because a prior `setsid` already made the process a session leader. Each has an
  unconfined `OK` baseline. (A correct filter makes "the escape survives teardown" impossible, so
  escape-survival is NOT the oracle — the denial errno is.)

- **Process tree / cgroup v2.** Monitor, target, an ordinary child, and a DOUBLE-FORKED (reparented)
  descendant are provably in ONE owned cgroup that is a DEDICATED non-root leaf, distinct from the
  harness's own cgroup (`0::<path>` per `/proc/<pid>/cgroup`, membership in `cgroup.procs`); after
  `force_stop`/drain the exact PID + start-time IDENTITIES disappear (not merely the PID number, so
  PID reuse cannot read as teardown) and the cgroup DIRECTORY is removed — a killpg-only backend that
  never creates a cgroup cannot pass. A forced setup failure, a report failure, and a cancellation
  all leave no cgroup.

- **Timeout.** A target outliving the policy deadline is killed WITH its double-forked descendant.
  The whole teardown — kill, reap, cgroup removal, monitor exit — must COMPLETE inside ONE ABSOLUTE
  window (`timeout_at` from the moment of spawn, `deadline + fixed grace`); the completion RESULT is
  asserted, so a monitor that finishes one millisecond late FAILS rather than passing. Member
  identities are captured in PARALLEL so acquisition cannot itself consume the window; the teardown
  oracle (identities gone, `survived` marker absent, cgroup directory removed) is read before any
  `force_stop`, which runs only as post-RED cleanup.

- **Report truthfulness + barrier.** `enforced` is emitted only after all five dimensions are
  installed and self-checked. A four-stage RED matrix injects a test-only fault on the backend's
  control-plane env and requires each stage to prove a DISTINCT failure path via a stage-specific
  control-plane WITNESS (so a backend that returns one generic error for every fault passes none):
  `landlock`, `seccomp`, `cgroup` each a setup failure → `spawn` fails closed, no target runs (an
  immediate first-action `ran` marker, never a marker written only after later steps), the backend
  witnesses reaching that install stage; and `self-check` → a syntactically valid report with a
  downgraded dimension that the parent rejects as a report-VERIFICATION failure (`NotFullyEnforced`,
  not a crash). GO follows verification only; the report stays bound to backend object, policy,
  nonce, and launch spec.

**Designated confinement CI job.** A separate Linux job runs the real-kernel acceptance matrix with
`--include-ignored` — but only after a REAL capability preflight that EXERCISES each mechanism, not
a presence grep:

- **cgroup v2** — create a child cgroup under the delegated subtree, require a WRITABLE `cgroup.kill`
  (no PID-kill fallback — the GREEN backend tears down via `cgroup.kill`, so a runner without it
  fails the gate), move a disposable process in, confirm `cgroup.procs` membership, kill via
  `cgroup.kill`, confirm the group drains, and remove the directory (a `trap` reaps the probe's own
  scratch process/cgroup on any intermediate failure);
- **Landlock** — query the ABI (must be `>= 3`, the minimum that governs file `TRUNCATE`) and CREATE
  a representative ruleset that includes the `TRUNCATE` access right (bit 14);
- **seccomp** — install a minimal BPF filter in a disposable child and confirm a chosen syscall is
  denied with the expected errno.

A missing or unusable capability is an EXPLICIT gate FAILURE — never a skip and never a green
"unsupported". Landlock/seccomp are driven through Python `ctypes` (a CI tool), so no confinement
syscall enters the forbid-unsafe Rust tree. The portable workspace tests stay host-agnostic; the
mandatory kernel job has a pre-provisioned environment. RED until the real backend lands; GREEN turns
the matrix green by making the backend actually enforce.
