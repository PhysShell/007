# VB-4 blocker: the frozen execution-authority oracle is unsatisfiable on real Landlock

**Status:** Reviewer approved realization **A** (private ruleable execution object). The mechanism is
now proven on the designated host (see §7). VB-4 stops at this checkpoint before the backend
implementation completes the remaining matrix.
**Raised by:** VB-4 real-host validation (Stage 4), the run the acceptance package mandates.
**Scope:** `crates/sandboy` Landlock composition + `crates/o7-worker/tests/sandbox_confinement.rs`
frozen oracle set. No VB-0…VB-3 commit is rewritten by this note.

This is **not** an implementation detail. It is a conflict between two *frozen* security
properties that only surfaces when the real backend actually executes a sealed target under a real
Landlock ruleset. VB-4 implementation is paused pending a reviewed contract correction; the oracle
must not be silently weakened inside the implementation.

---

## 1. What works

The VB-4 production launch state machine is built and validated end-to-end on the designated host
(cgroup v2 + Landlock ABI ≥ 3 + seccomp). **12 of the 17 frozen oracles pass on the real composed
backend**, including: writes confined to the worktree (create/overwrite/truncate each denied with
the exact errno), IPv4/IPv6 socket denial, `setsid`/`setpgid` denial, byte-exact `LaunchSpec` env,
the planted-non-CLOEXEC-socket scrub, and *both* sealed-fd execution cases. The two-party
READY/GO/release protocol, the fd ledger, the immutable `VerifiedLaunchPlan`, and the cgroup
teardown all function.

The remaining failures are **not** state-machine bugs. They are two contradictory requirements on
the *filesystem-execute* dimension, described below.

## 2. The contradiction

The boundary seals every target — real file or `/proc/<pid>/fd/<n>` source — into an **anonymous
`memfd`** (`SealedObject::stage` → `memfd_create` + `F_ADD_SEALS`). Seal-based immutability is the
whole point: even a same-UID process cannot mutate a fully-sealed `memfd`, closing the hash→exec
TOCTOU. The launch child inherits this `memfd` fd and becomes the target via
`execveat(fd, "", …, AT_EMPTY_PATH)`.

The frozen matrix simultaneously requires, on that same anonymous inode:

| Oracle | Requirement | Landlock need |
|---|---|---|
| `a_sealed_proc_fd_target_executes_under_confinement` | the sealed `memfd` **executes** | memfd must be `EXECUTE`-authorized |
| `the_owned_cgroup_contains_the_tree…`, `…deadline…` | target (`dash` memfd) **execs** `/bin/dash`, `/bin/sleep` (in `allow_exec`) | `EXECUTE` granted to `allow_exec` |
| `exec_of_a_non_allowed_binary_is_denied_by_the_kernel` | target **must not** exec `/usr/bin/env` (outside `allow_exec`) | `EXECUTE` **denied** outside `allow_exec` |

Real Landlock cannot provide all three for a pathless inode. This is proven, not inferred.

## 3. Kernel evidence

All results on the designated host (Linux 6.18, Landlock ABI 7), reproduced with a minimal C-like
Rust probe calling the raw `landlock_*` syscalls.

1. **A `memfd` cannot be named by a Landlock rule.** `landlock_add_rule(PATH_BENEATH, {parent_fd =
   memfd})` returns **`EBADFD` (errno 77)** — for the `memfd` fd directly, for a `dup`, for an
   `O_PATH` open of `/proc/self/fd/<n>`, and for an `O_RDONLY` reopen. A memfd lives on the internal
   anonymous `shmem` mount; it is not part of any path hierarchy, and the kernel says so.
   The production error observed verbatim during the run:
   `add_rule "/proc/<pid>/fd/<n>": File descriptor in bad state (os error 77)`.

2. **An anonymous inode is covered ONLY by a rule rooted at `/`.** `execveat` of the sealed memfd
   succeeds **only** when `EXECUTE` is granted on `/`. Every narrower grant — `/proc`,
   `/proc/self/fd`, `/proc/self`, `/dev/shm`, `/tmp`, the worktree — leaves the memfd `execveat`
   denied with `EACCES`.

3. **`execveat` of the memfd is checked whenever `EXECUTE` *or* `READ_FILE` is handled.** Handling
   `EXECUTE` alone, or `READ_FILE` alone, or both, denies the pre-opened memfd `execveat` (`EACCES`)
   unless it is root-granted. Handling **neither** lets it run — but then Landlock enforces no exec
   or read confinement at all. (The fd being opened *before* `restrict_self` does not exempt it: the
   `bprm` execute check re-applies at `execveat`.)

