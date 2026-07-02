#!/usr/bin/env bash
# Regression proof for the sts-stub fixture. Default: free + deterministic —
# asserts dedup/grouping/call-count via --dry-run (no claude calls). With
# O7_PROOF_LIVE=1 it also runs the real classification and checks the class tally.
# Run from the 007 repo root.
set -euo pipefail

here="judge/fixtures/sts-stub"
rubric="judge/rubric.example.md"
bin="target/debug/o7"; [ -x "$bin" ] || bin="target/release/o7"
[ -x "$bin" ] || { echo "build first: cargo build" >&2; exit 2; }

echo "[proof] dry-run (deterministic, no claude calls)"
dry="$("$bin" judge --repo "$here" --findings "$here/findings.json" \
        --rubric "$rubric" --dry-run)"
echo "$dry"
grep -q "6 findings -> 6 unique ids across 4 files" <<<"$dry" \
  || { echo "FAIL: expected 6 findings -> 6 ids across 4 files" >&2; exit 1; }
grep -q "4 claude call(s)" <<<"$dry" \
  || { echo "FAIL: expected 4 claude call(s)" >&2; exit 1; }
echo "[proof] PASS (dedup keeps the collision pair distinct; grouping = 4 calls)"

[ "${O7_PROOF_LIVE:-0}" = "1" ] || { echo "[proof] set O7_PROOF_LIVE=1 for the live classification"; exit 0; }

echo "[proof] live run (spends claude calls)"
out="$(mktemp)"
live="$("$bin" judge --repo "$here" --findings "$here/findings.json" \
         --rubric "$rubric" --out "$out" 2>&1)"
echo "$live"
grep -q '"real": 4' <<<"$live" && grep -q '"false_positive": 2' <<<"$live" \
  || { echo "FAIL: expected {real:4, false_positive:2}" >&2; exit 1; }
n="$(python3 -c "import json,sys;print(len(json.load(open('$out'))['verdicts']))")"
[ "$n" = "6" ] || { echo "FAIL: expected 6 verdicts in overlay, got $n" >&2; exit 1; }
echo "[proof] PASS (6/6 verdicts; both collision ids survived)"
