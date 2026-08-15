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

**Every git invocation in this file runs under the §2.1 control set.** It is
defined once, here, in full, and used as `CTRL` in every block below — so no
command's provenance depends on the shell that happened to run it:

```console
CTRL() {
  env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
         GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 \
         GIT_CONFIG_GLOBAL=/dev/null "$@"
}
```

This is uniform on purpose. Three separate findings on this PR were "that
particular command was not controlled", each about a different command, because
deciding case by case which invocations matter is a judgement that kept failing.
An inherited variable can redirect *any* invocation — demonstrated, not assumed:

```console
$ GIT_DIR=<sha1-repo>/.git /usr/bin/git -C <sha256-repo> rev-parse --show-object-format
sha1                          # -C names the sha256 repo; GIT_DIR wins
```

Two entries deliberately run a **variant** of the control set, to exhibit a route
that the full set closes. Each defines its variant in full at the point of use and
says why.

---

## E-1 — Environment anchor

**Question:** which git is every other entry talking about?

**Exact environment / tool version:**

```console
$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git --version
git version 2.43.0
```

**Observed:** the executable at the absolute path `/usr/bin/git` reports `2.43.0`.

**Inference:** none is required. **Every reproduction in this file invokes
`/usr/bin/git` by absolute path** — no entry resolves `git` through `PATH` — so
this anchor applies to each of them directly, by naming the same file, rather than
through any argument about what `PATH` resolves to.

**Normative clause supported:** none directly; scopes every other entry.

**Does NOT establish:** anything about other git versions, or about any executable
other than the one at that path at the time of running.

**Provenance of this entry.** It has been wrong twice, in instructive ways.

1. The first revision recorded `git --version` through `PATH` while the controlled
   entries invoked `/usr/bin/git` — so the anchor did not describe the executable
   under test at all.
2. The repair recorded a `command -v` and device:inode comparison and concluded the
   anchor therefore covered the entries that still used bare `git`. **That was
   overreach**: the inode check is a fact about the shell E-1 ran in, and says
   nothing about the shell E-2, E-3, E-5, E-6 or E-8 ran in. Those entries could
   have resolved `git` elsewhere with every recorded observation still true.

The second repair failed because it made the anchor's reach an *argument* instead
of removing the dependency. This revision removes it: every entry names the
absolute path, so there is no resolution step left to reason about.

---

## E-2 — `command` is a config scope, and the option that shows it

**Question:** is a `-c key=value` argument a configuration scope of its own, and
what command shows scopes?

**Reproduction and observed result:**

```console
$ CTRL /usr/bin/git --show-scope
unknown option: --show-scope

$ CTRL /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" config --show-scope --list
local     core.repositoryformatversion=0
local     core.filemode=true
local     core.bare=false
local     core.logallrefupdates=true
command   safe.directory=/…/x2

$ CTRL /usr/bin/git config -h | grep show-scope
    --[no-]show-scope     show scope of config (worktree, local, global, system, command)

$ CTRL /usr/bin/git -c example.key=value config --show-scope --get example.key
command value

$ CTRL /usr/bin/git config example.local fromrepo          # set IN the repository
$ CTRL /usr/bin/git config --show-scope --get example.local
local   fromrepo

$ CTRL /usr/bin/git -c example.key=value config --show-scope --list | grep example
local     example.local=fromrepo
command   example.key=value
```

**Observed:** `--show-scope` is rejected as a top-level git option; it is an option
of `git config`; a key passed with `-c` is reported under scope **`command`**,
while a key written into the repository is reported under scope **`local`**, and
the two appear as distinct scopes in the same listing.

**Inference:** a `-c key=value` argument is therefore not "the repository's
config", and a contract that grants one narrow `-c` key is not thereby reading
repository-controlled configuration.

**Provenance of this observation:** an earlier revision of this entry recorded only
the `-h` line enumerating scope *names* and inferred the `-c` behaviour from it.
Enumerating a scope in a help string is not a demonstration that `-c` produces it —
the same gap this record's Observed/Inference rule exists to catch. The two
commands above were added because that inference was not licensed by what had been
run.

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

