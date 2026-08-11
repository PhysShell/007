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
                                         AND exactly-one / exactly-zero
                                         structural carriers per event kind
    3. per-field ArtifactRef capability  each field's source and complete
                                         target domain == its carriers'
    4. forward carrier coverage          every carrier is graph-admitted
    5. reverse carrier coverage          every graph edge has >= 1 carrier

Edge identity is (source, target, class). Class is part of the identity, not
decoration: the frozen pairs happen to be distinct, so a mis-classed carrier
would not collide with anything - it would simply drop out of the proof.

Fails closed. An undeclared side is a FAILURE, never a pass.

Both non-ledger terms are re-derived by a preflight bound to the very path the
gate then reads, so an override cannot be checked against one artifact and read
from another. There is no --skip-preflight flag; the corpus uses an env var.

Usage:  python3 tools/a1_v2_ledger_gate.py [--graph P] [--schema P] [--ledger P]
Exit:   0 = every step passed;  1 = at least one step failed or is owed.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
GRAPH = REPO / "docs/tasks/a1-f-v2-graph.json"
SCHEMA = REPO / "docs/tasks/a1-f-v2-schema-facts.json"
LEDGER = REPO / "docs/tasks/a1-f-v2-realization-ledger.json"
GRAPH_EXTRACTOR = REPO / "tools/a1_v2_extract_graph.py"
SCHEMA_EXTRACTOR = REPO / "tools/a1_v2_extract_schema.py"


def _harness() -> bool:
    """Test-harness escape, deliberately NOT a command-line flag.

    A documented --skip-preflight is an invitation: in six months somebody
    adds it to CI to make the checks faster, which is the traditional way of
    optimising a check by deleting it. The corpus sets this env var; nothing
    a human types at a prompt does.
    """
    return os.environ.get("A1_V2_GATE_HARNESS") == "1"

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


def edge_keys(graph: dict) -> set[Key]:
    """The frozen edges, EXACTLY as written.

    Phase G 4.2.6 clause 5: a meta-target is declared ONCE as a meta-target
    expansion, never as eleven separate edges. So a carrier realizing row 69
    carries the relation `CampaignFeedItem -Causal-> AnyCommittedEnvelope`
    itself; it does not decompose into eleven member carriers, and a carrier to
    a single member is NOT that relation. Expansion belongs one level down, in
    step 3, where the field's permitted target domain is compared against the
    union's members.
    """
    return {(e["source"], e["target"], e["class"]) for e in graph["edges"]}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--graph", default=str(GRAPH))
    ap.add_argument("--schema", default=str(SCHEMA))
    ap.add_argument("--ledger", default=str(LEDGER))
    args = ap.parse_args()

    r = Report()

    # ---- preflight: both non-ledger terms must be DERIVABLE, not merely present.
    # Each --check is bound to the very path the gate then reads. Checking the
    # default artifact and then reading an overridden one would re-open exactly
    # the hole the preflight exists to close.
    if not _harness():
        for label, tool, out, ok_msg in (
            ("preflight: graph extract", GRAPH_EXTRACTOR, args.graph,
             "committed graph == derived from Phase G"),
            ("preflight: schema extract", SCHEMA_EXTRACTOR, args.schema,
             "committed facts == derived from the v2 schema source"),
        ):
            p = subprocess.run([sys.executable, str(tool), "--check", "--out", out],
                               capture_output=True, text=True)
            r.add(label, p.returncode == 0,
                  ok_msg if p.returncode == 0
                  else (p.stderr.strip().split("\n")[0] or "mismatch"))
            if p.returncode != 0:
                # Every step below would be measured against a forged ruler.
                return r.emit()

    graph = json.loads(Path(args.graph).read_text())
    schema = json.loads(Path(args.schema).read_text())
    ledger = json.loads(Path(args.ledger).read_text())

    carriers = ledger.get("carriers", [])
    frozen_kinds = set(graph["event_kinds"])
    meta = graph["meta_targets"]
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
        # EXACT CARDINALITY, not set membership. Frozen G-R11: exactly one
        # carrier for each of the eleven payload-bearing kinds, exactly zero for
        # the other ten. Collapsing carriers into a set would let two identical
        # structural rows for one event kind read as one — a hole in precisely
        # the exact-count contract G-R11 and G-R12 exist to enforce.
        struct_count: dict[str, int] = {}
        for c in carriers:
            if c.get("carrier_kind") == "event_payload_digest":
                struct_count[c["source"]] = struct_count.get(c["source"], 0) + 1
        schema_struct = {f"CampaignEvent({k})" for k in schema.get("structural_commitments") or []}
        for src in sorted(set(struct_count) | schema_struct):
            want = 1 if src in schema_struct else 0
            got = struct_count.get(src, 0)
            if got != want:
                bad.append(f"{src}: {got} structural carriers, expected exactly {want}")
        r.add("2. payload presence map", not bad,
              f"{n_bearing} one / {n_free} zero, structural carrier counts exact" if not bad
              else "; ".join(bad[:3]))

    # ---- 3. recursive ArtifactRef extraction, per-field capability -----------
    # Phase G 4.2.6 clause 1: a field declares the COMPLETE set of edges it may
    # realize. A bare path list cannot express that — two fields can swap their
    # targets and every global triple still appears somewhere, so steps 4 and 5
    # both pass while each field realizes the wrong relation.
    schema_fields = {f["path"]: f for f in (schema.get("artifact_ref_fields") or [])}
    by_path: dict[str, list[dict]] = {}
    for c in carriers:
        if c.get("carrier_kind") == "artifact_ref":
            by_path.setdefault(c["path"], []).append(c)
    if not drafted:
        r.add("3. per-field ArtifactRef capability", False,
              "no v2 schema extracted; nothing to extract fields from", owed=True)
    else:
        bad = []
        for path in sorted(set(schema_fields) | set(by_path)):
            f, cs = schema_fields.get(path), by_path.get(path, [])
            if f is None:
                bad.append(f"{path}: carrier for a field the extractor never saw")
                continue
            if not cs:
                bad.append(f"{path}: schema field with no carrier")
                continue
            wrong_src = {c["source"] for c in cs} - {f["source_semantic_kind"]}
            if wrong_src:
                bad.append(f"{path}: carrier source {sorted(wrong_src)} != schema "
                           f"{f['source_semantic_kind']}")
            declared: set[str] = set()
            for c in cs:
                declared |= set(meta.get(c["target"], [c["target"]]))
            allowed = set(f.get("allowed_concrete_target_kinds") or [])
            if declared != allowed:
                bad.append(f"{path}: target domain {sorted(declared)[:3]} != schema "
                           f"{sorted(allowed)[:3]}")
        r.add("3. per-field ArtifactRef capability", not bad,
              f"{len(schema_fields)} fields, source and target domain agree" if not bad
              else "; ".join(bad[:2]))

    # ---- 4. forward carrier coverage -----------------------------------------
    admitted = edge_keys(graph)
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
    carried = {(c["source"], c["target"], c.get("class")) for c in carriers}
    uncovered = [f"{src} -{cls}-> {tgt}" for src, tgt, cls in sorted(edge_keys(graph))
                 if (src, tgt, cls) not in carried]
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
