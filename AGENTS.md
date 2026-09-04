# AGENTS.md — 007 (`o7`)

Standing instructions for agents working in this repo: Codex code review on
pull requests, `claude`/`codex` runs driven by `o7` itself, and any other
harness that reads `AGENTS.md`.

`o7` is a Rust harness that drives external coding agents over target repos and
reduces each run to a verdict. The binding invariants live in `README.md`,
`docs/public-governance.md`, `docs/security-layers.md`, and
`docs/verification.md`; this file is the review-facing summary of them, not a
replacement.

**This repository is public.** The orchestration code is not a secret. The
absence of credentials in it is the claim the whole repo rests on.

## Code Review Rules

### Already enforced mechanically — do not spend review on these

CI fails the pull request on every item below, so a review comment about one is
redundant noise:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo check --workspace --all-targets`
- supply chain: advisories, bans, licenses, sources (`cargo-deny`)
- the test suites of every workspace member, including the root `o7` package's
  `proptest!` invariants

**Every workspace member is tested automatically unless explicitly excluded.
Any new `--exclude` entry must name the dedicated workflow that owns that
package.** Today the only exclusions are `o7-worktree` and `o7-verifier`, owned
by `pr3-worktree-verifier-gate.yml`. An exclusion added for any other reason —
a slow suite, a flaky test — silently drops that crate out of CI, and is a P0
review finding.

The lint set in `Cargo.toml` — `unsafe_code = "forbid"`, `unwrap_used`,
`expect_used`, `panic`, `dbg_macro`, `todo`, `unimplemented`,
`indexing_slicing` — is compiler-enforced for the same reason. Raise these only
when a change *adds an `#[allow(...)]`*; see rule 4.

### What to actually review (P0/P1 only)

1. **Credential leakage — P0, the repo's central claim.** No credential,
   OAuth/session-storage artifact, token, or environment dump may enter the
   tree. That includes the indirect paths a secret scanner cannot see: logging
   or `Debug`-printing an env var, serialising a config struct that holds one,
   or writing agent stdout into a run record without considering what the agent
   may have echoed. Auth for the CLI engines is external (`claude login` /
   `codex login`) and never read or stored. The one exception is a
   **maintainer-ratified credential-policy exception** — an explicit policy
   extension, not documentation catching up — scoped to exactly two
   consumers of a direct HTTPS API that has no vendor CLI to delegate auth
   to:

   ```text
   o7 invoke --engine arliai      ratified
   o7-model-gate                  ratified (MG-C; not yet implemented)
   ```

   Both consumers may access the ArliAI provider credential **only
   call-scoped**, in one function-local owned value, with only a borrowed
   trimmed view passed to the HTTP layer. It is never held in any struct
   and never composed by trusted code into `meta.json`, `stderr.log`,
   `result.json`, prompts, or error strings; it is stripped from every
   provider subprocess environment, and dispatch refuses fail-closed when
   the `log` facade's max level admits TRACE (HTTP wire logging).

   **The source differs, and the difference is normative:**

   ```text
   o7 invoke --engine arliai
       ARLIAI_API_KEY, per its ratified environment-based direct-path
       contract (docs/o7-invoke.md, "Key handling")

   o7-model-gate
       a trusted credential file/descriptor, per MG-C §8.4; the provider
       credential in the gate's environment is FORBIDDEN in every mode,
       and the gate refuses to start if it finds one
   ```

   A long-lived process holding the secret in its environment is the
   widening this rule exists to prevent, which is why the gate does not
   inherit the direct path's source, and why its call-scoped lifetime is
   never relaxed into daemon-lifetime ownership.

   **Provider bytes we relay are a separate question with a separate
   answer.** The guarantee above covers what trusted code *composes*. It
   does not extend to bytes the provider produced: a 2xx body reaches
   `stdout.raw` verbatim because that is what the schema re-validation
   judges. Two of the three cases there are closed by mechanism — a
   non-2xx diagnostic body is not persisted at all, and a **verbatim**
   credential echo in a 2xx body is refused (`BLOCKED_PROVIDER` /
   `credential_reflected`). What remains is a provider embedding the
   credential in *transformed* form in a successful body, which no byte
   comparison settles; that is the stated boundary of the promise, not a
   gap to be closed by wording.

   Normative contracts: `docs/o7-invoke.md` ("Key handling") for the
   direct path, `docs/tasks/mg-c-model-gate.md` §8 and §8.6 for the gate.
   Any change that widens where that key can flow — including any new
   process able to read it — is a P0 finding and needs its own
   ratification, not an inference from this one.