This entry needs replace refs **active** to characterise the mechanism, so the
lookup arm runs a named variant of the control set with `GIT_NO_REPLACE_OBJECTS`
omitted. The variant is defined here in full:

```console
NOREPL() {
  env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
         GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null "$@"
}
```

```console
$ CTRL /usr/bin/git init -q y3 && cd y3
$ O=$(printf 'SELF REPLACE TARGET' \
      | CTRL /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" hash-object -w --stdin)
$ echo $O
5ff4ab6f610a2a1a97a21c7c8600348be921b48e

$ NOREPL /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" replace "$O" "$O"
                             # exit 0 — the ref is created

# --- replace ACTIVE: the self-map is what fails ---
$ NOREPL /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" cat-file blob "$O"
fatal: replace depth too high for object 5ff4ab6f610a2a1a97a21c7c8600348be921b48e
                             # exit 128

# --- the FULL control set, for contrast: the route is closed outright ---
$ CTRL /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" cat-file blob "$O"
SELF REPLACE TARGET          # exit 0 — the replace ref is never consulted
```

**Observed:** the ref is created successfully. With replace refs active the lookup
fails with exit 128. Under the **full** §2.1 control set the same lookup returns
the original bytes at exit 0, because `GIT_NO_REPLACE_OBJECTS=1` stops the ref
being consulted at all.

**Why both arms are recorded.** An earlier revision showed only the first, run
without the control set, while the entry sat in a file whose other reproductions
were controlled. That is a reader trap: the recorded failure **cannot occur** under
the contract's own prerequisites. The mechanism is still worth characterising —
which is why the variant is used deliberately and named — but the contract-relevant
fact is the second arm.

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

**Reproduction and observed result:** W4 requires **`§2 holds`**, so this is run
under the same §2.1 controls as E-7 — absolute executable path, and a child
environment built with `env -i` carrying only `PATH=/usr/bin`,
`GIT_TERMINAL_PROMPT=0`, `GIT_NO_LAZY_FETCH=1`, `GIT_NO_REPLACE_OBJECTS=1`,
`GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`. Neither `--filters` nor
`--textconv` is passed.

```console
# --- construction, from an empty repository ---
$ CTRL /usr/bin/git init -q r4 && cd r4
$ A=$(printf 'AUTHENTIC AUTHORITY' | CTRL /usr/bin/git hash-object -w --stdin); echo $A
36ae691f4261ce7008c7bfc827233e74dc3fc96e
$ D=$(printf 'SUBSTITUTED CONTENT' | CTRL /usr/bin/git hash-object -w --stdin); echo $D
7feb07853362884e770de9cde3265336be1697fb
$ stat -c%a ".git/objects/${A:0:2}/${A:2}"   # loose objects are READ-ONLY
444
$ rm -f ".git/objects/${A:0:2}/${A:2}"       # required: cp alone cannot
$ cp  ".git/objects/${D:0:2}/${D:2}" \
      ".git/objects/${A:0:2}/${A:2}"         # overwrite a 0444 file

# --- the lookup: the env -i prefix is ON each command, not a separate step ---
$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" cat-file -t $A
blob                         # exit 0   <-- kind operation succeeds

$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" cat-file blob $A > body.out
                             # exit 0   <-- body succeeds: COMPLETE response,
$ cat body.out               #              so §2.2 holds too
SUBSTITUTED CONTENT

$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git hash-object --stdin < body.out
7feb07853362884e770de9cde3265336be1697fb     # != the requested 36ae691f…

$ python3 -c "                               # SECOND, INDEPENDENT implementation
import hashlib, sys                          # — no git involved at all
b = open('body.out','rb').read()
print(hashlib.sha1(b'blob %d\\0' % len(b) + b).hexdigest())"
7feb07853362884e770de9cde3265336be1697fb     # agrees

$ CTRL /usr/bin/git fsck
error: 7feb0785…: hash-path mismatch, found at: .git/objects/36/ae691f…
```

**The prefix constrains the child, and that is shown rather than assumed.** With a
variable deliberately present in the parent shell, the same prefix yields a child
that cannot see it:

