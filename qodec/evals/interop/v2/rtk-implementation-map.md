# RTK implementation map — pinned source audit

**Pinned source:** `rtk-ai/rtk` @ commit
`5d32d0736f686b69d1e8b9dc45c007d4eb77a0a2` (`flake = false`).

This is a *static design audit* of the RTK reducer as it will be built by
`packages.rtk-pinned`. It exists so the RTK↔qodec comparison is grounded in
RTK's *actual* behaviour at the pinned commit, not in a marketing description.
**No qodec policy is changed as a result of this analysis** — that is explicitly
out of scope. Where the audit cannot be completed offline, the gap is marked
`AUDIT-PENDING (build-from-pinned)` rather than guessed, and is resolved in CI
where `packages.rtk-pinned` builds from the pinned commit with network egress.

> Environment note: the scope that authored this file has no GitHub egress (org
> policy 403) and no Nix, so the RTK tree could not be fetched here. The audit
> dimensions below are the required checklist; each is resolved against the
> pinned build in CI and recorded with a source-line reference. The pin (commit
> SHA) is authoritative and committed in `flake.nix`.

## Audit dimensions

Each RTK subsystem is characterised on three axes that matter for composition
with qodec: **is it lossy?**, **does it drop identifiers/paths qodec would want
to preserve?**, and **does it leave a recovery affordance** (a hint the agent
can use to re-fetch the raw).

| Subsystem | What to verify at the pinned commit | Composition concern | Status |
|---|---|---|---|
| Command rewrite layer | how RTK intercepts/rewrites a command line | changes the `command` identity recorded in the manifest | AUDIT-PENDING (build-from-pinned) |
| Explicit command invocation | direct `rtk <tool> …` entry points | which tools have first-class support | AUDIT-PENDING |
| Per-command parsers | one parser per tool (build/test/grep/git/…) | parser coverage vs the 12 families | AUDIT-PENDING |
| Stats extraction | counts/totals RTK synthesises | may replace exact numbers with summaries | AUDIT-PENDING |
| Grouping | how RTK groups similar lines | reorders/merges — ordering questions at risk | AUDIT-PENDING |
| Deduplication | dedupe of repeated lines | removes nested repetition qodec folds losslessly | AUDIT-PENDING |
| Truncation | when/where RTK truncates | **lossy**; drops evidence spans | AUDIT-PENDING |
| Progress filtering | CR/ANSI progress stripping | removes carriage-return-progress hazard content | AUDIT-PENDING |
| State-machine parsers | multi-line stateful parsing | partial-state loss on malformed input | AUDIT-PENDING |
| JSON / NDJSON modes | structured input handling | may reshape structured-data-query payloads | AUDIT-PENDING |
| Passthrough conditions | when RTK returns input unchanged | defines where RTK == identity | AUDIT-PENDING |
| Parse-failure fallback | behaviour on unrecognised input | fallback to raw vs partial | AUDIT-PENDING |
| Exit-code preservation | whether the tool exit code survives | outcome classification depends on it | AUDIT-PENDING |
| Tee / raw recovery | keeping a copy of raw output | enables `raw-recovery-calls` metric | AUDIT-PENDING |
| Recovery hints | `[full output: …]` / `[see remaining: …]` markers | future protected spans must keep these | AUDIT-PENDING |
| Configurable limits | tunable caps (lines, bytes, groups) | changes reduction aggressiveness | AUDIT-PENDING |
| RTK token accounting | RTK's own token estimate | uses chars/4 — diagnostic only, never leaderboard | AUDIT-PENDING |
| RTK benchmark scripts | RTK's smoke/output benchmark harness | method reused, verdict refused (see external-baselines) | AUDIT-PENDING |
| RTK ON/OFF session benchmark | paired-session harness | informs agent A/B design | AUDIT-PENDING |

## Rule

The pinned commit is the audited artifact. Any RTK-aware qodec behaviour is a
**future** policy (`VG-RTK-v1`) with its own name and source SHA — see
`rtk-qodec-composition-risks.md`. This document never authorises a change to the
frozen VG policy.
