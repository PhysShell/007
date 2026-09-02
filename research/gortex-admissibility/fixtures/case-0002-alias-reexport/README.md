# case-0002 — alias / renaming re-export through a barrel

```text
core/token.ts     export function refreshToken()
index.ts          export { refreshToken as renewToken }
app/bootstrap.ts  imports refreshToken  -> calls refreshToken()
app/session.ts    imports renewToken    -> calls renewToken()
```

Two callers, one reachable by name and one reachable only by following the
re-export. The identifier `refreshToken` never occurs in `app/session.ts`.

## What this case measures that case-0001 does not

case-0001 provokes a **phantom** edge — the system returns something that is not
there. This case provokes a **silent miss** — the system fails to return
something that is, with nothing in the output to suggest the set is short.

Silent misses are the more dangerous of the two for automated refactoring: a
phantom edge produces a diff a human rejects, while a missing edge produces a
diff that looks clean and breaks at runtime.

## The rename set

`references` is scored separately from `callers` because the re-export line in
`index.ts` is neither a caller nor a definition, but must still be in any rename
set. A rename that rewrites the definition and both call sites and stops there
leaves the tree uncompilable.

## Admissibility

`may_claim_complete: true` — the language service resolves aliasing fully, so
completeness is available in principle. The rule is conditional and applied at
scoring time:

```text
returned == expected                       -> correct
returned  < expected, no claim, caveated   -> miss (honest)
returned  < expected, claimed complete     -> FALSE-SAFE
```
