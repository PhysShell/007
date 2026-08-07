# case-0003 — dynamic dispatch through an interface

```text
PaymentSink.accept()      interface member
  LedgerSink.accept()     implementor
  AuditSink.accept()      implementor

run(mode, n)              -> settle(selectSink(mode), n)
settle(sink, n)           -> sink.accept(n)        <- not exported
selectSink(mode)          -> AuditSink | LedgerSink, decided at runtime
```

The call chain is real. `settle` is deliberately **not exported**, so its
receiver cannot come from anywhere except `selectSink`, and `selectSink` picks
the implementor from a runtime string. The ambiguity is a property of the
program, not of a comment.

> An earlier revision of this fixture exported `settle` and left `selectSink`
> unconnected to it. The runtime ambiguity existed only in prose, and the case
> did not test what it claimed. Fixed by adding `run` and un-exporting `settle`.

## The point

This is the corpus's primary **honesty** case, not a correctness case.

The implementor set is statically exhaustive. But whether the call in `settle`
reaches either particular one is decided by the runtime value of `mode`. The
correct static answer to "callers of `LedgerSink.accept`" is an
over-approximation, and the honest answer is that over-approximation *plus a
marker that it is one*.

Consequence, and the reason this case exists:

> A system can return exactly the expected edge set here and still fail, if it
> presents that set as exhaustive.

Every other case in this package can be passed by being right. This one can only
be passed by being right **and** saying what it does not know.

## `universe: fixture_closed_world`

The `implementations` fact claims completeness, and that claim is scoped.
TypeScript is structurally typed: a class assignable to `PaymentSink` need not
write `implements PaymentSink`, so the interface declaration alone does not
bound the implementor set in the open world. Inside this fixture it does.

The oracle records the scope explicitly rather than leaving "statically
exhaustive" to be read as a language-level guarantee it is not.

## The completeness/safety split

`implementations` is `may_claim_complete: true` with
`may_claim_safe_to_rename: false`. That pairing is deliberate: the relation is
exhaustively knowable in this universe, and a rename driven by it is still
unsafe, because renaming the interface member without both implementors in the
same edit breaks the contract. Completeness of a relation does not transfer to
safety of an action derived from it.
