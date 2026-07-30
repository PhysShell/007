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
| **integrated** userns→mountns→private tmpfs→`O_TMPFILE`→copy→digest-recheck→close-writers→non-dumpable→narrow Landlock→`restrict_self`→`execveat` | **passes end to end** (`EXEC_INODE_RAN_IN_NS`, env denied, worktree writable, exec-inode write-reopen denied) |

The dynamic-loader detail: a real target is a PIE, so the ruleset must also grant `READ_FILE`/
`EXECUTE` on the loader + system library directories (`/usr/lib`, `/usr/lib64`, `/lib`, `/lib64`) or
`execveat` returns `EACCES` at the interpreter. This is standard for a path-exec sandbox and does
**not** weaken oracle 7 (`/usr/bin/env` lives in `/usr/bin`, which is not a library dir). See §8.

## 8. Implementation plan (backend), and the one design point it surfaces

Planned changes to the launch child, before Landlock, replacing the memfd root-grant:

1. `unshare(CLONE_NEWUSER | CLONE_NEWNS)`; write `setgroups=deny`, `uid_map`, `gid_map` (0→outer).
2. `mount("", "/", MS_REC|MS_PRIVATE)`; `mount` a private `tmpfs` staging dir.
3. `open(staging, O_TMPFILE|O_RDWR)`; copy bytes from the inherited sealed `memfd`.
4. Read back and require the digest equal the **sealed source digest** AND the **LaunchSpec target
   digest**; bind both source and execution identity into `READY_TO_EXEC`.
5. Reopen `O_RDONLY` (exec fd) + `O_PATH` (rule fd) of the same `(dev, ino)`; close every writable fd.
6. `PR_SET_DUMPABLE(0)`.
7. Landlock: rule the exec inode `EXECUTE|READ_FILE`; rule `allow_exec` paths; rule the loader/lib
   dirs `READ_FILE|EXECUTE|READ_DIR`; worktree write-family; `restrict_self`.
8. `execveat(exec_ro_fd, "", AT_EMPTY_PATH)`.

New `unsafe` (`unshare`, `mount`, `prctl(PR_SET_DUMPABLE)`) stays contained in the audited
`seccomp/sys.rs` — the VB-4 unsafe-surface gate is unchanged (still only `landlock/sys.rs` +
`seccomp/sys.rs`). New compile-gated FaultPoints per the directive: `target_namespace`,
`target_mount`, `target_tmpfile`, `target_copy`, `target_digest`, `target_close_writer`,
`target_identity`, `target_landlock_rule`, `target_proc_isolation`; each must yield no GO / no target
marker / `NotEnforced` at the exact stage / cgroup teardown / teardown-dominance. If private ruleable
staging cannot be established on a host, the backend returns `NotEnforced` and does **not** launch
(no silent fallback to B).

**Design point for the reviewer (needs a nod):** dynamic linking forces the ruleset to grant
`READ_FILE|EXECUTE` on the loader + system library directories (`/usr/lib`, `/usr/lib64`, `/lib`,
`/lib64`). This is necessary for *any* path-exec confinement of real PIE binaries (it is implicit in
oracle 11's "`/bin/dash`/`/bin/sleep` run"), and it does not weaken oracle 7. Confirming the exact
runtime-directory set (and whether it is fixed vs derived from the policy) is the only open contract
detail before the GREEN flip.

### Amended oracle (per the reviewer) — to be written openly before the flip

1. transport target fully sealed, digest verified; 2. execution inode same byte digest; 3. source +
execution identity bound into `READY_TO_EXEC`; 4. no writable fd to the execution inode survives
confinement; 5. the exact execution inode runs through its approved fd; 6. an executable inside
`allow_exec_paths` runs; 7. an executable outside `allow_exec_paths` is denied by Landlock; 8. the
execution inode cannot be reopened for write/truncate after confinement; 9. a second staged inode /
second memfd / unsealed memfd / digest-mismatched copy fails closed; 10. a same-UID external process
cannot mutate or reopen the staging object; 11. legitimate `/bin/dash`/`/bin/sleep` subprocesses stay
governed by ordinary Landlock path rules.
