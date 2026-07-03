# sts-stub — judge regression fixture

A tiny .NET target in the STS domain (event/timer lifetime, rules `OWN001` /
`OWN-TIMER`) with a hand-authored `own-check` `findings.json`. Exercises the judge
end-to-end on a realistic mix, and specifically the hardened edge cases:

- **real vs false_positive discrimination** across 4 files.
- **`(path,line,rule)` collision** — `MixedViewModel.cs:15` carries two distinct
  findings (different `message` → different `finding_id`) on the same tuple. The
  positional-pairing path must keep **both** verdicts (neither overwritten).
- **dedup / grouping** — 6 findings → 6 unique ids → 4 `claude` calls (grouped by file).

## Expected verdicts

| file:line | rule | class |
|---|---|---|
| ViewModels/EventLeakViewModel.cs:14 | OWN001 | real |
| ViewModels/TimerLeakViewModel.cs:13 | OWN-TIMER | real |
| ViewModels/CleanViewModel.cs:17 | OWN001 | false_positive (`-= OnQuote` in Dispose) |
| ViewModels/CleanViewModel.cs:18 | OWN-TIMER | false_positive (`_timer.Dispose()`) |
| ViewModels/MixedViewModel.cs:15 (QuoteReceived) | OWN001 | real |
| ViewModels/MixedViewModel.cs:15 (Disconnected) | OWN001 | real |

Totals: `{real: 4, false_positive: 2}`, 6 ids in the overlay.

## Run

    # free + deterministic — asserts dedup/grouping/call-count (no claude calls):
    bash judge/fixtures/sts-stub/proof.sh

    # live classification (spends ~4 claude calls, needs claude logged in):
    O7_PROOF_LIVE=1 bash judge/fixtures/sts-stub/proof.sh
