#!/usr/bin/env python3
"""Mutation corpus for the A1-F v2 wire realization gate.

A required acceptance gate that is only ever observed failing proves nothing, and
prose describing a test run that once happened proves less. Each case below
constructs a synthetic three-way world, mutates exactly one thing, and asserts
which step catches it.

Run: python3 tools/tests/test_a1_v2_ledger_gate.py
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
GATE = REPO / "tools/a1_v2_ledger_gate.py"
GRAPH = json.loads((REPO / "docs/tasks/a1-f-v2-graph.json").read_text())

STRUCTURAL = "event_payload_digest"
REF = "artifact_ref"


def faithful() -> tuple[dict, dict]:
    """A schema + ledger that faithfully realize the frozen graph."""
    carriers, paths, struct = [], [], []
    for e in GRAPH["edges"]:
        src, tgt, cls = e["source"], e["target"], e["class"]
        targets = GRAPH["meta_targets"].get(tgt, [tgt])
        for concrete in targets:
            is_struct = src.startswith("CampaignEvent(") and concrete.endswith("Payload")
            path = f"synthetic::{e['n']}::{concrete}"
            carriers.append({"source": src, "target": concrete, "class": cls,
                             "carrier_kind": STRUCTURAL if is_struct else REF,
                             "path": path})
            if is_struct:
                struct.append(src[len("CampaignEvent("):-1])
            else:
                paths.append(path)
    schema = {
        "extracted_from": "synthetic://v2-schemas@test",
        "extractor": "synthetic",
        "event_kinds": list(GRAPH["event_kinds"]),
        "payload_presence": dict(GRAPH["expected_payload"]),
        "structural_commitments": sorted(set(struct)),
        "artifact_ref_paths": sorted(set(paths)),
    }
    return schema, {"carriers": carriers}


def run(schema: dict, ledger: dict) -> tuple[int, dict[str, str]]:
    with tempfile.TemporaryDirectory() as d:
        sp, lp = Path(d) / "s.json", Path(d) / "l.json"
        sp.write_text(json.dumps(schema))
        lp.write_text(json.dumps(ledger))
        p = subprocess.run(
            [sys.executable, str(GATE), "--schema", str(sp), "--ledger", str(lp),
             "--skip-preflight"],
            capture_output=True, text=True)
    steps = {}
    for line in p.stdout.split("\n"):
        s = line.strip()
        if s and s[0].isdigit() and "." in s[:3]:
            n = s.split(".")[0]
            steps[n] = "PASS" if " PASS " in line else ("OWED" if " OWED " in line else "FAIL")
    return p.returncode, steps


CASES: list[tuple[str, object, int, dict[str, str]]] = []


def case(name: str, expect_exit: int, expect: dict[str, str]):
    def deco(fn):
        CASES.append((name, fn, expect_exit, expect))
        return fn
    return deco


@case("faithful realization", 0, {"1": "PASS", "2": "PASS", "3": "PASS", "4": "PASS", "5": "PASS"})
def _faithful():
    return faithful()


@case("nothing declared at all", 1, {"1": "OWED", "2": "OWED", "3": "OWED", "4": "OWED", "5": "OWED"})
def _empty():
    return ({"extracted_from": None, "event_kinds": [], "payload_presence": {},
             "structural_commitments": [], "artifact_ref_paths": []}, {"carriers": []})


@case("schema grows a 22nd event kind", 1, {"1": "FAIL"})
def _extra_kind():
    s, l = faithful()
    s["event_kinds"] = s["event_kinds"] + ["CampaignQuietlyRenamed"]
    return s, l


@case("schema drops an event kind", 1, {"1": "FAIL"})
def _dropped_kind():
    s, l = faithful()
    s["event_kinds"] = [k for k in s["event_kinds"] if k != "TransitionRejected"]
    return s, l


@case("G-R12: schema grows a payload on a payload-free kind, NO ledger row",
      1, {"1": "PASS", "2": "FAIL"})
def _ghost_payload_no_row():
    # The original failure mode: the wire schema acquires event_payload_digest on
    # one of the ten payload-free kinds and the drafter simply omits a carrier.
    # A ledger-only check cannot see this - there is no offending row to inspect.
    s, l = faithful()
    s["payload_presence"]["WorkOrderIssued"] = "WorkOrderIssuedPayload"
    return s, l


@case("carrier for a relation the registry does not hold", 1, {"4": "FAIL"})
def _unlisted_relation():
    s, l = faithful()
    l["carriers"].append({"source": "CoderReport", "target": "ReviewVerdict",
                          "class": "Intra", "carrier_kind": REF, "path": "synthetic::rogue"})
    s["artifact_ref_paths"] = sorted(s["artifact_ref_paths"] + ["synthetic::rogue"])
    return s, l


@case("frozen Intra edge realized as Causal", 1, {"4": "FAIL", "5": "FAIL"})
def _wrong_class():
    # Class is part of edge identity. The frozen pairs are distinct, so a
    # mis-classed carrier collides with nothing - it silently leaves the proof.
    s, l = faithful()
    for c in l["carriers"]:
        if c["source"] == "ReviewVerdict" and c["target"] == "ReviewerReport":
            c["class"] = "Causal"
    return s, l


@case("one admitted edge left with no carrier", 1, {"5": "FAIL"})
def _missing_carrier():
    s, l = faithful()
    drop = next(c for c in l["carriers"]
                if c["source"] == "WorkOrder" and c["target"] == "ScopeContract")
    l["carriers"].remove(drop)
    s["artifact_ref_paths"] = [p for p in s["artifact_ref_paths"] if p != drop["path"]]
    return s, l


@case("schema field with no carrier declared", 1, {"3": "FAIL"})
def _unledgered_field():
    s, l = faithful()
    s["artifact_ref_paths"] = sorted(s["artifact_ref_paths"] + ["synthetic::undeclared_field"])
    return s, l


@case("carrier path the schema extractor never saw", 1, {"3": "FAIL"})
def _phantom_path():
    s, l = faithful()
    c = dict(l["carriers"][0])
    c["path"] = "synthetic::phantom"
    l["carriers"].append(c)
    return s, l


@case("meta-target realized via a member (must be admitted)", 0, {"4": "PASS", "5": "PASS"})
def _meta_ok():
    # CampaignFeedItem -Causal-> AnyCommittedEnvelope is satisfied by a carrier
    # to any member. faithful() already expands it to all eleven; keep one.
    s, l = faithful()
    keep = [c for c in l["carriers"]
            if not (c["source"] == "CampaignFeedItem" and c["class"] == "Causal")]
    one = next(c for c in l["carriers"]
               if c["source"] == "CampaignFeedItem" and c["target"] == "WorkOrder")
    l["carriers"] = keep + [one]
    s["artifact_ref_paths"] = sorted({c["path"] for c in l["carriers"]
                                      if c["carrier_kind"] == REF})
    return s, l


@case("meta-target realized to a NON-member", 1, {"4": "FAIL"})
def _meta_non_member():
    s, l = faithful()
    c = {"source": "CampaignFeedItem", "target": "ScopeContract", "class": "Causal",
         "carrier_kind": REF, "path": "synthetic::meta_bad"}
    l["carriers"].append(c)
    s["artifact_ref_paths"] = sorted(s["artifact_ref_paths"] + [c["path"]])
    return s, l


def main() -> int:
    failures = 0
    print(f"gate mutation corpus — {len(CASES)} cases\n" + "=" * 76)
    for name, fn, want_exit, want_steps in CASES:
        schema, ledger = fn()
        code, steps = run(schema, ledger)
        problems = []
        if code != want_exit:
            problems.append(f"exit {code} != {want_exit}")
        for step, expected in want_steps.items():
            got = steps.get(step, "<missing>")
            if got != expected:
                problems.append(f"step {step}: {got} != {expected}")
        ok = not problems
        failures += 0 if ok else 1
        print(f"  [{'ok' if ok else 'XX'}] {name:<58} {'' if ok else '; '.join(problems)}")
    print("=" * 76)
    print(f"{len(CASES) - failures}/{len(CASES)} cases behaved as specified")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
