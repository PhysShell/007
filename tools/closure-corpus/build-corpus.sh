#!/bin/sh
# Regression corpus preserved from the PR #145 review rounds.  Issue #147 Step 0.
#
# WHAT THIS IS.  During #145 an independent reviewer repeatedly showed that the
# co-versioning check embedded in docs/tasks/a1-resolver-semantics-evidence.md
# could return a FALSE PASS.  Each round produced a repository shape that
# discriminated a healthy check from a broken one.  Those shapes existed only in
# a scratch directory of the review session; this script reconstructs them.
#
# WHAT THIS IS NOT.  It is not evidence that #145 is clean, and it must never be
# used as such -- see the sequencing invariant in issue #147.  Two of the cases
# below assert a DEFECT that is deliberately still present at the frozen head:
# they are recorded so a later design can show these inputs sit OUTSIDE its
# authority path, rather than being answered by yet another neutralizer.
#
# The subject under test is extracted FROM THE FROZEN RECORD, never retyped.
#
# usage:  ./build-corpus.sh [--repo URL] [--sha SHA] [--work DIR] [--keep]
set -eu

REPO="file:///home/user/007"
SHA="ed7969c4636362bcfc248bcb9957e140ca899f8c"
WORK=""
KEEP=0
while [ $# -gt 0 ]; do
  case "$1" in
    --repo) REPO=$2; shift 2 ;;
    --sha)  SHA=$2;  shift 2 ;;
    --work) WORK=$2; shift 2 ;;
    --keep) KEEP=1;  shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ -n "$WORK" ] || WORK=$(mktemp -d)
mkdir -p "$WORK"

CONTRACT=docs/tasks/a1-f-v2-resolver-semantics.md
RECORD=docs/tasks/a1-resolver-semantics-evidence.md
BASE=e70d019923a958bb18d8dbb266da007c6e93a88c   # PR base
MARK=f748f66c8b7deca55f66227d4b211c19d668b5ba   # verified_against

pass=0; fail=0
say() { printf '%s\n' "$*"; }

# ---------------------------------------------------------------- reference --
REF="$WORK/reference"
if [ ! -d "$REF" ]; then
  say "cloning reference from $REPO ..."
  git clone --quiet "$REPO" "$REF"
fi
git -C "$REF" cat-file -e "$SHA^{commit}" 2>/dev/null || {
  say "FATAL: frozen head $SHA not present in $REPO"; exit 2; }
# A clone copies BRANCHES, not remote-tracking refs.  Without a local ref the
# frozen subject is unreachable in every fixture cloned from this reference,
# and `checkout --detach` fails with "reference is not a tree".
git -C "$REF" update-ref refs/heads/corpus-subject "$SHA"

# RUN THE ARTIFACT: extract the documented block from the frozen record itself.
BLOCK="$WORK/block.sh"
git -C "$REF" show "$SHA:$RECORD" | awk '
  /^    set -eu$/ { on=1 }
  on { if ($0 ~ /^    / || $0 ~ /^$/) { sub(/^    /,""); print } else { exit } }
' > "$BLOCK"
[ -s "$BLOCK" ] || { say "FATAL: could not extract the documented block"; exit 2; }
say "extracted block: $(wc -l < "$BLOCK") lines, sha256 $(sha256sum "$BLOCK" | cut -c1-16)"

# helper programs used by the repository-local-config case
mkdir -p "$WORK/bin"
cat > "$WORK/bin/marker" <<EOF
#!/bin/sh
echo "EXECUTED \$*" >> "$WORK/marker.log"
exit 0
EOF
cat > "$WORK/bin/fakesign" <<'EOF'
#!/bin/sh
for a in "$@"; do
  case "$a" in --verify) echo "[GNUPG:] GOODSIG DEADBEEF fake" >&2; exit 0;; esac
