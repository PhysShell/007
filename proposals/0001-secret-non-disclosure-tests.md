# 0001 — Secret non-disclosure tests as an executable gate

```text
status:  raw
date:    2026-08-07
track:   ABR (adjacent), o7 invoke
touches: src/invoke.rs, docs/o7-invoke.md, AGENTS.md rule 1
```

**Not normative.** See [`README.md`](README.md) — a raw proposal is not authority
and may not be cited as grounds for a decision.

## Itch

The credential-policy exception for `o7 invoke --engine arliai` is stated as a
set of properties: the key is read at call time into one function-local owned
value, only a borrowed trimmed view reaches the HTTP layer, it never enters a
struct, artifact, run record or error string, it is stripped from every provider
subprocess environment, and dispatch refuses fail-closed when the `log` facade
admits TRACE (`docs/o7-invoke.md`, "Key handling"; `AGENTS.md` rule 1).

Every one of those properties is currently held by *how the code is written* and
by a P0 review rule that says a change widening the key's reach is a finding.

**Artifact says** — `src/invoke.rs` at commit
`2e74c5106821541296e7e4807811edff450bde67` (blob
`a84a4ee1e6ee75d28f7038f4699b33eceb80149c`), read
2026-08-07: the `#[cfg(test)]` module's test functions cover schema stripping,
hashing, final-JSON extraction for both engines, usage/auth marker
classification, codex argv isolation (`codex_command_is_ambient_isolated`
asserts argv and cwd only), the arliai classification matrix, the backend
parser, the blocked-provider label, timeout descendant-kill, and one
`#[ignore]`d live smoke. No test function in that module asserts the absence of
the key value from any output, and none asserts that `strip_provider_api_keys`
leaves the variable unset in a spawned child.

**Inference**: the strongest claim in the repository is therefore the one with
no failing test behind it — the property most expensive to lose is guarded by a
reviewer's attention span. That is backwards.

## Idea

Make non-disclosure an oracle rather than a habit. Set a sentinel key, run the
real code path against a local stub endpoint, then assert the sentinel does not
appear anywhere it must not:

```text
ARLIAI_API_KEY = "o7-canary-<random>"
    │
    ├─ artifacts written by the call     → sentinel absent
    ├─ the run record / meta.json        → sentinel absent
    ├─ every error string on every        → sentinel absent
    │   failure path (bad model, 401,
    │   timeout, malformed body)
    ├─ captured stdout/stderr             → sentinel absent
    └─ the environment of a spawned       → variable absent entirely
        claude/codex subprocess              (not merely redacted)
```

Two properties, not one. Most of the list is *the sentinel does not appear*; the
subprocess case is stronger — `strip_provider_api_keys` must leave the variable
unset, which an emitted-value scan would not catch. Worth testing as a distinct
assertion: spawn a child that dumps its own environment and assert the name is
missing.

The failure paths matter more than the success path. A key leaks into an error
message far more often than into a happy-path artifact, and error text is
precisely what gets pasted into an issue.

## Why it might be wrong

- A sentinel scan proves the key was absent from *the outputs the test looked
  at*. It does not prove confinement in general, and it must not be described as
  if it did — that is the "lower-layer signal is not an upper-layer fact"
  failure `docs/evidence-and-decision-discipline.md` exists to name. It is a
  regression tripwire, not a proof.
- The TRACE guard already covers the wire-logging path structurally, and it does
  so fail-closed. Adding tests around it risks implying the tests are what makes
  it safe.
- Cost: the useful version needs a stub HTTP endpoint and a real subprocess
  spawn, which is heavier than the current unit tests and is exactly the kind of
  slow suite someone later "fixes" with a CI `--exclude` — itself a P0 finding
  per `AGENTS.md`. If it cannot be cheap, it may not be worth having.
- Scope creep is the obvious trap: this wants to become a generic redaction
  framework. It should stay one sentinel and a handful of assertions.

## What would make it real

Write the negative test first and watch it pass — then deliberately break one
property (format the key into an error string on the 401 path, in a scratch
commit that is never pushed) and confirm the test fails. A non-disclosure test
that has never gone red is decoration.

The grounding above binds `2e74c5106821541296e7e4807811edff450bde67` in full,
not a prefix: an abbreviation stops resolving the moment another object shares
it. Any later reader — promotion or not —
treats it as stale the moment `src/invoke.rs` moves, and re-reads before acting
on it. That is rule 4's step 5, and it applies to this file as much as to
anything in `docs/`.
