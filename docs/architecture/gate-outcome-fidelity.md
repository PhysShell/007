# Gate outcome fidelity — accepted design

Status: **accepted / implementation-ready (R7)**.

This note freezes the corrective design for gate termination semantics and captured gate output. It is deliberately narrower than a general process-transport model: §2 is a corrective defect in the current implementation, §4 is a demonstrated evidence-loss finding, §5 is the accepted remedy, and §6 remains research pending measurement.

Related architecture: [`process-boundary.md`](./process-boundary.md), [`capability-fd-transport.md`](./capability-fd-transport.md).

## 1. Scope and status

| Item | Status |
| --- | --- |
| §2 signal → `Error` | implement |
| §3 Bash limitation + three termination tests | settled |
| §4 stdout/stderr stream identity | settled finding |
| §4b observed cross-stream ordering | deferred; necessity not demonstrated |
| §5 `ArtifactKind::GateCapture`, one `ArtifactRef`, event schema v1, state schema v1, structural replay | implement |
| §6 bounded capture | research — measure first |

No change is made to `Verdict`, `RunStatus`, `GateOutcome`, obligation semantics, `GateFinished` shape, A1's frozen artifact vocabulary, or the agent path.

## 2. Corrective defect — signal termination is not a gate `Fail`

The current gate runner derives the step verdict from `ExitStatus::success()` and therefore maps every non-successful `bash -lc` termination to `Verdict::Fail`.

That collapses two different facts:

- **domain failure**: the gate ran and reported a negative result;
- **harness/execution failure**: the shell process itself terminated by signal and no trustworthy gate result was produced.

The canonical contract already distinguishes these facts: `GateOutcome::Fail` means the gate ran and reported domain failure; `GateOutcome::Error` means the harness could not produce a trustworthy result.

On Unix, when the `bash` process itself is signalled, `ExitStatus::code()` is `None` and `ExitStatusExt::signal()` carries the terminating signal. That outcome MUST map to `Error`, not `Fail`.

This does **not** claim that `signal()` detects arbitrary signal termination inside the shell script. Bash represents an inner command terminated by signal `N` as command status `128 + N`; if the wrapper propagates that status, Rust observes an ordinary shell exit code. No `128 + N` heuristic is introduced because literal `exit 137` is observationally identical at this boundary.

### 2.1 Required termination tests

Three cases pin the boundary:

1. **Shell itself is killed**: `bash -lc` process receives `SIGKILL`; Rust observes `signal() == Some(9)`, `code() == None`; result is `Error`.
2. **Inner helper is killed, wrapper survives and propagates status**: Bash reports `128 + N` as an ordinary exit code; result remains `Fail` under the current boundary.
3. **Literal `exit 137`**: ordinary exit code, indistinguishable from case 2 at this boundary; result is `Fail`.

Also retain the regression showing the deliberate divergence between legacy `Verdict::reduce` and the canonical reducer for an **optional** gate whose step-level result is `Error`: legacy reduction treats any step error as run `ERROR`, while the canonical contract keeps optional-gate outcomes verdict-neutral. This change must not hide that pre-existing mismatch.

## 3. Shell limitation

The implementation invokes **Bash**, not an abstract POSIX shell. Statements about `128 + N` are therefore scoped to the invoked Bash wrapper.

Bash represents an inner command's signal termination as command status `128 + N`. Rust sees that as the wrapper's ordinary exit code only when the wrapper propagates that status. A wrapper that executes another command afterwards may exit with a different status.

No broader signal-detection claim is made.

## 4. Demonstrated evidence loss — stream identity

The current runner captures `stdout` and `stderr` separately via `Command::output()`, then immediately destroys their identity:

```rust
let mut buf = String::new();
buf.push_str(&String::from_utf8_lossy(&o.stdout));
buf.push_str(&String::from_utf8_lossy(&o.stderr));
```

Two independent losses occur:

1. `stdout` and `stderr` are concatenated into one undifferentiated byte sequence.
2. `String::from_utf8_lossy` irreversibly replaces malformed UTF-8 with U+FFFD.

A `lossy: true` flag would merely describe damaged evidence. It would not restore it.

**Raw bytes are authoritative; text is a projection.**

