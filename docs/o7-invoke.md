# `o7 invoke` — design note

Status: accepted (design note) · Scope: `src/invoke.rs`

`o7 invoke` is a narrow, read-only, schema-bound single-shot agent call —
`judge.rs`'s proven closed-world call pattern (stdin prompt, no ambient
tool/MCP surface, structured output) generalized to an arbitrary
caller-supplied prompt and JSON Schema instead of `judge`'s own hardcoded
per-file verdict shape. It is not a workflow engine, not a DAG, not a
provider abstraction layer — see `docs/workflow-scripting.md` for why those
are explicitly out of scope here, same as everywhere else in this repo.

The one real client today is the sibling `demand-radar` repo
(`agents/o7_invoke.py::O7InvokeRunner`); its own migration record, live
verification results, and cross-repo conformance gate are documented there
(`demand-radar/docs/o7-invoke.md`) — this note covers the primitive from
007's side: what it is, what each engine's flags actually guarantee today,
and where responsibility for those guarantees sits.

## Signature

```bash
o7 invoke
  --engine claude|codex
  --prompt-file prompt.txt
  --input-manifest input-manifest.json   # optional; hashed for provenance
  --schema output.schema.json
  --capability-profile read-only-data
  --out run-dir
  --model <alias>                        # optional
  --timeout-secs 120                     # optional, default 120
```

Writes `<run-dir>/{prompt.txt, stdout.raw, stderr.log, result.json (if
any), meta.json}`. Exit `0` on `PASS`, `1` on every `BLOCKED_*`/`FAIL_*`
status — callers must read `meta.json`, not just the exit code.

## Capability profiles

Exactly one exists: `read-only-data`. An unrecognized profile name is
refused before any subprocess spawns (`run`'s first check) — fail closed,
not a silent fallback to some default posture. `read-only-data` maps to:

- **Claude**: `--tools ""` + `--strict-mcp-config` + `--setting-sources ""`
  — the tool surface is structurally absent, not policy-restricted.
  Verified live against a real `claude` install (v2.1.210) in the
  environment this was built in.
- **Codex**: `--sandbox read-only` + `-c features.shell_tool=false` +
  `--skip-git-repo-check` + `--ephemeral`. **Neither flag has been
  exercised against a real `codex` binary** — `codex` is not installed
  anywhere this was built or tested. `--sandbox read-only` is documented
  (by `judge.rs`'s own comments) to deny writes without disabling network;
  whether `features.shell_tool=false` removes the shell tool the way
  Claude's `--tools ""` does, or merely narrows what it can do inside the
  sandbox, has never been observed.

**`o7 invoke` itself does not refuse `--engine codex`** — it is a general
primitive, and a caller reaching for codex purely to check
reachability/auth (no untrusted content involved) is a legitimate use this
layer shouldn't block. The refusal belongs one layer up, at whoever decides
whether a given call is exposed to untrusted external content: Demand
Radar's `cli.py::run` refuses `--analyst codex`/`--critic codex` for
exactly this reason (its own `docs/trust-boundaries.md` has the full
argument), while its `smoke-agents` command still reaches for codex with a
fixed, non-adversarial prompt. Any future caller of `o7 invoke` inherits
the same unverified posture and needs to make the same call for its own
untrusted-content paths — this primitive documents the gap accurately, it
does not close it by fiat.

## Output re-validation

Both engines' structured output is independently re-validated here
(`jsonschema::validator_for`), never just trusted from the backend's own
claim of conformance. Claude additionally gets `--json-schema` (verified
live) with a `$schema`-meta-key-stripping fix applied to the copy sent to
it — `claude --json-schema` rejects a schema declaring `$schema` with
`Error: --json-schema is not a valid JSON Schema: no schema with key or ref
"https://json-schema.org/draft/2020-12/schema"`; `$id` alone does not
trigger it (`strip_dollar_schema`). Codex gets no assumed
`--output-schema`-equivalent flag (unverified); the schema is appended to
its prompt as an instruction instead, and the same independent validator
decides `schema_valid` regardless of engine.

## Auth

Neither engine's credential storage is read directly — both shell out to
whichever CLI the user already authenticated interactively. `ANTHROPIC_API_KEY`/
`CLAUDE_API_KEY`/`OPENAI_API_KEY`/`CODEX_API_KEY` are stripped from the
subprocess environment before every call, for both engines
(`strip_provider_api_keys`) — added here rather than assumed from
`judge.rs`, which strips neither, after comparing what each existing
integration actually stripped and finding neither prior implementation
(this repo's `judge.rs`, Demand Radar's now-deleted `codex_cli.py`) covered
both engines consistently.

## What's needed to lift the Codex restriction

1. Install and authenticate `codex` somewhere reachable.
2. Re-verify every flag against `codex --help` and real behavior, not
   public documentation — the actual instruction this repo's own task
   history has repeated since `judge.rs` was first written.
3. A live adversarial smoke test: a prompt-injection payload that
   specifically attempts the command-execution/exfiltration path Claude's
   `--tools ""` structurally forecloses, run against the real `codex`
   binary, to confirm `features.shell_tool=false` does what its name
   claims rather than assuming it from the config key existing.
4. Only then should a caller like Demand Radar lift its own
   `--analyst`/`--critic codex` refusal.

## Cross-repo conformance gate

`demand-radar/scripts/o7_conformance_gate.py` runs the same prompt/schema/
input through `o7 invoke` directly and through `O7InvokeRunner`, for both
engines, asserting they agree on `status`/`schema_valid`/`error_kind`/
structured output/`input_hashes`/`provider`/`model`/`exit_code`, plus an
independently-recomputed prompt hash in a third language. It is a
translation-fidelity gate (does the wrapper faithfully relay what `o7
invoke` reports), not a codex-safety gate — codex being unreachable here
means both sides currently agree on `BLOCKED_NOT_INSTALLED`, which the gate
correctly treats as agreement, not as evidence of anything about codex's
actual behavior once installed.
