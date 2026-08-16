"""Mechanical check for the RS-P0 evidence record.

Parses fenced console blocks, joins backslash-continuations into whole
commands, then applies five properties. Earlier versions inspected only
lines beginning with `$`, so a continuation line was never examined.
"""
from pathlib import Path
import re, sys

import subprocess as _sp
# Constructed environment for EVERY git call, including this bootstrap: an
# inherited GIT_WORK_TREE/GIT_DIR made --show-toplevel return a foreign
# checkout, and every later -C "$root" call then examined that repository.
# Repository-local config can NAME A PROGRAM that git executes:
# log.showSignature+gpg.program (via `log` on a signed commit) and
# core.fsmonitor (via `diff-tree`). .git/config is neither system nor
# global, and git has no variable that disables it — so these are refused
# BY NAME, and the list is NOT claimed complete.
GC = ("-c", "log.showSignature=false", "-c", "core.fsmonitor=false")
GENV = {"PATH": "/usr/bin", "GIT_NO_REPLACE_OBJECTS": "1",
        "GIT_GRAFT_FILE": "/dev/null",
        "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null"}
_r = _sp.run(("/usr/bin/git",) + GC + ("rev-parse", "--show-toplevel"),
             capture_output=True, text=True, env=GENV)
if _r.returncode != 0:
    raise SystemExit("CANNOT CHECK: not inside a work tree")
# anchor to the repo root: run from a subdirectory, a relative path reads
# nothing and every property scores zero on an empty file.
p = Path(_r.stdout.strip()) / "docs/tasks/a1-resolver-semantics-evidence.md"
lines = p.read_text().splitlines()

# --- collect commands inside ```console blocks, joining continuations ---
cmds, in_block, buf, start = [], False, None, None
for n, l in enumerate(lines, 1):
    if l.startswith("```"):
        in_block = l.startswith("```console")
        if buf is not None:
            cmds.append((start, buf)); buf = None
        continue
    if not in_block:
        continue
    s = l.lstrip()
    if buf is not None:                      # continuing a command
        buf += " " + s
        if not s.endswith("\\"):
            cmds.append((start, buf)); buf = None
        continue
    if s.startswith("$") or s.startswith("CTRL(") or s.startswith("NOREPL("):
        body = s[1:].strip() if s.startswith("$") else s
        if body.endswith("\\"):
            buf, start = body, n
        else:
            cmds.append((n, body))

def controlled(c):
    return any(k in c for k in ("CTRL /usr/bin/git", "NOREPL /usr/bin/git",
                                "CTRL /usr/bin/env", "CTRL /bin/sh", "env -i "))

fail = 0
def rep(tag, items):
    global fail
    print(f"{tag}: {len(items)}"); fail += len(items)
    for n, c in items: print("    ", n, c[:110])

rep("A. uncontrolled /usr/bin/git",
    [(n,c) for n,c in cmds if "/usr/bin/git" in c and not controlled(c)])
rep("B. bare git",
    [(n,c) for n,c in cmds if re.search(r'(?<![\w/])git(?:\s|$)', c) and "/usr/bin/git" not in c])
rep("C. prose placeholder",
    [(n,c) for n,c in cmds if re.search(r'<[a-z0-9][a-z0-9 _-]*>', c)])
writes = []
for n, c in cmds:
    if re.search(r'\bcp\b', c) or "open(p,'wb')" in c or 'open(p,"wb")' in c:
        window = "\n".join(lines[max(0,n-7):n+2])
        if not re.search(r'rm -f|os\.remove', window):
            writes.append((n,c))
rep("D. .git/objects write without prior removal", writes)
rep("E. abbreviated oid in command position",
    [(n,c) for n,c in cmds if re.search(r'\b[0-9a-f]{7,39}\b', c)])