4. **A root `EXECUTE` grant authorizes every path exec.** With `/` granted, the memfd runs *and*
   `/usr/bin/env` runs — so `exec_of_a_non_allowed_binary` fails. A narrow `allow_exec` grant denies
   `/usr/bin/env` correctly *and* denies the memfd. There is no middle grant, and **stacked Landlock
   layers are an intersection**, so any layer narrow enough to deny `/usr/bin/env` also denies the
   memfd.

5. **An `F_ADD_SEALS` memfd is not usable as a Landlock `PATH_BENEATH` execution rule object.**
   `F_ADD_SEALS` succeeds only on a `memfd` (anonymous → `add_rule` → `EBADFD`); an `O_TMPFILE`/named
   `tmpfs` inode is Landlock-ruleable (`add_rule` → 0) but rejects `F_ADD_SEALS` with `EPERM`. (This
   is the *narrow, proven* claim. The broader "no object is both immutable and ruleable" is **not**
   established and is in fact false in general: e.g. **fs-verity** makes a supported regular file
   read-only and content-verified against a Merkle tree while it remains an ordinary, ruleable
   filesystem object — it just needs filesystem + host support, so it is not the immediate VB-4
   answer. The point is only that *seal-based* immutability and Landlock path-ruleability do not
   co-occur on a single fd.)

**Conclusion.** For a pathless sealed `memfd`, "the target executes" and "Landlock denies
non-`allow_exec` path execs" reduce to the *same* all-or-nothing Landlock `EXECUTE` switch and cannot
both hold in one domain. seccomp cannot substitute: a blanket `execve` deny also kills the legitimate
`/bin/sleep`/`/bin/dash` subprocesses the cgroup/deadline scripts spawn, and seccomp cannot inspect
the path argument to be selective.

## 4. Recommended reconciliation — correct the authority model (preferred)

The frozen contract implicitly assumes **every executable object must be an element of one
path-based `allow_exec`**. A sealed `memfd` is, by construction, *not* a path-hierarchy object — the
kernel's `EBADFD` is that exact statement. Path authority and object-capability authority are
**different kinds of authority**; the contract should say so.

**Split executable authority:**

```
struct ExecPolicy {
    allow_exec_paths:     Vec<PathRule>,              // Landlock EXECUTE|READ_FILE path-beneath rules
    sealed_launch_target: SealedTargetCapability,     // exactly one, fd-bound
}
```

The **detached sealed-target capability** is valid only when *all* hold:

- the launch target was opened **before** confinement;
- the `allow_exec` entry and the launch target refer to the **same opened object identity**;
- `F_GET_SEALS` proves `WRITE | GROW | SHRINK | SEAL`;
- the target digest matches the launch specification;
- execution uses that same fd via `execveat(fd, "", AT_EMPTY_PATH)`;
- **no other** anonymous, detached, unruleable, or merely-sealed object receives the exception.

**Restated security invariant.**

> *Old (too narrow):* every allowed executable must have a Landlock path rule.
>
> *New:* every executable object is authorized **either** by a Landlock path rule **or** by the
> single verified sealed-target capability — and by nothing else.

This preserves both real properties: the target stays immutable, and no arbitrary executable outside
`allow_exec_paths` runs. It is the model VB-2 already adopted in code for the sealed target; VB-4
merely exposed that the frozen *oracle text* still encodes the older, narrower path-only semantics.

**Amended oracle assertions (proposed):**

1. the sealed target executes through the exact approved fd;
2. an ordinary executable **inside** `allow_exec_paths` executes;
3. an ordinary executable **outside** `allow_exec_paths` is denied by Landlock;
4. a second / unsealed / non-target `memfd` allowance is denied (fails closed);
5. target digest, seals, identity, and nonce bindings are adversarially tested.

## 5. Residual kernel-enforcement decision the reviewer must also make

The authority-model correction fixes the **specification** framing, but by itself it does **not**
dissolve the kernel constraint from §3. Amended oracle points **(1)** "the sealed memfd executes"
and **(3)** "Landlock denies an executable outside `allow_exec`" still reduce to the one Landlock
`EXECUTE` switch on a pathless inode. So the reviewer must additionally rule on **how the
object-capability is enforced at the kernel level.** The realistic realizations, with honest costs:

- **(A) Sealed target on a ruleable inode (e.g. a backend-private mount).** Stage the sealed bytes
  onto a real inode Landlock *can* rule (private `tmpfs` in an unshared mount namespace, or a
  read-only bind), then grant it `EXECUTE` narrowly as the detached exception alongside
  `allow_exec`. This is the **only** realization that keeps *both* amended oracle (1) and (3) true in
  one domain. Cost, and why it is **not** preferred as a casual "local fix": a mount/user-namespace
  lifecycle, host capability/namespace-policy requirements, a byte copy out of the sealed memfd into
  another inode, re-proving immutable identity between source bytes and the executed file, and a
  larger teardown surface. It is a genuinely new security mechanism inside an already-large VB-4.

