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
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
GATE = REPO / "tools/a1_v2_ledger_gate.py"
sys.path.insert(0, str(REPO / "tools"))
import a1_v2_ledger_gate as gate  # noqa: E402
GRAPH = json.loads((REPO / "docs/tasks/a1-f-v2-graph.json").read_text())

STRUCTURAL = "event_payload_digest"
REF = "artifact_ref"


def faithful() -> tuple[dict, dict]:
    """A schema + ledger that faithfully realize the frozen graph.

    Row 69 gets ONE carrier whose target is the meta-target itself — Phase G
    4.2.6 clause 5: declared once as a meta-target expansion, never as eleven
    separate edges. The expansion appears only in that field's permitted target
    domain.
    """
    carriers, fields, struct = [], [], {}
    for e in GRAPH["edges"]:
        src, tgt, cls = e["source"], e["target"], e["class"]
        is_struct = src.startswith("CampaignEvent(") and tgt.endswith("Payload")
        path = f"synthetic::{e['n']}"
        carriers.append({"source": src, "target": tgt, "class": cls,
                         "carrier_kind": STRUCTURAL if is_struct else REF,
                         "path": path})
        if is_struct:
            struct[src[len("CampaignEvent("):-1]] = path
        else:
            fields.append({"path": path, "source_semantic_kind": src,
                           "allowed_concrete_target_kinds":
                               sorted(GRAPH["meta_targets"].get(tgt, [tgt]))})
    schema = {
        "extracted_from": "synthetic://v2-schemas@test",
        "extractor": "synthetic",
        "event_kinds": list(GRAPH["event_kinds"]),
        "payload_presence": dict(GRAPH["expected_payload"]),
        "structural_commitments": dict(sorted(struct.items())),
        "artifact_ref_fields": fields,
    }
    return schema, {"carriers": carriers}


def run(schema: dict, ledger: dict) -> tuple[int, dict[str, str]]:
    """Call the steps directly.

    Previously this shelled out with A1_V2_GATE_HARNESS=1 to skip the
    preflights. That env var no longer exists: it skipped both preflights
    while adding no report line, so a harness run read exactly like a proven
    PASS, and a shell that exported it once disarmed every later invocation.
    The seam is now an import, which cannot be switched on by accident.
    """
    r = gate.run_steps(GRAPH, schema, ledger)
    return (1 if r.failed else 0), r.steps


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
             "structural_commitments": {}, "artifact_ref_fields": []}, {"carriers": []})


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
    s["artifact_ref_fields"].append({"path": "synthetic::rogue",
                                     "source_semantic_kind": "CoderReport",
                                     "allowed_concrete_target_kinds": ["ReviewVerdict"]})
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
    s["artifact_ref_fields"] = [f for f in s["artifact_ref_fields"]
                                if f["path"] != drop["path"]]
    return s, l


@case("schema field with no carrier declared", 1, {"3": "FAIL"})
def _unledgered_field():
    s, l = faithful()
    s["artifact_ref_fields"].append({"path": "synthetic::undeclared",
                                     "source_semantic_kind": "WorkOrder",
                                     "allowed_concrete_target_kinds": ["ScopeContract"]})
    return s, l


@case("carrier path the schema extractor never saw", 1, {"3": "FAIL"})
def _phantom_path():
    s, l = faithful()
    c = dict(l["carriers"][0]); c["path"] = "synthetic::phantom"
    l["carriers"].append(c)
    return s, l


@case("two fields swap their targets (globals still balance)", 1, {"3": "FAIL"})
def _target_swap():
    # The attack a bare path list cannot see: both frozen triples still exist
    # somewhere, so steps 4 and 5 pass while each field carries the wrong one.
    s, l = faithful()
    a = next(c for c in l["carriers"] if c["source"] == "WorkOrder"
             and c["target"] == "CandidateStateReceiptRef")
    b = next(c for c in l["carriers"] if c["source"] == "WorkOrder"
             and c["target"] == "CandidateMaterializationRef")
    a["path"], b["path"] = b["path"], a["path"]
    return s, l


