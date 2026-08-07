# case-0006 — generated member with a non-code source of truth

```text
contracts/api.yaml        operations[].id: fetchInvoice   <- the real source
generated/api-client.ts   ApiClient.fetchInvoice()        <- @generated, DO NOT EDIT
app/usage.ts              loadInvoice() calls it          <- ordinary caller
tools/codegen.py          renders the generated file from the contract
```

## Why an easy case is in an adversarial corpus

Every code edge here is trivial. The language service resolves the single caller
without effort, and any tool will get it right.

The trap is one layer up. A rename driven by that correct caller set edits the
generated file, compiles, passes review, and is reverted by the next codegen
run. Correctness of the relation and safety of the action derived from it come
apart completely — which is the distinction the whole harness is built around,
appearing here in its cleanest form.

## The instability is executable, not narrated

`tools/codegen.py` is a deterministic renderer: contract in, generated file out.
`--check` compares the rendering against the committed file.

```console
$ sed -i 's/fetchInvoice/loadInvoiceById/' source/generated/api-client.ts
$ tools/codegen.py --check
  codegen: DRIFT -- the committed generated file is not what the
  contract generates. ...
    -  fetchInvoice(id: string): Promise<string> {
    +  loadInvoiceById(id: string): Promise<string> {
  exit=1
```

That is precisely the action `may_claim_safe_to_rename: false` forbids, and the
fixture rejects it mechanically. An earlier revision of this case asserted the
same thing in prose with no generator present; the claim was then an
interpretation of a comment rather than a property of the fixture.

## The second fact is a capability probe

`references` expects the contract file. Recovering it requires non-code
artifacts to be graph nodes, not opaque blobs. A code-only graph **should** miss
this edge, and missing it is scored as an expected limitation, not a defect.

What is scored as a defect is missing it *while claiming the rename set is
complete*. The probe measures the boundary of the tool's world model and whether
the tool knows where that boundary is.

The independent TypeScript oracle reports this expectation as out of reach
rather than silently dropping it — a non-`.ts` path is something that oracle
cannot speak to, and saying so is the point.