- **(B) `EXECUTE` unhandled; capability authorizes the transition only.** Landlock handles
  write-family (writes stay worktree-confined) but **not** `EXECUTE`/`READ_FILE`, so the memfd runs;
  the sealed-target capability (fd identity + seals + digest, proven pre-exec by the child) is the
  authorization for the transition. Consequence: the running target's **own** subprocess path-execs
  are **not** Landlock-confined, so amended oracle (3) as a *Landlock* assertion cannot hold — it
  would have to be dropped or re-expressed (e.g. exec-allowlist enforcement acknowledged as the
  parent's lexical Vertical-A gate, not a kernel Landlock denial for a memfd launch).

- **(C) seccomp `USER_NOTIF` exec broker — not recommended.** A monitor-mediated broker checking
  each `execve` path is path-aware in principle but **TOCTOU-prone**: the notification carries
  register values, not an atomic snapshot of the pointed-to path memory, and the kernel docs
  explicitly warn against reading tracee pointer arguments this way. It also adds a listener fd,
  per-exec mediation, a new IPC lifecycle, monitor/child death races, another monitor-only fd, and a
  new mandatory teardown path — to re-impose path semantics on an object deliberately made pathless.

The implementer's recommendation matches the reviewer's stated preference: **do not** reach for (A)
private-mount copying or (C) a `USER_NOTIF` broker as a reflex. The cleanest resolution is the
authority-model correction of §4, with the reviewer explicitly choosing between realization **(A)**
(if oracle (3) must remain a kernel-Landlock denial for the memfd-launch case) and realization
**(B)** (if oracle (3) is re-expressed so that a pathless sealed target does not also carry
path-exec-allowlist enforcement of its subprocesses). That choice is a security-contract decision,
not an implementation detail, which is why VB-4 stops here for a reviewed correction rather than
silently picking one.

## 6. Why not just weaken the oracle in code

Silently granting `/` `EXECUTE` (so the memfd runs) makes `exec_of_a_non_allowed_binary` pass *only*
because nothing is denied — the exact false "confined" the differential oracles exist to prevent. The
correction must be a **reviewed contract change** with the invariant text amended in the open, then
the code follows. This document is that request.

---

### Appendix: exact invariant diff

```
- Every executable authorized for a confined launch MUST resolve to a Landlock
-   path-beneath rule under allow_exec.
+ Every executable object authorized for a confined launch MUST be authorized by
+   EITHER a Landlock path-beneath rule under allow_exec_paths
+   OR the single verified sealed-target capability, bound to the exact opened fd
+     whose identity, full seal set (WRITE|GROW|SHRINK|SEAL), and digest match the
+     launch specification, and executed only via execveat(fd, "", AT_EMPTY_PATH);
+   and by nothing else. A path rule and an object capability are distinct kinds of
+   authority; a pathless anonymous object (memfd) is not, and cannot be, a member
+   of the path allowlist (landlock_add_rule → EBADFD).
```

---

## 7. Reviewer decision + focused real-host proof of realization A

The reviewer accepted the contradiction, approved the §4 authority-model correction, and chose
**realization A** in a specific form: keep the fully-sealed `memfd` as the transport/source-identity
object, and derive a **private, ruleable execution inode** for the kernel-confined exec. Not B
(`EXECUTE` unhandled would drop kernel enforcement of the subprocess exec allowlist — a material
weakening, so B is only ever an explicitly weaker, `not_enforced` profile, never this milestone's
`FullyEnforced`). Not a `USER_NOTIF` broker (TOCTOU-prone).

The write-up's earlier §3.5 phrasing ("no object is both sealable and Landlock-ruleable") was too
broad and has been narrowed above; fs-verity is a standing counterexample to the universal form.

### 7.1 Mechanism — every stage proven on the designated host (Linux 6.18, Landlock ABI 7)

Standalone raw-syscall probes reproduce each property, and one integrated probe runs the whole
sequence end to end:

| Property | Result |
|---|---|
| `O_TMPFILE` tmpfs inode is Landlock-ruleable | `landlock_add_rule(PATH_BENEATH, exec_inode)` → **0** (vs memfd → `EBADFD`) |
| exec inode runs under a **narrow** ruleset (no root grant) | `execveat(ro_fd, AT_EMPTY_PATH)` → **runs** (`EXEC_INODE_RAN`) |
| path-exec allowlist is kernel-enforced | `/usr/bin/env` (outside `allow_exec`) → **`EACCES`** while `/bin`-style targets + their libs run |
| immutable inside the domain | write-reopen of the exec inode after `restrict_self` → **`EACCES`** |
| worktree writes still confined | inside write allowed; outside write → **`EACCES`** |
| no writer survives | writable staging fd closed before `restrict_self`; `execveat` clean (no `ETXTBSY`) |
| same-UID external reopen blocked | `PR_SET_DUMPABLE(0)`: attacker `open("/proc/<child>/fd/<n>")` for **read and write** → **`EACCES`** |
| private staging filesystem available | unprivileged `unshare(CLONE_NEWUSER\|CLONE_NEWNS)` + `uid_map`/`gid_map`/`setgroups=deny` + `mount` private `tmpfs` → **all succeed** (`unprivileged_userns_clone=1`) |
| **integrated** userns→mountns→private tmpfs→**non-dumpable**→`O_TMPFILE`→copy→digest-recheck→close-writers→narrow Landlock→`restrict_self`→`execveat` | **passes end to end** (`EXEC_INODE_RAN_IN_NS`, env denied, worktree writable, exec-inode write-reopen denied) |

The dynamic-loader detail: a real target is a PIE, so the ruleset grants `EXECUTE | READ_FILE` on the
EXACT interpreter and `READ_FILE`(+`READ_DIR`) **ONLY** on the system library directories (`/usr/lib`,
`/usr/lib64`, `/lib`, `/lib64`) — loading a shared object is a read, not an execute — or `execveat`
returns `EACCES` at the interpreter. This does **not** weaken oracle 7 (`/usr/bin/env` lives in
`/usr/bin`, not a library dir), and is proven by `a_runtime_read_root_executable_is_denied…`. See §8.

## 8. Implementation plan (backend), and the one design point it surfaces

Planned changes to the launch child, before Landlock, replacing the memfd root-grant:

1. `unshare(CLONE_NEWUSER | CLONE_NEWNS)`; write `setgroups=deny`, `uid_map`, `gid_map` (0→outer).
2. `mount("", "/", MS_REC|MS_PRIVATE)`; `mount` a private `tmpfs` staging dir.
3. `PR_SET_DUMPABLE(0)` **BEFORE any staging fd exists** (after the userns/id-map transition, which can
   reset dumpability), then READ IT BACK (`PR_GET_DUMPABLE == 0`) — so `/proc/<pid>` is already
   root-owned and the writable inode below is never reopenable by a same-UID external process.
4. `open(staging, O_TMPFILE|O_RDWR)`; copy bytes from the inherited sealed `memfd`.
5. Read back and require the digest equal the **sealed source digest** AND the **LaunchSpec target
   digest**; bind both source and execution identity into `READY_TO_EXEC`.
6. Reopen `O_RDONLY` (exec fd) + `O_PATH` (rule fd) of the same `(dev, ino)`; close every writable fd.
7. Landlock (SPLIT rights, as implemented): rule the exec inode `EXECUTE|READ_FILE`; rule `allow_exec`
   paths `EXECUTE|READ_FILE|READ_DIR`; rule the EXACT interpreter `EXECUTE|READ_FILE`; rule the library
   read roots + loader support files `READ_FILE`(+`READ_DIR`) **ONLY — never `EXECUTE`**; worktree
   write-family; `restrict_self`.
8. `execveat(exec_ro_fd, "", AT_EMPTY_PATH)`.

New `unsafe` (`unshare`, `mount`, `prctl(PR_SET_DUMPABLE)`) stays contained in a THIRD audited module
`staging/sys.rs` — so the VB-4 unsafe-surface gate now names `landlock/sys.rs` + `seccomp/sys.rs` +
`staging/sys.rs` (the staging syscalls were NOT smuggled into `seccomp/sys.rs`). New compile-gated
FaultPoints per the directive: `target_namespace`,
`target_mount`, `target_tmpfile`, `target_copy`, `target_digest`, `target_close_writer`,
`target_identity`, `target_landlock_rule`, `target_proc_isolation`; each must yield no GO / no target
marker / `NotEnforced` at the exact stage / cgroup teardown / teardown-dominance. If private ruleable
staging cannot be established on a host, the backend returns `NotEnforced` and does **not** launch
(no silent fallback to B).

**Design point — RESOLVED as implemented (not as originally sketched):** dynamic linking does NOT
force `EXECUTE` on the library directories. The implemented `RuntimePolicy` (see `runtime/mod.rs`,
`PROFILE_V1`) is split-rights: only the EXACT ELF interpreter (`/lib64/ld-linux-x86-64.so.2` or
`/usr/lib64/…`) is granted `EXECUTE|READ_FILE` (the kernel execs it as `PT_INTERP`); the system
library directories (`/usr/lib`, `/usr/lib64`, `/lib`, `/lib64`) are granted `READ_FILE`(+`READ_DIR`)
**ONLY**, because loading a shared object is a read, not an execute. This is a versioned compiled-in
profile, verified against the target's `PT_INTERP`, and is proven by
`a_runtime_read_root_executable_is_denied_unless_allowlisted` (a runtime-read-root file execs
unconfined but is EACCES under confinement).

### Amended oracle (per the reviewer) — to be written openly before the flip

1. transport target fully sealed, digest verified; 2. execution inode same byte digest; 3. source +
execution identity bound into `READY_TO_EXEC`; 4. no writable fd to the execution inode survives
confinement; 5. the exact execution inode runs through its approved fd; 6. an executable inside
`allow_exec_paths` runs; 7. an executable outside `allow_exec_paths` is denied by Landlock; 8. the
execution inode cannot be reopened for write/truncate after confinement; 9. a second staged inode /
second memfd / unsealed memfd / digest-mismatched copy fails closed; 10. a same-UID external process
cannot mutate or reopen the staging object; 11. legitimate `/bin/dash`/`/bin/sleep` subprocesses stay
governed by ordinary Landlock path rules.

---

## 9. Phase 7b.2 — frozen execution-authority oracle amendment (one-for-one mapping)

The frozen matrix (`crates/o7-worker/tests/sandbox_confinement.rs`) is amended to the reviewed
source→private-execution-inode model. Every affected oracle has exactly one row; no oracle disappears
without a row. The backend selector `confinement_backend()` is **unchanged** in this layer (the
atomic flip is Phase 8); the amended non-fault oracles are exercised through the explicitly-selected
real backend via `O7_SANDBOY_BIN`.

| old test | old property (why obsolete) | new test | new property | fixture/target | expected kernel outcome | disposition |
|---|---|---|---|---|---|---|
| `a_sealed_proc_fd_target_executes_under_confinement` | a sealed **memfd** executes under Landlock with the sealed `/proc/fd` path **inside `allow_exec`** — impossible: an anonymous inode is not a Landlock rule object (`add_rule`→EBADFD), and only a root grant would run it | `a_sealed_source_runs_as_private_execution_inode` | the sealed source is materialized into a private ruleable execution inode and executes under a NARROW ruleset; the `FullyEnforced` verdict attests the internal source→inode bindings (source seals+digest, destination-digest equality, exact exec-inode identity, complete resolved-runtime binding) — the backend refuses `FullyEnforced` unless they held | live sealed memfd source; `allow_exec` = ORDINARY dirs only (the sealed source is authorized by its seals, NOT `allow_exec`; `/proc/fd` is never in the allowlist — see §9.1); `fs` probe | `inside=OK`, `create/overwrite/truncate=ERR:13`, spawn `FullyEnforced` | REPLACED |
| `the_confined_target_inherits_no_planted_socket` | planted non-CLOEXEC socket scrubbed — still valid, but the fd census must be provable, not a fixed constant, and the plant must sit at a HIGH fd | `the_confined_target_inherits_no_planted_socket` | planted socket **raised to a high fd** is still scrubbed; the target counts inherited sockets via `fstat` over `[3, RLIMIT_NOFILE)` (procfs-free, provable) | `fds` probe; sentinel `dup2`'d to a high fd | `inherited_sockets=0` | AMENDED |
| `the_owned_cgroup_contains_the_tree_and_is_removed_on_teardown` | `/bin/dash -c` script builds the tree — host-shell dependency; and `force_stop` was used to end it (see finding B) | `the_owned_cgroup_contains_the_tree_and_is_removed_on_teardown` | hermetic fixture: leader + ordinary child + reparented double-fork descendant, all observed inside the owned cgroup; the MONITOR's OWN deadline teardown removes the leaf (NOT `force_stop`, which would SIGKILL the monitor first) | `__tree-fixture` (landed in `a26b63f`); short deadline; window from target-start | whole tree in the leaf; monitor completes teardown in-window; leaf gone | AMENDED |
| `a_target_outliving_the_deadline_is_killed_with_its_descendants` | `/bin/dash` sleeps past the deadline; absolute window was measured from the `spawn` CALL, so slow backend setup made the monitor's deadline fire after the window (finding B) | `a_target_outliving_the_deadline_is_killed_with_its_descendants` | hermetic fixture sleeps past the absolute deadline; monitor kills the whole tree and removes the leaf within `deadline + grace` measured from TARGET-START (setup excluded); no `survived` marker | `__tree-fixture`; window from target-start | tree killed in-window; no survivor | AMENDED |
| — (additive) | — | `an_explicitly_allowlisted_subprocess_executes` | a subprocess **inside `allow_exec`** exec'd BY the target runs | `exec` probe → `/usr/bin/touch`, `allow_exec`=`probe_dir + /usr/bin` | `secondary` created (the direct-exec'd `touch` ran) | ADDED |
| — (additive) | — | `a_worktree_executable_is_denied_unless_allowlisted` | a binary in the writable worktree, exec'd BY the target, is denied (writable ≠ executable) | `exec` probe → worktree `touch` copy, `allow_exec`=`probe_dir + /usr/bin` (not the worktree) | `exec=ERR:13`, no `secondary` | ADDED |
| — (additive) | — | `a_runtime_read_root_executable_is_denied_unless_allowlisted` | a file in a runtime read-root (`/usr/lib`, granted `READ_FILE` only) exec'd BY the target is denied — loading a shared object is a read, not an execute | `exec` probe → `/usr/lib/libc.so.6`, `allow_exec`=`probe_dir + /usr/bin` (runtime roots `READ_FILE` only) | `exec=ERR:13` (via `primary`) | ADDED |
| — (additive) | — | `the_running_execution_inode_is_immutable_to_write_reopen` | while the target runs, a SAME-UID external open of its executable inode for write is denied (`ETXTBSY`) — supplementary to the mid-staging proc-isolation proof below | `/proc/<pid>/exe` opened `O_RDWR` from the test process | `ETXTBSY` | ADDED |
| — (additive) | — | `a_second_sealed_memfd_is_not_executable_by_the_target` | the sealed target is the ONE exact capability: a SECOND sealed memfd the owner holds is exec-DENIED via `/proc/<owner>/fd/<other>` (launch-source authority ≠ `allow_exec`; `/proc/fd` never ruled) | two sealed memfds; `allow_exec` = ordinary dirs | `exec=ERR:13`, no `secondary` | ADDED (re-gate) |
| — (additive) | — | `a_proc_fd_symlink_alias_in_allow_exec_fails_closed` | a SYMLINK ALIAS to `/proc/<owner>/fd` in `allow_exec` (lexical path NOT under `/proc`) is rejected by OPENED-OBJECT identity (`fstatfs`→PROC_SUPER_MAGIC) → setup fails closed, no target | `allow_exec` = `probe_dir` + alias→`/proc/<pid>/fd` | spawn `Err`, no target marker | ADDED (re-gate) |
| — (additive) | — | `the_staging_inode_is_not_reopenable_during_materialize` | DIRECT proc-isolation proof: with `PR_SET_DUMPABLE(0)` set BEFORE the writable staging inode, a same-UID external `/proc/<child>/fd/<staging-fd>` reopen for READ and WRITE is denied mid-materialize; the post-release launch still completes FullyEnforced | compile-gated staging barrier witness + external reopen | both `EACCES`; launch `FullyEnforced` | ADDED (re-gate) |
| `writes_are_confined_to_the_worktree` | unchanged | — | — | — | — | PRESERVED |
| `exec_of_a_non_allowed_binary_is_denied_by_the_kernel` | already the "non-allowlisted subprocess denied by Landlock" oracle (target execs `/usr/bin/env`, not in `allow_exec`); assertions UNCHANGED, but its shared `exec_probe` helper was de-shelled (finding C) | — | — | — | — | PRESERVED (helper hermeticized) |
| `network_sockets_are_denied` / `the_target_env_is_exactly_the_allowlist` / `setsid_is_denied_by_seccomp` / `setpgid_is_denied_by_seccomp` / `a_parseable_marker_paired_with_a_nonzero_exit_…` | unchanged | — | — | — | — | PRESERVED |
| `a_landlock_setup_failure_runs_no_target` / `a_seccomp_setup_failure_runs_no_target` / `a_cgroup_setup_failure_runs_no_target` / `a_self_check_downgrade_runs_no_target` (+ `observe_setup_fault` / `assert_install_fault`) | `O7_FAKE_MODE` fake-driven setup faults | 16 `fault_*` oracles (real `O7_FAULT_POINT` artifact + monitor-owned `O7_FAULT_WITNESS`) | pre-GO A / self-check B / post-GO + teardown C — see §10 | real fault artifact via `matrix_backend()` | per §10 mapping | REPLACED in 7b.3 (`173a584`→this layer) |

