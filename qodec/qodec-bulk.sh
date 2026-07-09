#!/usr/bin/env bash
# qodec-bulk — measure qodec on a file too big for the single-threaded miner
# in one shot. Splits INPUT into fixed byte chunks, encodes each in parallel
# across all cores, and sums the token report.
#
# The mine/deep/squeeze miner is single-threaded and superlinear, so one big
# file takes minutes-to-hours; N small chunks across N cores is dramatically
# faster for the same measurement. Byte-lossless per chunk (concatenated chunks
# == original). The reported gain is a LOWER bound: repetition that straddles a
# chunk boundary is not captured, so the whole-file miner would do slightly
# better — if it ever finished.
#
#   ./qodec-bulk.sh <file> [chunk=150k] [codec=deep] [jobs=nproc]
#   $QODEC overrides the binary path.
set -euo pipefail
export LC_ALL=C   # stable number formats for split sizes and awk output

IN=${1:?usage: qodec-bulk.sh <file> [chunk] [codec] [jobs]}
CHUNK=${2:-150k}
CODEC=${3:-deep}
JOBS=${4:-$(nproc)}

here=$(cd "$(dirname "$0")" && pwd)
# An explicit $QODEC must win outright — a broken override should fail
# loudly below, not fall back to whatever the repo happens to contain.
Q=${QODEC:-}
if [ -z "$Q" ]; then
  Q="$here/target/release/qodec.exe"                   # Windows build
  [ -x "$Q" ] || Q="$here/target/release/qodec"        # everyone else
fi

[ -f "$IN" ] || { echo "no such file: $IN" >&2; exit 1; }
[ -x "$Q" ] || { echo "no qodec binary (run: cargo build --release), or set \$QODEC" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

bytes=$(wc -c <"$IN")
split -b "$CHUNK" "$IN" "$TMP/c."
n=$(ls "$TMP"/c.* | wc -l)
echo "input : $IN  (${bytes} bytes)"
echo "split : ${n} chunks x ${CHUNK}   codec=${CODEC}   jobs=${JOBS}"
echo "encoding on ${JOBS} cores…"

# each chunk: token report -> stderr -> .rep file; encoded artifact discarded.
# On failure, surface the chunk's stderr now — the tmpdir is gone after EXIT.
printf '%s\n' "$TMP"/c.* | xargs -P "$JOBS" -I{} sh -c \
  '"$0" encode -i "$1" --codec "$2" --report >/dev/null 2>"$1.rep" \
     || { echo "FAIL $1:" >&2; cat "$1.rep" >&2; }' \
  "$Q" {} "$CODEC"

# Field-anchored POSIX awk (mawk/busybox fine — no gawk match(...,arr)).
# Report shape: qodec: IN -> COLD tokens (cold, X%), body-only WARM (warm, Y%),
# key overhead OVH [meter]. Anchors double as a format check: if the shape
# drifts or a chunk failed, k != n and the totals refuse instead of lying.
awk -v n="$n" '
  $1 == "qodec:" && $3 == "->" && $5 == "tokens" && $8 == "body-only" {
    tin += $2; cold += $4; warm += $9; ovh += $14; k++
  }
  END {
    if (k != n) {
      printf "parsed %d of %d chunk reports — failed chunks or a changed report format; totals would lie, refusing\n", k, n > "/dev/stderr"
      exit 1
    }
    printf "\n--- totals over %d chunks (o200k proxy tokens) ---\n", k
    printf "tokens in    : %d\n", tin
    printf "tokens cold  : %d  (%+.1f%%)   legend travels in-message\n", cold, (cold-tin)*100.0/tin
    printf "tokens warm  : %d  (%+.1f%%)   legend amortized in cached prefix\n", warm, (warm-tin)*100.0/tin
    printf "key overhead : %d tokens total\n", ovh
    printf "SAVED        : cold %d   warm %d   tokens\n", tin-cold, tin-warm
  }
' "$TMP"/c.*.rep