# --- Property F: no ellipsis or placeholder ANYWHERE inside a console block,
# --- including OUTPUT lines. Properties C and E cover command positions only,
# --- which is how three editorially-truncated git outputs survived: the record
# --- claimed "the outputs shown are from that execution" while showing `…`.
in_console = False
fhits = []
for n, l in enumerate(lines, 1):
    if l.startswith("```console"):
        in_console = True; continue
    if in_console and l.startswith("```"):
        in_console = False; continue
    if not in_console:
        continue
    if re.search(r'…|(?<![.\d])\.\.\.(?![.\d])|<[a-z0-9][a-z0-9 _-]*>', l):
        fhits.append((n, l))
rep("F. ellipsis/placeholder in a console block (commands OR output)", fhits)

# --- Field parsing, shared by G and H. A field runs to a blank line or to the
# --- start of the NEXT field, which is a line-initial bold ending in `:` or `.`.
# --- Stopping at any line-initial bold (the earlier rule) truncated a field at
# --- an ordinary emphasised phrase such as `**not** the operative clause`, which
# --- would silently hide a witness id or a §5 citation further down the field.
NEXT_FIELD = re.compile(r'^\*\*[^*]{1,60}[:.]\*\*')

def field_at(src, i):
    """src[i] starts a field; return the whole field joined."""
    buf, j = [src[i]], i + 1
    while j < len(src) and src[j].strip() and not NEXT_FIELD.match(src[j]):
        buf.append(src[j]); j += 1
    return " ".join(buf)

# --- Property G: a `Normative clause supported` field must never name a witness.
# --- Witness ids live only in `Related required witness`, with status attached.
# --- Merging them let an owed witness read as evidence-backed (E-4/E-7/E-8.3).
ghits = []
for n, l in enumerate(lines, 1):
    if not l.startswith("**Normative clause supported:**"):
        continue
    field = field_at(lines, n - 1)
    if re.search(r'\bW\d\b', field):
        ghits.append((n, field[:110]))
rep("G. witness id inside a `supported` field", ghits)

# --- Property H: an entry whose reproduction consults a repository-supplied
# --- indirection layer (`git replace`, or a write to objects/info/alternates)
# --- must cite §2.1 as operative, and may name §5 only when the same field says
# --- WITHOUT indirection. Post-2305127 such a lookup fails §2.1, so §3's
# --- ANYTHING ELSE fires and §5 is never reached. E-3 and E-5 both still routed
# --- to §5; a reviewer named only E-3.
bounds = [n for n, l in enumerate(lines, 1) if re.match(r'^## E-', l)] + [len(lines)+1]
hhits = []
for a, b in zip(bounds, bounds[1:]):
    body = lines[a-1:b-1]
    # trigger on the entry's CONSOLE text only, and on the layer name rather
    # than on a command shape: `git -c ... -C ... replace` puts options between
    # `git` and the subcommand, so a `git replace` pattern missed E-3 entirely.
    console, inb = [], False
    for l in body:
        if l.startswith("```"):
            inb = l.startswith("```console"); continue
        if inb:
            console.append(l)
    if not re.search(r'\breplace\b|\balternates\b', "\n".join(console)):
        continue
    for i, l in enumerate(body):
        if not l.startswith("**Normative clause supported:**"):
            continue
        field = field_at(body, i)
        if "§2.1" not in field:
            hhits.append((a + i, "no §2.1: " + field[:100]))
        if "§5" in field and "WITHOUT indirection" not in field:
            hhits.append((a + i, "unqualified §5: " + field[:100]))
rep("H. indirection entry citing §5 as operative / omitting §2.1", hhits)

# --- Property I: a revision-bound claim must name the revision, not its relation
# --- to another revision. "`260c3f0`'s successor" resolved correctly the day it
# --- was written and stops resolving the moment the branch is gone.
ihits = [(n, l.strip()) for n, l in enumerate(lines, 1)
         if re.search(r"`[0-9a-f]{7,40}`\s*(?:'|’)s\s+(?:successor|predecessor|parent|child)"
                      r"|the commit (?:after|before)\s+`[0-9a-f]{7,40}`", l)]
rep("I. revision named by relation instead of by id", ihits)

