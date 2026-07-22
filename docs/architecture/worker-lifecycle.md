# Worker lifecycle (`o7-worker`)

A generic runtime that launches ONE external process, owns its whole process GROUP
(the members that remain in the host process group — not a tree/cgroup: a descendant
that starts its own group/session escapes ownership),
streams typed observations, cancels deterministically, and yields exactly one
terminal result. It knows nothing about Claude/Codex/MCP/worktrees/verifiers/the
ledger — those are other crates/PRs. Unix-only by construction.

## What PR 2 is (and is not)
- **Is:** a process spine — spawn, own the process group, stream stdout/stderr as
  bytes, heartbeat, cancel idempotently, clean up the whole owned set, one terminal
  result.
- **Is not:** security isolation (that is Sandboy — a separate, mandatory boundary
  implementation, see `process-boundary.md`), provider adapters, or ledger
  persistence.

## State machine
```
Created → Starting → Running → Cancelling → Exited
                │        └────────────────→ Exited
                ├──────────────────────────→ FailedToStart
                └──────────────────────────→ Cancelling
```
Terminal states: `Exited`, `FailedToStart` (see `state.rs`; any other transition is
rejected). The supervisor emits exactly ONE [`WorkerResult`] even when several
terminating events race (natural exit, cancel, sink failure, handle drop, grace
timeout).

`WorkerResult`: `ExitedNormally(code)`, `ExitedBySignal(sig)`, `CancelledGracefully`,
`CancelledForcefully`, `FailedToStart(e)`, `BoundaryFailure(e)`, `ObservationFailure(e)`,
`OutputFailure(e)`, `CleanupFailure(e)`. **If the process exited cleanly but the owned
group could not be proven gone, the result is `CleanupFailure` — never a success.**

When faults co-occur, terminal precedence is fixed:
`CleanupFailure > ObservationFailure > Boundary/Output outcome`. An unprovable/failed
cleanup (possible leaked processes) dominates everything; a lost authoritative sink then
dominates a boundary/output fault (and the ObservationFailure message preserves that
underlying fault so it is not erased).

## Observations are NOT ledger events
The supervisor publishes `WorkerObservation`s (an INTERNAL lifecycle model:
`BoundaryAttested`, `SpawnRequested`, `Spawned`, `OutputChunk`, `Heartbeat`,
`CancellationRequested`, `GracefulStopSent`, `ForceStopSent`, `DescendantsRemaining`,
`Exited`, `CleanupCompleted`, `SupervisorFailed`) through an `ObservationSink`. This is
**not** the canonical 007 event protocol and **not** a stable persistence schema — PR 1
froze the ledger event set and PR 4 owns the canonical protocol. In PR 4 an adapter maps
`WorkerObservation → canonical event → append-only ledger`.

The **`ObservationSink` is authoritative**: a publish failure (error or backpressure
timeout) is FATAL — the supervisor cancels the worker and cleans up, yielding
`ObservationFailure`. A UI disconnect is unrelated: the UI is not a sink.

## Cancellation
`WorkerHandle::cancel()` is idempotent, safe to call concurrently, works in `Starting`
and `Running`, and does not return until cleanup is complete. The host escalation:
1. record the request, 2. SIGTERM the whole group, 3. wait the grace period,
4. if survivors remain, SIGKILL the whole group, 5. reap the direct child,
6. verify the group is gone, 7. only then publish the terminal completion. It is never
just `child.kill()` (that would kill the leader and orphan its descendants). A **failed**
graceful stop never waits the grace and then reports a graceful cancel: it force-closes
immediately and preserves the boundary fault. Any emergency force-stop taken on a fault
path is a real teardown action, so it is published as `ForceStopSent` on the
authoritative stream BEFORE it is performed — never an invisible SIGKILL. Every reap
(`wait()`) performed AFTER a force-stop is **bounded**: a boundary whose `force_stop()`
fails while the leader stays alive, or whose `wait()` never completes, cannot hang the
supervisor — an un-reapable teardown is reported as a bounded `CleanupFailure` instead of
an infinite wait. `BoundaryProcess::wait()` therefore carries an explicit cancel-safety
contract (the `select!` loop drops and recreates the pending `wait()` future each
iteration; a leader exit reached while it was not polled must still be observed).

## Drop semantics
Dropping the last `WorkerHandle` requests cancellation (it does not silently walk
away — it signals the supervisor to tear down). Dropping a `WorkerJoin` *does* detach
the supervisor task (as dropping any Tokio `JoinHandle` does) and discards its
terminal `WorkerResult` — but detaching is not orphaning: the task keeps running,
still owns the boundary process, and performs its own verified cleanup, and its
completion stays observable via the terminal watch behind `WorkerHandle::cancel`. So
a dropped join loses the RESULT, not the cleanup. Async cleanup happens in the task,
not in `Drop`.

