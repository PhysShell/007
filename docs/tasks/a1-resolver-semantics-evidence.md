# A1-F v2 — Resolver semantics: evidence record

```text
NON-NORMATIVE EVIDENCE

Nothing in this file defines resolver states, classification precedence,
or consumer authority. If this file conflicts with the normative contract
in docs/tasks/a1-f-v2-resolver-semantics.md, the normative contract wins
and the discrepancy is a defect in this evidence record.

The contract is cited by PATH. A pin is IMPOSSIBLE here rather than merely
inconvenient: this file and the contract are added in the SAME commit, so
any revision naming the contract would have to be a commit that did not
yet exist when this line was written.

That impossibility does NOT establish that the pairing holds, and neither
does co-versioning by itself: **SIMULTANEITY IS NOT RE-VERIFICATION.** A
commit may touch both files while moving the contract for a reason these
observations were never re-checked against. So TWO things are tested -- has
the contract moved since the last re-check, and did any commit move it
without this record at all:

    set -eu
    E='/usr/bin/env -i PATH=/usr/bin GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null'
    root=$($E /usr/bin/git rev-parse --show-toplevel) ||
      { echo 'CANNOT CHECK: not inside a work tree'; exit 2; }
    G() { $E /usr/bin/git -C "$root" "$@"; }

    base=e70d019923a958bb18d8dbb266da007c6e93a88c
    contract=docs/tasks/a1-f-v2-resolver-semantics.md
    record=docs/tasks/a1-resolver-semantics-evidence.md
    verified_against=f748f66c8b7deca55f66227d4b211c19d668b5ba

    G rev-parse --verify --quiet "$base^{commit}" >/dev/null ||
      { echo 'CANNOT CHECK: base commit absent'; exit 2; }

    now=$(G log -1 --format=%H -- "$contract") ||
      { echo 'CANNOT CHECK: contract history unreadable'; exit 2; }
    [ "$now" = "$verified_against" ] ||
      printf 'RE-VERIFICATION OWED: contract at %s, verified against %s\n' \
             "$now" "$verified_against"

    commits=$(G log --format=%H "$base"..HEAD -- "$contract") ||
      { echo 'CANNOT CHECK: history enumeration failed'; exit 2; }

    for c in $commits; do
      names=$(G diff-tree --no-commit-id --name-only -r "$c") ||
        { echo 'CANNOT CHECK: diff-tree failed'; exit 2; }
      hit=0
      for n in $names; do
        if [ "$n" = "$record" ]; then hit=1; fi
      done
      [ "$hit" -eq 1 ] ||
        printf 'contract changed alone at %s\n' "$c"
    done

The exact-name test uses only shell BUILTINS. After this block the only
external programs are `/usr/bin/env` and `/usr/bin/git`, both absolute;
`for`, `[`, `printf` and `echo` are builtins, which `PATH` cannot redirect.
A path containing whitespace would word-split and fail to match, printing a
GAP that is not one -- wrong in the FAIL-CLOSED direction, and neither of
the two paths compared here contains whitespace. If the caller controls the
shell itself, nothing in this block helps; that is the stated boundary, not
an oversight.

`verified_against` names the contract's last-touching commit as of the
re-check recorded below. It is nameable precisely because it already exists:
the self-reference that blocks pinning the contract in the SAME commit does
not block naming a commit that is already in history. Moving the contract
therefore obliges an edit here, which is the diff a silent re-verification
otherwise fails to produce.

FOUR outcomes, and they must not be collapsed:

    exit 2                  CANNOT CHECK -- says NOTHING about the pairing
    RE-VERIFICATION OWED    the contract moved since the last re-check
    commits printed         GAPS -- each must appear in the ledger below
    exit 0, no output       NONE FOUND -- examined, and the obligation held

Every commit that prints must appear in the ledger, naming what was
re-checked and what was found. One in neither is a GAP: the contract moved
and nothing here records whether these observations still support it.

An earlier revision ran this as a bare pipeline with `git log` as producer.
With the base absent -- a shallow clone, or an exported tree -- `git log`
died with `fatal: Invalid revision range`, and because a pipeline reports
only its LAST command's status, the check exited 0 and printed nothing. It
therefore rendered CANNOT CHECK as NONE FOUND, announcing a healthy pairing
having examined no history at all. Reproduced before this repair was
written, not reasoned about.

That is the failure this repository's own verdict rule already forbids:
`FAIL` means the gate ran and the target failed it; `ERROR` means the
harness could not obtain a trustworthy answer. A check that cannot run must
not report success.

RE-VERIFICATION LEDGER -- contract revisions changed without this record.
A re-verification that finds nothing to change produces NO DIFF, so without
this ledger there is no way to tell "checked, nothing needed changing" from
"never checked", and that ambiguity resolves toward the flattering reading.

  260c3f03c1c62939dca87b00accbb400ce6bc79a
      §2.2 defines `usable kind`, excluding suitability.
      Checked E-8.2, whose inference speaks of "a usable kind and the
      complete bytes" and never appeals to suitability.   -> UNCHANGED

  f164924100bda43e03400b58eae61e722ab14fae
      §2.2 completeness defined by the evidence obtained; §3's third case
      made structural.  Checked E-8.2.                    -> UNCHANGED

  6114102b8058cb452c68a648b9830e6301c2a78a
      §2.2 covers a required operation that fails producing nothing.
      Checked E-8.2.                                      -> UNCHANGED

  02fd180e3311b4750040cb5b9706ee049fce23f9
      §3.1 requires plain SHA-1; the permitted-variant escape removed.
      Checked that no entry cites a per-implementation declared variant.
      E-8.3's "declared format" is the REPOSITORY's object format, which
      is what §3.1 requires.                              -> UNCHANGED

  f748f66c8b7deca55f66227d4b211c19d668b5ba
      §2.3 states its routing rule before its counterfactual.
      Checked E-4, which performs the format read itself (exit 0, `sha1`),
      exercising the prerequisite rather than the counterfactual.
                                                          -> UNCHANGED

UNCHANGED means the re-check found no claim here made stale. It does NOT
mean the contract change was immaterial, and it is not a witness for
anything owed.

Two earlier revisions of this block are withdrawn, both quoted so the
claims are on the record rather than edited away:

  1. "the governing revision of the contract is always the revision at
     which this file is read" -- asserted the pairing instead of exposing
     it, and in the drift case told the reader to accept a contract these
     observations were never made against.

  2. comparing the two files' last-touch commits and calling the pairing
     BROKEN only when the contract's was the later -- a ONE-SAMPLE test in
     ONE direction. It cannot see a contract-only commit followed by any
     evidence-only commit, and FIVE such commits exist in this branch's
     own history. It reported a healthy pairing over the history it was
     written to police. A reviewer found it, one commit after it was
     written.

  3. running the enumeration with an INHERITED environment and RELATIVE
     pathspecs, and matching the record path with `grep -qx`. Three
     defects, two of them reproduced:

       - run from any subdirectory, the pathspec matches nothing, so the
         check printed NOTHING and exited 0 -- NONE FOUND again. Verified
         from `docs/`: zero candidates where five exist. It then caught
         the author's own verification command by the same route, which
         is how thoroughly it was not theoretical;
       - `grep -qx "$record"` is a BRE, so `.` is a wildcard and a decoy
         path `a1-resolver-semantics-evidenceXmd` satisfies the check.
         Verified; `grep -Fqx --` refuses it;
       - `diff-tree | grep` reports only grep's status. This one was NOT
         reproduced as a fail-open: a diff-tree that fails producing no
         output makes grep fail, which PRINTS a gap. The status is now
         captured anyway, because relying on that is relying on an
         accident of which command's status survives.

     The record's own header requires every git invocation here to run
     under a constructed environment and an absolute executable. The check
     policing that record did neither.

  4. sanitising every git call EXCEPT the one that decides which repository
     the others examine. The repair for (3) discovered the work-tree root
     with an inherited environment, so an inherited `GIT_WORK_TREE` or
     `GIT_DIR` pointed `root` at a foreign checkout and every later `-C
     "$root"` command then validated that checkout's history -- NONE FOUND,
     exit 0, about the wrong repository. Verified: with `GIT_WORK_TREE` and
     `GIT_DIR` set to a scratch repo, `/usr/bin/git rev-parse
     --show-toplevel` returned the foreign root; under the constructed
     environment it returned this one.

     The control set is not a property of most invocations. **An
     unsanitised bootstrap selects the subject that every sanitised command
     then examines carefully.**

  5. deciding the exact-name test with `grep`, the one program in that block
     still resolved through the CALLER's `PATH`. The constructed environment
     covered the git calls and not the comparison that interprets them. With
     a `grep` shim that exits 0, the block printed nothing and exited 0 --
     NONE FOUND -- while five contract-only revisions sat in the history it
     claimed to have examined. Reproduced. `env` was bare for the same
     reason and is now absolute.

     Pinning `grep` would have repaired the site. The comparison is done
     with builtins instead, because **a decision must not depend on a
     program the environment gets to choose.**

Citations of any OTHER artifact are pinned to a full 40-hex revision --
see E-4, E-7 and E-8.3. The distinction is co-versioning, not convenience.
```

**`Normative clause supported` never names a witness.** A witness id appears only
in a separate **`Related required witness`** field, always carrying its status. The
two were previously merged, and a reader scanning the support field could count an
owed witness as evidence-backed without reading the prose that owed it — E-8.3 said
`witness W9` in its support field and `W9 is OWED on both arms` a few lines below,
in the same entry. Support and debt must not be left for the reader to reconcile,
because that reconciliation reliably resolves toward PASS. The separation is
mechanically checked (property G).

Every entry uses the same fixed shape. The **Observed** and **Inference** fields
are separated by rule: a statement may appear under *Observed* only if it is a
property of a recorded command's output, exit status or files. Statements about
what a program does internally belong under *Inference*, or nowhere.

Unless stated otherwise, every reproduction below was executed in a fresh
`git init` scratch repository, and the outputs shown are from that execution.

**Every git invocation in this file runs under the §2.1 control set.** It is
defined once, here, in full, and used as `CTRL` in every block below, so that no
command's provenance depends on the **environment** it inherited.

**"The §2.1 control set" names the environment controls below, and only those.**
It implements the §2.1 clauses that an environment can implement — the executable
named by absolute path, and a constructed rather than inherited environment. It
does **not** establish §2.1's **no-network-access** clause: `GIT_NO_LAZY_FETCH=1`
is a setting, not an observation that no packet left the process. No entry in this
file may be read as witnessing that clause, whatever control set it ran under; the
debt is recorded in E-8.1 and is owed by every §2.1-conditioned witness here.

```console
CTRL() {
  env -i PATH=/usr/bin GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
         GIT_NO_REPLACE_OBJECTS=1 GIT_CONFIG_NOSYSTEM=1 \
         GIT_CONFIG_GLOBAL=/dev/null "$@"
}
```

**What `CTRL` does NOT control, stated because this record has been caught by it
twice.** `env -i` replaces the **environment**. It does not touch **inherited
process state**, which crosses `exec` regardless: the **umask**, the **uid**, the
working directory, and resource limits. Two recorded observations have already
turned out to rest on process state rather than on anything `CTRL` fixes:

| Inherited property | What it silently changed | Found by |
|---|---|---|
| uid — this environment runs as **root** | a bare `cp` over a write-protected loose object succeeded | reviewer, E-4/E-5 |
| **umask** | the recorded loose-object mode: `022` gives `444`, `027` gives `440`, `077` gives `400` | reviewer, E-6/E-7 |

Neither announces itself in a transcript, and both produce commands that run
correctly for the recorder. Where such a property affects a recorded value, the
entry states the **invariant** rather than the value this machine happened to
produce — see the mode claims in E-4, E-6 and E-7, which assert *no write
permission for anyone* rather than a specific octal number.

This is uniform on purpose. Three separate findings on this PR were "that
particular command was not controlled", each about a different command, because
deciding case by case which invocations matter is a judgement that kept failing.
An inherited variable can redirect *any* invocation — demonstrated, not assumed:

```console
$ CTRL /usr/bin/git init -q gd1
$ CTRL /usr/bin/git init -q --object-format=sha256 gd256
$ CTRL /usr/bin/git -C gd1   rev-parse --show-object-format
sha1
$ CTRL /usr/bin/git -C gd256 rev-parse --show-object-format
sha256
$ CTRL /usr/bin/env GIT_DIR="$(pwd)/gd1/.git" \
       /usr/bin/git -C gd256 rev-parse --show-object-format
sha1                          # -C names gd256; GIT_DIR names gd1, and wins
```

The **setup** commands are controlled too, and that is not decoration. An
uncontrolled `init` can be redirected by an inherited `GIT_DIR` or
`GIT_OBJECT_DIRECTORY` exactly as the command being demonstrated can, so the
final line would still print `sha1` while proving nothing about *which*
repositories it compared. The two `rev-parse` calls in the middle exist for the
same reason: they establish that `gd1` and `gd256` really do differ in object
format, so the last line's `sha1` is a precedence result and not a coincidence.

`CTRL /usr/bin/env GIT_DIR=… /usr/bin/git …` is the full control set **plus one
deliberately added variable**, not a weakened one: `CTRL` clears the environment
first, and the inner `env` then adds `GIT_DIR` to that cleared set.

**The "every invocation" claim above has EXACTLY TWO EXCEPTIONS, named here.**
Both deliberately omit `GIT_NO_REPLACE_OBJECTS=1`, because each exists to exhibit
a route that the full control set closes — a route that is invisible if the
control is in force:

| Arm | Where | Omits | Why |
|---|---|---|---|
| `NOREPL` | E-3, defined in full at its point of use | `GIT_NO_REPLACE_OBJECTS=1` | shows what a replace ref does when the route is open |
| route-OPEN lookup | E-5, marked inline | `GIT_NO_REPLACE_OBJECTS=1` | contrasts with the route-CLOSED arm in the same entry |

**Neither arm is §2.1-controlled, and no observation made under them may be cited
as a §2.1-controlled result.** Stating this in the header rather than only at the
point of use matters: a reader who takes "every git invocation runs under the
§2.1 control set" at face value could classify active replace-ref behaviour as
something the contract's provenance requirements permit, when in fact these two
arms are exactly where those requirements were switched off.

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
# --- setup: the repository must exist and be entered FIRST. Without this,
# --- `config example.local fromrepo` fails `fatal: not in a git directory`.
# --- A fixed path is used so every output below is a literal value.
$ rm -rf /tmp/rs-x2 && CTRL /usr/bin/git init -q /tmp/rs-x2 && cd /tmp/rs-x2

$ CTRL /usr/bin/git --show-scope
unknown option: --show-scope

$ CTRL /usr/bin/git -c safe.directory=/tmp/rs-x2 -C /tmp/rs-x2 config --show-scope --list
local	core.repositoryformatversion=0
local	core.filemode=true
local	core.bare=false
local	core.logallrefupdates=true
command	safe.directory=/tmp/rs-x2

$ CTRL /usr/bin/git config -h | grep show-scope
    --[no-]show-scope     show scope of config (worktree, local, global, system, command)

$ CTRL /usr/bin/git -c example.key=value -C /tmp/rs-x2 config --show-scope --get example.key
command	value

$ CTRL /usr/bin/git -C /tmp/rs-x2 config example.local fromrepo   # set IN the repository
$ CTRL /usr/bin/git -C /tmp/rs-x2 config --show-scope --get example.local
local	fromrepo

$ CTRL /usr/bin/git -c example.key=value -C /tmp/rs-x2 config --show-scope --list | grep example
local	example.local=fromrepo
command	example.key=value
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

**Inference:** an oid→itself replace ref self-terminates, so **that shape** is not
a delivery route. **Nothing wider follows.** In particular this does not establish
that a prepared collision cannot be delivered through a replace ref — **E-5, two
entries below, exhibits the composition that does exactly that**: a map `A -> C`
whose target path holds bytes `D`, served because a hash-path mismatch is not
refused (E-4). Were `D` chosen to collide with `A`, the recomputed id would match.
The self-map fails for a reason — replace depth — that no other shape shares.

**Normative clause supported:** §2.1's **no-repository-controlled-indirection**
clause. A lookup that consults a replace ref fails §2.1, so §3's `ANYTHING ELSE`
branch requires `LOOKUP_UNOBTAINABLE` and **no other state is available**. §5 is
**not** the operative clause here: it governs prepared collisions reached WITHOUT
indirection, which is what E-10's replace-ref rows already say.

**Related required witness:** none is discharged here. The `refs/replace` arm of
the indirection clause is exhibited by E-5; **W7 remains OWED on the alternates
arm** (E-5.1).

**Does NOT establish:** that no other ref-based composition exists — one is
recorded two entries below; that a prepared collision is undeliverable through a
ref; or anything about routes of more than one hop.

**Provenance of this entry's scope.** Until this revision the inference read *"the
prepared-collision case cannot be delivered through a replace ref, because the
mapping would have to be oid→itself"* — a universal claim resting on a false
premise, contradicted by E-5 **in this same file**. A reviewer asked for it to be
narrowed to self-maps at `bcc7518`; **that request was declined, and the decline
was wrong.** The decline reasoned that this entry's title and its *Does NOT
establish* line already said "self-map" — both true — and never checked the
*Inference* line, which did not. A hedge in one field does not narrow an
unconditional sentence in another. The misrouting to §5 is a second and later
defect: §2.1's indirection clause did not exist at `bcc7518`, having been added at
`2305127`.

---

## E-4 — A hash-path mismatch is served, not refused

**Question:** if an object file is overwritten with a *different* valid object,
does the lookup refuse it?

**Reproduction and observed result:** W4 requires **`§2 holds`**, which is THREE
prerequisites, not one. This entry exercises what it can: the §2.1 controls as in
E-7 — absolute executable path, and a child environment built with `env -i`
carrying only `PATH=/usr/bin`, `GIT_TERMINAL_PROMPT=0`, `GIT_NO_LAZY_FETCH=1`,
`GIT_NO_REPLACE_OBJECTS=1`, `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`,
with neither `--filters` nor `--textconv` passed; §2.2, by obtaining both the kind
and the complete bytes through operations that succeed; and **§2.3, by performing
the object-format read the contract requires** — added below after a reviewer
observed that an earlier revision claimed `§2 holds` while never reading the
format at all. See *"What this record cannot witness"* at the end of this entry
for what remains owed.

```console
# --- construction, from an empty repository ---
$ CTRL /usr/bin/git init -q r4 && cd r4
$ A=$(printf 'AUTHENTIC AUTHORITY' | CTRL /usr/bin/git hash-object -w --stdin); echo $A
36ae691f4261ce7008c7bfc827233e74dc3fc96e
$ D=$(printf 'SUBSTITUTED CONTENT' | CTRL /usr/bin/git hash-object -w --stdin); echo $D
7feb07853362884e770de9cde3265336be1697fb
$ umask                                      # this run; see below — it matters
0022
$ stat -c%a ".git/objects/${A:0:2}/${A:2}"   # loose objects carry NO write bit
444
$ rm -f ".git/objects/${A:0:2}/${A:2}"       # required: cp alone cannot
$ cp  ".git/objects/${D:0:2}/${D:2}" \
      ".git/objects/${A:0:2}/${A:2}"         # overwrite a write-protected file

# --- the lookup: the env -i prefix is ON each command, not a separate step ---
# --- §2.3: the identity function must be OBTAINED, not assumed ---
$ CTRL /usr/bin/git -c safe.directory=$(pwd) -C "$(pwd)" rev-parse --show-object-format
sha1                         # exit 0   <-- §2.3 holds: a recognised format

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
7feb07853362884e770de9cde3265336be1697fb     # != the requested
                                             # 36ae691f4261ce7008c7bfc827233e74dc3fc96e

$ python3 -c "                               # SECOND, INDEPENDENT implementation
import hashlib, sys                          # — no git involved at all
b = open('body.out','rb').read()
print(hashlib.sha1(b'blob %d\\0' % len(b) + b).hexdigest())"
7feb07853362884e770de9cde3265336be1697fb     # agrees

$ CTRL /usr/bin/git fsck
error: 7feb07853362884e770de9cde3265336be1697fb: hash-path mismatch, found at: .git/objects/36/ae691f4261ce7008c7bfc827233e74dc3fc96e
```

**The prefix constrains the child, and that is shown rather than assumed.** With a
variable deliberately present in the parent shell, the same prefix yields a child
that cannot see it:

```console
$ export SNEAKY=leaked       # a name deliberately present in the PARENT

$ CTRL /bin/sh -c 'test -z "${SNEAKY-}"' && echo child_sees_SNEAKY=no
child_sees_SNEAKY=no

$ [ -n "${SNEAKY-}" ] && echo parent_still_has_it=yes
parent_still_has_it=yes
```

**No environment is printed, by rule.** `AGENTS.md` at
`e70d019923a958bb18d8dbb266da007c6e93a88c` forbids an environment dump entering
the tree at all — its P0 list names an *environment dump* alongside
OAuth/session-storage artifacts and tokens, under credential leakage, the
repository's central claim — so this arm asserts the property as a **boolean** instead of
displaying the child's variables. An earlier revision printed both the child
environment and the parent value; that was a violation of the rule, in the file
whose purpose is careful evidence, and it was introduced by me while demonstrating
rigour. A recorded dump is also a live hazard rather than a stale one: anyone
re-running the block with a real secret in the parent would write it into the
record.

**Observed:** under that invocation both operations exit 0, the lookup returns
bytes whose recomputed id differs from the requested one, and a separate command,
`fsck`, reports the mismatch.

**What this record cannot witness — `§2 holds` is THREE prerequisites.**

| Prerequisite | Witnessed by this entry? |
|---|---|
| §2.1 executable, environment, raw read, repository cannot choose | **yes** |
| §2.1 **no network access during the operation** | **NO — unwitnessable here** |
| §2.2 kind and complete bytes, each via a succeeding operation | **yes** |
| §2.3 identity function obtained, value in the enumerated set | **yes**, since this revision |

**No entry in this file establishes the complete `§2 holds` condition, and none
ever can under the present controls**, because §2.1's network clause is a claim
about what did *not* happen on a socket and nothing here observes it. Therefore
**every §2-conditioned witness — W1, W4, W5 and W8 — is OWED on that clause**, not
only W8, whose debt was already recorded for a different reason (collision
material).

This is the second time the same property has failed. §2.3 was ADDED at `6d02367`
and §2.1 was WIDENED at `4eb074c`; on both occasions the clause was changed and
the entries *claiming `§2 holds`* were not re-checked. E-4 went on citing W4 while
never reading the object format at all — an implementation that defaults to SHA-1
when that read fails would reproduce every value in this entry and still be wrong,
emitting `IDENTITY_VIOLATION` where the contract requires `LOOKUP_UNOBTAINABLE`.

**The rule, stated so it does not have to be rediscovered a third time: changing
any §2 prerequisite obliges a sweep of every site that asserts §2 — or any part of
§2 — is satisfied.** Widening the ledger row is not the fix; it is one third of it.

**Provenance of the `rm` step, and why it was missing.** Earlier revisions of this
entry and of E-5 recorded a bare `cp` over the object file. Git writes loose
objects **with no write bit for anyone**, so that command **fails with
`Permission denied` for an ordinary user**. It succeeded when recorded only
because this environment runs as **root**, and root ignores the write bit.

**The exact octal is umask-dependent; the invariant is not.** Git creates the file
`0444 & ~umask`, so a reader may observe `444`, `440` or `400`:

```console
$ for U in 000 022 027 077; do
    ( umask $U; CTRL /usr/bin/git init -q "u$U" && cd "u$U" \
      && B=$(printf 'X' | CTRL /usr/bin/git hash-object -w --stdin) \
      && echo "umask $U -> $(stat -c%a ".git/objects/${B:0:2}/${B:2}")" )
  done
umask 000 -> 444
umask 022 -> 444
umask 027 -> 440
umask 077 -> 400
```

**Observed:** the mode varies with the umask, and **no umask produces a write
bit** — `000`, the most permissive possible, still yields `444`. **Inference:** git
masks write off unconditionally, so *"an ordinary user cannot overwrite this file
in place"* holds in every case, which is the only property the `rm` step needs.
The specific `444` recorded elsewhere in this file is this machine's `umask 0022`,
not a general fact.

**`CTRL` does not fix this, and that is the wider point.** `env -i` replaces the
environment; the umask is inherited **process state** and crosses `exec`
untouched — verified: `CTRL /bin/sh -c 'umask'` reports the caller's value. So a
recorded number can still depend on the shell that ran it even under the full
control set. That is the second such premise in this record after root privilege,
and the header now names the class rather than these two instances.

That is a reproducibility defect of a kind the earlier mechanical sweep could not
see: the command was syntactically complete, used the absolute executable, carried
no placeholder, and *worked when run* — it simply worked for a reason that does not
generalise to the reader. A privilege the recorder happens to hold is exactly the
sort of premise that never gets written down, because it never announces itself.
The re-run with `rm -f` reproduces every recorded value unchanged, so the defect
was in the instructions, never in the result.

**The first repair of this fixed the instance and not the class.** The rule adopted
then was "every `cp` must be preceded by `rm -f`" — which names the *command that
was reported*, not the *property that fails*. E-6 and E-7 truncate a loose object
with `open(p,'wb')` in python3, which is the same write to the same write-protected file and
was left untouched by that rule; both were unrunnable outside a root environment
until a later reviewer named them. The rule is therefore stated by property now:

> **Any write to a path under `.git/objects` must remove the target first** —
> `rm -f` before `cp`, `os.remove(p)` before `open(p,'wb')`, and equally for any
> other write introduced later. Git creates loose objects with **no write bit for
> anyone** (`0444 & ~umask`); nothing that overwrites one in place is runnable by
> an ordinary user.

Both truncations were re-executed in that form. E-6 still reports `23 -> 11` with
both operations exiting 128 and an empty `body.out`; E-7 still reports
`10,400,000 -> 60,565 -> 36,339`, kind `blob` at exit 0, body exit 128, **6,225,920
bytes** of stdout, and digest `4ba9f2dda3a7db27a6ab082e60c2aeee535d5681`. As
before, the defect was in the instructions and never in the result.

**Provenance of these controls:** as with E-7, the first revision of this entry ran
plain `git` from `PATH` with an inherited environment while citing witness W4 —
which requires `§2 holds`. That gap was **not reported by any reviewer**; it was
found by sweeping every entry citing a witness after the same defect was reported
against E-7. Re-running under the controls reproduces the result unchanged, so the
entry's conclusion stands and only its standing as a W4 witness was ever in doubt.

**Inference:** the mismatch is detectable by recomputation at the call site, and
this is the channel the identity postcondition exists for.

**Normative clause supported:** §3 `IDENTITY_VIOLATION`.

**Related required witness:** **W4 — OWED.** §2.1's no-network clause is
unwitnessed here, so this entry does not complete W4's `§2 holds` condition.

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
$ rm -f ".git/objects/${C:0:2}/${C:2}"               # no write bit (E-4);
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

**Normative clause supported:** §2.1's **no-repository-controlled-indirection**
clause — the requirement this entry exhibits the need for. The composed route
fails §2.1, so §3's `ANYTHING ELSE` branch requires `LOOKUP_UNOBTAINABLE`; the
composition creates no fourth state because §3 is the only partition, not because
§5 absorbs it. §5 governs the same object-store substitution reached WITHOUT
indirection.

**Provenance of this citation.** Until `d993e48` this line read *"§2.1 (route
closure)"* while §2.1 contained **no such clause** — the evidence cited a
requirement the contract did not make, and an implementation conforming to §2.1 as
written could have left the route open. A reviewer found it. The clause now exists,
so the citation is checkable; before, it was not.