Internal staging-window properties: the same-UID reopen block DURING staging is now proven DIRECTLY by
`the_staging_inode_is_not_reopenable_during_materialize` (not merely attested by `FullyEnforced`). The
remaining internal properties (source seals verified before staging, no writable fd surviving
confinement) are attested by the `FullyEnforced` verdict and get dedicated FORCED-FAILURE oracles
(`target_close_writer`, `target_proc_isolation`, `target_digest`, …) in Phase 7b.3.

### 9.1 Mechanism findings surfaced while running the amendment (no production change)

Running the amended oracles through the real backend surfaced three interactions. Findings B and C were
test-layer corrections; Finding A was initially mis-resolved as a test-only workaround and, on re-gate,
corrected in the backend + boundary (commit `33c1b0e`):

- **A — launch-source authority is SEPARATE from `allow_exec` (CORRECTED).** The frozen Vertical-A gate
  `SandboxPolicy::permits_exec` authorizes an ordinary target by path. A caller-held sealed
  `/proc/<pid>/fd/<n>` source is NOT forced into `allow_exec`: the o7-worker boundary's
  `source_authorized` authorizes it by its FULL SEAL SET (0xf) + bound digest at acquisition, and it
  runs via its private execution inode — `permits_exec` (the frozen protocol) is unchanged. An earlier
  attempt granted the `/proc/<pid>/fd` **directory** in `allow_exec` — **REJECTED**: that authorizes
  EXECUTE over EVERY fd the owner holds. The backend now fail-closes any `allow_exec` object whose
  OPENED identity is `procfs` (`fstatfs`→`PROC_SUPER_MAGIC`, so a symlink/bind alias to `/proc/<pid>/fd`
  cannot bypass it). Proven by `a_second_sealed_memfd_is_not_executable_by_the_target` and
  `a_proc_fd_symlink_alias_in_allow_exec_fails_closed`.

