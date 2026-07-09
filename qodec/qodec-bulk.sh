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

IN=${1:?usage: qodec-bulk.sh <file> [chunk] [codec] [jobs]}
CHUNK=${2:-150k}
CODEC=${3:-deep}
JOBS=${4:-$(nproc)}

here=$(cd "$(dirname "$0")" && pwd)
Q=${QODEC:-"$here/target/release/qodec.exe"}
[ -x "$Q" ] || Q="$here/target/release/qodec"          # non-Windows build

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

# each chunk: token report -> stderr -> .rep file; encoded artifact discarded
printf '%s\n' "$TMP"/c.* | xargs -P "$JOBS" -I{} sh -c \
  '"$0" encode -i "$1" --codec "$2" --report >/dev/null 2>"$1.rep" || echo "FAIL $1" >&2' \
  "$Q" {} "$CODEC"

awk '
  match($0, /: ([0-9]+) -> ([0-9]+) tokens.*body-only ([0-9]+) \(warm.*overhead ([0-9]+)/, m) {
    tin += m[1]; cold += m[2]; warm += m[3]; ovh += m[4]; k++
  }
  END {
    if (tin == 0) { print "no reports parsed" > "/dev/stderr"; exit 1 }
    printf "\n--- totals over %d chunks (o200k proxy tokens) ---\n", k
    printf "tokens in    : %d\n", tin
    printf "tokens cold  : %d  (%+.1f%%)   legend travels in-message\n", cold, (cold-tin)*100.0/tin
    printf "tokens warm  : %d  (%+.1f%%)   legend amortized in cached prefix\n", warm, (warm-tin)*100.0/tin
    printf "key overhead : %d tokens total\n", ovh
    printf "SAVED        : cold %d   warm %d   tokens\n", tin-cold, tin-warm
  }
' "$TMP"/c.*.rep
