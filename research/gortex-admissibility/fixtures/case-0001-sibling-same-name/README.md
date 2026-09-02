# case-0001 — same-named methods on sibling classes

**Control vertical.** The cheapest case in the corpus that separates real
receiver-typed resolution from name matching.

```text
Invoice.send()      <- called only by dispatchInvoice
MailMessage.send()  <- called only by dispatchMail
```

Both call sites live in `source/app/dispatch.ts`, so a resolver cannot separate
them by file, module, or import graph. The only discriminator is the declared
type of the receiver expression.

## Why this case first

It is a minimal reproduction of the defect class behind upstream Gortex issue
[#461](https://github.com/zzet/gortex/issues/461) — call edges resolved by
method name, receiver ignored — *without* depending on GDScript, on that issue,
or on whether its fix landed. The same shortcut in any language produces the
same two symptoms here: a phantom edge and an over-large caller set.

## What each outcome means

| Returned caller set for `Invoice.send` | Reading |
|---|---|
| `dispatchInvoice` only | correct |
| `dispatchInvoice` + `dispatchMail` | name-only resolution; phantom edge |
| `dispatchMail` only | receiver bound to the wrong declaration |
| empty | the seed was not resolved at all — check the seed binding before scoring |

The last row is why `seeds[].gold_identity` exists: an unresolved or
mis-resolved seed must be caught and recorded as a localization failure (S1),
not scored as a fidelity result (S2).

## Admissibility

Nothing here is genuinely undecidable, so `may_claim_complete` and
`may_claim_safe_to_rename` are both **true**. That makes the case a clean
false-safe detector: a wrong caller set delivered without caveats cannot be
excused by ambiguity in the source.