- **B — the cgroup directory is removed ONLY by the monitor's OWN teardown.** `supervise` runs the
  teardown (move-out → `cgroup.kill` → drain → `rmdir`) on the monitor's own exit paths (deadline or
  child-exit); an external `force_stop` SIGKILLs the monitor before it can `rmdir`, leaking the leaf
  DIRECTORY (the processes still die). The reviewer's model ("killed by the absolute deadline") is
  the correct trigger: the amended cgroup-tree and deadline oracles capture the live tree, then let
  the monitor's DEADLINE teardown run and assert leaf removal. The absolute window is measured from
  TARGET-START (≈ `spawn` return) — where the monitor arms its deadline — not the `spawn` call, so
  backend setup (which the deadline never governed) is correctly excluded; measuring from the call
  made a slow debug setup push the monitor's deadline past the window.

- **C — the exec probe is de-shelled.** `exec_probe` execed `env /bin/dash -c 'touch <secondary>'`,
  a host-shell dependency (`/bin/dash` is absent here). It now execs the target DIRECTLY
  (`execv(target, [target, secondary])`): an allowed `/usr/bin/touch` creates `secondary` (proving
  it ran) and a Landlock-denied exec writes `exec=ERR:<errno>` to `primary`. Assertions of the
  preserved `exec_of_a_non_allowed_binary_…` oracle are unchanged; the helper is now hermetic.