**Does NOT establish:** that a colliding `D` was ever produced, or that the
composition is undetectable in the general case. The three blobs used here do not
collide, and the substitution above **is** detectable by recomputation.

---

## E-5.1 — The control set does NOT close the alternates route

**Question:** §2.1 forbids repository-controlled indirection. Does the control set
used by every entry in this file actually close it?

**Reproduction and observed result:**

```console
$ rm -rf /tmp/rs-alt-donor /tmp/rs-alt-main
$ CTRL /usr/bin/git init -q /tmp/rs-alt-donor
$ O=$(printf 'OBJECT ONLY IN THE ALTERNATE\n' \
      | CTRL /usr/bin/git -C /tmp/rs-alt-donor hash-object -w --stdin); echo "$O"
08cbb3149b7d9df0a0c1eda1155a9416b33b74a0

$ CTRL /usr/bin/git init -q /tmp/rs-alt-main
$ CTRL /usr/bin/git -C /tmp/rs-alt-main cat-file -t "$O"
fatal: git cat-file: could not get object info

# --- the repository supplies the redirection, as a FILE in its own object store ---
$ printf '/tmp/rs-alt-donor/.git/objects\n' \
      > /tmp/rs-alt-main/.git/objects/info/alternates

$ CTRL /usr/bin/git -C /tmp/rs-alt-main cat-file -t "$O"
blob
$ CTRL /usr/bin/git -C /tmp/rs-alt-main cat-file blob "$O"
OBJECT ONLY IN THE ALTERNATE
```

