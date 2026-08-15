# A1-F v2 — Resolver semantics: evidence record

```text
NON-NORMATIVE EVIDENCE

Nothing in this file defines resolver states, classification precedence,
or consumer authority. If this file conflicts with the normative contract
in docs/tasks/a1-f-v2-resolver-semantics.md, the normative contract wins
and the discrepancy is a defect in this evidence record.
```

Every entry uses the same fixed shape. The **Observed** and **Inference** fields
are separated by rule: a statement may appear under *Observed* only if it is a
property of a recorded command's output, exit status or files. Statements about
what a program does internally belong under *Inference*, or nowhere.

Unless stated otherwise, every reproduction below was executed in a fresh
`git init` scratch repository, and the outputs shown are from that execution.

---

## E-1 — Environment anchor

**Question:** which git is every other entry talking about?

**Exact environment / tool version:**

```console
$ git --version
git version 2.43.0
```

**Observed:** the version string above.

**Inference:** none.

**Normative clause supported:** none directly; scopes every other entry.

**Does NOT establish:** anything about other git versions. Every entry below is a
statement about 2.43.0 and carries no claim of stability across releases.

---

## E-2 — `command` is a config scope, and the option that shows it

**Question:** is a `-c key=value` argument a configuration scope of its own, and
what command shows scopes?

**Reproduction and observed result:**

```console
$ git --show-scope
unknown option: --show-scope

$ git config --show-scope --list
global  user.name=Claude
global  user.email=noreply@anthropic.com
local   core.repositoryformatversion=0
local   core.filemode=true

$ git config -h | grep show-scope
    --[no-]show-scope     show scope of config (worktree, local, global, system, command)
```

**Observed:** `--show-scope` is rejected as a top-level git option; it is an option
of `git config`; the scope vocabulary it documents includes `command`.

**Inference:** a `-c key=value` argument is therefore not "the repository's
config", and a contract that grants one narrow `-c` key is not thereby reading
repository-controlled configuration.

**Normative clause supported:** §2.1 — "the repository cannot choose which program
runs or which transformation path is taken" is compatible with this layer passing
its own `command`-scope key.

**Does NOT establish:** that any particular `-c` grant is safe. Scope membership is
not a safety argument; it only refutes the claim that such a key is repository
configuration.

**Provenance of this entry:** the previous attempt named this check
`git --show-scope`, which does not exist. Corrected here.

---

## E-3 — A replace ref pointing an oid at itself is not a delivery route

**Question:** can `refs/replace` carry a prepared-collision substitution, given
that colliding objects share an oid?

**Reproduction and observed result:**

```console
$ O=$(printf 'SELF REPLACE TARGET' | git hash-object -w --stdin)
$ echo $O
5ff4ab6f610a2a1a97a21c7c8600348be921b48e

$ git replace $O $O          # exit 0 — the ref is created

$ git cat-file blob $O
fatal: replace depth too high for object 5ff4ab6f610a2a1a97a21c7c8600348be921b48e
                             # exit 128
```

**Observed:** the ref is created successfully; the subsequent lookup fails with
exit 128.

**Inference:** the prepared-collision case cannot be delivered through a replace
ref, because the mapping would have to be oid→itself. Its delivery channel is the
object file.

**Normative clause supported:** §5 — the prepared-collision case is reasoned about
as an object-store substitution, not a ref substitution.

**Does NOT establish:** that no other ref-based composition exists; only that this
one self-terminates.

---

## E-4 — A hash-path mismatch is served, not refused

**Question:** if an object file is overwritten with a *different* valid object,
does the lookup refuse it?

**Reproduction and observed result:**

```console
$ A=$(printf 'AUTHENTIC AUTHORITY' | git hash-object -w --stdin)   # 36ae691f…
$ D=$(printf 'SUBSTITUTED CONTENT' | git hash-object -w --stdin)   # 7feb0785…
$ cp .git/objects/7f/eb07…  .git/objects/36/ae69…

$ git cat-file blob $A
SUBSTITUTED CONTENT          # exit 0

$ git cat-file blob $A | git hash-object --stdin
7feb07853362884e770de9cde3265336be1697fb     # != the requested 36ae691f…

$ git fsck
error: 7feb0785…: hash-path mismatch, found at: .git/objects/36/ae691f…
```

**Observed:** the lookup completes with exit 0 and returns bytes whose recomputed
id differs from the requested one. A separate command, `fsck`, reports the
mismatch.

**Inference:** the mismatch is detectable by recomputation at the call site, and
this is the channel the identity postcondition exists for.

**Normative clause supported:** §3 `IDENTITY_VIOLATION`; witness W4.

**Does NOT establish:** *that git performs no internal validation.* The experiment
observes only that the lookup **did not reject** the object. `fsck` computing the
same mismatch shows that something in git can compute it; what was observed is a
non-rejection by this command, not an absence of validation anywhere.

---

## E-5 — Composition can make the substitution undetectable, and the route closes

