# case-0005 — overload set collapsed to one symbol

```text
format(value: number): string     <- overload signature, selected by formatAmount
format(value: Date): string       <- overload signature, selected by formatWhen
format(value: number | Date)      <- implementation, reached by both
```

## Two seeds, two different right answers

Seeding the **implementation** signature should return both callers. Seeding the
**number overload** signature should return only `formatAmount`. A resolver that
keys symbols by name and arity returns the same set for both seeds — and that
set is correct for one of them, which is what makes the failure easy to miss in
a spot check.

This is why the case carries two seeds rather than one. A single seed cannot
distinguish "resolved overloads correctly" from "collapsed the set and happened
to be asked the question the collapse answers correctly".

## Admissibility

Both facts are `may_claim_safe_to_rename: false`. Renaming any one of the three
declarations without the other two in the same edit does not compile — so a
rename proposal derived from a caller set at either seed is unsafe unless it
also carries the sibling declarations, which is not something the `callers`
relation reports.
