#!/usr/bin/env python3
"""A1-F v2 wire realization gate.

A THREE-WAY proof, not a self-consistency check:

    FROZEN GRAPH  (graph.json, regenerated from Phase G by the preflight)
          |
          v
    SCHEMA FACTS  (schema-facts.json, extracted from the ACTUAL v2 schemas)
          |
          v
    REALIZATION   (realization-ledger.json, the declared carriers)

The five steps run in the order frozen by Phase G 4.2.6:

    1. event-kind universe equality      schema.event_kinds == graph.event_kinds
    2. payload presence map              schema.payload_presence == graph map,
                                         AND structural carriers == schema
    3. recursive ArtifactRef extraction  schema.artifact_ref_paths == carriers
    4. forward carrier coverage          every carrier is graph-admitted
    5. reverse carrier coverage          every graph edge has >= 1 carrier

Edge identity is (source, target, class). Class is part of the identity, not
decoration: the frozen pairs happen to be distinct, so a mis-classed carrier
would not collide with anything - it would simply drop out of the proof.

Fails closed. An undeclared side is a FAILURE, never a pass.

Usage:  python3 tools/a1_v2_ledger_gate.py [--graph P] [--schema P] [--ledger P]
                                           [--skip-preflight]
Exit:   0 = every step passed;  1 = at least one step failed or is owed.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
GRAPH = REPO / "docs/tasks/a1-f-v2-graph.json"
SCHEMA = REPO / "docs/tasks/a1-f-v2-schema-facts.json"
LEDGER = REPO / "docs/tasks/a1-f-v2-realization-ledger.json"
EXTRACTOR = REPO / "tools/a1_v2_extract_graph.py"

Key = tuple[str, str, str]


class Report:
    def __init__(self) -> None:
        self.failed = False
        self.lines: list[str] = []

    def add(self, label: str, ok: bool, detail: str, owed: bool = False) -> None:
        if not ok:
            self.failed = True
        status = "PASS" if ok else ("OWED" if owed else "FAIL")
        self.lines.append(f"  {label:<37} {status}   {detail}")

    def emit(self) -> int:
        print("A1-F v2 wire realization gate")
        print("=" * 76)
        for line in self.lines:
            print(line)
        print("=" * 76)
        print("RESULT:", "FAIL — Envelope v2 is not realized" if self.failed else "PASS")
        return 1 if self.failed else 0


def admitted_keys(graph: dict) -> set[Key]:
    """Edges admitted at carrier level, with meta-targets expanded.

    A meta-target is a union in TARGET position. Expanding edge
    (S -> M, class) yields (S -> member, class) for each member. It never
    yields (M -> member): the meta-target is not a source and has no
    out-edges (the extractor asserts this).
    """
    meta = graph["meta_targets"]
    out: set[Key] = set()
    for e in graph["edges"]:
        src, tgt, cls = e["source"], e["target"], e["class"]
        if tgt in meta:
            for member in meta[tgt]:
                out.add((src, member, cls))
        else:
            out.add((src, tgt, cls))
    return out


def required_keys(graph: dict) -> set[Key]:
    """Edges that must each have >= 1 carrier.

    A meta-target edge is satisfied by a carrier to ANY of its members, so it
    is required as the un-expanded relation and checked against the expansion.
    """
    return {(e["source"], e["target"], e["class"]) for e in graph["edges"]}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--graph", default=str(GRAPH))
    ap.add_argument("--schema", default=str(SCHEMA))
    ap.add_argument("--ledger", default=str(LEDGER))
    ap.add_argument("--skip-preflight", action="store_true")
    args = ap.parse_args()

    r = Report()

    # ---- preflight: the frozen side must be derivable, not merely present ----
    if args.skip_preflight:
        r.add("preflight: graph extract", True, "skipped (test harness)")
    else:
        p = subprocess.run([sys.executable, str(EXTRACTOR), "--check"],
                           capture_output=True, text=True)
        r.add("preflight: graph extract", p.returncode == 0,
              "committed graph == derived from Phase G"
              if p.returncode == 0 else (p.stderr.strip().split("\n")[0] or "mismatch"))
        if p.returncode != 0:
            return r.emit()  # every step below would be measured against a forged ruler

    graph = json.loads(Path(args.graph).read_text())
    schema = json.loads(Path(args.schema).read_text())
    ledger = json.loads(Path(args.ledger).read_text())

    carriers = ledger.get("carriers", [])
    frozen_kinds = set(graph["event_kinds"])
    expected_payload = graph["expected_payload"]
    drafted = bool(schema.get("extracted_from"))

    # ---- 1. event-kind universe equality -------------------------------------
    schema_kinds = set(schema.get("event_kinds") or [])
    if not drafted:
        r.add("1. event-kind universe equality", False,
              f"no v2 schema extracted; frozen set has {len(frozen_kinds)}", owed=True)
    else:
        added = sorted(schema_kinds - frozen_kinds)
        dropped = sorted(frozen_kinds - schema_kinds)
        ok = not added and not dropped
        r.add("1. event-kind universe equality", ok,
              f"{len(schema_kinds)} schema == {len(frozen_kinds)} frozen" if ok
              else f"added={added} dropped={dropped} -> reopen/supersede Phase G")

    # ---- 2. payload presence map ---------------------------------------------
    # Two halves. The first compares the SCHEMA to the frozen map, so a schema
    # that grows event_payload_digest on a payload-free kind fails even when the
    # drafter never wrote a ledger row for it. The second ties declared
    # structural carriers to the schema's own commitments.
    n_bearing = sum(1 for v in expected_payload.values() if v)
    n_free = len(expected_payload) - n_bearing
    if not drafted:
        r.add("2. payload presence map", False,
              f"no v2 schema extracted; map requires {n_bearing} one / {n_free} zero",
              owed=True)
    else:
        presence = schema.get("payload_presence") or {}
        bad = []
        for kind in sorted(frozen_kinds | set(presence)):
            want = expected_payload.get(kind, "<kind not frozen>")
            got = presence.get(kind, "<absent from schema>")
            if want != got:
                bad.append(f"{kind}: schema={got!r} frozen={want!r}")
        declared_struct = {c["source"] for c in carriers
                           if c.get("carrier_kind") == "event_payload_digest"}
        schema_struct = {f"CampaignEvent({k})" for k in schema.get("structural_commitments") or []}
        if declared_struct != schema_struct:
            miss = sorted(schema_struct - declared_struct)
            extra = sorted(declared_struct - schema_struct)
            bad.append(f"structural carriers vs schema: unledgered={miss[:2]} phantom={extra[:2]}")
        r.add("2. payload presence map", not bad,
              f"{n_bearing} one / {n_free} zero, structural carriers agree" if not bad
              else "; ".join(bad[:3]))

    # ---- 3. recursive ArtifactRef extraction ---------------------------------
    schema_paths = set(schema.get("artifact_ref_paths") or [])
    carrier_paths = {c["path"] for c in carriers if c.get("carrier_kind") == "artifact_ref"}
    if not drafted:
        r.add("3. recursive ArtifactRef extraction", False,
              "no v2 schema extracted; nothing to extract paths from", owed=True)
    else:
        missing = sorted(schema_paths - carrier_paths)
        phantom = sorted(carrier_paths - schema_paths)
        ok = not missing and not phantom
        r.add("3. recursive ArtifactRef extraction", ok,
              f"{len(schema_paths)} schema fields == {len(carrier_paths)} carrier paths" if ok
              else f"unledgered fields={missing[:2]} phantom carriers={phantom[:2]}")

    # ---- 4. forward carrier coverage -----------------------------------------
    admitted = admitted_keys(graph)
    unadmitted = [f'{c["source"]} -{c.get("class")}-> {c["target"]}'
                  for c in carriers
                  if (c["source"], c["target"], c.get("class")) not in admitted]
    if not carriers:
        r.add("4. forward carrier coverage", False, "no carriers declared", owed=True)
    else:
        r.add("4. forward carrier coverage", not unadmitted,
              f"{len(carriers)} carriers, all admitted (source, target, class)" if not unadmitted
              else f"unlisted relations: {unadmitted[:2]} -> reopen/supersede Phase G")

    # ---- 5. reverse carrier coverage -----------------------------------------
    meta = graph["meta_targets"]
    carried = {(c["source"], c["target"], c.get("class")) for c in carriers}
    uncovered = []
    for src, tgt, cls in sorted(required_keys(graph)):
        if tgt in meta:
            if not any((src, m, cls) in carried for m in meta[tgt]):
                uncovered.append(f"{src} -{cls}-> {tgt}")
        elif (src, tgt, cls) not in carried:
            uncovered.append(f"{src} -{cls}-> {tgt}")
    total = len(graph["edges"])
    if uncovered:
        r.add("5. reverse carrier coverage", False,
              f"{len(uncovered)}/{total} admitted edges have no wire carrier",
              owed=not carriers)
    else:
        r.add("5. reverse carrier coverage", True, f"all {total} edges carried")

    return r.emit()


if __name__ == "__main__":
    sys.exit(main())
