# Public-repo governance

Status: accepted · Scope: this repository's visibility boundary and what may
land in it.

## The inconsistency this fixes

The repo is (as of this note) actually **public** on GitHub. The README
previously said the opposite — "Private, personal harness... Keep this repo
private" — a stale claim from before the repo's visibility changed, not a
current decision. Config that asserts its own falsehood is worse than no
config: a contributor or reviewer trusting the README would reasonably
believe secrets are safe to commit here on the theory that the tree itself
is access-controlled. It is not. This note is the correction, not a proposal
to change visibility — visibility already changed; the docs hadn't caught up.

## The actual public boundary

**May be public:** the orchestration/routing code itself — `o7 run`, `o7
judge`, `o7 invoke` (this repo's whole reason to exist), their prompts,
schemas, and design docs. None of that is a secret; it is a thin CLI wrapper
around already-authenticated `claude`/`codex` subprocesses. Publishing it
does not expose Own.NET's or OwnAudit's source, nor any credential.

**Must never be committed, regardless of visibility:**
- API keys, OAuth tokens, session cookies, or any bearer credential for
  Anthropic, OpenAI, or any other provider.
- `claude`/`codex` local auth state (`~/.claude/`, `~/.codex/`, or
  equivalent session/credential storage) — this project never reads that
  storage directly in the first place (`agent.rs`, `judge.rs`: it shells out
  to the already-logged-in CLI, the same way a human would), so there is
  nothing of that shape to accidentally commit if that boundary holds.
- `.env` files, machine-specific auth artifacts, or environment dumps.
- Real run records that happen to embed proprietary target-repo source
  (`judge/proof.*`, `judge/*.verdicts.json` — already gitignored) or sealed
  benchmark material (`qodec/evals/interop/v2/private/` — already
  gitignored).

**Authorization stays external.** `o7` never stores, reads, or manages
`claude`/`codex` credentials itself — auth is `claude login` / `codex login`
against the CLI, entirely outside this repo's code and config. There is no
"secrets file" for this project to protect because there is no secret this
project holds.

## Secret scan

Run before this note was written, and the command to re-run it:

```bash
gitleaks detect --source . --log-opts="--all" --report-format json --report-path <out>.json
```

**Result (gitleaks 8.16.0, 142 commits, full history + working tree):** 21
findings, all `generic-api-key` (a high-entropy-string heuristic, not a
credential-shaped rule). Every one of them is a `tokenizer_sha256` /
`tokenizer_config_sha256` field inside `qodec/evals/interop/**` result JSON —
content hashes of tokenizer configs, recorded for eval reproducibility, not
credentials. Only two distinct hash values appear across all 21 hits. A
manual sweep of `git log --all --name-only` for common
credential-filename patterns (`.env`, `id_rsa`/`id_ed25519`, `.pem`/`.pfx`/
`.p12`, `.npmrc`, `.netrc`, `credentials`, `secret`, `token`) matched
nothing but `docs/token-codec.md` (a design doc whose name contains
"token"). **No real secret found**, in history or working tree.

This is a point-in-time result, not a standing guarantee — re-run it before
any future visibility/governance change, and treat "I don't think I
committed a token" as the reason to check, not the reason to skip checking.

## Stale cross-reference

`docs/zero-trust-framework.md` reasons from the same now-false premise this
note corrects ("007 itself stays private (per its own README) so Scorecard's
public-repo signals don't apply to it directly"). Updated alongside this
note. Whether to actually run Scorecard/CodeQL/Semgrep over 007 itself (not
just the public siblings) given the corrected visibility is a roadmap
decision for whoever owns that backlog — this note only fixes the factual
premise, it does not resequence the zero-trust backlog.