Observed cross-stream ordering is not part of this decision. `Command::output()` already gives two independent buffers, not a total ordering between writes to the two streams. PTY emulation and a general transport-semantics model are out of scope.

## 5. Capture format — decided

### 5.1 Constraint

`GateFinished` has one optional artifact slot:

```rust
GateFinished {
    gate,
    outcome,
    log: Option<ArtifactRef>,
}
```

The reducer currently validates that slot as `ArtifactKind::GateLog`.

A `foo.log` + `foo.err` pair would therefore leave one stream canonically digest-bound and the other an unbound sidecar. Adding a second file without changing the canonical event is not a remedy.

The accepted shape keeps **one `ArtifactRef`** and binds a framed capture containing both raw streams.

### 5.2 Decision

Add a new canonical artifact kind:

```rust
ArtifactKind::GateCapture
```

`GateFinished.log` remains the single slot and accepts legacy `GateLog` or new `GateCapture` evidence. New writers emit `GateCapture`; historical records remain `GateLog`.

`GateCaptureV1` is a small binary frame:

```text
magic
format_version
stdout_len
stderr_len
stdout raw bytes
stderr raw bytes
```

The exact integer widths/endianness and magic are implementation details to be fixed once in the parser/encoder and pinned by tests. The contract-level requirements are:

- the frame is self-identifying;
- `format_version` is explicit;
- stdout and stderr lengths are explicit;
- the payload preserves each stream's raw bytes exactly;
- no total cross-stream ordering is claimed;
- trailing bytes are forbidden;
- malformed or unsupported frames fail closed.

`ArtifactKind` answers **what evidence is this**. The frame header answers **which version of that evidence representation is this**. Those are separate contract levels.

### 5.2.1 Why a magic-discriminated `GateLog` is rejected

The objection is not that an old canonical reader would render binary bytes as text. It would not: canonical replay resolves artifact bytes and checks their digest.

The problem is stronger. Under a magic-discriminated `GateLog`, a pre-`GateCapture` verifier sees:

```text
kind = GateLog              // known
bytes = GateCaptureV1       // semantics unknown to it
digest = correct
```

and can report the artifact verified without performing the structural validation that gives `GateCaptureV1` its meaning.

With a distinct closed-enum `GateCapture` kind, the same old verifier encounters an unknown variant and fails closed.

The compatibility advantage of reusing `GateLog` is therefore the defect: it buys deserializability by allowing an older verifier to certify new evidence under weaker semantics. An old verifier must refuse evidence whose semantics it cannot check.

### 5.3 No `RUN_EVENT_SCHEMA_VERSION` bump

This repository already treats additive closed-enum vocabulary as a backward-compatible extension of the current event schema when old serialized records and their semantics remain unchanged.

Commit `ab38968f` added:

- `ArtifactKind::ProviderSession`;
- `ArtifactKind::CommandBinding`;
- `RunEventKind::ProviderSessionCaptured`;
- `RunEventKind::CommandBindingCaptured`;

without a schema bump, explicitly so pre-existing sealed records remained replayable. `CandidateState` and `CandidatePatch` followed the same policy.

The compatibility direction is therefore:

```text
new reader -> old record     MUST remain replayable
old reader -> new vocabulary MAY fail closed
```

`from_jsonl` deserializes `RunEvent` before the reducer sees it, so an old reader encountering a new closed-enum variant fails at serde decoding ahead of the reducer's schema-version check. That is an accepted forward-incompatibility mechanism, not a reason to bump the schema.

Existing numeric `artifact_kind_tag` values are stable. `GateCapture` takes the previously unused tag **11**; tags 1..10 do not move. Existing event digests therefore remain untouched.

### 5.3.1 Why a bump would break the wrong direction

The reducer supports exactly one event version:

```text
schema_version != RUN_EVENT_SCHEMA_VERSION -> UnsupportedSchema
```

There is no v1-or-v2 reader. Changing the constant to 2 would make the new binary reject every existing valid v1 record. That is the opposite of the repository's established compatibility policy.

### 5.3.2 `RUN_STATE_SCHEMA_VERSION` does not move

`GateFinished` folds into `RunState` as gate progress containing the gate outcome. The log/capture `ArtifactRef` is not part of the reduced state.

Changing `GateLog` to `GateCapture` for newly written evidence changes:

