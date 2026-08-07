# case-0007 — case-0001 mirrored into a language that settles less

Semantically identical to [case-0001](../case-0001-sibling-same-name/): two
unrelated `send` methods, call sites in one file, receiver as the only
discriminator. Written in Lua, where the receiver of a parameter-passed call is
not statically determined.

## Why the pair is the measurement

The two cases have the same shape and **different correct answers about what may
be claimed**:

| | case-0001 (TypeScript) | case-0007 (Lua) |
|---|---|---|
| receiver type at the call site | declared | unknown for parameters |
| `may_claim_complete` | true | false |
| `may_claim_safe_to_rename` | true | false |
| phantom edges definable | yes | no — nothing is excludable |

A tool that reports the same confidence for both has not connected what the
source settles to what it is willing to assert. That connection — not the edge
sets — is what surface S3 exists to measure, and this pair is the cheapest
instrument for it.

## The internal control

`dispatch_new` constructs its receiver in scope:

```lua
local invoice = Invoice.new(id)
return invoice:send()
```

Local dataflow settles this one, in the same file and the same language. So the
case does not merely say "Lua is hard" — it distinguishes a tool that gives up
per language from one that resolves what is resolvable and caveats the rest.

## Note on `forbidden`

Empty, deliberately. In this file no static ground excludes any edge, so no
returned edge can be scored as phantom. All scoring weight is on the
admissibility block. This is the one case in the package where a tool cannot
fail on correctness at all — only on honesty.

## Tier

Not asserted here. Which extractor tier Lua receives is a property of the tool
and its configuration at measurement time; it is recorded in the result, never
in the oracle.