## Environment isolation
`UnconfinedHostBoundary` is not a sandbox, but the environment is still strictly
controlled: `env_clear()` then only `WorkerSpec.environment`. Nothing is inherited — no
API keys, SSH agent, cloud creds, HOME, PATH, proxy vars, RUST_LOG, shell hooks. The
working directory must be absolute; the executable must be absolute (relative → rejected,
so there is no PATH search); stdin is `Null` by default; stdout/stderr are always piped;
the process is spawned directly — never via a shell.

## Output streaming
stdout and stderr are read independently as raw bytes (`OutputChunk`, never assumed
UTF-8), each with its own monotonic sequence. Per-stream order is guaranteed; global
stdout-vs-stderr interleaving is not. Chunk size and the internal channel are bounded, so
memory never grows without limit; trailing output is drained before the terminal result;
and if the sink cannot keep up within the backpressure timeout, the worker fails closed
(`ObservationFailure`) rather than silently truncating. The trailing-output drain is
itself **bounded**, by three SEPARATE limits so no one of them cancels a healthy publish:
on a cleanup error the supervisor does not wait on pipe closure at all (it aborts the
readers and lets `CleanupFailure` dominate); the wait for the NEXT trailing message has an
idle timeout (an escaped descendant may hold an inherited pipe open with no further
output); each trailing PUBLISH keeps its own `sink_backpressure_timeout` — the drain never
wraps it, so a slow-but-within-contract sink is delivered, not drain-cancelled; and a
trailing BYTE budget (the configured channel buffer plus a pipe allowance) bounds an
escaped descendant that keeps WRITING forever. Any of the idle-timeout / byte-budget /
read-error outcomes is an `OutputFailure`, never a clean pass, and it is preserved even
when the terminal sink then fails (the dominating `ObservationFailure` carries it).

## Heartbeat
A heartbeat means **the supervisor is alive and owns a live process** — NOT that the
process is doing useful work. It is driven by a monotonic timer, independent of
stdout/stderr; it flows during silence, stops after the terminal state, and the absence
of output is never treated as a hang. Any hang/timeout policy belongs to a future
manager/o7d, not to the worker. When heartbeats are **disabled** no timer is constructed
at all; when **enabled** the interval is validated pre-spawn (non-zero and ≤ `MAX_TIMEOUT`,
like the other timer durations) so an absurd `Duration::MAX` interval is a `FailedToStart`,
not a later missed-tick `Instant + period` overflow.

## Orphan detection — exact scope
PR 2 guarantees, while the supervisor is alive: the leader exiting while descendants
remain is detected and the group cleaned; cancel/drop terminates the whole group; the
terminal result is not produced until cleanup is verified; the direct child is reaped (no
zombie); and no owned process survives a normal lifecycle.

The membership scan (`/proc`) is the AUTHORITATIVE proof, so it fails closed on anything
it cannot positively resolve: a top-level `/proc` failure, a directory-ENTRY I/O error,
or a per-PID `stat` I/O error (EACCES/EIO/…) all propagate; a `stat` that reads but does
not parse is a membership failure; and only a confirmed `NotFound` (the PID vanished) is
treated as a benign exit race. It never treats "unknown" as "gone", so a live member
whose `stat` errors can never be silently dropped from the proof.

**Live vs terminated.** The scan parses each entry's scheduler `state` alongside its
PGID. A **terminal corpse** — a zombie (`Z`, exited but not yet reaped) or dead (`X`/`x`)
process — executes nothing and cannot be signalled, so it is treated as GONE even though
it still carries the group's PGID in `/proc`; every other state (`R`/`S`/`D`/`T`/`t`/`I`,
…) counts as live. This matters when the group's own init does not reap orphans: after the
supervisor `SIGKILL`s an escaped descendant that has reparented away, the descendant
becomes an unreapable zombie that would otherwise be miscounted as a live survivor forever,
turning a successful teardown into a false `CleanupFailure`. Because the state is only read
*after* a successful, parseable `stat`, a zombie is **proven** terminal — never confused
with a `stat` the scan merely failed to read. Direct-child reaping is proven separately:
after `wait()` the leader's `/proc/<pid>` entry is gone entirely (a raw path check, not the
live-members scan, which by design would accept a zombie as gone).

**Deferred (NOT PR 2):** orphan RECOVERY after the daemon (o7d) itself is SIGKILLed. Once
the in-memory supervisor is gone, a raw PID is insufficient (PID reuse) and reliable
adoption/cleanup needs cgroups/Sandboy or a persisted process identity. This is closed by
the Sandboy boundary + durable identity later.
```
PR 2 orphan detection:  in-supervisor descendant cleanup
Deferred:               post-o7d-crash orphan recovery
```
