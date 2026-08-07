# case-0101 — index lifecycle: what happens to evidence already handed out

Package 2, and the only case in it. One entity, five states, four questions per
state.

```text
entity      RateTable.lookup           (repo-main/src/core/rates.ts)
observed    callers = { quote }        taken at s0, before any mutation
```

Every later state is scored against **that observation**, not against a
re-query. Package 1 asks what a graph knows. This asks what happens to what it
already said.

## States

| State | Mutation | World |
|---|---|---|
| `s0-fresh` | — | committed tree; the observation is taken here |
| `s1-file-modified` | `quote.ts` gains a second caller | entity intact, old answer now **short** |
| `s2-symbol-deleted` | `lookup` and its call site removed | entity gone, tree valid |
| `s3a-file-removed` | `core/rates.ts` deleted | entity gone, import left dangling |
| `s3b-repo-removed` | `repo-main/` deleted | repository gone |

`s3a` and `s3b` are the same state — *the file or repository disappears* — at
two granularities. Most indexers take a different code path for a lost
repository than for a lost file, and a test that only removes a file cannot tell
whether disappearance is handled or whether one particular path happens to work.

## The four questions

```text
Q1  graph_current_correct     does a fresh query return the right answer now
Q2  stale_signal_required     is the holder of the old answer told it expired
Q3  old_observation_valid     does the fact handed out still hold
Q4  action_admissible         may an action derived from the old fact proceed
```

They are scored separately because **passing Q1 is routinely mistaken for
passing the rest**. A watcher that rebuilds the graph in fifty milliseconds
passes Q1 at every state here and can still fail Q2, Q3 and Q4 at all of them.
An index that is instantly correct about the present and silent about what it
said in the past is not fresh — it is racing, and the consumer is the one who
loses.

`s1` is the sharpest state for this. The old observation is not contradicted
there, only **short**: `quote` is still a caller, so nothing about the cached
answer looks wrong. A rename driven by it edits one of two sites. Nothing in the
data protests.

## Running the states

```console
$ tools/mutate.py list
$ tools/mutate.py apply s1-file-modified
$ tools/mutate.py reset
$ tools/mutate.py selfcheck     # apply each, assert postcondition, reset
```

States are **absolute**, not incremental: each is defined against the committed
baseline, so applying `s2` yields the same tree whether or not `s1` ran first. A
run may measure states in any order, repeat one, or resume after a crash. The
transition is still exercised — the runner observes at `s0`, then applies a
later state.

The first `apply` snapshots `source/` into `.baseline/` (git-ignored); `reset`
restores from it. No git dependency, so the corpus survives being vendored
somewhere else.

## Why scripted mutation and not a snapshot pair

A before/after pair of directories compares two pictures of the world. It cannot
show whether a tool *noticed the transition*, which is the only thing this case
is for. It also cannot express `s3b` at all: a snapshot of a repository that no
longer exists is just an absent directory, indistinguishable from one that was
never indexed.

## Scope

This case is deliberately the whole of Package 2. No symlinks, no `git reset`,
no concurrent-watcher matrix, no network filesystems. If the first real
measurement run surfaces a defect that needs a new fixture to characterise, that
fixture gets added then — on evidence, not in anticipation.