```console
$ export SNEAKY=leaked
$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/env | sort
GIT_CONFIG_GLOBAL=/dev/null
GIT_CONFIG_NOSYSTEM=1
GIT_NO_LAZY_FETCH=1
GIT_NO_REPLACE_OBJECTS=1
GIT_TERMINAL_PROMPT=0
PATH=/usr/bin
$ echo $SNEAKY               # still set in the PARENT
leaked
```

**Observed:** under that invocation both operations exit 0, the lookup returns
bytes whose recomputed id differs from the requested one, and a separate command,
`fsck`, reports the mismatch.

**Provenance of the `rm` step, and why it was missing.** Earlier revisions of this
entry and of E-5 recorded a bare `cp` over the object file. Git writes loose
objects mode `0444`, so that command **fails with `Permission denied` for an
ordinary user**. It succeeded when recorded only because this environment runs as
**root**, and root ignores the write bit.

That is a reproducibility defect of a kind the earlier mechanical sweep could not
see: the command was syntactically complete, used the absolute executable, carried
no placeholder, and *worked when run* — it simply worked for a reason that does not
generalise to the reader. A privilege the recorder happens to hold is exactly the
sort of premise that never gets written down, because it never announces itself.
The re-run with `rm -f` reproduces every recorded value unchanged, so the defect
was in the instructions, never in the result.

**Provenance of these controls:** as with E-7, the first revision of this entry ran
plain `git` from `PATH` with an inherited environment while citing witness W4 —
which requires `§2 holds`. That gap was **not reported by any reviewer**; it was
found by sweeping every entry citing a witness after the same defect was reported
against E-7. Re-running under the controls reproduces the result unchanged, so the
entry's conclusion stands and only its standing as a W4 witness was ever in doubt.

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
# --- construction, from an empty repository ---
$ CTRL /usr/bin/git init -q w5 && cd w5
$ A=$(printf 'AUTHENTIC AUTHORITY'           | CTRL /usr/bin/git hash-object -w --stdin)
$ C=$(printf 'DECOY OBJECT'                  | CTRL /usr/bin/git hash-object -w --stdin)
$ D=$(printf 'ATTACKER CONTENT VIA TWO HOPS' | CTRL /usr/bin/git hash-object -w --stdin)
$ printf '%s\n%s\n%s\n' "$A" "$C" "$D"
36ae691f4261ce7008c7bfc827233e74dc3fc96e
bf637ab698f8fbfd8346edb378c904d2bb6f4064
7eef2c30522157ac6ca7771c2f0172a42cbd57f4

$ CTRL /usr/bin/git replace "$A" "$C"                     # exit 0 — hop 1: A -> C
$ rm -f ".git/objects/${C:0:2}/${C:2}"               # loose objects are 0444;
$ cp  ".git/objects/${D:0:2}/${D:2}" \
      ".git/objects/${C:0:2}/${C:2}"                 # hop 2: C's path holds D

# --- route OPEN: every control EXCEPT GIT_NO_REPLACE_OBJECTS ---
$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" cat-file blob "$A"
ATTACKER CONTENT VIA TWO HOPS                        # exit 0

# --- route CLOSED: the same command with the full §2.1 control set ---
$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" cat-file blob "$A"
AUTHENTIC AUTHORITY                                  # exit 0
```

The two lookups differ in **exactly one** environment name, which is what makes
this a controlled comparison rather than two separate observations.

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
# --- construction, from an empty repository ---
$ CTRL /usr/bin/git init -q r6 && cd r6
$ B=$(python3 -c "print('X'*400)" | CTRL /usr/bin/git hash-object -w --stdin); echo $B
5106a63ecf7df82b1b45855bf64ba14d90f978ca
$ p=".git/objects/${B:0:2}/${B:2}"; stat -c%s "$p"
23

$ python3 -c "
import sys
p = sys.argv[1]
d = open(p,'rb').read()
open(p,'wb').write(d[:11])" "$p"
$ stat -c%s "$p"
11

# --- the lookup ---
$ CTRL /usr/bin/git cat-file -t "$B"
error: header for 5106a63ecf7df82b1b45855bf64ba14d90f978ca too long, exceeds 32 bytes
fatal: git cat-file: could not get object info      # exit 128

$ CTRL /usr/bin/git cat-file blob "$B" > body.out
error: header for 5106a63ecf7df82b1b45855bf64ba14d90f978ca too long, exceeds 32 bytes
fatal: loose object 5106a63ecf7df82b1b45855bf64ba14d90f978ca
       (stored in .git/objects/51/06a63ecf7df82b1b45855bf64ba14d90f978ca) is corrupt
                                                    # exit 128
$ stat -c%s body.out
0

$ sha1sum body.out
da39a3ee5e6b4b0d3255bfef95601890afd80709            # SHA-1 of the empty input
```

