# Process boundary

The boundary is the seam between the generic worker supervisor and *how* a process
tree is owned and confined. It is an abstraction over a SET of processes, not just a
leader PID, so a future Sandboy implementation can use cgroups/namespaces/etc. instead
of a POSIX process group — and the supervisor never needs to know which.

## Traits (`boundary.rs`)
```rust
trait ProcessBoundary {
    async fn spawn(&self, spec: BoundarySpawnSpec) -> Result<Box<dyn BoundaryProcess>, BoundaryError>;
    fn attestation(&self) -> BoundaryAttestation;
}
trait BoundaryProcess {
    fn identity(&self) -> ProcessIdentity;
    fn take_stdout(&mut self) -> Option<ChildStdout>;
    fn take_stderr(&mut self) -> Option<ChildStderr>;
    async fn request_graceful_stop(&mut self) -> Result<(), BoundaryError>; // e.g. SIGTERM group
    async fn force_stop(&mut self)            -> Result<(), BoundaryError>; // e.g. SIGKILL group
    async fn wait(&mut self)                  -> Result<BoundaryExit, BoundaryError>;
    async fn remaining_members(&self)         -> Result<Vec<ProcessIdentity>, BoundaryError>;
}
```
The supervisor never learns whether `force_stop` is `killpg`, a cgroup kill, a Sandboy
shutdown, or a namespace teardown.

## Attestation — honesty, not inference
```rust
struct BoundaryAttestation { implementation: BoundaryKind, enforcement: EnforcementLevel }
enum BoundaryKind { UnconfinedHost, Sandboy }
enum EnforcementLevel { None, Partial, FullyEnforced }
```
A boundary states its own enforcement; the supervisor never guesses it.

## Requirement — fail closed, no default, no silent fallback
```rust
enum BoundaryRequirement { AllowUnconfined, RequireFullyEnforced }
```
- There is **no default** — a `WorkerSpec` must state it.
- An unconfined boundary is usable ONLY under `AllowUnconfined`.
- `RequireFullyEnforced` + an `UnconfinedHost` boundary **fails closed BEFORE spawn**
  (`FailedToStart`) — there is never a silent fallback from Sandboy to a host process.
- A production provider run must not use `AllowUnconfined`.

## PR 2 implementation: `UnconfinedHostBoundary`
Runs the process in its own POSIX process group (`process_group(0)`, so `pgid == leader
pid`) so the whole tree can be signalled together, and enumerates group membership via
`/proc`. It attests:
```
implementation = UnconfinedHost
enforcement    = None
```
It provides **lifecycle control, not isolation**. The name says so on purpose: process
group ownership is not sandboxing, and "None" must never be mistaken for a basic level of
protection. `ProcessIdentity` carries the kernel start-time alongside the PID so a reused
PID does not masquerade as a group member (sufficient for in-supervisor lifetime; durable
cross-crash identity is out of scope here).

## Sandboy — a separate, MANDATORY boundary (scheduled)
Sandboy is NOT a "nice to have later". It is a required boundary implementation before any
live provider execution:
```
PR Sandboy ProcessBoundary
- implements ProcessBoundary (cgroups/namespaces/seccomp — the real fence)
- attests EnforcementLevel::FullyEnforced
- supports RequireFullyEnforced (no fallback)
- post-daemon-crash cleanup strategy (durable process identity)
- MANDATORY before the PR 5 Claude vertical slice
```
Roadmap position:
```
PR 1   ledger                     MERGED
PR 2   generic worker lifecycle   THIS PR
PR 3   worktree + verifier
PR 4   canonical event protocol
PR 4.5 Sandboy ProcessBoundary    REQUIRED (before any real provider run)
PR 5   Claude vertical slice
```
(The 4.5 number is nominal; the invariant is that Sandboy lands before the first real
provider execution — a live Claude/Codex run must never use `UnconfinedHostBoundary`.)