# --- Property J: any artifact cited from OUTSIDE the co-versioned pair must
# --- carry a full 40-hex revision nearby. The header claimed every other
# --- artifact was pinned while citing AGENTS.md by bare name; a reviewer named
# --- the contract citation only. The pair itself cannot be pinned from inside
# --- itself, so it is exempt by construction and by the header's stated rule.
COVERSIONED = ("a1-f-v2-resolver-semantics.md", "a1-resolver-semantics-evidence.md")
CITE = re.compile(r'AGENTS\.md|tools/[\w./-]+\.py|docs/[\w./-]+\.md')
jhits = []
for n, l in enumerate(lines, 1):
    m = CITE.search(l)
    if not m or any(c in m.group(0) for c in COVERSIONED):
        continue
    if not re.search(r'[0-9a-f]{40}', "\n".join(lines[max(0, n-5):n+4])):
        jhits.append((n, l.strip()[:110]))
rep("J. artifact cited without a 40-hex revision", jhits)

# --- Property K: every commit that changed the contract WITHOUT changing this
# --- record must appear in the re-verification ledger. The previous rule
# --- compared the two files' last-touch commits and could not see a
# --- contract-only commit followed by any evidence-only commit — five such
# --- commits existed while that rule reported the pairing healthy.
import subprocess
BASE = "e70d019923a958bb18d8dbb266da007c6e93a88c"
CONTRACT = "docs/tasks/a1-f-v2-resolver-semantics.md"
EVIDENCE = "docs/tasks/a1-resolver-semantics-evidence.md"

MARKER = "f748f66c8b7deca55f66227d4b211c19d668b5ba"   # verified_against

def git(*a):
    # Fail CLOSED, and anchor to the repo ROOT. Two prior versions of this
    # helper failed open: the first ignored returncode (empty stdout read as
    # "no gaps"), and both ran git from the cwd, so from a subdirectory the
    # relative pathspec matched nothing and K reported zero gaps.
    r = subprocess.run(("/usr/bin/git",) + GC + ("-C", ROOT) + a,
                       capture_output=True, text=True, env=GENV)
    if r.returncode != 0:
        raise SystemExit(f"K. CANNOT CHECK: git {' '.join(a)}: {r.stderr.strip()}")
    return r.stdout.split()

# Reuse the sanitised bootstrap from the top of this file. An earlier revision
# discovered the root a SECOND time here, with an inherited environment — so
# fixing one bootstrap left the other selecting a foreign repository.
ROOT = str(p.parent.parent.parent)

git("rev-parse", "--verify", "--quiet", f"{BASE}^{{commit}}")

# --- Property N: the enumerated RANGE must not be truncated. Replace refs and
# --- grafts SUBSTITUTE parentage and are refused by name; shallowness
# --- TRUNCATES it and cannot be, because a boundary is legitimate state. A
# --- truncated walk reports no error and no short count — it simply stops, so
# --- K counted one contract-only revision instead of five, found it in the
# --- ledger, and returned zero gaps. Coverage claims require the range to be
# --- anchored at BASE and uncut inside.
_anc = subprocess.run(("/usr/bin/git",) + GC + ("-C", ROOT, "merge-base",
                       "--is-ancestor", BASE, "HEAD"),
                      capture_output=True, text=True, env=GENV)
if _anc.returncode != 0:
    raise SystemExit("N. CANNOT CHECK: base is not a reachable ancestor of HEAD")
if git("rev-parse", "--is-shallow-repository") != ["false"]:
    _sf = Path(ROOT) / git("rev-parse", "--git-path", "shallow")[0]
    if not _sf.is_file():
        raise SystemExit("N. CANNOT CHECK: shallow repository, boundary set unreadable")
    _range = set(git("rev-list", f"{BASE}..HEAD")) | {BASE}
    _cut = _range & set(_sf.read_text().split())
    if _cut:
        raise SystemExit("N. CANNOT CHECK: shallow boundary inside the enumerated "
                         f"range ({sorted(_cut)[0][:10]})")
print("N. enumerated range anchored at base and uncut: 0")

