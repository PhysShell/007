#!/usr/bin/env bash
# qodec guided tour — run from qodec/: `./demo.sh [your-file]`
# Builds the release binary if needed, then walks every lab surface on the
# bundled corpus. Pass a file of your own as $1 to see it encoded too.
set -euo pipefail
cd "$(dirname "$0")"

Q=./target/release/qodec
if [ ! -x "$Q" ]; then
  echo "==> building (first run only)…"
  cargo build --release
fi

hr() { printf '\n\e[1m== %s ==\e[0m\n\n' "$1"; }

hr "1/6 what do aliases cost under the tokenizer? (probed, not assumed)"
$Q aliases --meter o200k --top 12

hr "2/6 encode a stack trace — legend on top is the 'decryption key'"
$Q encode --codec deep -i corpus/stacktrace.txt --report | head -20
echo "…"

hr "3/6 losslessness — decode(encode(x)) is byte-identical"
$Q encode --codec deep -i corpus/stacktrace.txt | $Q decode | diff - corpus/stacktrace.txt \
  && echo "OK: byte-identical roundtrip"

hr "4/6 the honest fallback — unique prose refuses to pretend"
$Q encode --codec deep -i corpus/prose.md --report >/dev/null

hr "5/6 full bench: every codec × every sample, roundtrip-verified"
$Q bench --corpus corpus --meter o200k

hr "6/6 comprehension probe — paste this into a Claude/ChatGPT tab and ask questions"
PROBE=$(mktemp -t qodec-probe.XXXX.txt)
$Q probe -i corpus/build-log.txt --codec deep > "$PROBE"
echo "probe artifact written to: $PROBE"

if [ $# -ge 1 ] && [ -f "$1" ]; then
  hr "bonus: your file — $1"
  $Q encode --codec deep -i "$1" --report >/dev/null
  echo "(add | \$PAGER after 'encode' without --report redirect to see the artifact)"
fi

cat <<'EOF'

── try it on your own data ──────────────────────────────────────────────
  git diff | ./target/release/qodec encode --codec deep --report | less
  rg "PropertyChanged" ~/src/big-repo | ./target/release/qodec encode --codec deep --report >/dev/null
  ./target/release/qodec bench --corpus /path/to/dir/of/text/files
  ./target/release/qodec probe -i huge.log --codec deep > probe.txt   # paste into a chat

  cold Δ = savings when the legend travels in-message
  warm Δ = savings when the legend lives in a cached prompt prefix
  result 'raw' = the codec refused: artifact would not beat the original
EOF
