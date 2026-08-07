#!/usr/bin/env bash
# Self-check for the fixture corpus. Validates the instrument, not any tool
# under test: no code-graph engine is invoked and none needs to be installed.
#
# Five checks, reported separately because they prove different things and a
# missing toolchain must never read as a pass:
#
#   A. oracle integrity   -- parses, enums, paths, line references
#   B. type validity      -- tsc --strict: the fixture is a valid program
#   C. relation oracle    -- the language service agrees with the claimed
#                            caller/reference/implementation sets
#   D. codegen stability  -- case-0006's generated file is what its contract
#                            generates
#   E. lifecycle package  -- lifecycle oracles are well-formed and every
#                            mutation state applies, asserts its postcondition,
#                            and resets
#
# B is NOT C. Type-checking proves the program compiles; it says nothing about
# whether the caller set is the one the oracle claims. An earlier revision of
# this corpus ran only B while its README claimed the constructed oracles had
# been validated against TypeScript. They had not been.
#
# Unavailable toolchain => UNAVAILABLE, never PASS.
#
# Run from the corpus root:  tools/check.sh
# Exit: 0 all available checks passed | 1 a check failed
set -uo pipefail
cd "$(dirname "$0")/.."

status_a="UNAVAILABLE"; status_b="UNAVAILABLE"
status_c="UNAVAILABLE"; status_d="UNAVAILABLE"; status_e="UNAVAILABLE"
rc=0

echo "[A] oracle integrity"
if python3 - <<'PY'
import json, pathlib, sys
import yaml

schema = json.loads(pathlib.Path("schema/oracle-v0.schema.json").read_text())
facts_props = schema["properties"]["facts"]["items"]["properties"]
allowed_rel = set(facts_props["relation"]["enum"])
allowed_cav = set(schema["$defs"]["admissibility"]["properties"]["required_caveats"]["items"]["enum"])

problems = []
for p in sorted(pathlib.Path("fixtures").glob("*/oracle.yaml")):
    base = p.parent
    d = yaml.safe_load(p.read_text())
    if d["case"] != base.name:
        problems.append(f"{p}: case '{d['case']}' != directory '{base.name}'")
    seeds = {s["id"] for s in d["seeds"]}

    def check_span(path, line, label, want=None):
        f = base / path
        if not f.exists():
            problems.append(f"{p}: {label}: missing file {path}")
            return None
        lines = f.read_text().splitlines()
        if line is not None and not (1 <= line <= len(lines)):
            problems.append(f"{p}: {label}: line {line} out of range in {path}")
            return None
        return lines

    for s in d["seeds"]:
        g = s["gold_identity"]
        a, b = g["line_range"]
        lines = check_span(g["path"], a, f"seed {s['id']}")
        if lines and not (1 <= a <= b <= len(lines)):
            problems.append(f"{p}: seed {s['id']}: bad line_range {a}..{b}")

    for i, f in enumerate(d["facts"]):
        if f["seed"] not in seeds:
            problems.append(f"{p}: fact {i}: unknown seed '{f['seed']}'")
        if f["relation"] not in allowed_rel:
            problems.append(f"{p}: fact {i}: relation '{f['relation']}' not in oracle-v0")
        for c in f["admissibility"].get("required_caveats", []):
            if c not in allowed_cav:
                problems.append(f"{p}: fact {i}: caveat '{c}' not in oracle-v0")
        for kind in ("expected", "forbidden"):
            for e in f[kind]:
                check_span(e["path"], e.get("line"), f"fact {i} {kind}")

    print(f"  {d['case']:38s} seeds={len(d['seeds'])} facts={len(d['facts'])} "
          f"oracle={d['oracle_kind']} independent={d.get('independent_oracle', {}).get('available')}")

if problems:
    print("\noracle problems:", file=sys.stderr)
    for x in problems:
        print(f"  {x}", file=sys.stderr)
    sys.exit(1)
print("\n  oracles: parse clean, paths and line references resolve")
PY
then status_a="PASS"; else status_a="FAIL"; rc=1; fi

TSC="$(command -v tsc || true)"
NODE="$(command -v node || true)"

echo
echo "[B] type validity (tsc --strict)"
if [ -z "$TSC" ]; then
  echo "  SKIPPED -- no tsc on PATH; the fixtures are UNVERIFIED as programs"