**Observed:** under the **full control set defined in this file's header**, an
object absent from the repository became retrievable — kind and bytes, both at
exit 0 — because a file inside the repository named another object directory.
Nothing about the invocation changed between the two attempts.

**Inference:** `GIT_NO_REPLACE_OBJECTS=1` closes the `refs/replace` route and
**does not close the alternates route**. §2.1's indirection clause therefore has
two halves, and this record's controls implement only one of them.

**Normative clause supported:** §2.1's no-repository-controlled-indirection
clause — this entry exhibits *why* the clause is needed for alternates, exactly as
E-5 does for `refs/replace`.

**Related required witness:** **W7 — OWED on an alternates arm.** W7 requires a
§2.1 prerequisite to be ABSENT; the alternates arm of the indirection clause has
no witness in this record.

**Does NOT establish:** that `refs/replace` and alternates are the only such
layers. §2.1 states the property and explicitly declines to claim its table is
complete.

**Why this entry exists.** §2.1's indirection clause was added at `2305127` after
a reviewer observed that the contract never closed the replace-ref route. Adding
the clause immediately made a second thing true, which this entry records rather
than leaves implicit: **the control set every reproduction here runs under does
not satisfy the clause it now cites.** A clause can be correct and still be
unwitnessed by the very controls that were supposed to embody it.

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

