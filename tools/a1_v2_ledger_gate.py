#!/usr/bin/env python3
"""A1-F v2 wire realization gate.

Checks the V2 WIRE REALIZATION LEDGER against the frozen graph of FD-v2-GRAPH,
in the order frozen by Phase G 4.2.6:

    1. event-kind universe equality
    2. exact 11/10 payload presence map
    3. recursive ArtifactRef extraction
    4. forward carrier coverage
    5. reverse carrier coverage

Fails closed. An undeclared left-hand side is a FAILURE, never a pass: the gate
exists to catch a v2 schema that grows a field or a kind nobody declared, and a
check that is silent on absence cannot catch that. Until Envelope v2 declares
anything, every step below is expected to report OWED, and the exit code is 1.

Usage:  python3 tools/a1_v2_ledger_gate.py [--graph PATH] [--ledger PATH]
Exit:   0 = every step passed;  1 = at least one step failed or is owed.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
GRAPH = REPO / "docs/tasks/a1-f-v2-graph.json"
LEDGER = REPO / "docs/tasks/a1-f-v2-realization-ledger.json"


class Report:
    def __init__(self) -> None:
        self.failed = False
        self.lines: list[str] = []

    def step(self, n: int, name: str, ok: bool, detail: str, owed: bool = False) -> None:
        status = "PASS" if ok else ("OWED" if owed else "FAIL")
        if not ok:
            self.failed = True
        self.lines.append(f"  {n}. {name:<34} {status}   {detail}")

    def emit(self) -> int:
        print("A1-F v2 wire realization gate")
        print("=" * 72)
        for line in self.lines:
            print(line)
        print("=" * 72)
        print("RESULT:", "FAIL — Envelope v2 is not realized" if self.failed else "PASS")
        return 1 if self.failed else 0


def edge_key(e: dict) -> tuple[str, str]:
    return (e["source"], e["target"])


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--graph", default=str(GRAPH))
    ap.add_argument("--ledger", default=str(LEDGER))
    args = ap.parse_args()

    graph = json.loads(Path(args.graph).read_text())
    ledger = json.loads(Path(args.ledger).read_text())
    r = Report()

    frozen_kinds = set(graph["event_kinds"])
    expected_payload = graph["expected_payload"]
    edges = graph["edges"]
    meta = graph["meta_targets"]
    carriers = ledger.get("carriers", [])

    # ---- 1. event-kind universe equality -------------------------------------
    declared_kinds = set(ledger.get("event_kinds") or [])
    if not declared_kinds:
        r.step(1, "event-kind universe equality", False,
               f"v2 declares no event kinds; frozen set has {len(frozen_kinds)}", owed=True)
    else:
        added = sorted(declared_kinds - frozen_kinds)
        dropped = sorted(frozen_kinds - declared_kinds)
        ok = not added and not dropped
        detail = f"{len(declared_kinds)} declared == {len(frozen_kinds)} frozen"
        if not ok:
            detail = f"added={added} dropped={dropped} -> reopen/supersede Phase G"
        r.step(1, "event-kind universe equality", ok, detail)

    # ---- 2. exact payload presence map ---------------------------------------
    # A total function over ALL frozen event kinds: exactly one structural
    # carrier for each payload-bearing kind, exactly zero for the others.
    struct = {}
    for c in carriers:
        if c.get("carrier_kind") == "event_payload_digest":
            struct.setdefault(c["source"], []).append(c)
    bad: list[str] = []
    for kind in sorted(frozen_kinds):
        want = expected_payload[kind]
        got = struct.get(f"CampaignEvent({kind})", [])
        if want is None:
            if got:
                bad.append(f"{kind}: expected 0 carriers, found {len(got)}")
        else:
            if len(got) != 1:
                bad.append(f"{kind}: expected exactly 1 carrier -> {want}, found {len(got)}")
            elif got[0]["target"] != want:
                bad.append(f"{kind}: carrier targets {got[0]['target']}, expected {want}")
    n_bearing = sum(1 for v in expected_payload.values() if v)
    n_free = sum(1 for v in expected_payload.values() if v is None)
    if not carriers:
        r.step(2, "payload presence map", False,
               f"no carriers declared; map requires {n_bearing} one / {n_free} zero", owed=True)
    else:
        r.step(2, "payload presence map", not bad,
               f"{n_bearing} one / {n_free} zero" if not bad else "; ".join(bad[:3]))

    # ---- 3. recursive ArtifactRef extraction ---------------------------------
    declared_fields = set(ledger.get("artifact_ref_fields") or [])
    ref_paths = {c["path"] for c in carriers if c.get("carrier_kind") == "artifact_ref"}
    if not declared_fields and not ref_paths:
        r.step(3, "recursive ArtifactRef extraction", False,
               "no v2 schema fields extracted and no artifact_ref carriers", owed=True)
    else:
        missing = sorted(declared_fields - ref_paths)   # a schema field nobody declared a carrier for
        extra = sorted(ref_paths - declared_fields)     # a carrier for a field the extractor never saw
        ok = not missing and not extra
        detail = f"{len(declared_fields)} fields == {len(ref_paths)} carriers"
        if not ok:
            detail = f"unledgered fields={missing[:3]} phantom carriers={extra[:3]}"
        r.step(3, "recursive ArtifactRef extraction", ok, detail)

    # ---- 4. forward carrier coverage -----------------------------------------
    admitted = {edge_key(e) for e in edges}
    for name, members in meta.items():
        for m in members:
            admitted.add((name, m))  # meta-target expansion, declared once
    unadmitted = [f'{c["source"]} -> {c["target"]}' for c in carriers
                  if edge_key(c) not in admitted]
    if not carriers:
        r.step(4, "forward carrier coverage", False, "no carriers declared", owed=True)
    else:
        r.step(4, "forward carrier coverage", not unadmitted,
               f"{len(carriers)} carriers, all admitted" if not unadmitted
               else f"unlisted relations: {unadmitted[:3]} -> reopen/supersede Phase G")

    # ---- 5. reverse carrier coverage -----------------------------------------
    carried = {edge_key(c) for c in carriers}
    uncovered = [f'{e["source"]} -> {e["target"]}' for e in edges if edge_key(e) not in carried]
    if uncovered:
        r.step(5, "reverse carrier coverage", False,
               f"{len(uncovered)}/{len(edges)} admitted edges have no wire carrier",
               owed=not carriers)
    else:
        r.step(5, "reverse carrier coverage", True, f"all {len(edges)} edges carried")

    return r.emit()


if __name__ == "__main__":
    sys.exit(main())
