# Policy provenance

Status: accepted · Scope: the `policy-provenance.json` record-dir artifact — what it
records, what may depend on it (nothing), and what it is structurally forbidden to
contain.

## The gap it fills

`SandboxPolicy::digest()` (`crates/o7-sandbox-protocol/src/policy.rs`) answers **identity**:
which confinement was installed. It is a hash over `canonical_bytes()`, so it binds the
policy's *meaning* — the same allowances in a different order, or reached a different way,
digest identically. That is the correct property and it is load-bearing: a report's
`policy_digest` is what ties an enforcement claim to a specific policy.

It cannot answer **derivation**: *why* is `/usr/bin/git` in `allow_exec`, which config line
set the timeout, was deny-all networking asked for or defaulted. Audit needs both questions
answered, and one hash cannot answer both — collapsing them would mean two identical
confinements hashing differently because they were spelled differently, which would break
the very property that makes `policy_digest` worth having.

So derivation gets its own artifact.

## The dependency direction

```text
inputs / defaults / config
          │
          ├──────────────► PolicyProvenance          (audit metadata)
          │
          ▼
     SandboxPolicy ──► validate ──► canonical_bytes ──► digest ──► execution
```

Never the reverse. Nothing in admission, execution, or replay reads provenance, so a
missing, truncated, or actively wrong provenance artifact cannot change a policy, a
verdict, or a replay result. `src/events.rs::provenance_is_not_a_verdict_input` holds that
open: it replays one record five ways — valid provenance, a different derivation, a record
whose digest names a policy the run never had, corrupt bytes, no file — and asserts the
recomputed verdict, chain anchor, and normalized-state digest are byte-identical in all
five.

Two consequences follow, and they are not in tension:

- **The artifact is not referenced by any canonical event.** An artifact referenced by a
  digest-chained event is one replay verifies, and verifying it would make it load-bearing.
  Compare `gate/unit.log`, where tampering makes replay fail loudly — that is what a
  load-bearing artifact looks like, and provenance is deliberately not one.
- **Writing it is still a hard error.** `RunRecord::write_policy_provenance` propagates I/O
  failures like every other writer on that struct. "Non-load-bearing" constrains what may
  depend on the artifact's *contents*; it is not a licence to swallow `ENOSPC`.

Reading is correspondingly total: `read_policy_provenance` returns
`Present`/`Missing`/`Malformed`/`UnsupportedVersion` and never `Result`, because a reader that
can return `Err` invites a caller to `?` it into a path that decides something.

`UnsupportedVersion` is separate from `Malformed` on purpose, and the ORDER of the two checks
is what makes that separation real. `PolicyProvenance` is `deny_unknown_fields`, so a genuine
future v2 — today's fields plus a new one — fails to deserialize. Classifying after a strict
parse would therefore have recognised only a future document that happened to fit the current
shape, and reported every real schema evolution as corruption:

```text
future version, v1-compatible shape  →  UnsupportedVersion   (the accident)
future version, genuinely new shape  →  Malformed            (the case that matters)
```

So the reader runs a version preflight — `probe_schema_version`, reading a `schema_version`
envelope that tolerates unknown fields — and only a supported version earns the strict parse:

```text
bytes
  ↓ envelope parse
schema_version?
  ├─ missing / not a u32  → Malformed
  ├─ != supported         → UnsupportedVersion { found }
  └─ == supported
          ↓ strict parse
     Present / Malformed
```

The preflight widens what is CLASSIFIABLE, never what is accepted: a v1 document carrying a
payload probes as supported and is then rejected by the strict parse, exactly as before.

There is deliberately **no `provenance_digest`**. Anything that gets a digest eventually
gets checked, and a check is a trust dependency. If provenance ever becomes part of a
provable contract, that is a separate change that moves the artifact into the replay path
on purpose.

## The invariant: identify a source, never reproduce it

This repository is public and forbids committing environment dumps or credential-bearing
artifacts (`docs/public-governance.md`). Provenance is precisely the artifact that would be
tempted to write `source: "ANTHROPIC_API_KEY=sk-live-…"` or quote a config file's contents,
and a free-form `String` payload is how that lands six months from now — "the type allowed
it".