$ stat -c%a "$p"
444                                     # no write bit; exact octal is umask-
                                        # dependent — see E-4
$ python3 -c "
import sys, os
p = sys.argv[1]
d = open(p,'rb').read()
os.remove(p)                            # required: no write bit on the target
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
satisfies each **observable** §2.1 clause — an absolute executable path, and a child environment
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
$ stat -c%a "$p"
444                                     # no write bit; exact octal is umask-
                                        # dependent — see E-4
$ python3 -c "
import sys, os
p = sys.argv[1]
d = open(p,'rb').read()
os.remove(p)                            # required: no write bit on the target
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
fatal: unable to stream a394ec43d91e167d3dae803e62e1049e0b642694 to stdout
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

**Inference:** the invocation above meets the §2.1 clauses this record can
observe, and **one clause it cannot**:

| §2.1 clause | Status here |
|---|---|
| executable selected by this layer, by absolute path | **observed** — `/usr/bin/git`, never `PATH` |
| child environment constructed, not inherited | **observed** — `env -i` with a named set |
| **NO network access during the operation** | **NOT OBSERVED** — see below |
| raw read, no transformation requested | **observed** — neither `--filters` nor `--textconv` passed |
| repository cannot choose the program or the transformation path | **observed**, as a consequence of the two above |

