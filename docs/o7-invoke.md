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
  --engine claude|codex|arliai
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

The `--out` dir must be **absent or empty**: a non-empty one is refused up
front, before the version probe or any backend spawn, and never partially
overwritten. A given outcome writes only its own files — precisely:
`result.json` exists iff a JSON value was extracted, so
`FAIL_INVALID_OUTPUT` (and every `BLOCKED_*`) leaves none, while
`FAIL_SCHEMA` DOES write one (the offending schema-invalid value is
evidence worth keeping; `schema_valid: false` in `meta.json` is what marks
it failed). Refusing a dirty dir is what stops a previous run's stale
`result.json` from masquerading as this run's output.

## Capability profiles

Exactly one exists: `read-only-data`. An unrecognized profile name is
refused before any subprocess spawns (`run`'s first check) — fail closed,
not a silent fallback to some default posture. `read-only-data` maps to:

- **Claude**: `--tools ""` + `--strict-mcp-config` + `--setting-sources ""`
  — the tool surface is structurally absent, not policy-restricted.
  Verified live against a real `claude` install (v2.1.210) in the
  environment this was built in.
- **Codex**: `--sandbox read-only` + `-c features.shell_tool=false` +
  `--skip-git-repo-check` + `--ephemeral`, plus **ambient-context
  isolation**: `--ignore-user-config` (refuse the user-level
  `~/.codex/config.toml`), `--ignore-rules` (refuse ambient rule files), and
  launch from a **fresh empty temp working directory** (removed after the
  call) so no project `.codex/config.toml` / `AGENTS.md` is discovered by
  walking up from wherever `o7` ran — the codex-side analogue of Claude's
  `--setting-sources ""` + `--strict-mcp-config`. **None of these flags has
  been exercised against a real `codex` binary** — `codex` is not installed
  anywhere this was built or tested. `--sandbox read-only` is documented (by
  `judge.rs`'s own comments) to deny writes without disabling network;
  whether `features.shell_tool=false` removes the shell tool the way Claude's
  `--tools ""` does, or merely narrows what it can do inside the sandbox, has
  never been observed. So the honest closed-world claim for codex today is
  **"ambient user/project context refused, writes denied"** — *not* "no
  shell" and *not* "no network"; that gap is what keeps `--engine codex`
  unfit for untrusted content until live-verified (below).

- **Arli AI**: there is no subprocess and no tool surface at all — the
  backend is a direct HTTPS API call (below). `read-only-data` is satisfied
  structurally: the request carries `tool_choice: "none"`, and a response
  that nevertheless contains `tool_calls` is rejected as
  `FAIL_INVALID_OUTPUT` (belt and braces — this layer never executes a tool
  either way).

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

## Arli AI backend (`--engine arliai`)

The third backend is not a CLI: it is a direct HTTPS call to Arli AI's
OpenAI-compatible chat-completions API. It exists so `o7 invoke` can be
exercised against a cheap hosted open-weights provider without installing
another vendor CLI. The contract below is normative and was written before
the implementation (contract-first).

### Endpoint binding

- The endpoint is a **compile-time constant**:
  `https://api.arliai.com/v1/chat/completions`. There is no flag, no
  environment variable, and no config file that can point the call anywhere
  else — an endpoint override would be a credential-redirection vector (the
  `Authorization` header goes wherever the URL says).
- **Redirects are disabled.** Any 3xx response is terminal:
  `BLOCKED_PROVIDER` (`error_kind: "redirect"`), never followed. HTTP
  clients generally strip `Authorization` on cross-origin redirects anyway;
  the ban is about endpoint binding, not only header leakage — this
  primitive talks to exactly one URL or it doesn't talk.
- **No proxy support** in this slice: the connection is direct, proxy
  environment variables are deliberately ignored (same endpoint-binding
  rationale; revisit as an explicit decision if a deployment ever needs
  one).
- **No retry** in this slice, on anything — one request, one classified
  outcome. Retry policy belongs to the caller (and, for canonical runs, to
  the R1 dispatch-boundary rules), not inside this primitive.

### Configuration refusals (pre-network)

Both are refused with a plain error before any artifact is written or any
connection is opened — config errors, not run outcomes (no `meta.json`,
same as an unknown `--capability-profile`):

- `ARLIAI_API_KEY` unset or empty — there is no interactive login state to
  fall back to, unlike the CLI backends;
- `--model` absent — Arli AI documents `model` as optional, falling back
  to a provider-selected "default served model". 007 refuses that
  implicit identity: a call whose model the provider silently chose is
  not attributable evidence, so the request must carry the explicit
  requested model (`o7` itself still pins none, consistent with the CLI
  backends);
- a global `log` max level that admits TRACE — see the key-handling
  boundary below; this is a fail-closed wire-logging preflight, checked
  before the run dir, artifacts, or any connection.

### Request shape

```json
{
  "model": "<--model>",
  "messages": [{ "role": "user", "content": "<prompt-file text>" }],
  "response_format": { "type": "json_schema", "json_schema": { "name": "o7_result", "schema": { } } },
  "tool_choice": "none",
  "stream": false
}
```

- `response_format` carries the OpenAI-flavored `name` field
  (`"o7_result"`, a fixed label with no semantics here). This is a
  **live-fixture arbitration result**, not Arli's documentation: their
  docs show a bare `schema` with no `name`, but the deployed endpoint's
  request validator (vLLM-style) rejects that form with
  `400 — {'loc': ('body','response_format',…,'json_schema','name'),
  'msg': 'Field required'}` (observed 2026-08-05 against `GLM-4.7`; the
  same probe confirmed `tool_choice: "none"` and
  `include_reasoning: false` are accepted). The documented form was
  implemented first and lost to the live evidence — exactly the
  arbitration this section originally reserved.
- The schema sent is the same `$schema`-meta-key-stripped copy the claude
  path sends (`strip_dollar_schema`) — one precedent, one behavior.
- **No reasoning controls are sent — deliberately, by live-fixture
  arbitration.** `include_reasoning: false` (Arli-documented, defaults
  true) was implemented first and turned out to be actively harmful on
  the deployed endpoint: with it, a reasoning model (`GLM-4.7`,
  2026-08-05) generated the answer (6 completion tokens,
  `finish_reason: "stop"`) but the response carried `content: null` AND
  `reasoning: null` — the generated output was placed nowhere. The same
  request without the parameter returns the answer in `content`
  correctly (with the chain in the non-normative `reasoning` field).
  So nothing is sent: reasoning output may then appear in the response's
  `reasoning` field and therefore in `stdout.raw` (raw provider
  evidence), but it is structurally excluded from the normative result —
  `extract_content` reads `message.content` only, and only that value
  reaches `result.json`. (`chat_template_kwargs: {"enable_thinking":
  false}` was verified to suppress thinking cheaply on GLM, but it is a
  model-family-specific template kwarg; this primitive takes an
  arbitrary `--model` and injects no per-model magic. A future explicit
  opt-in flag may expose it; silently defaulting it is out.)
- Server-side `response_format` enforcement is **best effort, advisory**:
  the local `jsonschema` re-validation (next section) is the only judge
  that counts, exactly as for both CLI backends.

### Response handling

`message.content` must be a string that parses as **bare JSON** after
whitespace trimming — no fence tolerance (with server-side constrained
decoding requested, a fenced answer is a real anomaly worth failing loudly;
`stdout.raw` keeps the evidence). The extracted value then goes through the
same independent schema validation as every other engine.

The response body is bounded by an **explicit** limit,
`MAX_ARLIAI_RESPONSE_BYTES` (10 MiB): a body exceeding it is
`BLOCKED_PROVIDER` (`error_kind: "response_too_large"`). The bound is part
of this contract, not an inherited SDK default — a runtime boundary must
not exist only as "the library happens to do that".

Classification matrix (normative):

| Observation | status | `error_kind` |
|---|---|---|
| 2xx, content parses, schema-valid | `PASS` | — |
| 2xx, content parses, schema-invalid | `FAIL_SCHEMA` | `schema_violation` |
| 2xx, body not JSON / envelope malformed | `FAIL_INVALID_OUTPUT` | `bad_envelope` |
| 2xx, `choices` empty | `FAIL_INVALID_OUTPUT` | `empty_choices` |
| 2xx, `content` null/absent | `FAIL_INVALID_OUTPUT` | `null_content` |
| 2xx, `tool_calls` present (non-empty) | `FAIL_INVALID_OUTPUT` | `tool_calls_present` |
| 2xx, content string is not JSON | `FAIL_INVALID_OUTPUT` | `invalid_json` |
| 3xx (any) | `BLOCKED_PROVIDER` | `redirect` |
| 401 / 403 | `BLOCKED_AUTH` | `auth` |
| 402 / 429 | `BLOCKED_USAGE` | `usage_limit` |
| 408 (server-side request timeout) | `BLOCKED_TIMEOUT` | `timeout` |
| any other 4xx (request rejected — nothing was generated) | `BLOCKED_PROVIDER` | `http_request_rejected` |
| 5xx | `BLOCKED_PROVIDER` | `http_5xx` |
| body exceeds `MAX_ARLIAI_RESPONSE_BYTES` | `BLOCKED_PROVIDER` | `response_too_large` |
| any other non-2xx (1xx, non-standard codes) | `BLOCKED_PROVIDER` | `http_status_unclassified` |
| DNS / TLS / connection refused / reset | `BLOCKED_PROVIDER` | `transport` |
| timeout (`--timeout-secs`) | `BLOCKED_TIMEOUT` | `timeout` |

`BLOCKED_PROVIDER` is **new, and 007-side only**: it has no counterpart in
`demand_radar.models.AgentRunStatus` yet. That is deliberate and safe —
the cross-repo conformance gate runs `claude`/`codex` only (see the gate
section below), so the shared-vocabulary invariant it checks is untouched.
If Demand Radar ever grows an `arliai` path, its vocabulary must adopt
`BLOCKED_PROVIDER` first.

**The FAIL/BLOCKED boundary** (review-round corrections, rounds 5–6):
`FAIL_*` is reserved for the 2xx path — the provider claimed success and
the payload proved unusable (bad envelope, null content, non-JSON,
schema violation). Every non-2xx response means no trustworthy answer was
produced — the request was rejected, redirected, throttled, or failed
upstream — and classifies as a `BLOCKED_*`, with `error_kind`
distinguishing the cause; an *unknown* cause (`http_status_unclassified`)
is still an absence of an answer, never a model failure. This is the
repo's FAIL/ERROR distinction applied to HTTP: recording a non-2xx as
`FAIL_INVALID_OUTPUT` would claim the model produced bad output when
nothing ran. The CLI backends' `nonzero_exit` precedent does not
transfer: a CLI that exits nonzero *ran*, and its output is genuinely
ambiguous.

### Artifacts and `meta.json` mapping

Same run-dir contract (`--out` absent-or-empty, refused otherwise):

- `stdout.raw` — the **raw HTTP response body**, byte-for-byte, for a **2xx**
  response within the `MAX_ARLIAI_RESPONSE_BYTES` bound. The analogue of a CLI
  backend's raw stdout: the unmodified provider evidence, and the input the
  local schema re-validation judges. It is empty for transport failures (no
  body existed), for an over-limit response (the body is deliberately not
  persisted; the size message goes to `stderr.log`), and — see the amendment
  below — for every **non-2xx** response.

#### Amendment: a non-2xx body is not canonical run evidence

**Status: contract amended; the implementation does not yet match — see
"Divergence" below.**

The rule above previously read "whatever the status". It does not any more, and
the reason is the credential boundary rather than tidiness.

A non-2xx body is the provider's **diagnostic channel**, and diagnostics are
where servers echo request context back at the caller. That is not speculative
for this endpoint: the request-shape arbitration recorded above observed a
`400` whose body echoed the request's own field path (`{'loc': ('body',
'response_format',…,'json_schema','name'), 'msg': 'Field required'}`). A
diagnostic that echoes request headers rather than request fields is the same
mechanism, and this backend sends `Authorization` on every call.

Meanwhile nothing needs that body. The classification matrix above decides
every non-2xx case from the **status code alone** — `401`/`403` → `BLOCKED_AUTH`,
`402`/`429` → `BLOCKED_USAGE`, `408` → `BLOCKED_TIMEOUT`, other `4xx` →
`http_request_rejected`, `5xx` → `http_5xx`. `extract_content` is never reached.
So persisting the diagnostic body bought convenience while widening the set of
places an echoed credential could land — and `AGENTS.md` rule 1 counts the
indirect path (writing provider output into a run record without considering
what it may have echoed) as a P0, not as a lesser concern.

The amended contract:

```text
2xx      stdout.raw = the exact provider body      (canonical evidence)
non-2xx  stdout.raw is empty
         status + error_kind remain the evidence   (meta.json)
         the diagnostic body is not persisted