## 10. Phase 7b.3 — fault-point mapping (real fault-injection artifact)

The four `O7_FAKE_MODE` setup oracles + `observe_setup_fault` are REPLACED by oracles that drive one
typed `FaultId` (`O7_FAULT_POINT`) into the REAL composed launch through the committed `matrix_backend()`
seam, and read a MONITOR-owned stage witness (`O7_FAULT_WITNESS`, written by the unconfined
monitor/pre-exec child — never the target). `confinement_backend()` stays the fake; no production flip.

Fault selection is typed, parsed ONCE in the unconfined monitor (`launch::monitor_faults`), permits
exactly one fault/launch (single `Option<FaultId>` slot), and is `remove_var`'d before the fork so it
never reaches the child/target; an unknown selection aborts (`invalid_fault`). The witness names the
FINE stage actually reached (the child reports it in its bound proof; the monitor writes it), so a
generic/malformed failure cannot satisfy a stage-specific oracle.

**Witness delivery:** the child records the FINE stage of its first failing step into the bound
`READY_TO_EXEC` proof; the monitor writes it to `O7_FAULT_WITNESS` from `forced_downgrade` (child faults
+ `ready_validate` + `ready_eof`), `refuse_before_child` (`cgroup_create`/`invalid_fault`), the
`Release`/`Execveat` trips, and the teardown-`Err` path (`kill`/`drain`). Pre-exec only — never the target.

