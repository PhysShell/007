#!/usr/bin/env python3
"""project_case.py — task-dependent projection only, for every task in a fixture.

    python3 research/b1-context/tools/project_case.py \
      --fixture case-0001 --out research/b1-context/results/case-0001-v0

Why this exists separately from `run_case.py`
---------------------------------------------
Projection is a pure function of

    gold state + task + selector contract/version + budget

and needs NO private data: the gold state is committed, and nothing here reads a
RAW blob. The 3-arm evaluation is different — it needs the derived transcripts
and the sealed negative control out of the owner's local CAS, so `run_case.py`
cannot run anywhere else.

The task set comes from the single committed registry
(`fixtures/<fixture>/tasks-v0.yaml`) via `o7b1.registry`; there is no hardcoded
task list here or in `run_case.py`. The projection itself is produced by the
shared `o7b1.pipeline`, so the artifacts this writes are byte-identical to the
ones `run_case.py` writes. `projection-comparison.json` records the
task-dependence result — including the generic acceptance block — as data.

This tool is READ-ONLY over the fixture and writes only under --out.
"""
from __future__ import annotations

import argparse
import json
import os
import sys

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _HERE)

from o7b1 import pipeline as pl
from o7b1 import registry as rg
from o7b1 import selector as sl
from o7b1.canonical import canonical_file_bytes, sha256_hex, sha256_of_file
from o7b1.schema import default_schema_path, validate_gold_state

BYTE_BUDGET = 20000
RECORD_BUDGET = 32


def _fixture_dir(fixture: str) -> str:
    return os.path.normpath(os.path.join(_HERE, "..", "fixtures", fixture))


def run(fixture: str, out_dir: str) -> dict:
    fdir = _fixture_dir(fixture)
    schema_path = default_schema_path()
    schema_digest = "sha256:" + sha256_of_file(schema_path)
    with open(os.path.join(fdir, "gold-state-v0.json"), encoding="utf-8") as fh:
        gold = json.load(fh)
    validate_gold_state(gold, schema_path)

    budget = {"byte_budget": BYTE_BUDGET, "record_budget": RECORD_BUDGET,
              "unit": "utf8_bytes+records"}
    proj = pl.run_projection(fixture, fdir, gold, schema_digest, out_dir, budget)

    data = canonical_file_bytes(proj["comparison"])
    path = os.path.join(out_dir, "projection-comparison.json")
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)
    return {"comparison": proj["comparison"],
            "projection_comparison_digest": "sha256:" + sha256_hex(data),
            "out_dir": out_dir}


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="Project every task of a fixture (offline, no CAS required).")
    ap.add_argument("--fixture", default="case-0001")
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)
    try:
        res = run(args.fixture, args.out)
    except (pl.PipelineError, rg.RegistryError, sl.SelectorError) as e:
        sys.stderr.write("FAIL CLOSED: %s\n" % e)
        return 2
    td = res["comparison"]["task_dependence"]
    for t in res["comparison"]["tasks"]:
        sys.stderr.write("%s: %d selected, %d omitted, context.json %s\n"
                         % (t["task_id"], t["selected_count"], t["omitted_count"],
                            t["artifact_digests"]["context.json"]))
    sys.stderr.write("projections differ: %s (symmetric difference %d); accepted: %s\n"
                     % (td["projections_differ"], td["symmetric_difference_count"],
                        td["acceptance"]["accepted"]))
    sys.stderr.write("projection-comparison.json: %s\n"
                     % res["projection_comparison_digest"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