This entry cites **no witness**, so its conclusion does not *depend* on the §2.1
controls — what a returncode-ignoring resolver concludes is independent of
provenance. It is nonetheless run under them, like every other entry, because the
file-wide rule removes exactly the per-entry judgement that produced three
separate "that command was not controlled" findings. E-7 is the entry that carries
the W3 claim.

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

W3 requires **`§2.1 holds`**, so the reproduction is run under an invocation that
satisfies each §2.1 clause — an absolute executable path, and a child environment
built with `env -i` rather than inherited. The environment below is the complete
set of names the child received:

```console
# --- construction, from an empty repository ---
$ CTRL /usr/bin/git init -q r7 && cd r7
$ python3 -c "import sys; sys.stdout.write('COMPRESSIBLE PAYLOAD LINE\n'*400000)" > big.txt
$ stat -c%s big.txt
10400000
$ B=$(CTRL /usr/bin/git hash-object -w big.txt); echo $B
a394ec43d91e167d3dae803e62e1049e0b642694
$ p=".git/objects/${B:0:2}/${B:2}"; stat -c%s "$p"
60565

# --- truncate the loose object to 60% of its length ---
$ python3 -c "
import sys
p = sys.argv[1]
d = open(p,'rb').read()
open(p,'wb').write(d[:36339])" "$p"
$ stat -c%s "$p"
36339

# --- the lookup: the env -i prefix is ON each command, not a separate step ---
$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" cat-file -t $B
blob                                    # exit 0   <-- the kind operation SUCCEEDS

$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" cat-file blob $B > body.out
fatal: unable to stream a394ec43… to stdout
                                        # exit 128 <-- the body operation FAILS
$ stat -c%s body.out
6225920                                 # 6.2 MB of stdout from the FAILED command

$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git hash-object --stdin < body.out      # id of the debris
4ba9f2dda3a7db27a6ab082e60c2aeee535d5681

$ python3 -c "                               # SECOND, INDEPENDENT implementation
import hashlib, sys
b = open('body.out','rb').read()
print(hashlib.sha1(b'blob %d\\0' % len(b) + b).hexdigest())"
4ba9f2dda3a7db27a6ab082e60c2aeee535d5681     # agrees
$ echo $B                               # what was requested
a394ec43d91e167d3dae803e62e1049e0b642694
```

The `env -i` prefix is demonstrated to constrain the child in E-4, using a variable
deliberately left set in the parent; the same prefix is used here.

**Observed:** with that environment and that executable, the kind operation exits 0
and reports `blob`; the body operation emits 6,225,920 bytes and then exits 128;
the recomputed id over that output differs from the requested id. Neither
`--filters` nor `--textconv` was passed, and the repository has no remote.

**Inference:** the invocation above meets every §2.1 clause — the executable is
named by absolute path rather than found through `PATH` resolution, the environment
is constructed rather than inherited, lazy fetch is disabled and unreachable, and
the read is raw — so what is exhibited is the full **`§2.1 holds` + §2.2 fails**
shape witness W3 names, not the §2.2 half alone. A resolver that checks the
first command's status but not the second's will hash 6.2 MB of debris and report
`IDENTITY_VIOLATION` where the correct state is `LOOKUP_UNOBTAINABLE`. The failure
is not hypothetical. **Pinned to the reviewed base commit `e70d019`**, not to a
mutable branch name: in `tools/a1_v2_extract_graph.py` at that revision,
`_local_object` tests `kind.returncode != 0` and returns `None`, then runs the body
command and passes `body.stdout` directly into `hashlib.sha1(...)` **without
testing `body.returncode`**. That exact property is what E-7 exhibits.