| `O7_FAULT_POINT` | phase | class | witness stage | GO sent? | target start? | boundary result | cleanup evidence | oracle |
|---|---|---|---|---|---|---|---|---|
| `cgroup_create` | monitor, pre-fork | A | `cgroup_create` | no | no | `Evidence(NotFullyEnforced)` | no leaf created | `fault_cgroup_create_runs_no_target` |
| `target_tmpfile` | child, staging | A | `target_tmpfile` | no | no | `Evidence(NFE)` | leaf removed | `fault_staging_target_tmpfile_runs_no_target` |
| `runtime_interpreter` | child, runtime | A | `runtime_interpreter` | no | no | `Evidence(NFE)` | leaf removed | `fault_runtime_interpreter_runs_no_target` |
| `landlock_restrict` | child, Landlock | A | `restrict_self` | no | no | `Evidence(NFE)` | leaf removed | `fault_landlock_restrict_runs_no_target` |
| `seccomp_apply` | child, seccomp | A | `apply` | no | no | `Evidence(NFE)` | leaf removed | `fault_seccomp_apply_runs_no_target` |
| `fd_verify` | child, fd scrub | A | `fd_verify` | no | no | `Evidence(NFE)` | leaf removed | `fault_fd_verify_runs_no_target` |
| `env_verify` | child, env | A | `env_verify` | no | no | `Evidence(NFE)` | leaf removed | `fault_env_verify_runs_no_target` |
| `cgroup_verify` | child, placement | A | `cgroup_verify` | no | no | `Evidence(NFE)` | leaf removed | `fault_cgroup_verify_runs_no_target` |
| `ready_write` | child dies pre-proof | A | `ready_eof` | no | no | `Evidence(NFE)` | leaf removed | `fault_ready_write_runs_no_target` |
| `ready_validate` | monitor, post-fork | A | `ready_validate` | no | no | `Evidence(NFE)` | leaf removed | `fault_ready_validate_runs_no_target` |
| *(unknown string)* | monitor, pre-fork | A | `invalid_fault` | no | no | `Evidence(NFE)` | no leaf created | `fault_invalid_selection_runs_no_target` |
| `seccomp_self_check` | child, seccomp effect-check | **B** | `seccomp_self_check` | no | no | `Evidence(NFE)` — correctly-bound report rejected on ENFORCEMENT merits (not a crash `apply`, not a binding mismatch) | leaf removed | `fault_seccomp_self_check_downgrade_is_rejected_on_merits` |
| `release` | monitor, post-GO pre-exec | **C** | `release` | **yes** | no (token withheld) | `Ok(launch)`; monitor exits PLUMBING | leaf removed | `fault_release_runs_no_target` |
| `execveat` | child, post-GO at exec | **C** | `execveat` | **yes** | no (exec aborted) | `Ok(launch)`; child exits 93 | leaf removed | `fault_execveat_runs_no_target` |
| `kill` | monitor, deadline teardown | **C** | `kill` | **yes** | **yes** (tree ran) | `Ok(launch)` → boundary surfaces cleanup `Err` (tree survived faulted kill) | teardown-DOMINATES (exit TEARDOWN); tree reaped by boundary; leaf may leak (swept) | `fault_teardown_kill_dominates` |
| `drain` | monitor, deadline teardown | **C** | `drain` | **yes** | **yes** (tree ran) | `Ok(launch, exit TEARDOWN)` | teardown-DOMINATES; tree killed (drain OBSERVATION faulted); leaf may leak (swept) | `fault_teardown_drain_dominates` |