**Question:** can a replace ref be composed with an object-file overwrite, and does
the route-closure variable stop it?

**Reproduction and observed result:**

```console
$ A=36ae691f…  "AUTHENTIC AUTHORITY"
$ C=bf637ab6…  "DECOY OBJECT"
$ D=7eef2c30…  "ATTACKER CONTENT VIA TWO HOPS"

$ git replace $A $C
$ cp <D's object file> <C's object file path>

$ git cat-file blob $A
ATTACKER CONTENT VIA TWO HOPS                       # exit 0

$ GIT_NO_REPLACE_OBJECTS=1 git cat-file blob $A
AUTHENTIC AUTHORITY                                 # exit 0
```

**Observed:** with the replace ref active, the lookup returns the attacker's bytes
and exits 0. With `GIT_NO_REPLACE_OBJECTS=1`, the same lookup returns the authentic
bytes.

**Inference:** the route is real and the environment variable closes it. Separately
— and this half was **not executed** — if `D` were chosen to collide with `A`, the
recomputed id would match and the postcondition would see nothing wrong. That step
requires SHA-1 collision material that was not available here; it is arithmetic
over the observed route, not an observation.

**Normative clause supported:** §2.1 (route closure), §5 (why composition does not
create a fourth state).

**Does NOT establish:** that a colliding `D` was ever produced, or that the
composition is undetectable in the general case. The three blobs used here do not
collide, and the substitution above **is** detectable by recomputation.

---

## E-6 — A damaged object misclassifies if the exit status is ignored

**Question:** what does a returncode-ignoring resolver conclude about a corrupt
object?

**Reproduction and observed result:** a 400-byte blob whose loose object file was
truncated from 23 to 11 bytes.

```console
$ git cat-file -t $B
error: header for 5106a63e… too long, exceeds 32 bytes
fatal: git cat-file: could not get object info      # exit 128

$ git cat-file blob $B > body.out                   # exit 128
$ stat -c%s body.out
0

$ sha1sum body.out
da39a3ee5e6b4b0d3255bfef95601890afd80709            # SHA-1 of the empty input
```

**Observed:** both commands fail with exit 128; the failed body command emitted
**zero** bytes; hashing that empty output yields an id unequal to the requested
one.

**Inference:** a resolver that hashes stdout without checking the exit status
reports `IDENTITY_VIOLATION` — "this checkout substitutes object contents" — for
what is actually an unreadable object. The error is fail-closed but **semantically
wrong**: it accuses the object store of substitution on evidence of corruption.

**Normative clause supported:** §2.2 (debris is never evidence); §6 (corruption is
not what `LOOKUP_UNOBTAINABLE` claims, and must not be reported as a substitution).

**Does NOT establish:** the partial-emission case — here stdout was empty, not
partial. See E-7, which was constructed because this entry did **not** demonstrate
what it was intended to.

---

## E-7 — The kind succeeds, the body streams megabytes, then fails

**Question:** can a failed body command emit a large amount of stdout first, so
that "hash whatever arrived" is not merely hashing an empty buffer?

**Reproduction and observed result:** a 10,400,000-byte blob whose 60,565-byte
loose object file was truncated to 36,339 bytes.

```console
$ git cat-file -t $B
blob                                    # exit 0   <-- the kind operation SUCCEEDS

$ git cat-file blob $B > body.out
fatal: unable to stream a394ec43… to stdout
                                        # exit 128 <-- the body operation FAILS
$ stat -c%s body.out
6225920                                 # 6.2 MB of stdout from the FAILED command

$ sha1("blob 6225920\0" + body.out)
4ba9f2dda3a7db27a6ab082e60c2aeee535d5681
$ requested
a394ec43d91e167d3dae803e62e1049e0b642694
```

**Observed:** the kind operation exits 0 and reports `blob`; the body operation
emits 6,225,920 bytes and then exits 128; the recomputed id over that output
differs from the requested id.

**Inference:** this is the exact shape witness W3 names. A resolver that checks the
first command's status but not the second's will hash 6.2 MB of debris and report
`IDENTITY_VIOLATION` where the correct state is `LOOKUP_UNOBTAINABLE`. The failure
is not hypothetical: the current `_local_object` in `tools/a1_v2_extract_graph.py`
on `main` checks the kind command's returncode and does not check the body's.

**Normative clause supported:** §2.2; §3 (`LOOKUP_UNOBTAINABLE`); witness W3.

**Does NOT establish:** that this is the only route to partial output, or anything
about how git buffers its output. The observation is the byte count and the exit
status, nothing more.

---

## E-8 — Repository configuration can transform the returned bytes, but only on request

**Question:** can a repository-configured program change the bytes a lookup
returns, and if so, what selects that path?

**Reproduction and observed result:** a repository whose `.gitattributes` binds
`f.txt` to a filter and a textconv driver, both configured to rewrite the content.