```

`stderr.log` may carry a bounded, non-body detail for non-2xx (the status and
its classification), never the body itself.

**Divergence (open, tracked).** At the time of this amendment `run_arliai`
still writes `ArliOutcome::Http { body, .. }` to `stdout.raw` for every in-bound
status, including non-2xx. The contract is amended first, deliberately: the
implementation change is a separate slice, and until it lands the direct
`--engine arliai` path retains the diagnostic-body surface described here. Do
not read this section as describing current behaviour.

First consumer of the amended rule: `docs/tasks/mg-c-model-gate.md` (stage
MG-C), which inherits it rather than defining a second, divergent evidence
semantics for gate-brokered calls.
- `stderr.log` — transport/timeout/over-limit detail; empty on an
  in-bound HTTP response.
- `result.json` — the extracted (normalized) content value, written only
  when one was extracted, same as the CLI paths.
- `meta.json` — `provider: "arliai-api"`, `command_version: null` (there
  is no CLI to probe), `exit_code: null` (no subprocess; both fields are
  already nullable in the schema — no schema change), `model` always set
  (it is required), everything else as for the CLI backends.

### Key handling (the fifth security boundary)

The existing four key rules (stdin-not-argv prompts, key-stripping before
provider subprocesses, no credential storage read, independent
re-validation) gain a fifth for a backend that *does* hold a key in
process memory:

- `ARLIAI_API_KEY` is read once in `run` into one function-local owned
  value; only a borrowed, trimmed view of it is passed into the one
  function that sets the `Authorization` header. The load-bearing
  properties: it is **never stored in any struct**, never formatted into
  any error/log/artifact string, and never reaches `meta.json` or the
  run dir.
- It is added to `strip_provider_api_keys`, so a `claude`/`codex`
  subprocess never inherits it either.
- HTTP-client wire logging is the classic leak path for exactly this kind
  of header (`ureq` logs via the `log` facade, and its wire-level TRACE
  is documented as unredacted). Today the `o7` binary initializes no
  logger, so the facade's records go nowhere — but that is an ambient
  fact about the current binary, not a boundary anyone enforces. The
  enforced boundary is a **fail-closed preflight**: `run_arliai` refuses
  to dispatch when the global `log` max level admits TRACE
  (`log::max_level() >= Trace`), before the run dir, any artifact, or any
  connection. A future logger in `o7` (or any embedder of this crate)
  degrades that run to a refusal, never to a key on a log sink.

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

### Claude `--output-format json` envelope compatibility

The `result` field of Claude's JSON envelope has been observed in two shapes
across CLI versions, so `o7` accepts exactly those two and nothing looser:

- **2.1.210** returned a **bare JSON** value in `result`;
- **2.1.162** returned **one full ```json fenced block** in `result`
  (` ```json\n<JSON>\n``` `), even with `--json-schema`;