So no `PolicySource` variant carries free-form content. Every leaf is a validated,
length-bounded newtype whose grammar admits an identifier and rejects a payload:

| Leaf | Accepts | Rejects |
| --- | --- | --- |
| `EnvName` | a POSIX-portable variable NAME | anything containing `=` — the `NAME=VALUE` shape is unrepresentable |
| `ConfigLocator` | `/`-separated segments over `[A-Za-z0-9._-]`, none of them `.` or `..` | `=`, `\`, `:`, whitespace, control characters, non-ASCII, absolute paths, traversal |
| `PolicyKey` | a dotted `lower_snake` key path | the value stored under it |
| `CliOption` | a long flag name | the argument passed to it |

`ConfigLocator` is a portable grammar rather than a platform path, and that is a correction,
not a preference. `Component::Normal` accepts nearly any byte string, so an is-absolute plus
reject-`..` check let `ANTHROPIC_API_KEY=sk-live-…` through as a perfectly good
single-component "relative path"; `C:\Users\alice\secret\o7.toml` is not a Windows prefix
on Linux either, just a name containing backslashes. The charset is now an allowlist, so a
shape nobody anticipated is refused rather than accepted by default.

A locator also means nothing without an anchor, so `PolicySource::Config` carries a
`ConfigAnchor` — one variant today (`TargetRepo`: the repo `o7 run --repo` points at, whose
`.007/gate.toml` supplies the manifest), the same shape as `NetworkPolicy::DenyAll`. A second
anchor is added when a second config root exists, not in anticipation of one.

Restricting KNOWN fields is only half the job. Serde accepts UNKNOWN fields by default, so
every representation is `deny_unknown_fields`: without it,
`{"source":"environment","name":"PATH","value":"sk-live-…"}` parses cleanly into
`Environment { name: "PATH" }`, drops the secret on the floor, and is then reported as a
well-formed record — an armoured door beside an open window. `PolicySource::Default` is an
empty *struct* variant for the same reason: under internal tagging serde deserializes a unit
variant by ignoring the rest of the map, so the attribute would never have reached it. The
wire form is unchanged (`{"source":"default"}`).

Each leaf re-validates on the untrusted deserialize path, so a hand-edited artifact cannot
reintroduce a shape the constructors forbid. The bound is structural, not a sanitizer: **if
this artifact ever needs its output scrubbed before writing, the design has already
failed.**

Note what is *not* lost by omitting arguments and values: the effective values are already
in the policy itself, canonically and digest-bound. Provenance adds the coordinate, not a
second copy of the data.

## Totality

`PolicyProvenance::describe` destructures `SandboxPolicy` exhaustively with no `..`, so a
new policy field fails to build until it is given a `PolicyField` variant and a
`PolicySources` entry — the same compile-time authority link `sandbox_dimensions!` uses
between the dimension list and the report's trust predicates. A policy field cannot exist
with no recorded origin.

`missing_fields()` reports gaps in a *parsed* record (a hand-edited or truncated artifact).
It is diagnostic only; nothing gates on it.

## Not in this slice

`provenance_digest`; any verifier or replay dependency; any change to `SandboxPolicy`'s
semantic identity or to the seven boundary traits.

Also deliberately absent: a prototype/delta authoring layer for policies. It was considered
and rejected **for now, on evidence, not on principle** — a closed-schema declarative
prototype layer resolved totally at admission is compatible with this design. It buys
authoring compression, and there is currently nothing to compress: one policy shape, one
`NetworkPolicy` variant, no policy family. Against that it costs a second resolution
language that would itself need fail-closed proof — cycle, conflict, and unresolved-field
rejection, plus a fuzz target, on the pattern this repo already applies to `gate.toml`.

Preregistered trigger for revisiting it: **at least two policy configurations in real use
with measured structural duplication of their effective dimensions.** The likely first
occasion is the second target/platform (README Phase 2: OwnAudit's Windows-bound gates
alongside cross-platform Own.NET). Observe the duplication before designing the
inheritance, not the other way round.