**The network clause is not witnessed here, and this entry does not claim it.**
`GIT_NO_LAZY_FETCH=1` is a *setting*, and "the repository has no remote" is a
*configuration fact*; neither is an observation that no packet left the process.
An implementation that performed a reachability or metadata probe before emitting
the same partial output would reproduce **every recorded value in this entry** and
still violate §2.1. E-8.1 records that debt, and it is owed for this entry too.

So what is exhibited is the `§2.2 fails` half plus the observable part of §2.1 —
**not** the complete `§2.1 holds` condition W3 names. W3 remains owed on the
network clause like every other §2.1-conditioned witness. A resolver that checks the
first command's status but not the second's will hash 6.2 MB of debris and report
`IDENTITY_VIOLATION` where the correct state is `LOOKUP_UNOBTAINABLE`. The failure
is not hypothetical. **Pinned to the reviewed base commit `e70d019923a958bb18d8dbb266da007c6e93a88c`**, not to a
mutable branch name: in `tools/a1_v2_extract_graph.py` at that revision,
`_local_object` tests `kind.returncode != 0` and returns `None`, then runs the body
command and passes `body.stdout` directly into `hashlib.sha1(...)` **without
testing `body.returncode`**. That exact property is what E-7 exhibits.

A reader checking this after the implementation lands should read it at `e70d019923a958bb18d8dbb266da007c6e93a88c`
and expect the property to be **gone** at later revisions — its removal is the
point of the work this contract precedes, so finding it absent on a current branch
confirms the record rather than contradicting it.