done
echo "[GNUPG:] SIG_CREATED C 1 8 00 0 DEADBEEF" >&2
printf -- "-----BEGIN PGP SIGNATURE-----\n\nZmFrZQ==\n-----END PGP SIGNATURE-----\n"
EOF
mkdir -p "$WORK/evil"
for p in git env grep; do
  printf '#!/bin/sh\nexit 0\n' > "$WORK/evil/$p"; chmod +x "$WORK/evil/$p"
done
chmod +x "$WORK/bin/marker" "$WORK/bin/fakesign"

fixture() {                       # fixture <name> -> fresh checkout at $SHA
  d="$WORK/fx"; rm -rf "$d"
  git clone --quiet --no-hardlinks "$REF" "$d"
  git -C "$d" checkout -q --detach "$SHA"
  git -C "$d" config user.email corpus@example.invalid
  git -C "$d" config user.name  corpus
  printf '%s' "$d"
}

# expect <label> <class> <exit> <gaps> <mergegaps> [cwd] [env-prefix...]
expect() {
  label=$1 class=$2 xexit=$3 xgaps=$4 xmg=$5 dir=$6; shift 6
  # `out=$(...)` takes the substitution's status, so under `set -e` a CANNOT
  # CHECK case would abort the whole run instead of being scored.  Test it.
  if out=$(cd "$dir" && env "$@" /bin/sh "$BLOCK" 2>/dev/null); then rc=0; else rc=$?; fi
  g=$(printf '%s' "$out" | grep -c 'changed alone' || true)
  m=$(printf '%s' "$out" | grep -c 'by merge'      || true)
  if [ "$rc" = "$xexit" ] && [ "$g" = "$xgaps" ] && [ "$m" = "$xmg" ]; then
    pass=$((pass+1)); printf '  ok    %-34s %-18s exit=%s gaps=%s merge=%s\n' \
      "$label" "$class" "$rc" "$g" "$m"
  else
    fail=$((fail+1)); printf '  FAIL  %-34s %-18s got exit=%s gaps=%s merge=%s want exit=%s gaps=%s merge=%s\n' \
      "$label" "$class" "$rc" "$g" "$m" "$xexit" "$xgaps" "$xmg"
  fi
}

say ""
say "frozen subject: $SHA"
say "GUARD HOLDS = the check behaves correctly on this shape"
say "NOT DISCRIM = the shape does not exercise the mechanism it targets (open work)"
say ""

# ------------------------------------------------------------------ baseline --
d=$(fixture); expect "baseline (repo of record)" "GUARD HOLDS" 0 5 0 "$d" X=1
expect "invocation from a subdirectory" "GUARD HOLDS" 0 5 0 "$d/docs/tasks" X=1
expect "hostile PATH (git/env/grep shims)" "GUARD HOLDS" 0 5 0 "$d" "PATH=$WORK/evil"
expect "hostile GIT_DIR + GIT_WORK_TREE" "GUARD HOLDS" 0 5 0 "$d" \
       "GIT_DIR=$WORK/reference/.git" "GIT_WORK_TREE=$WORK/reference"

# --------------------------------------------------- substitution: replace ---
d=$(fixture)
T=$(git -C "$d" rev-parse "$MARK^{tree}")
NEW=$(git -C "$d" commit-tree "$T" -p "$BASE" -m counterfeit)
git -C "$d" replace -f "$MARK" "$NEW" >/dev/null 2>&1
expect "refs/replace substitution" "GUARD HOLDS" 0 5 0 "$d" X=1

# ---------------------------------------------------- substitution: graft ----
d=$(fixture)
mkdir -p "$d/.git/info"; printf '%s %s\n' "$MARK" "$BASE" > "$d/.git/info/grafts"
expect "legacy .git/info/grafts" "GUARD HOLDS" 0 5 0 "$d" X=1

# ------------------------------------------------------ truncation: shallow --
d="$WORK/fx"; rm -rf "$d"
# file:// is required: --depth is silently IGNORED for local-path clones, which
# produced a complete repository and a vacuously passing shallow case.
git clone --quiet --depth=10 --single-branch --branch corpus-subject "file://$REF" "$d"
git -C "$REF" cat-file commit "$BASE" | git -C "$d" hash-object -t commit -w --stdin >/dev/null
expect "shallow boundary, base injected" "GUARD HOLDS" 2 0 0 "$d" X=1

