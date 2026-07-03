# o7 judge — FP-triage (proof-kit + contract)

`o7 judge` classifies analyzer findings as **real / false_positive / uncertain**.
Separate mode from `o7 run`:

- `o7 run`   → agent edits code, gate emits a **gate verdict** (`PASS/FAIL/ERROR`).
- `o7 judge` → read-only, per-file, emits a **classification overlay**
  (`fp-verdicts.json`). Classifies; never gates, never edits.

Two different verdict types — do NOT reuse the gate `Verdict` enum.

## Contract — one source of truth
Overlay shape + finding identity are **domain-owned** and live in OwnAudit:
- **`OwnAudit/docs/fp-judge/verdict-contract.md`** — SOURCE OF TRUTH (identity + fields).
- **`OwnAudit/docs/fp-judge/rubric.md`** — what each class *means*.
- `007/judge/fp-verdicts.schema.json` here = the **machine encoding** (JSON Schema)
  of that contract. Keep it in sync; the `.md` wins.

**finding identity** (both sides compute identically — `line` drifts ±3 run-to-run):
`finding_id = sha1(path + 0x1f + rule + 0x1f + message).hexdigest()[:16]`.
One id may cover >1 physical findings (same message, different lines) →
**judge once, report expands** (210 findings → 198 ids on the current audit).

**overlay** = envelope `{schema, tool, generated_from (sha256 of the judged
findings.json — staleness guard), model, run_id, verdicts}`, where `verdicts` is a
**map** `finding_id → {class, confidence 0..1, reason, evidence?, lines?}`.

## Flow
1. The judge (this kit) outputs a raw **per-finding array** (`class/confidence/reason/evidence`).
2. 007 computes `finding_id`, dedupes, and assembles the `fp-verdicts.json` map +
   `generated_from` (Phase 2, inside the `o7 judge` subcommand).

## Files here

| file | owner | what |
|---|---|---|
| `prompt.template.md` | 007 | judge prompt; slots `{{RUBRIC}} {{FILE_PATH}} {{FILE_CONTENT}} {{FINDINGS_IN_FILE}}` |
| `fp-verdicts.schema.json` | 007 (encodes domain contract) | machine schema for the overlay |
| `rubric.example.md` | placeholder — **real rubric is `OwnAudit/docs/fp-judge/rubric.md`** | reference only |
| `build-proof.sh` | 007 | fills the template for one file, prints the claude command |

## Phase 1 — manual proof (BEFORE building `o7 judge`)
The judge needs the **scanned source** (whole-file context). STS/Broker source may
not be local — so prove on OwnAudit's `oracle/LeakyOracle` (local + known leaks).
Fixture paths are **repo-root-relative**, so `--src-root` = the OwnAudit root and
`--file` = the full finding path:

```bash
# from the 007 repo root, in the nix devShell (needs jq + claude)
bash judge/build-proof.sh \
  --src-root ../OwnAudit \
  --findings ../OwnAudit/oracle/fixtures/findings.json \
  --file     oracle/LeakyOracle/ViewModels/WatchlistViewModel.cs \
  --rubric   ../OwnAudit/docs/fp-judge/rubric.md
# then run the printed:  claude -p "$(cat judge/proof.____.prompt.md)" --model opus
```
Repeat for `oracle/LeakyOracle/ViewModels/TickerViewModel.cs`.

**Ground truth:** the fixture is 2 *intentional* leaks (subscription + undisposed
`Timer`) — both should come back **`real`**. So this proves "does the judge confirm
real leaks?"; it has **no false_positive example**, so it does NOT exercise the FP
side. For that, add an FP fixture (a properly-unsubscribed handler) or move to STS.

Domain refines `rubric.md`; loop until verdicts are trustworthy — **that trust is
the gate to Phase 2.** Proof prompts embed source → gitignored.

## Phase 2 — `o7 judge` (built)
Batches per file, computes `finding_id`, dedupes, assembles the overlay + `generated_from`:

```bash
o7 judge \
  --repo     ../OwnAudit \
  --findings ../OwnAudit/oracle/fixtures/findings.json \
  --rubric   ../OwnAudit/docs/fp-judge/rubric.md \
  --out      ../OwnAudit/artifacts/fp-verdicts.json
# --dry-run : print files/ids/calls, no backend  |  --only <path> : one file
# --max-files N : cap cost   |  --model : default opus  |  --provider claude|codex|auto
```
Writes the overlay to `--out` (for the domain report) **and** a judge run-record to
`runs/<target>/judge-<id>/` (overlay + per-file raw output + `meta.json` with
`provider`, `by_class`, cost, session ids). Start with `--dry-run`, then
`--only <one file>` to sanity-check, then the full set.

**Read-only, but the two backends differ.** The default **claude** backend runs
closed-world — `--tools ""` + `--strict-mcp-config`, no built-in tool and no ambient
MCP — so a prompt-injection payload in a judged file has no read/network/exfil path.
The **codex** backend (`--provider codex`) runs `--sandbox read-only`, which denies
**writes** but does **not** disable network (codex has no one-flag equivalent). So
under `--provider codex` a payload still can't write, but could reach the network —
prefer the claude backend when judging untrusted source, and see
[`docs/security-layers.md`](../docs/security-layers.md).