@case("duplicate structural carrier for one event kind", 1, {"2": "FAIL"})
def _duplicate_structural():
    # G-R11 requires EXACTLY one carrier per payload-bearing kind. Set-based
    # membership collapses duplicates and reads two as one.
    s, l = faithful()
    dup = next(c for c in l["carriers"]
               if c["carrier_kind"] == STRUCTURAL
               and c["source"] == "CampaignEvent(CampaignCreated)")
    l["carriers"].append({**dup, "path": dup["path"] + "::dup"})
    return s, l


@case("schema commits no structural digests at all", 1, {"2": "FAIL"})
def _no_structural_commitments():
    # The requirement is keyed off the FROZEN presence map. A schema that
    # declares a correct payload_presence but commits no event_payload_digest
    # is not permitted to lower the bar to zero.
    s, l = faithful()
    s["structural_commitments"] = {}
    return s, l


@case("payload edges relabelled as artifact_ref carriers", 1, {"2": "FAIL"})
def _payloads_as_refs():
    # Found by independent review of a10d845, and it was a full green: with
    # `want` read from the schema, dropping the structural commitments made
    # ZERO the expected count, and the eleven payload edges re-declared as
    # artifact_ref carriers satisfied steps 3, 4 and 5 on their own.
    s, l = faithful()
    s["structural_commitments"] = {}
    for c in l["carriers"]:
        if c["carrier_kind"] == STRUCTURAL:
            c["carrier_kind"] = REF
            s["artifact_ref_fields"].append(
                {"path": c["path"], "source_semantic_kind": c["source"],
                 "allowed_concrete_target_kinds": [c["target"]]})
    return s, l


@case("payload digest moved onto a sibling edge of the same source", 1, {"2": "FAIL"})
def _digest_substitution():
    # Found by independent review of 4a5ad37. Counting structural carriers by
    # SOURCE bound the digest to an event kind but not to its payload, and
    # eleven kinds carry more than one frozen edge. Rows 56 and 64 share the
    # source CampaignEvent(HumanCommandRejected); swapping which one carried the
    # digest kept the count at one and passed all five steps, while the required
    # relation --event_payload_digest--> HumanCommandRejectedPayload was absent.
    s, l = faithful()
    src = "CampaignEvent(HumanCommandRejected)"
    struct = next(c for c in l["carriers"]
                  if c["carrier_kind"] == STRUCTURAL and c["source"] == src)
    sibling = next(c for c in l["carriers"]
                   if c["carrier_kind"] == REF and c["source"] == src)
    struct["carrier_kind"], sibling["carrier_kind"] = REF, STRUCTURAL
    s["artifact_ref_fields"] = [f for f in s["artifact_ref_fields"]
                                if f["path"] != sibling["path"]]
    s["artifact_ref_fields"].append(
        {"path": struct["path"], "source_semantic_kind": src,
         "allowed_concrete_target_kinds": [struct["target"]]})
    return s, l


@case("structural carrier path the schema never declared", 1, {"2": "FAIL"})
def _structural_path_phantom():
    # Found by independent review of 4eee8ec. Steps 4 and 5 ignore paths and
    # step 3 only inspects artifact_ref rows, so the path column on a structural
    # carrier was checked by nothing: every digest could name a path that does
    # not exist and all five steps passed.
    s, l = faithful()
    for c in l["carriers"]:
        if c["carrier_kind"] == STRUCTURAL:
            c["path"] = "synthetic::does-not-exist"
    return s, l


@case("meta-target narrowed to one member by the schema", 1, {"3": "FAIL"})
def _meta_narrowed():
    # A subject_refs field permitting only WorkOrder is NOT a faithful
    # realization of a union of eleven kinds.
    s, l = faithful()
    f = next(f for f in s["artifact_ref_fields"]
             if f["source_semantic_kind"] == "CampaignFeedItem")
    f["allowed_concrete_target_kinds"] = ["WorkOrder"]
    return s, l