# boundary INSIDE the range while ancestry survives: needs a merge shape
d=$(fixture)
EMPTY=$(git -C "$d" hash-object -t tree -w /dev/null)
ORPH=$(git -C "$d" commit-tree "$EMPTY" -m side)
MRG=$(git -C "$d" commit-tree 'HEAD^{tree}' -p HEAD -p "$ORPH" -m merge)
git -C "$d" reset -q --hard "$MRG"
printf '%s\n' "$ORPH" > "$d/.git/shallow"
expect "shallow boundary inside range" "GUARD HOLDS" 2 0 0 "$d" X=1

# ------------------------------------------------- simplification / merges ---
d=$(fixture)
MAIN=$(git -C "$d" rev-parse HEAD)
printf '\nSIDE\n' >> "$d/$CONTRACT"; git -C "$d" commit -q -am "side: contract only"
SIDE=$(git -C "$d" rev-parse HEAD)
MRG=$(git -C "$d" commit-tree "$MAIN^{tree}" -p "$MAIN" -p "$SIDE" -m "discard side")
git -C "$d" reset -q --hard "$MRG"
expect "simplified walk hides side commit" "GUARD HOLDS" 0 6 1 "$d" X=1

d=$(fixture)
MAIN=$(git -C "$d" rev-parse HEAD)
git -C "$d" checkout -q -b side; echo x > "$d/UNRELATED.txt"
git -C "$d" add UNRELATED.txt; git -C "$d" commit -q -m "side: unrelated"
S2=$(git -C "$d" rev-parse HEAD)
git -C "$d" checkout -q "$MAIN"; printf '\nEVIL\n' >> "$d/$CONTRACT"
git -C "$d" add "$CONTRACT"; T=$(git -C "$d" write-tree)
M=$(git -C "$d" commit-tree "$T" -p "$MAIN" -p "$S2" -m "evil merge")
git -C "$d" checkout -q -f "$MAIN"; git -C "$d" reset -q --hard "$M"
expect "evil merge (contract vs all parents)" "GUARD HOLDS" 0 5 1 "$d" X=1

d=$(fixture)
MAIN=$(git -C "$d" rev-parse HEAD)
git -C "$d" checkout -q -b pa; printf '\nAC\n' >> "$d/$CONTRACT"; printf '\nAR\n' >> "$d/$RECORD"
git -C "$d" commit -q -am "parent A"; CA=$(git -C "$d" rev-parse HEAD)
git -C "$d" checkout -q -b pb "$MAIN"; printf '\nBC\n' >> "$d/$CONTRACT"; printf '\nBR\n' >> "$d/$RECORD"
git -C "$d" commit -q -am "parent B"; CB=$(git -C "$d" rev-parse HEAD)
git -C "$d" checkout -q "$CA"; git -C "$d" checkout "$CB" -- "$RECORD"; git -C "$d" add "$RECORD"
T=$(git -C "$d" write-tree); M=$(git -C "$d" commit-tree "$T" -p "$CA" -p "$CB" -m "mixed merge")
git -C "$d" checkout -q -f "$CA" 2>/dev/null; git -C "$d" reset -q --hard "$M"
expect "mixed merge (contract A, record B)" "GUARD HOLDS" 0 5 1 "$d" X=1

d=$(fixture)
MAIN=$(git -C "$d" rev-parse HEAD)
git -C "$d" checkout -q -b s; printf '\nC\n' >> "$d/$CONTRACT"; printf '\nR\n' >> "$d/$RECORD"
git -C "$d" commit -q -am "side: co-versioned"; S=$(git -C "$d" rev-parse HEAD)
M=$(git -C "$d" commit-tree "$S^{tree}" -p "$MAIN" -p "$S" -m "ordinary merge")
git -C "$d" checkout -q -f "$MAIN" 2>/dev/null; git -C "$d" reset -q --hard "$M"
expect "ordinary co-versioned merge (silent)" "NEGATIVE CONTROL" 0 5 0 "$d" X=1