```console
$ git config filter.evil.smudge 'sed s/ORIGINAL/SMUDGED/'
$ git config diff.evil.textconv 'sed s/ORIGINAL/TEXTCONV/'
$ cat .gitattributes
f.txt filter=evil diff=evil

$ git cat-file blob 700c458a…                       # the contract's invocation
ORIGINAL CONTENT

$ git cat-file --filters  --path=f.txt 700c458a…
SMUDGED CONTENT

$ git cat-file --textconv --path=f.txt 700c458a…
SMUDGED CONTENT

$ rm f.txt && git checkout -- f.txt && cat f.txt
SMUDGED CONTENT

$ git cat-file -h
    --textconv            run textconv on object's content
    --filters             run filters on object's content
```

**Observed:** the plain `cat-file blob` invocation returned the stored bytes
unchanged. The same object returned rewritten bytes when `--filters` or
`--textconv` was passed. A checkout of the same path also produced rewritten bytes.
The two flags are documented in this version's own `-h` output.

**Inference:** the repository can supply the *program*, but it cannot supply the
*decision to run it* through this command — that decision is the caller's, made by
passing a flag. §2.1's raw-read prerequisite is therefore an obligation on this
layer's own invocation, not a property it must detect in the repository.

**Normative clause supported:** §2.1, raw read.

**Does NOT establish:** that these are the only transformation paths, or anything
about invocations other than `cat-file`. The checkout line is included only to show
the configured filter was genuinely active — a filter that never fired would make
the plain-invocation result meaningless.

---

## E-8.1 — Prerequisites with no witness in this record

Recorded so the remaining gap is visible rather than implied by silence.

| Prerequisite | Status of evidence here |
|---|---|
| counterfeit executable selected via `PATH` | **no reproduction recorded** |
| lazy / promisor fetch satisfying a lookup | **no reproduction recorded** |

Neither is demonstrated here, and no reader should treat its presence in §2.1 as
evidence-backed by this file. Producing both reproductions is outstanding work.

An earlier revision of this entry justified all three prerequisites as "documented
git capabilities" without naming the documentation or its exact property. That was
the same defect this record exists to prevent — an assertion outrunning its
evidence — and it is withdrawn. What replaces it is narrower and does not depend on
either mechanism being reachable:

> §2.1 states **exclusions**. An exclusion is justified by the cost of being wrong,
> not by a demonstration that the excluded thing is reachable. If a counterfeit
> executable or a lazy fetch turns out to be unreachable in some environment, the
> corresponding requirement is **redundant there** — never incorrect. The
> reproductions are owed because a contract should know which of its clauses are
> load-bearing, not because the clauses fail without them.

---

## E-9 — Cryptographic caveats (literature, not observation)

**Question:** does the contract depend on SHA-1 properties?

**Observed:** nothing. No cryptographic experiment was run.

**Inference:** SHA-1 collision resistance is broken in practice — identical-prefix
(2017) and chosen-prefix (2020) attacks are published. Second-preimage resistance
has no published practical attack. These are statements from the literature, and
this record cites no source for them.

**Normative clause supported:** none. §4 was written so that no state depends on
either property: `RESOLVED` claims only local acquisition and id equality, both of
which remain true observations regardless.

**Does NOT establish:** the current status of either property. This entry exists to
record that the contract deliberately does **not** rest on it, not to assert
cryptographic facts on this document's authority.

---

## E-10 — Disposition of the terminated attempt's findings

Recorded so that a reviewer can ask the sharpest available question: *name a
substantial finding from the rejected attempt that this core either contradicts or
leaves without a defined state and downstream action.*

| Finding family | Disposition |
|---|---|
| failed-body debris hashed as evidence | core — §2.2, W3 |
| counterfeit executable | core — §2.1 provenance, W7; no witness (E-8.1) |
| lazy / network acquisition | core — §2.1 provenance; no witness (E-8.1) |
| smudge / textconv / filter transformation | core — §2.1 raw read; witness E-8 |
| `refs/replace` yielding a different id | core — §3 `IDENTITY_VIOLATION`, W4 |
| `refs/replace` composed to yield a matching id | core — §5, W8 |
| prepared collision | core — §5 `RESOLVED` + §4 non-claim, W8 |
| hash-strength premises (collision / second preimage) | core — §4 removes the dependency entirely; the caveats are evidence-only |
| mechanism-vs-effect misclassification | core — §3 partitions by outcome; no mechanism vocabulary exists here to misclassify with |
| over-strong claims about git internals | evidence appendix only, under its mandatory *Observed / Inference* split |
| `git --show-scope` naming error | evidence appendix, corrected to `git config --show-scope` |
| wrong object kind treated as resolver failure | core — §8 |
| `NOT_PRESENT_LOCALLY` frozen without an oracle | core — §10 refuses it |
| diagnostic text used as a state discriminator | core — §6, W6 |

This table is a coverage argument, like everything else in this file. An error in
it is an error about the previous attempt or about coverage — never a change to
the contract, which the header of this file subordinates the whole record to.