- `o7` accepts exactly those two shapes — an absent/other/uppercase/`json
  extra` tag, prose outside the one block, a second block, or unbalanced
  fences all fall through to `FAIL_INVALID_OUTPUT`;
- the raw stdout on disk (`stdout.raw`) is unchanged — this only affects how
  `result` is read;
- the extracted value is then **independently schema-validated** (a
  syntactically valid but schema-violating value is `FAIL_SCHEMA`, not
  `FAIL_INVALID_OUTPUT`).

## Auth

Neither CLI engine's credential storage is read directly — both shell out
to whichever CLI the user already authenticated interactively (`arliai`,
having no CLI, authenticates with `ARLIAI_API_KEY` directly — see its own
section above). `ANTHROPIC_API_KEY`/
`CLAUDE_API_KEY`/`OPENAI_API_KEY`/`CODEX_API_KEY`/`ARLIAI_API_KEY` are stripped
(`strip_provider_api_keys`) before **every provider subprocess** — the real
call *and* the best-effort `--version` probe (the probe also runs under a
short bounded timeout, degrading to `command_version: null` rather than
stalling the call). This holds for both engines regardless of which is
selected — added here rather than assumed from `judge.rs`, which strips
neither, after comparing what each existing integration actually stripped and
finding neither prior implementation (this repo's `judge.rs`, Demand Radar's
now-deleted `codex_cli.py`) covered both engines consistently.

Auth-failure classification uses **engine-specific** markers (Claude's
`claude login` / `/login`, Codex's `codex login`) on top of a small shared
set (`not logged in`, `please log in`, `no active session`, `unauthorized`,
`authentication`). Bare `login` and bare `please run` are deliberately *not*
markers: they fire on ordinary diagnostics ("failed to login to the
database", "please run cargo test") and a false `BLOCKED_AUTH` would bury the
real error.

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
input through `o7 invoke` directly and through `O7InvokeRunner`, for the
two **CLI** engines (`claude`, `codex` — the gate is deliberately not
`arliai`-aware, and `BLOCKED_PROVIDER` stays 007-side vocabulary until
Demand Radar adopts it), asserting they agree on `status`/`schema_valid`/`error_kind`/
structured output/`input_hashes`/`provider`/`model`/`exit_code`, plus an
independently-recomputed prompt hash in a third language. It is a
translation-fidelity gate (does the wrapper faithfully relay what `o7
invoke` reports), not a codex-safety gate — codex being unreachable here
means both sides currently agree on `BLOCKED_NOT_INSTALLED`, which the gate
correctly treats as agreement, not as evidence of anything about codex's
actual behavior once installed.