2. **Verdict semantics.** `PASS` / `FAIL` / `ERROR` are three distinct states:
   `FAIL` means the gate ran and the target failed it; `ERROR` means the
   harness could not obtain a trustworthy answer. Collapsing `ERROR` into
   either neighbour turns a broken harness into a green run. The process exit
   code is `0` on `PASS` and non-zero otherwise — callers and CI gate on that.

3. **Run-record integrity.** `runs/<target>/<run-id>/` is the canonical
   artifact. Review changes to its layout or contents for backward
   compatibility, and for whether a partially-failed run can leave behind a
   record that reads as complete.

4. **A new `#[allow(...)]` on a restriction lint.** The tree has exactly two
   justified sites (`reps[i]`, each carrying the in-bounds invariant in a
   comment). Any new one must state the invariant that makes it sound;
   "clippy was noisy" is not that invariant. It must also state its EXTENT, on a
   line reading `// Extent: <n> \`<lint>\` sites`, because an invariant with no
   stated subject cannot be held to the code: twelve files in
   `o7-closure-provenance` once justified an allowance on "JSON literals written
   in this file" while covering matcher-registry lookups that no fixture
   invariant ever described. A site the extent covers for a DIFFERENT reason
   must say so beside the exception rather than borrow the invariant next to it.
   Nothing in this repository checks either requirement tree-wide. Some crates
   audit their own allowances; most do not, and no audit sees a lint level set
   outside the source text. The rule is on whoever writes the allowance.

5. **Untrusted-input parsers.** `o7::judge::extract_json_array`,
   `o7::judge::parse_findings_json`, and `o7::gate::GateManifest::parse`
   consume model output and third-party manifests. They are fuzzed, and the
   slicing ones are Kani-proved panic-free. A change to their
   slicing/indexing logic is P0 and should extend the corresponding harness in
   the same pull request.

6. **The lint ratchet.** `docs/verification.md` defers `pedantic`/`nursery` and
   friends on purpose — "a false positive is worse than a miss." Do not propose
   enabling them wholesale; per-slice adoption against a baseline is the agreed
   path.

### Review style

- Flag **P0 and P1 only.** Style, naming, and taste belong to the formatter and
  clippy, which already own that layer and run before you.
- Prefer a concrete failure scenario — inputs, then wrong behaviour — over a
  general concern. If you cannot state one, it is probably not P1.
- `o7` is subprocess-bound (`docs/performance.md`). Micro-optimising code that
  is not on a subprocess boundary is not a finding.

### Diagnosing is not repairing

A run that was asked to explain a failure may not fix it in the same breath.
The four lines below are one failure class each — a convenient partial signal
being reported as a whole answer:

```text
diagnosing is not repairing             a diagnostic run may not mutate
a likely boundary is not a root cause   name the evidence or say "unknown"
missing evidence is not a passed check  an absent signal is not a negative result
a first page is not the whole result    partial success is not success
```

Line 3 is the same failure the harness already refuses in code: a skipped
required gate scores `BLOCKED`, never `PASS` (the `o7-run`
`GateApplicability::Waived` doctrine — only a pre-declared `waive_reason`
legitimises a skip). Line 4 is its pagination case. Stating them here closes
the gap between what the harness enforces on itself and what a driven agent is
told; the boundary itself is owned by the sandbox and the campaign machine
(`docs/security-layers.md`, `docs/autonomy-controller.md`'s `HUMAN_REQUIRED`),
not by this paragraph. Source of the contract and what was deliberately not
taken with it: `docs/evokoa-transplant-map.md` §3.2.

### Grounding factual claims

A claim about what the code or an existing mechanism already does — in a review,
a commit message, a doc, or a rationale for a change — must name the
authoritative artifact and its exact property, and separate what the artifact
*says* from the inference drawn on top of it. Do not accept (or write) "the core
already guarantees this" without the file, the revision, and the specific
property that establish it; a claim about existing architecture whose governing
artifact has changed is stale until re-checked. Full rule and the failure class
it guards against (a lower-layer signal is not an upper-layer fact):
`docs/evidence-and-decision-discipline.md`.
