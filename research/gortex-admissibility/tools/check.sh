#!/usr/bin/env bash
# Self-check for the fixture corpus. Validates the instrument, not any tool
# under test: no code-graph engine is invoked and none needs to be installed.
#
#   1. every oracle.yaml parses and matches oracle-v0's enums
#   2. every path referenced by a seed or an edge exists
#   3. every cited line number still points at the code it names
#   4. every TypeScript case type-checks under --strict
#
# Run from the corpus root:  tools/check.sh
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - <<'PY'
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

TSC="$(command -v tsc || true)"
if [ -z "$TSC" ]; then
  echo "  typecheck: SKIPPED (no tsc on PATH)"
  exit 0
fi

FLAGS=(--noEmit --strict --target es2020 --module esnext --moduleResolution bundler)
for c in case-0001-sibling-same-name case-0002-alias-reexport \
         case-0003-interface-dispatch case-0005-overload-set \
         case-0006-generated-members; do
  mapfile -t files < <(find "fixtures/$c/source" -name '*.ts')
  "$TSC" "${FLAGS[@]}" "${files[@]}"
done

# case-0004 is compiled once per repository root on purpose: a single project
# spanning both trees reproduces the cross-repo merge the case tests for, and
# would certify it as correct.
for r in repo-a repo-b; do
  mapfile -t files < <(find "fixtures/case-0004-cross-repo-same-name/source/$r" -name '*.ts')
  "$TSC" "${FLAGS[@]}" "${files[@]}"
done

echo "  typecheck: all TypeScript cases clean under --strict"