A reader checking this after the implementation lands should read it at `e70d019`
and expect the property to be **gone** at later revisions — its removal is the
point of the work this contract precedes, so finding it absent on a current branch
confirms the record rather than contradicting it.

**Normative clause supported:** §2.2; §3 (`LOOKUP_UNOBTAINABLE`); witness W3.

**Does NOT establish:** that this is the only route to partial output, or anything
about how git buffers its output. The observation is the byte count and the exit
status, nothing more.

It also does not establish that §2.1 is *satisfiable in general* — only that **this
invocation satisfies it**, which is what W3 needs. §2.1 is a requirement on how the
resolver calls git, so a witness for it is an invocation exhibiting each clause,
not a proof about environments the resolver does not control.

**Provenance of these controls:** the first revision of this entry ran plain `git`
from `PATH` with an inherited environment, and still called the result "the exact
shape witness W3 names". It was not: absent the §2.1 controls it exhibited only the
§2.2 half, and an implementation could have passed an E-7-derived test while using
a repository-selected executable — a case that must yield `LOOKUP_UNOBTAINABLE` for
a different reason. The controls were added rather than the claim narrowed, because
the stronger statement is the one W3 actually needs.

---

## E-8 — Repository configuration can transform the returned bytes, but only on request

**Question:** can a repository-configured program change the bytes a lookup
returns, and if so, what selects that path?

**Reproduction and observed result:** a repository whose `.gitattributes` binds
`f.txt` to a filter and a textconv driver, both configured to rewrite the content.

```console
# --- construction, from an empty repository ---
$ CTRL /usr/bin/git init -q w8 && cd w8
$ printf 'ORIGINAL CONTENT\n'              > f.txt
$ printf 'f.txt filter=evil diff=evil\n'   > .gitattributes
$ CTRL /usr/bin/git config filter.evil.smudge 'sed s/ORIGINAL/SMUDGED/'
$ CTRL /usr/bin/git config filter.evil.clean  'cat'
$ CTRL /usr/bin/git config diff.evil.textconv 'sed s/ORIGINAL/TEXTCONV/'
$ CTRL /usr/bin/git add f.txt
$ O=$(CTRL /usr/bin/git rev-parse :f.txt); echo "$O"
700c458a4944f938d69a631d20ba0ba0b44c9563

# --- the contract's invocation: no transformation flag ---
$ CTRL /usr/bin/git cat-file blob "$O"
ORIGINAL CONTENT

# --- the caller opts in ---
$ CTRL /usr/bin/git cat-file --filters  --path=f.txt "$O"
SMUDGED CONTENT

$ CTRL /usr/bin/git cat-file --textconv --path=f.txt "$O"
SMUDGED CONTENT

# --- checkout, to show the configured filter was genuinely live ---
$ rm f.txt && CTRL /usr/bin/git checkout -- f.txt && cat f.txt
SMUDGED CONTENT

$ CTRL /usr/bin/git cat-file -h
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

## E-8.2 — Absence can be reported with a ZERO exit status

**Question:** does a successful lookup command imply the object was available?

**Reproduction and observed result:** a fresh empty repository and an oid that is
certainly not in it.

```console
$ CTRL /usr/bin/git init -q bc
$ MISSING=1111111111111111111111111111111111111111

$ printf '%s\n' "$MISSING" | env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 \
      GIT_NO_LAZY_FETCH=1 GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 \
      GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git -c safe.directory=$(pwd)/bc -C bc \
        cat-file --batch-check='%(objecttype)'
1111111111111111111111111111111111111111 missing
                                        # exit 0

$ printf '%s\n' "$MISSING" | env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 \
      GIT_NO_LAZY_FETCH=1 GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 \
      GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git -c safe.directory=$(pwd)/bc -C bc cat-file --batch