# --- Property L: the verified_against marker must name the contract's CURRENT
# --- last-touching commit. Enumeration alone covers only contract-only
# --- commits; a commit touching BOTH files can move the contract without the
# --- record being re-checked, and simultaneity is not re-verification.
_last = git("log", "-1", "--format=%H", "--", CONTRACT)
lhits = []
if not _last:
    raise SystemExit("L. CANNOT CHECK: contract has no history here")
if _last[0] != MARKER:
    lhits.append((0, f"contract at {_last[0][:10]}, verified_against {MARKER[:10]}"))
if MARKER not in p.read_text():
    lhits.append((0, "verified_against marker absent from the record"))
rep("L. RE-VERIFICATION OWED (contract moved since the last re-check)", lhits)
# A path-limited `git log` SIMPLIFIES by default: it shows the simplest history
# explaining the final tree, so a contract-only commit on a side branch whose
# change a later merge discards is dropped from the walk entirely — enumerated
# as zero, reported as coverage. --full-history keeps every path. Merges need
# their own arm: --no-merges excludes them, and an EVIL MERGE can change the
# contract relative to EVERY parent without any non-merge commit doing so.
khits = []
for sha in git("log", "--full-history", "--no-merges", "--format=%H",
               f"{BASE}..HEAD", "--", CONTRACT):
    touched = git("diff-tree", "--no-commit-id", "--name-only", "-r", sha)
    if EVIDENCE in touched:
        continue                      # co-versioned; nothing owed
    if sha not in p.read_text():
        khits.append((0, f"contract-only {sha[:10]} absent from the ledger"))

def _blob(rev, path):
    r = subprocess.run(("/usr/bin/git",) + GC + ("-C", ROOT, "rev-parse", "--verify",
                        "--quiet", f"{rev}:{path}"),
                       capture_output=True, text=True, env=GENV)
    return r.stdout.strip() or "absent"

for m in git("rev-list", "--merges", f"{BASE}..HEAD"):
    mc, mr = _blob(m, CONTRACT), _blob(m, EVIDENCE)
    parents = git("log", "-1", "--format=%P", m)
    # JOINTLY per parent. Aggregating `any(contract differs)` and
    # `any(record same)` independently let the two halves be satisfied by
    # DIFFERENT parents: a merge taking the contract from A and the record
    # from B changes the contract alone relative to B, and scored clean.
    alone = any(_blob(par, CONTRACT) != mc and _blob(par, EVIDENCE) == mr
                for par in parents)
    if alone and m not in p.read_text():
        khits.append((0, f"merge {m[:10]} changed the contract alone"))
rep("K. contract-only commit missing from the re-verification ledger", khits)

# --- Property M: the documented co-versioning block must RUN. Extract it from
# --- the record verbatim and execute it; CANNOT CHECK (exit 2) in this
# --- repository means the reader cannot perform the check at all. A shipped
# --- line-continuation inside single quotes made every run exit 2 while a
# --- hand-retyped single-line copy passed — the transcription was verified,
# --- the artifact was not.
mstart = next((i for i, l in enumerate(lines) if l.strip().startswith("set -eu")), None)
mhits = []
if mstart is None:
    mhits.append((0, "documented check block not found"))
else:
    body, i = [], mstart
    while i < len(lines) and (lines[i].startswith("    ") or not lines[i].strip()):
        body.append(lines[i][4:] if lines[i].startswith("    ") else "")
        i += 1
    r = subprocess.run(("/bin/sh", "-s"), input="\n".join(body),
                       capture_output=True, text=True, cwd=ROOT)
    if r.returncode == 2:
        mhits.append((mstart + 1, f"block exits CANNOT CHECK: {r.stdout.strip()[:70]}"))
    elif r.returncode != 0:
        mhits.append((mstart + 1, f"block exits {r.returncode}: {r.stderr.strip()[:70]}"))
rep("M. documented check block does not run", mhits)

print(f"\ncommands parsed: {len(cmds)}    TOTAL GAPS: {fail}")
sys.exit(1 if fail else 0)
