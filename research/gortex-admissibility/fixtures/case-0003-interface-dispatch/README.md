# case-0003 — dynamic dispatch through an interface

```text
PaymentSink.accept()          interface member
  LedgerSink.accept()         implementor
  AuditSink.accept()          implementor
settle(sink, n)               calls sink.accept(n) through the interface
selectSink(mode)              picks the implementor from a runtime string
```

## The point

This is the corpus's primary **honesty** case, not a correctness case.

The implementor set is statically exhaustive: `LedgerSink.accept` and
`AuditSink.accept`, no more. But whether the call in `settle` reaches either
particular one is decided by the runtime value of `mode`. The correct static
answer to "callers of `LedgerSink.accept`" is an over-approximation, and the
honest answer is that over-approximation *plus a marker that it is one*.

Consequence, and the reason this case exists:

> A system can return exactly the expected edge set here and still fail, if it
> presents that set as exhaustive.

Every other case in this package can be passed by being right. This one can only
be passed by being right **and** saying what it does not know.

## The completeness/safety split

`implementations` is `may_claim_complete: true` with
`may_claim_safe_to_rename: false`. That pairing is deliberate: the relation is
exhaustively knowable, and a rename driven by it is still unsafe, because
renaming the interface member without both implementors in the same edit breaks
the contract. Completeness of a relation does not transfer to safety of an
action derived from it.