**FaultIds mapped but covered by a same-stage representative** (all fail closed identically; the
witness is the shared fine stage): `landlock_create`→`create_ruleset`, `landlock_add_rule`/
`target_landlock_rule`/`runtime_rule`→`add_rule`, `landlock_self_check`→`self_check_outside_inconclusive`,
`runtime_object_open`/`runtime_identity`→`open_parent`, `runtime_profile`→`runtime_profile`,
`runtime_preload_check`→`runtime_preload_check`, `target_namespace`/`target_mount`/`target_copy`/
`target_digest`/`target_close_writer`/`target_identity`/`target_proc_isolation`→their `target_*` stage,
`seccomp_no_new_privs`→`no_new_privs`. (Staging `IdMap`/`MountPrivate`, several landlock ABI/omit knobs,
cgroup `move_out`, and the VB-3 `O7_SC_FAULT` seccomp variants are NOT reachable from a `FaultId`.)

**Concrete defect fixed (exposed by the pre-GO fault matrix):** on a report-verification rejection the
boundary `drop(sock)`'d then IMMEDIATELY `kill_and_reap`'d, SIGKILLing the monitor before its
NACK→`cgroup.kill`→drain→`rmdir`→exit could remove the owned cgroup leaf — an empty-leaf leak. Fixed by
granting the monitor a bounded self-teardown window before the fallback reap (`sandboy_boundary.rs`).
