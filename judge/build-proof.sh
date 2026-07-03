#!/usr/bin/env bash
# 007 judge proof-kit (Phase 1): fill the prompt template for ONE file's findings
# and print the exact `claude -p` command. Run this BEFORE building `o7 judge`.
# Needs: jq, awk, bash (all in the nix devShell). Run from the 007 repo root.
set -euo pipefail

SRC_ROOT=""; FINDINGS=""; FILE=""
RUBRIC="judge/rubric.example.md"; TEMPLATE="judge/prompt.template.md"
while [ $# -gt 0 ]; do
  case "$1" in
    --src-root) SRC_ROOT="$2"; shift 2 ;;
    --findings) FINDINGS="$2"; shift 2 ;;
    --file)     FILE="$2";     shift 2 ;;
    --rubric)   RUBRIC="$2";   shift 2 ;;
    --template) TEMPLATE="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done
[ -n "$SRC_ROOT" ] && [ -n "$FINDINGS" ] && [ -n "$FILE" ] || {
  echo "usage: bash judge/build-proof.sh --src-root <dir> --findings <findings.json> --file <relpath> [--rubric md] [--template md]" >&2
  exit 2
}

srcfile="$SRC_ROOT/$FILE"
[ -f "$srcfile" ] || {
  echo "source not found: $srcfile" >&2
  echo "  (STS/Broker sources may not be local — prove on OwnAudit oracle/LeakyOracle first)" >&2
  exit 1
}

tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

# findings for exactly this file (own-check shape: {findings:[{path,line,rule,category_name,message}]})
jq --arg p "$FILE" \
  '[.findings[] | select(.path==$p) | {path,line,rule,category_name,message}]' \
  "$FINDINGS" > "$tmp/fif.json"
n="$(jq 'length' "$tmp/fif.json")"
[ "$n" -gt 0 ] || { echo "no findings for '$FILE' in $FINDINGS (check --file matches a finding .path)" >&2; exit 1; }
echo "[proof] $FILE — $n finding(s)" >&2

# Fill the template. Big multiline slots are whole lines -> awk swaps them safely.
out="judge/proof.$(echo "$FILE" | tr '/\\.' '___').prompt.md"
awk -v path="$FILE" -v rubric="$RUBRIC" -v src="$srcfile" -v fif="$tmp/fif.json" '
  $0=="{{RUBRIC}}"           { while ((getline l < rubric)>0) print l; close(rubric); next }
  $0=="{{FILE_CONTENT}}"     { while ((getline l < src)>0)    print l; close(src);    next }
  $0=="{{FINDINGS_IN_FILE}}" { while ((getline l < fif)>0)    print l; close(fif);    next }
  { gsub(/\{\{FILE_PATH\}\}/, path); print }
' "$TEMPLATE" > "$out"

echo "[proof] filled prompt -> $out" >&2
echo >&2
echo "Run it (whole file is in the prompt; it needs no tools — deny anything it asks):" >&2
echo "  claude -p \"\$(cat '$out')\" --model opus" >&2
