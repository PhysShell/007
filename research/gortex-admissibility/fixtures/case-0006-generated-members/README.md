# case-0006 — generated member with a non-code source of truth

```text
contracts/api.yaml        operations[].id: fetchInvoice   <- the real source
generated/api-client.ts   ApiClient.fetchInvoice()        <- @generated, DO NOT EDIT
app/usage.ts              loadInvoice() calls it          <- ordinary caller
```

## Why an easy case is in an adversarial corpus

Every code edge here is trivial. The language service resolves the single caller
without effort, and any tool will get it right.

The trap is one layer up. A rename driven by that correct caller set edits the
generated file, compiles, passes review, and is reverted by the next codegen
run. Correctness of the relation and safety of the action derived from it come
apart completely — which is the distinction the whole harness is built around,
appearing here in its cleanest form.

## The second fact is a capability probe

`references` expects the contract file. Recovering it requires non-code
artifacts to be graph nodes, not opaque blobs. A code-only graph **should** miss
this edge, and missing it is scored as an expected limitation, not a defect.

What is scored as a defect is missing it *while claiming the rename set is
complete*. The probe measures the boundary of the tool's world model and whether
the tool knows where that boundary is.