1111111111111111111111111111111111111111 missing
                                        # exit 0

$ env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
      GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
      /usr/bin/git -c safe.directory=$(pwd)/bc -C bc cat-file -t "$MISSING"
fatal: git cat-file: could not get object info
                                        # exit 128
```

**Observed:** for the same absent oid under the same controls, the two batch forms
exit `0` and print `missing`; the non-batch form exits `128`. Exit status alone
does not distinguish "the object was produced" from "the object is absent".

**Inference:** a resolver cannot treat command success as evidence of
availability. §2.2 therefore carries a second, independent condition — that the
required operations actually yield a usable kind and the complete bytes — rather
than inferring that from their exit statuses.

**Normative clause supported:** §2.2, second condition; §3's exhaustiveness
argument, which reads availability off §2.2 rather than off exit status.

**Does NOT establish:** that these are the only zero-exit absence reports, or
anything about which lookup form an implementation ought to use. The contract does
not mandate a form; it requires that whichever form is used be judged on the
answer it produced.

**Why this entry exists.** §2.2 previously said only that every required operation
must succeed. That was written to close a case where an operation FAILED producing
nothing, and it silently assumed the converse — that a succeeding operation had
produced something. These commands are the counterexample, and they are ordinary
supported usage rather than an exotic edge.

---

## E-8.3 — The object format is declared per repository, and changes the digest

**Question:** is a git object id always a SHA-1 digest?

**Reproduction and observed result:**

```console
$ CTRL /usr/bin/git init -q of1
$ CTRL /usr/bin/git init -q --object-format=sha256 of256

$ CTRL /usr/bin/git -c safe.directory=$(pwd)/of1 -C of1 rev-parse --show-object-format
sha1                                    # exit 0

$ CTRL /usr/bin/git -c safe.directory=$(pwd)/of256 -C of256 rev-parse --show-object-format
sha256                                  # exit 0

# --- the SAME content, stored in each ---
$ printf 'AUTHENTIC AUTHORITY' \
    | CTRL /usr/bin/git -c safe.directory=$(pwd)/of1 -C of1 hash-object -w --stdin
36ae691f4261ce7008c7bfc827233e74dc3fc96e                           # 40 chars

$ printf 'AUTHENTIC AUTHORITY' \
    | CTRL /usr/bin/git -c safe.directory=$(pwd)/of256 -C of256 hash-object -w --stdin
b959693d240b3325123359c1907748c20df3be759f79d5525f2bf64416ca055a   # 64 chars
```

**Observed:** the two repositories report different object formats; the identical
content receives a 40-character id in one and a 64-character id in the other. The
format is reported successfully under the full §2.1 control set.

**Inference:** a resolver that fixes its recomputation at SHA-1 would, in a
`sha256` repository, produce a 40-character value that cannot equal the requested
64-character oid **for any input** — so every authentic object would be reported
`IDENTITY_VIOLATION`. The digest function must therefore be derived from the
repository's declared format, which is what §3.1 requires.

**Normative clause supported:** §3.1, algorithm selection.

**Does NOT establish:** that these are the only formats git will ever support, nor
that reading the format is itself trustworthy in an adversarial repository. §3.1
addresses the latter directly: a repository that misreports its format yields ids
that do not match its own objects, which is exactly what the §3 comparison
detects.

**Provenance of these controls.** The first revision of this entry ran the two
`hash-object` commands with an inherited environment while using their output to
support the width claim. An inherited `GIT_DIR` overrides `-C` — shown in this
file's header — so neither recorded digest was established to have come from the
repository whose format had just been read. Both now run through `CTRL`, and both
values are unchanged.

**Why this entry exists.** §3.1 previously fixed the algorithm at plain SHA-1. That
was introduced to guarantee totality, and it did — but it also silently narrowed
the contract to one object format, contradicting the frozen scalar definition in
`docs/q-deck/a1-authority-contracts.md`, which defines a full object id as "the
repository's object-format width". Totality was the right requirement; naming a
specific algorithm to obtain it was not.

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
