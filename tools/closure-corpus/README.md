# Closure regression corpus (preserved from PR #145)

Issue **#147, Step 0**. The #145 review session built these repository shapes in
scratch storage while an independent reviewer repeatedly demonstrated false
PASSes in a Markdown-embedded Git-history check. The prose descriptions survive
in `docs/tasks/a1-resolver-semantics-evidence.md` at `ed7969c`; the executable
fixtures did not. `build-corpus.sh` reconstructs them.

## What this is not

It is **not** evidence that #145 is clean, and it must not be used as such —
see the sequencing invariant in #147. #145's terminal report at `ed7969c`
records that no clean reviewer verdict was obtained or requested at that head,
that the embedded checker is **not** a closure oracle, and that O-1/O-2 plus the
prior owed items remain open.

It is also not a test suite for the *new* tool. It is a record of how the *old*
approach failed. The intended use is to show that these inputs sit **outside**
the new design's authority path — because GitHub-owned observations come from
the GitHub API rather than from local Git reconstruction — instead of being
answered by yet another neutralizer.

## Running it

```sh
./build-corpus.sh                        # defaults to file:///home/user/007 @ ed7969c
./build-corpus.sh --repo <url> --sha <40-hex> --work <dir> [--keep]
```

Exit 0 iff every case matches its recorded expectation. One fixture exists at a
time; peak disk is roughly two clones.

The subject under test is **extracted from the frozen record**, never retyped —
`sha256(block) = bb31eb26728e82ad…` at `ed7969c`. A hand-retyped copy of this
block once passed while the shipped one could not run at all, which is why
extraction is part of the harness rather than a convenience.

## Cases

| # | Case | Class | Origin |
|---|------|-------|--------|
| 1 | baseline (repo of record) | GUARD HOLDS | — |
| 2 | invocation from a subdirectory | GUARD HOLDS | relative pathspec matched nothing; printed NONE FOUND |
| 3 | hostile `PATH` (`git`/`env`/`grep` shims) | GUARD HOLDS | `grep` and `env` resolved through the caller's `PATH` |
| 4 | hostile `GIT_DIR` + `GIT_WORK_TREE` | GUARD HOLDS | unsanitised bootstrap selected a foreign checkout |
| 5 | `refs/replace` substitution | GUARD HOLDS | `env -i` clears the environment; replace refs live in the repository |
| 6 | legacy `.git/info/grafts` | GUARD HOLDS | `GIT_NO_REPLACE_OBJECTS` does not disable grafts |
| 7 | shallow boundary, base injected | GUARD HOLDS | base object *present* is not base *reachable* |
| 8 | shallow boundary inside range | GUARD HOLDS | needs a merge shape; on a linear range truncation also severs ancestry |
| 9 | simplified walk hides a side commit | GUARD HOLDS | path-limited `log` simplifies history by default |
| 10 | evil merge (contract vs all parents) | GUARD HOLDS | `--no-merges` cannot see it; `diff-tree` on a merge prints nothing |
| 11 | mixed merge (contract from A, record from B) | GUARD HOLDS | the two halves of a conjunction answered by different parents |
| 12 | ordinary co-versioned merge | **NEGATIVE CONTROL** | must stay silent |
| 13 | repo-local config spawns programs | GUARD HOLDS | `log.showSignature`+`gpg.program`, `core.fsmonitor` |
| 14 | repo-local helper never executed | GUARD HOLDS | marker-file assertion for case 13 |
| 15 | O-1 fixture | **NOT DISCRIMINATING** | see below |

Case 12 is not decoration. The mixed-merge defect (case 11) survived a round
precisely because the positive witnesses passed and nothing exercised a shape
where the predicate's halves disagree.

## Status

```text
13 Git-level scenarios reproducible          (14 assertions; case 13 carries
                                              a second, marker-based one)
 1 preserved but NON-DISCRIMINATING          O-1
 3 API-level scenarios deliberately deferred to the classifier's own fixtures
```

O-1's honest status is **RECIPE PRESERVED / DISCRIMINATING WITNESS OWED** —
neither PASS nor FAIL. The script exits 0 because every case matches its
*recorded* expectation, and O-1's recorded expectation says the shape does not
reach the mechanism. That is not the same as fifteen green checks, and this
file should not be summarised as such.

This is Step **0A** of #147. Step **0B** — the API-level fixtures — is still
owed. The split is not bookkeeping: it marks the boundary of the future tool.
Git-level fixtures exist to show *why closure authority must not be built on
local Git*; API-level fixtures test *the classifier itself*, and should run
with neither Git nor network.

## Two rules this corpus earned the hard way

**A negative regression fixture must discriminate the mechanism it claims to
test.** Ideally three points, not one:

```text
control, defect absent      -> expected result
defective mechanism active  -> wrong result
repaired mechanism          -> expected result
```

O-1 currently has only the first and third. Case 12 (ordinary co-versioned
merge) is the counterexample that makes case 11 meaningful, and case 11's defect
survived a round precisely because nothing exercised a shape where the
predicate's two halves disagree.

**Every adversarial fixture must carry a positive control proving the hostile
condition actually exists before the asserted result is evaluated.** Two
failures while writing this script are the reason:

| Construction failure | Consequence |
|---|---|
| remote-tracking refs are not copied by `clone` | fixtures silently lacked the intended subject |
| `--depth` is silently ignored for local-path clones | the "shallow" fixture was a complete repository |

Both are one class:

```text
fixture construction fails -> test still runs -> hostile condition absent -> VACUOUS PASS
```

The second is the more dangerous, because it produced a green result for a
scenario that did not exist. Any harness for #147 should assert the hostile
precondition (`rev-parse --is-shallow-repository`, the presence of the replace
ref, the marker file, …) *before* evaluating the property under test.

## Known gaps in this corpus

Stated because a corpus that hides its own holes is the failure it exists to
catch.

- **O-1 has no discriminating fixture.** Removing the `docs/tasks` tree object
  makes the block exit 2 — the *correct* outcome, reached for the wrong reason:
  the history enumeration fails before `blob()` is consulted, so the mechanism
  under test never runs. A real discriminator must break only a tree that the
  **merge comparison** consults — reachable from a merge parent, and from no
  commit the contract-limited enumeration diff-trees. Open work for #147. O-1
  itself is reproducible at the command level: with that tree removed,
  `rev-parse HEAD:<contract>` exits 1 while `rev-parse 'HEAD^{tree}'` exits 0,
  which is why root-tree readability is the wrong discriminator.
- **O-2 has no external fixture at all.** It is a shape defect in the block: two
  command substitutions convert a failed enumeration into an empty list — the
  shallow-boundary `range` assignment, whose `rev-list` status is masked by a
  following `printf`, and the merge loop's inline parent enumeration. It is
  found by reading for `$( )` whose status is neither tested nor propagated, not
  by constructing a repository.
- **Three cases from #147's Step 0 list are not representable here**, because
  they are GitHub-API-level rather than Git-level: a stale reviewer artifact
  bound to the wrong SHA; a non-verdict surface carrying a real falsification
  claim; and same-vendor, same-SHA conflicting surfaces where only the submitted
  commit-bound review object counts. Those belong to the new classifier's own
  tests against recorded API fixtures.

## Companion file

`check.py` is the fourteen-property mechanical checker as it stood at `ed7969c`.
The evidence record refers to it throughout but never contained it, so it would
otherwise have been lost with the session. It carries the same standing as the
embedded block: a mechanical aid, **not** a completeness oracle, with the same
open defects.
