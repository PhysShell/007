# Dataset source map — Interop Benchmark v2

Survey of candidate sources for future v2 payloads and agent tasks. This scope
**investigates only** — it downloads nothing, ingests nothing, and creates no
real corpus case. A source being large and on GitHub is not evidence it is
useful; the internet is already overflowing with gigabytes someone once called a
benchmark.

Sources are split by *purpose*:

- **Payload corpus** — real logs / tool outputs usable directly as payloads.
- **Task corpus** — good for future agent runs, but not necessarily shipping
  ready-made tool outputs.
- **Generation source** — repos/tasks from which we can reproducibly run tools
  ourselves and capture raw outputs.

## Candidates

| Source | Upstream rev/version | License | Content type | Raw artifacts | Families | Ecosystems | Reproducibility | Est. size | PII/secret risk | Contamination risk | Recommended use | Ingest decision |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Loghub / LogPAI | pin release tag at ingest | mixed (per-dataset) | system/app logs | yes (raw logs) | application-ci-log, exception-stacktrace, container-orchestrator | language-neutral, jvm | high (static files) | large (GBs) | medium (hostnames/IPs) | medium (public, may be in pretraining) | payload corpus | **investigate** — small pinned slice only, sanitize |
| BugSwarm | pin manifest version | see upstream | reproduced CI failures | yes (build/test logs) | compiler-build, test-runner, application-ci-log | jvm, python, javascript-typescript | medium (images) | very large | medium | medium-high | generation source | **investigate** — regenerate logs, do not bulk-ingest |
| Terminal-Bench 2.0 | pin release | see upstream | terminal agent tasks | partial (task specs) | patch-and-test, CI triage, navigation | multi | medium | medium | high (agent-task overlap) | task corpus | **investigate** — future agent A/B only, not payloads |
| TerminalWorld | pin release | see upstream | terminal environments/tasks | partial | multi-step change, config/infra | multi | medium | medium | high | task corpus | **investigate** — future agent A/B only |
| SWE-bench | pin release + split | MIT (harness) / repo licenses vary | issue+patch tasks | via harness runs | patch-and-test, failure diagnosis | python (mostly) | large | low-medium | **very high** (widely trained on) | task corpus / generation source | **investigate** — generation source only; never as sealed held-out |
| Multi-SWE-bench | pin release + split | see upstream | multi-language issue+patch | via harness runs | patch-and-test, multi-step change | dotnet, rust, go, jvm, js-ts, python | large | low-medium | high | task corpus / generation source | **investigate** — multi-ecosystem generation source |
| RTK repository benchmark fixtures | `rtk-ai/rtk` @ `5d32d073…` (already pinned) | upstream RTK license | tool-output fixtures | yes | search-listing, git-diff-history, test-runner | multi | high (pinned) | small | low | medium | payload corpus / method reference | **investigate** — method reference; fixtures usable if license compatible |
| existing qodec / 007 real captures | this repo @ HEAD | in-repo | captured tool outputs | yes | several (existing) | dotnet, rust, … | high (in-tree) | small | low (first-party) | low | payload corpus / generation source | **prefer** — first-party, known provenance |

## Future ingest rules (binding on the later ingest scope)

- pin the upstream revision;
- verify SHA256 of every fetched artifact;
- record the license;
- preserve provenance;
- sanitize secrets / PII (synthetic or irreversibly sanitized);
- deterministic extraction;
- no train/test leakage;
- **no sealed content visible to the policy tuner**;
- large datasets are **not** downloaded on every PR;
- large corpus jobs run via `workflow_dispatch` or schedule;
- their outputs are published as checksummed Actions artifacts;
- a small committed slice is allowed **only** with a compatible license and
  explicit provenance.

## Contamination caution

SWE-bench / Multi-SWE-bench and terminal-task suites are widely present in model
pretraining. They may serve as *generation sources* (run tools, capture fresh
raw output) or public *task corpus*, but their known instances must **never**
become sealed held-out material — a set the model has already seen is not
held-out (see `heldout-policy.md`, rule 7).