- no reduced-state field;
- no verdict semantics;
- no normalized-state bytes for existing records.

The compile-time equality assertion between run-state and run-event schema versions prices a future event-version decision; it is not an instruction to bump both constants for an additive artifact kind.

### 5.4 Resulting model

```text
legacy record:
  GateFinished.log -> ArtifactKind::GateLog
  payload          -> opaque legacy evidence bytes
  replay           -> resolve + digest verify

new record:
  GateFinished.log -> ArtifactKind::GateCapture
  payload          -> GateCaptureV1(stdout bytes, stderr bytes)
  replay           -> resolve + digest verify + structural parse of the same bytes
```

## 5.5 Replay obligation

`GateLog` is opaque evidence bytes with no inner structure canonical replay claims to understand, so digest verification is the whole canonical content check.

`GateCapture` is different. Its meaning — two raw streams with preserved identity — exists only if the frame parses.

```text
SHA256(garbage) == declared SHA256(garbage)
```

proves **these are those bytes**. It does not prove **this is a GateCaptureV1**.

Canonical replay MUST structurally parse every `GateCapture` artifact. A digest-valid but structurally invalid capture is not verified gate evidence and replay fails.

### 5.5.1 Same bytes

Canonical replay MUST parse the **same resolved bytes whose content digest it just verified**.

Preferred shape:

```rust
let bytes = artifacts.resolve(artifact)?;
verify_digest(artifact, &bytes)?;
if artifact.kind == ArtifactKind::GateCapture {
    parse_gate_capture(&bytes)?;
}
```

A later re-resolve is also acceptable only if that second byte sequence is digest-verified again before parsing, matching the discipline already used by candidate-state semantic verification.

This wording is load-bearing: the bytes interpreted must be the bytes proved.

### 5.5.2 Deduplication must not erase a kind-specific obligation

`verify_prefix_core` currently deduplicates referenced artifacts by physical identity:

```text
(locator, digest)
```

and keeps only the first encountered `ArtifactRef`. `ArtifactKind` is not part of that dedup key.

Once the gate slot accepts `GateLog | GateCapture`, a reducer-valid stream can contain two gate references to the same physical bytes:

```text
gate A -> GateLog     { locator = X, digest = D }
gate B -> GateCapture { locator = X, digest = D }
```

If `X` contains a malformed capture whose actual digest is `D`, a naive implementation that performs capture parsing only inside the existing `for artifact in unique` loop can retain the first `GateLog`, discard the later `GateCapture` reference as a duplicate, verify the digest, skip parsing, and falsely report the stream verified.

That violates the rule that **every `GateCapture` reference contributes a structural-validation obligation**.

Artifact deduplication is a physical optimization and MUST NOT erase a kind-specific semantic obligation.

Physical identity remains:

```text
(locator, digest)
```

The bytes may be resolved and hashed once. The semantic requirements for those bytes are the **union over every canonical reference** to that identity:

```text
requires_gate_capture_validation =
    any(reference.kind == ArtifactKind::GateCapture)
```

If any reference declares `GateCapture`, the digest-verified bytes MUST be parsed as `GateCaptureV1`, regardless of encounter order.

Re-resolving each `GateCapture` and re-verifying its digest before parsing is also permitted; it simply costs extra I/O.

### 5.5.3 Dedicated replay error

A structural capture failure gets its own replay error carrying at least locator and reason, conceptually:

```rust
GateCaptureInvalid { locator, reason }
```

It is **not** `ArtifactDigestMismatch`: the digest matches.

It is **not** a `ReduceError`: the reducer intentionally does not read artifact content.

This belongs alongside candidate-state semantic verification: chain validity, event digests, reducer validity and artifact digest validity do not by themselves establish the semantics claimed by typed evidence.

Structural parsing fails closed on at least:

- bad magic;
- unknown `format_version`;
- declared lengths outside the artifact;
- length arithmetic overflow;
- truncated frame;
- trailing bytes.

Use one parser for writer-side generation/tests, canonical replay validation, and the human-readable projection.

## 5.6 Acceptance gate

These tests are part of the implementation contract, not suggestions.

### Primary semantic witness — digest-valid malformed capture

