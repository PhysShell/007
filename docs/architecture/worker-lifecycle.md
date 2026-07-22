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
`CleanupFailure(e)`. **If the process exited cleanly but the owned group could not be
proven gone, the result is `CleanupFailure` — never a success.**

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
just `child.kill()` (that would kill the leader and orphan its descendants).

## Drop semantics
Dropping the last `WorkerHandle` requests cancellation (it never silently detaches).
The supervisor task is independent and observed via `WorkerJoin` — there are no
detached, unobservable tasks. Async cleanup happens in the task, not in `Drop`.

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
(`ObservationFailure`) rather than silently truncating.

## Heartbeat
A heartbeat means **the supervisor is alive and owns a live process** — NOT that the
process is doing useful work. It is driven by a monotonic timer, independent of
stdout/stderr; it flows during silence, stops after the terminal state, and the absence
of output is never treated as a hang. Any hang/timeout policy belongs to a future
manager/o7d, not to the worker.

## Orphan detection — exact scope
PR 2 guarantees, while the supervisor is alive: the leader exiting while descendants
remain is detected and the group cleaned; cancel/drop terminates the whole group; the
terminal result is not produced until cleanup is verified; the direct child is reaped (no
zombie); and no owned process survives a normal lifecycle.

**Deferred (NOT PR 2):** orphan RECOVERY after the daemon (o7d) itself is SIGKILLed. Once
the in-memory supervisor is gone, a raw PID is insufficient (PID reuse) and reliable
adoption/cleanup needs cgroups/Sandboy or a persisted process identity. This is closed by
the Sandboy boundary + durable identity later.
```
PR 2 orphan detection:  in-supervisor descendant cleanup
Deferred:               post-o7d-crash orphan recovery
```