# ------------------------------------------ repository-local config programs --
d=$(fixture)
git -C "$d" config gpg.program "$WORK/bin/fakesign"
printf '\nSIGNED\n' >> "$d/$CONTRACT"; git -C "$d" add "$CONTRACT"
T=$(git -C "$d" write-tree)
SC=$(git -C "$d" commit-tree -S "$T" -p HEAD -m "signed contract-only")
git -C "$d" reset -q --hard "$SC"
git -C "$d" config log.showSignature true
git -C "$d" config gpg.program "$WORK/bin/marker"
git -C "$d" config core.fsmonitor "$WORK/bin/marker"
rm -f "$WORK/marker.log"
expect "repo-local config spawns programs" "GUARD HOLDS" 0 6 0 "$d" X=1
if [ -f "$WORK/marker.log" ]; then
  fail=$((fail+1)); printf '  FAIL  %-34s %-18s helper executed %s times\n' \
    "repo-local helper neutralized" "GUARD HOLDS" "$(wc -l < "$WORK/marker.log")"
else
  pass=$((pass+1)); printf '  ok    %-34s %-18s helper never executed\n' \
    "repo-local helper neutralized" "GUARD HOLDS"
fi

# -------------------------------------------------- O-1 (NOT DISCRIMINATING) --
# INTENT: a descendant tree needed to resolve the paths is missing, so blob() --
# which uses ROOT tree readability as its discriminator -- reports the traversal
# failure as `absent` and the merge comparison scores clean.
#
# WHAT THIS FIXTURE ACTUALLY DOES: it exits 2 for a DIFFERENT reason.  Removing
# the docs/tasks tree breaks the history enumeration (`log`/`diff-tree`) further
# up, so the block reaches CANNOT CHECK before blob() is ever consulted.  The
# outcome is right and the mechanism is untested.
#
# It is kept, labelled honestly, because a test that cannot distinguish the
# defect from its absence is not a witness -- the rule this whole corpus exists
# to enforce.  A real discriminator must break ONLY a tree that the merge
# comparison consults: reachable from a merge PARENT, and not from any commit
# the contract-limited enumeration diff-trees.  Constructing it is open work
# for issue #147; O-1 itself is reproduced directly at the command level in
# the record's own prose (rev-parse HEAD:<path> exits 1 while
# rev-parse HEAD^{tree} exits 0).
d=$(fixture)
printf '\nLOOSEN\n' >> "$d/$CONTRACT"; git -C "$d" commit -q -am "loosen trees"
SUB=$(git -C "$d" rev-parse 'HEAD:docs/tasks')
rm -f "$d/.git/objects/$(printf '%s' "$SUB" | cut -c1-2)/$(printf '%s' "$SUB" | cut -c3-)"
MAIN=$(git -C "$d" rev-parse HEAD)
EMPTY=$(git -C "$d" hash-object -t tree -w /dev/null)
ORPH=$(git -C "$d" commit-tree "$EMPTY" -m side)
M=$(git -C "$d" commit-tree "$MAIN^{tree}" -p "$MAIN" -p "$ORPH" -m "merge over broken subtree" 2>/dev/null)
git -C "$d" reset -q --hard "$M" 2>/dev/null || true
expect "O-1 fixture (does NOT reach blob)" "NOT DISCRIMINATING" 2 0 0 "$d" X=1

say ""
say "corpus cases: $((pass+fail))   ok: $pass   FAIL: $fail"
say ""
say "O-2 has no external fixture: it is a SHAPE defect in the block itself --"
say "two command substitutions convert a failed enumeration into an empty list"
say "(the shallow-boundary 'range' assignment, whose rev-list status is masked"
say "by a following printf, and the merge loop's inline parent enumeration)."
say "Grep the block for \$( ) whose status is neither tested nor propagated."
[ "$KEEP" = 1 ] || rm -rf "$WORK/fx"
[ "$fail" = 0 ]