Construct a structurally malformed `GateCapture`, compute the **correct** `ArtifactRef.digest` over those exact malformed bytes, embed it in an otherwise valid sealed stream, and prove full canonical replay rejects specifically on capture structural validation.

The witness must have:

```text
chain valid
event digest valid
artifact digest valid
reducer valid
capture structure invalid
-> replay fails
```

This is the end-to-end counterexample to `digest consistency == semantic validity`.

### Cross-kind dedup witness

Construct two otherwise-valid gate events referring to the same `(locator, digest)`, one as `GateLog` and one as `GateCapture`. The shared bytes are a structurally malformed capture with the correct digest.

Full replay MUST reject with the capture-invalid error.

Run the witness in both event orders, or otherwise prove that the result is independent of which kind is encountered first. This test establishes that "every `GateCapture`" means every canonical reference, not every reference that happens to survive physical deduplication.

### Additional acceptance coverage

- **Legacy replay preservation** — a pinned existing v1 `GateLog` fixture replays byte/digest-identically after the change.
- **New capture** — valid `GateCaptureV1` with arbitrary raw bytes, including invalid UTF-8 and NUL, replays successfully.
- **Stream identity** — stdout and stderr with identical contents still parse as two distinct slices.
- **Malformed-frame matrix** — bad magic, unknown version, oversize/overflowing lengths, short payload, and one trailing byte are rejected.
- **Stable tags** — existing tags 1..10 are unchanged; `GateCapture` is 11.

A test that builds an old binary and proves it rejects `GateCapture` is optional, not an acceptance blocker. Closed-enum serde decoding is already the repository's mechanism for this forward-fail behavior.

## 5.7 A1 explicitly out of scope

`o7-a1-contracts::ArtifactKindV1` is a separate frozen closed vocabulary. It contains `GateLog` and does not contain `GateCapture`.

No production conversion currently requires every `o7_run::ArtifactKind` to have an A1 counterpart. Therefore this change does not opportunistically extend A1.

If A1 later needs to reference gate-capture evidence, that is a separate supersede/compatibility decision against its frozen cross-implementation contract.

## 6. Bounded capture — research, not implementation scope

`Command::output()` currently captures both streams without a configured bound. That is a real resource-risk surface, but this note does **not** invent a cap or truncation policy without measurement and process-tree ownership semantics.

Before designing bounded capture, measure representative gate-output sizes and determine how a limit interacts with process ownership, cancellation, partial evidence, and child processes that continue writing after a leader changes state.

No cap, truncation marker, kill policy, or process-tree rule is ratified here.

## 7. Implementation boundary

The implementation PR for this note should contain only the ratified corrective surface:

1. map shell-process signal termination to gate `Error`, with the three termination witnesses;
2. add `ArtifactKind::GateCapture` as tag 11 without moving existing tags or schema versions;
3. encode raw stdout/stderr into one `GateCaptureV1` artifact;
4. let `GateFinished.log` accept legacy `GateLog` and new `GateCapture`;
5. make authoritative replay structurally validate every `GateCapture` using the same digest-verified bytes;
6. preserve kind-specific validation obligations across `(locator, digest)` deduplication;
7. add the §5.6 acceptance witnesses and legacy preservation test.

Do not fold §6 research into this change.

## 8. Non-goals

- No PTY emulation.
- No general transport-semantics model.
- No total stdout/stderr ordering claim.
- No change to `Verdict`, `RunStatus`, `GateOutcome`, or obligation semantics.
- No change to `GateFinished` shape and no second `ArtifactRef`.
- No `RUN_EVENT_SCHEMA_VERSION` bump.
- No `RUN_STATE_SCHEMA_VERSION` bump.
- No change to A1's frozen `ArtifactKindV1`.
- No change to artifact physical dedup identity: `(locator, digest)` remains the key; only semantic obligations are unioned.
- No retroactive change to the agent path.
- No claim that `signal()` detects arbitrary in-step signal termination.
- No `128 + N` heuristic.
- No new `StepVerdict` field.

## 9. Disposition

```text
R7 — ACCEPTED / CLOSED FOR DESIGN

§2  implement
§3  settled
§4  settled
§5  implement
§6  research / measure first
```

Further design revision requires a new counterexample or repository fact. Otherwise the next evidence is code and the acceptance tests above.