**Normative clause supported:** §2.2; §3 (`LOOKUP_UNOBTAINABLE`).

**Related required witness:** **W3 — OWED.** W3 begins with `§2.1 holds`, and
§2.1's no-network clause is unwitnessed here; this entry supplies the
partial-output arm only.

**Does NOT establish:** that this is the only route to partial output, or anything
about how git buffers its output. The observation is the byte count and the exit
status, nothing more.

It also does not establish that §2.1 is *satisfiable in general* — only that **this
invocation satisfies the observable clauses**, with the network clause owed as set
out above. §2.1 is a requirement on how the
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
$ CTRL /usr/bin/git config diff.evil.textconv 'sed s/SMUDGED/TEXTCONV/'
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
TEXTCONV CONTENT

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

**Provenance of the textconv expression, and why it changed.** An earlier revision
configured `diff.evil.textconv` as `sed s/ORIGINAL/TEXTCONV/`. Under that fixture
the recorded output was:

```text
--filters   -> SMUDGED CONTENT
--textconv  -> SMUDGED CONTENT      <- IDENTICAL
```

The value was correct — git really does print that — and it was still worthless as
a textconv witness. The smudge filter rewrites `ORIGINAL` before the textconv
program runs, so that program's `sed` matched nothing and the arm returned the
*filter's* result. The entry therefore demonstrated the filter twice and the
textconv driver **not at all**, while E-10 credited E-8 with closing the whole
smudge/textconv/filter family.