@case("meta-target realized as eleven separate carriers", 1, {"4": "FAIL", "5": "FAIL"})
def _meta_decomposed():
    # Frozen clause 5 forbids this: declared ONCE, never as eleven edges.
    s, l = faithful()
    row69 = next(c for c in l["carriers"] if c["target"] == "AnyCommittedEnvelope")
    l["carriers"].remove(row69)
    for m in GRAPH["meta_targets"]["AnyCommittedEnvelope"]:
        l["carriers"].append({**row69, "target": m, "path": f"{row69['path']}::{m}"})
    f = next(f for f in s["artifact_ref_fields"] if f["path"] == row69["path"])
    s["artifact_ref_fields"].remove(f)
    for m in GRAPH["meta_targets"]["AnyCommittedEnvelope"]:
        s["artifact_ref_fields"].append(
            {"path": f"{row69['path']}::{m}", "source_semantic_kind": "CampaignFeedItem",
             "allowed_concrete_target_kinds": [m]})
    return s, l


# ---------------------------------------------------------------------------
# Preflight corpus. These run WITHOUT the harness escape, because the preflight
# is exactly what they test. E-R1 added the protection and shipped no regression
# for it, so the check existed while nothing defended it.
# ---------------------------------------------------------------------------

GRAPH_PATH = REPO / "docs/tasks/a1-f-v2-graph.json"
SCHEMA_PATH = REPO / "docs/tasks/a1-f-v2-schema-facts.json"
GRAPH_EXTRACTOR = REPO / "tools/a1_v2_extract_graph.py"
PHASE_G = REPO / "docs/tasks/a1-f-v2-phase-g.md"


def run_real(graph: Path, schema: Path) -> tuple[int, str]:
    ledger = REPO / "docs/tasks/a1-f-v2-realization-ledger.json"
    p = subprocess.run(
        [sys.executable, str(GATE), "--graph", str(graph), "--schema", str(schema),
         "--ledger", str(ledger)],
        capture_output=True, text=True)
    return p.returncode, p.stdout


