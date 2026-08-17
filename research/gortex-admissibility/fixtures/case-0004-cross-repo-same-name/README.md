# case-0004 — identically named symbol in a second repository

```text
repo-a/src/config.ts   class Config { static load() }   <- seed
repo-a/src/boot.ts     bootA() calls Config.load()      <- the only true caller
repo-b/src/config.ts   class Config { static load() }   <- unrelated
repo-b/src/boot.ts     bootB() calls Config.load()      <- must NOT appear
```

The two trees are byte-identical apart from names in a comment and a literal.
Only the repository root separates the symbols.

## Failure mode

Over-reach, not omission. A workspace-wide index keyed on qualified name merges
`repo-a::Config.load` with `repo-b::Config.load`, and the caller set for either
silently gains the other's callers. Downstream, blast radius doubles and a
rename proposes edits in a repository that never referenced the symbol.

This is the multi-repo analogue of case-0001: same shortcut, larger scope, and
harder to spot by eye because the extra edge is in a file the reviewer is not
looking at.

## Running the independent oracle

Run the language service **once per repository root**. A single project spanning
both trees reproduces the conflation under test and would certify it as correct.
