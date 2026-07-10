# Comprehension A/B — diag + tmpl notations, run record, 2026-07-10

- **Panel:** 4 fresh-context Claude subagents (Claude Code session model),
  one per (payload × side), tools forbidden, payload inline. Fresh contexts
  had no access to the raw originals on the encoded side.
- **Payloads:**
  - `ownsharp-broker.txt` (real OwnAudit `sts_audit/` output, 13 findings,
    920 tokens raw) encoded with `--codec diag` → 770 tokens (−16.3%);
  - `2026-07-10-build-slice.log` (first 46 lines of the MSBuild-style
    generator log, committed alongside as the fixture, 1 570 tokens raw)
    encoded with `--codec tmpl` → 1 264 tokens (−19.5%).
- **Questions:** `ab/broker-ownsharp.json`, `ab/build-tmpl.json` (6 per
  payload; one accept calibrated post-run for a correct paraphrase —
  "line 748, column 16" vs the literal `748,16` — applied to both sides).
- **Answers:** `ab/results/2026-07-10-*.answers.json`, graded by
  `qodec ab grade --prompt` (the accuracy/1k column is new).

| payload | side | score | prompt tokens | accuracy/1k |
|---|---|---:|---:|---:|
| broker (diag) | raw | 6/6 | 1 098 | 91.1 |
| broker (diag) | encoded | 6/6 | 1 209 | 82.7 |
| build (tmpl) | raw | 6/6 | 1 748 | 57.2 |
| build (tmpl) | encoded | 6/6 | 1 703 | 58.7 |
| **total** | | **24/24** | | |

Comprehension holds for both new notations, including the hard case:
counting rows *by template alias* across the tmpl body (17 `Restoring`
rows, 7 `CS8618` rows) — aggregation over the notation, not lookup.

**The accuracy/1k column earns its keep immediately:** on payloads this
small the ~230-token notation brief is the dominant cost — it *exceeds*
diag's 150-token saving on broker (encoded prompt ends up bigger than
raw), and eats most of tmpl's 306 on the build slice. Perfect scores at
lower accuracy/1k = the codec pays but the *teaching* doesn't, at this
scale. Two consequences, both already in the lab's vocabulary: the brief
is byte-stable (unlike a mine legend) and belongs in a cached prompt
prefix exactly like the extern legend; and cold one-shot encoding of
sub-2k-token payloads is not where codecs earn — at the real 133 KB
ownsharp scale (32 410 → 15 687 tokens) the brief is noise.

Wall-time asymmetry is not one-sided at this size: the encoded broker
reader took ~2.2× the raw one (23 s vs 10 s, alias-dense diag), but the
encoded build reader was *faster* than raw (37 s vs 42 s) — counting 17
raw `Restoring` lines is work too; the tmpl table made the count easier.

Scope honesty: one model family, one run each, 2 payloads, retrieval
questions, and the panel operator authored the questions knowing the
payloads. The gate stays *open*, not *proven* — same standing caveat as
the 2026-07-08 record; the STS-scale judge A/B remains the next rung.