def preflight_cases() -> list[tuple[str, bool]]:
    out = []
    with tempfile.TemporaryDirectory() as d:
        d = Path(d)

        # 1. A forged graph passed via --graph. E-R1's preflight checked the
        #    DEFAULT artifact and then read the override, so this bypassed it.
        forged = json.loads(GRAPH_PATH.read_text())
        forged["edges"][0]["class"] = "Causal"
        fg = d / "forged-graph.json"
        fg.write_text(json.dumps(forged, indent=2) + "\n")
        code, txt = run_real(fg, SCHEMA_PATH)
        out.append(("forged graph via --graph is rejected",
                    code == 1 and "preflight: graph extract" in txt
                    and " FAIL " in txt.split("preflight: graph extract")[1].split("\n")[0]
                    and "1. event-kind" not in txt))

        # 2. Hand-filled schema facts: the middle term must come from an
        #    extractor, not a keyboard.
        forged_schema = json.loads(SCHEMA_PATH.read_text())
        forged_schema["extracted_from"] = "wishful://thinking"
        forged_schema["event_kinds"] = list(GRAPH["event_kinds"])
        fs = d / "forged-schema.json"
        fs.write_text(json.dumps(forged_schema, indent=2) + "\n")
        code, txt = run_real(GRAPH_PATH, fs)
        out.append(("hand-filled schema facts are rejected",
                    code == 1 and "preflight: schema extract" in txt
                    and " FAIL " in txt.split("preflight: schema extract")[1].split("\n")[0]))

        # 3. The Phase G source itself moving: the extractor must notice that the
        #    authority it claims to derive from is no longer the pinned blob.
        moved = d / "phase-g-moved.md"
        moved.write_text(PHASE_G.read_text() + "\n<!-- authority drifted -->\n")
        p = subprocess.run([sys.executable, str(GRAPH_EXTRACTOR), "--check",
                            "--source", str(moved), "--out", str(GRAPH_PATH)],
                           capture_output=True, text=True)
        out.append(("Phase G source blob mismatch is rejected", p.returncode == 1))

        # 3a. A re-pin that arrives WITHOUT its evidence. This is the shape the
        #     ceremony exists to stop: a --check failure tempts exactly one
        #     one-token edit, and that edit must not be enough on its own.
        sys.path.insert(0, str(REPO / "tools"))
        import a1_v2_extract_graph as xg  # noqa: E402
        out.append(("bare PINNED_BLOB edit is rejected as unevidenced",
                    xg.pin_chain_defect("f" * 40, []) is not None))

        # 3b. Evidence filed for a pin that never moved, and a chain that does
        #     not run from the original blob. Both are paperwork without a fact.
        orphan = [{"old_blob": xg.ORIGINAL_BLOB, "new_blob": "a" * 40,
                   "superseding_authority": "Phase G supersede, hypothetical"}]
        broken = [{"old_blob": "b" * 40, "new_blob": "a" * 40,
                   "superseding_authority": "Phase G supersede, hypothetical"}]
        incomplete = [{"old_blob": xg.ORIGINAL_BLOB, "new_blob": "a" * 40}]
        out.append(("pin evidence that binds nothing is rejected",
                    xg.pin_chain_defect(xg.ORIGINAL_BLOB, orphan) is not None
                    and xg.pin_chain_defect("a" * 40, broken) is not None
                    and xg.pin_chain_defect("a" * 40, incomplete) is not None
                    and xg.pin_chain_defect("a" * 40, orphan) is None))

        # 3c. PYTHONOPTIMIZE strips `assert`. The frozen checks - pin evidence,
        #     69 edges, 56/13, 21/11, 47/20 - were all asserts, so one env var
        #     deleted the entire ceremony of 2.5 and reported `extract OK`.
        probe = ("import sys; sys.path.insert(0, %r); import a1_v2_extract_graph as x; "
                 "x.PINNED_BLOB = 'f' * 40; x.extract(x.SOURCE)" % str(REPO / "tools"))
        p = subprocess.run([sys.executable, "-c", probe], capture_output=True,
                           text=True, env={**os.environ, "PYTHONOPTIMIZE": "1"})
        out.append(("frozen checks survive PYTHONOPTIMIZE=1",
                    p.returncode != 0 and "PIN EVIDENCE DEFECT" in p.stderr))

        # 3d. The retired harness env var must be inert, not merely undocumented.
        p = subprocess.run([sys.executable, str(GATE), "--graph", str(fg)],
                           capture_output=True, text=True,
                           env={**os.environ, "A1_V2_GATE_HARNESS": "1"})
        out.append(("A1_V2_GATE_HARNESS=1 no longer bypasses the preflight",
                    p.returncode == 1 and "preflight: graph extract" in p.stdout
                    and " FAIL " in p.stdout.split("preflight: graph extract")[1].split("\n")[0]))

        # 4. Control: the real artifacts must still pass their preflights, or the
        #    cases above would be proving nothing.
        code, txt = run_real(GRAPH_PATH, SCHEMA_PATH)
        out.append(("real artifacts pass both preflights",
                    "preflight: graph extract" in txt and "preflight: schema extract" in txt
                    and txt.count(" PASS ") >= 2))
    return out


def main() -> int:
    failures = 0
    print(f"gate mutation corpus — {len(CASES)} step cases + preflight\n" + "=" * 76)
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
    print("-" * 76)
    pre = preflight_cases()
    for name, ok in pre:
        failures += 0 if ok else 1
        print(f"  [{'ok' if ok else 'XX'}] {name}")
    total = len(CASES) + len(pre)
    print("=" * 76)
    print(f"{total - failures}/{total} cases behaved as specified")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