The expression now matches `SMUDGED`, which the smudge stage actually produces, so
the two arms return **different** strings and each names its own mechanism.

The general rule this is an instance of: **an observation must discriminate the
thing it is cited for.** An arm whose output coincides with another arm's is
consistent with the mechanism being absent, so it cannot witness that mechanism —
regardless of whether the recorded value is accurate. Accuracy of a transcript and
discriminating power are independent properties, and only the second one makes an
entry evidence.

---

## E-8.1 — Prerequisites with no witness in this record

Recorded so the remaining gap is visible rather than implied by silence.

| Prerequisite | Status of evidence here |
|---|---|
| counterfeit executable selected via `PATH` | **no reproduction recorded** |
| any network access during the operation (lazy/promisor fetch, reachability probe, authentication, metadata) | **no reproduction recorded** |
| repository-controlled indirection — the **alternates** half of §2.1's clause | **route demonstrated OPEN under the control set (E-5.1); no closing witness** |

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

**The network prerequisite was WIDENED at `4eb074c6df30a1cc954f73ddfa1552bf6dccc026`** —
the revision at which §2.1 stopped forbidding only network acquisition that
*satisfies* the lookup and began forbidding contact with a remote **at all** during
the operation — and the gap
widened with it. **Every entry that CLAIMS §2.1 is satisfied was widened too**,
which is the step that was missed the first time: E-7 went on asserting that its
invocation "meets every §2.1 clause" while E-8.1 conceded that nothing here
observes the network property. Widening the ledger row is not enough — the sites
that cite the clause as satisfied have to be swept as well, or the record
contradicts itself in exactly the direction that flatters it. §2.1 previously forbade only network acquisition *satisfying the
lookup*; it now forbids **contacting a remote at all** during the operation. The
old wording let an implementation probe a remote for reachability or
authentication and then serve the object locally — no network-delivered byte
satisfies the lookup, so §2.1 held — while §1 asks whether this layer can answer
**without network access** and §4 has `RESOLVED` claim a **local** acquisition.

This record witnesses **neither** form. `GIT_NO_LAZY_FETCH=1` appears in every
control set here, but no entry demonstrates that it closes anything, and nothing
here observes the wider property at all. The row above is widened to match the
clause so the debt is not understated by describing a narrower one.

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

**The variable is normally UNSET, and the command still answers.** This matters
because it decides which of the two a resolver may read:

```console
$ CTRL /usr/bin/git -C of1 config --local --get extensions.objectformat
                                        # exit 1 — unset
$ CTRL /usr/bin/git -c safe.directory=$(pwd)/of1 -C of1 rev-parse --show-object-format
sha1                                    # exit 0 — answered anyway
```

**Observed:** the two repositories report different object formats; the identical
content receives a 40-character id in one and a 64-character id in the other; and
in the `sha1` repository `extensions.objectFormat` is unset while
`--show-object-format` still reports `sha1`. All under the full §2.1 control set.

**Inference:** a resolver that fixes its recomputation at SHA-1 would, in a
`sha256` repository, produce a 40-character value that cannot equal the requested
64-character oid **for any input** — so every authentic object would be reported
`IDENTITY_VIOLATION`. The digest function must therefore be derived from the
repository's declared format, which is what §3.1 requires.

**Normative clause supported:** §3.1, algorithm selection; §2.3, the availability
prerequisite.

**Related required witness:** **W9 — OWED ON BOTH ARMS.** This entry records only
the two SUCCEEDING format reads; neither a failed read nor an out-of-set value is
reproduced anywhere in this record.

**Does NOT establish:** that these are the only formats git will ever support, nor
that reading the format is itself trustworthy in an adversarial repository. §3.1
addresses the latter directly: a repository that misreports its format yields ids
that do not match its own objects, which is exactly what the §3 comparison
detects.

**This entry witnesses only the two SUCCEEDING reads.** Both commands above
answered, and both answers were inside the enumerated set. The two arms of W9 —
a format read that **fails**, and one that returns a value **outside the set** —
are **not reproduced here**, and W9 is therefore **OWED** on both arms. The first
"does NOT establish" clause above is exactly why §2.3 states availability as a
prerequisite rather than deriving it from git's behaviour: this record cannot
establish what git does with a format it does not implement, so the contract must
not depend on it.