else
  FLAGS=(--noEmit --strict --target es2020 --module esnext --moduleResolution bundler)
  b_ok=0
  for c in case-0001-sibling-same-name case-0002-alias-reexport \
           case-0003-interface-dispatch case-0005-overload-set \
           case-0006-generated-members case-0101-index-lifecycle; do
    mapfile -t files < <(find "fixtures/$c/source" -name '*.ts')
    "$TSC" "${FLAGS[@]}" "${files[@]}" || b_ok=1
  done
  # case-0004 is compiled once per repository root on purpose: a single project
  # spanning both trees reproduces the cross-repo merge the case tests for, and
  # would certify it as correct.
  for r in repo-a repo-b; do
    mapfile -t files < <(find "fixtures/case-0004-cross-repo-same-name/source/$r" -name '*.ts')
    "$TSC" "${FLAGS[@]}" "${files[@]}" || b_ok=1
  done
  if [ $b_ok -eq 0 ]; then
    status_b="PASS"; echo "  all TypeScript cases compile under --strict"
  else
    status_b="FAIL"; rc=1
  fi
fi

echo
echo "[C] independent relation oracle (TypeScript language service)"
if [ -z "$NODE" ]; then
  echo "  SKIPPED -- no node on PATH"
  echo "  -> relation claims are UNPROVED. [B] passing does not cover this."
else
  "$NODE" tools/ts-reference-oracle.mjs
  case $? in
    0) status_c="PASS" ;;
    3) status_c="UNAVAILABLE" ;;
    *) status_c="FAIL"; rc=1 ;;
  esac
fi

echo
echo "[E] lifecycle oracles + mutation steps (package 2)"
if python3 - <<'PY2'
import json, pathlib, sys
import yaml

schema = json.loads(pathlib.Path("schema/lifecycle-v0.schema.json").read_text())
qprops = set(schema["$defs"]["questions"]["properties"])
problems = []
found = 0
for p in sorted(pathlib.Path("fixtures").glob("*/lifecycle.yaml")):
    base = p.parent
    d = yaml.safe_load(p.read_text())
    found += 1
    if d["case"] != base.name:
        problems.append(f"{p}: case '{d['case']}' != directory '{base.name}'")
    ids = [st["id"] for st in d["states"]]
    if d["observation"]["taken_at_state"] not in ids:
        problems.append(f"{p}: observation taken at unknown state")
    ent = d["entity"]
    f = base / ent["path"]
    if not f.exists():
        problems.append(f"{p}: entity file missing: {ent['path']}")
    else:
        lines = f.read_text().splitlines()
        lo, hi = ent["line_range"]
        if not (1 <= lo <= hi <= len(lines)):
            problems.append(f"{p}: entity line_range {lo}..{hi} out of range")
    # every edge cited anywhere must resolve AT THE STATE THAT DEFINES IT;
    # only the committed (fresh) state is on disk, so check that one only.
    fresh = d["observation"]["taken_at_state"]
    for e in d["observation"]["expected"]:
        ef = base / e["path"]
        if not ef.exists():
            problems.append(f"{p}: observation edge path missing: {e['path']}")
        elif "line" in e and e["line"] > len(ef.read_text().splitlines()):
            problems.append(f"{p}: observation edge line out of range: {e['path']}:{e['line']}")
    for st in d["states"]:
        missing_q = qprops - set(st["questions"]) - {"note"}
        req = set(schema["$defs"]["questions"]["required"])
        if not req <= set(st["questions"]):
            problems.append(f"{p}: state {st['id']}: missing {sorted(req - set(st['questions']))}")
        if st["id"] != fresh and st["questions"]["action_admissible"]:
            problems.append(f"{p}: state {st['id']}: action_admissible true after a mutation -- "
                            "that is the alarm condition, not an expectation")
    print(f"  {d['case']:30s} states={len(d['states'])} entity={d['entity']['symbol']}")

if not found:
    print("  no lifecycle fixtures found")
if problems:
    print("\nlifecycle problems:", file=sys.stderr)
    for x in problems:
        print(f"  {x}", file=sys.stderr)
    sys.exit(1)
PY2
then
  for c in fixtures/*/tools/mutate.py; do
    [ -e "$c" ] || continue
    python3 "$c" selfcheck || { status_e="FAIL"; rc=1; }
  done
  [ "${status_e:-}" = "FAIL" ] || status_e="PASS"
else
  status_e="FAIL"; rc=1
fi

echo
echo "[D] codegen stability (case-0006)"
if python3 fixtures/case-0006-generated-members/tools/codegen.py --check; then
  status_d="PASS"
else
  status_d="FAIL"; rc=1
fi

echo
echo "  [A] oracle integrity    $status_a"
echo "  [B] type validity       $status_b"
echo "  [C] relation oracle     $status_c"
echo "  [D] codegen stability   $status_d"
echo "  [E] lifecycle package   $status_e"
[ "$status_c" = "UNAVAILABLE" ] && echo "
  NOTE: [C] did not run. The corpus is syntactically sound and type-valid, but
  its caller/reference/implementation claims stand UNPROVED in this run."
exit $rc