**Provenance of these controls.** The first revision of this entry ran the two
`hash-object` commands with an inherited environment while using their output to
support the width claim. An inherited `GIT_DIR` overrides `-C` — shown in this
file's header — so neither recorded digest was established to have come from the
repository whose format had just been read. Both now run through `CTRL`, and both
values are unchanged.

**Why this entry exists.** §3.1 previously fixed the algorithm at plain SHA-1. That
was introduced to guarantee totality, and it did — but it also silently narrowed
the contract to one object format, contradicting the frozen scalar definition in
`docs/q-deck/a1-authority-contracts.md` at the reviewed base commit **`e70d019923a958bb18d8dbb266da007c6e93a88c`**,
line 1216, blob `e22539ddf4f7c9ab260e16835eef8ef18abbe726`, which defines a full object id as "the repository's
object-format width". Totality was the right requirement; naming a specific
algorithm to obtain it was not.

**Both citations of that document are pinned**, here and in §3.1 of the contract.
An earlier revision named only the path. That was the same defect this record had
already corrected for `_local_object` — a claim about another artifact bound to a
mutable name — applied inconsistently, since the standard had been adopted for
this record's own subject and not for its cross-artifact citation.

---

## E-9 — The contract asserts NO cryptographic status

**Question:** does the contract depend on SHA-1 properties?

**Observed:** nothing. No cryptographic experiment was run, and none is needed.

**Inference:** none is drawn. **This entry makes no claim about the status of
SHA-1 collision resistance or second-preimage resistance**, because no clause of
the contract depends on either. §4 was written that way deliberately: `RESOLVED`
claims only that an admissible local acquisition occurred and that the returned
bytes hash to the requested id. Both remain true observations whatever any hash
property turns out to be.

**Normative clause supported:** none. The entry exists to record the *absence* of
a dependency, which is a property of this contract and is checkable by reading §4.

**Does NOT establish:** anything about SHA-1, in either direction.

**Provenance — why the claims were removed rather than sourced.** An earlier
revision stated that identical-prefix (2017) and chosen-prefix (2020) attacks are
published and that second-preimage resistance has no published practical attack —
and in the same paragraph admitted *"this record cites no source for them."* That
is an assertion outrunning its evidence, stated in the file whose header forbids
exactly that, and it was flagged by a reviewer.

Citing papers would have fixed only two thirds of it. **The third claim is an
assertion of ABSENCE** — that no practical attack is known — and no citation can
establish it. It is a statement about the entire literature at one moment, it
decays silently as that literature grows, and a reader in a year cannot tell
whether it was ever checked or has merely gone stale.

So the claims are gone rather than sourced. The entry loses nothing the contract
needed: an argument that a clause does not depend on a property never has to say
what that property's status is. Stating the status is what created the debt.

---

## E-10 — Disposition of the terminated attempt's findings

Recorded so that a reviewer can ask the sharpest available question: *name a
substantial finding from the rejected attempt that this core either contradicts or
leaves without a defined state and downstream action.*

| Finding family | Disposition |
|---|---|
| failed-body debris hashed as evidence | core — §2.2, W3 |
| counterfeit executable | core — §2.1 provenance, W7; no witness (E-8.1) |
| lazy / network acquisition | core — §2.1 provenance, now stated as NO network access at all; no witness (E-8.1) |
| smudge / textconv / filter transformation | core — §2.1 raw read; witness E-8, both arms now discriminating |
| `refs/replace` yielding a different id | core — §2.1 indirection clause FAILS, so §3's `ANYTHING ELSE` -> `LOOKUP_UNOBTAINABLE`, W7 |
| `refs/replace` composed to yield a matching id | core — §2.1 indirection clause FAILS first, so `LOOKUP_UNOBTAINABLE`, W7. §5 governs only prepared collisions reached WITHOUT indirection |
| prepared collision | core — §5 `RESOLVED` + §4 non-claim; **W8 OUTSTANDING — never exercised** |
| hash-strength premises (collision / second preimage) | core — §4 removes the dependency entirely; E-9 asserts no cryptographic status at all |
| mechanism-vs-effect misclassification | core — §3 partitions by outcome; no mechanism vocabulary exists here to misclassify with |
| over-strong claims about git internals | evidence appendix only, under its mandatory *Observed / Inference* split |
| `git --show-scope` naming error | evidence appendix, corrected to `git config --show-scope` |
| wrong object kind treated as resolver failure | core — §8 |
| `NOT_PRESENT_LOCALLY` frozen without an oracle | core — §10 refuses it |
| diagnostic text used as a state discriminator | core — §6, W6 |

**W8 is marked outstanding wherever it appears here.** E-5 records that no
colliding `D` was ever produced, and the contract states that W8 is OWED until
genuine collision material exists. A disposition row citing W8 without that mark
would let a reader treat the witness as evidence-backed, which is the precise
failure the OWED rule exists to prevent.

**W9 is also OUTSTANDING, on both arms**, and it is deliberately absent from the
table above. The table's scope is the terminated attempt's corpus; W9 comes from a
finding raised against *this* attempt at head `6e830c8` — that §2 could hold while
`oid` had no algorithm, leaving the same input pointed at two states. It is
recorded here so that a reader scanning this file for `OUTSTANDING` finds every
owed witness in one place, without widening a table that answers a different
question. E-8.3 witnesses only the two succeeding format reads; neither the
failed-read arm nor the outside-the-set arm is reproduced anywhere in this record.

This table is a coverage argument, like everything else in this file. An error in
it is an error about the previous attempt or about coverage — never a change to
the contract, which the header of this file subordinates the whole record to.
