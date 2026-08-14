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
import a1_v2_extract_graph as _xg  # noqa: E402
GRAPH = json.loads((REPO / "docs/tasks/a1-f-v2-graph.json").read_text())

# Every git child THIS CORPUS spawns, under the same rule as the extractor's own
# and for the same reason. Six git call sites were added here over four review
# rounds; four of them inherited `os.environ`, so a corpus written to prove that
# credentials stay out of git children was handing them to its own. The rule in
# AGENTS.md is "any new process able to read it", with no exemption for test
# scaffolding — and a harness is not a lower-trust place to leak a key than the
# tool it tests. Witness 3v enforces this at every call site, including ones not
# written yet, because a helper that exists is not a helper that is used.
GIT = _xg._git_binary()


def git_env(**extra: str) -> dict[str, str]:
    """Constructed, never inherited. `extra` is for control arms that need one
    specific variable back — pass it by name, so the exception is visible."""
    env = {"PATH": str(Path(GIT).parent)}
    if "HOME" in os.environ:
        env["HOME"] = os.environ["HOME"]
    env.update(extra)
    return env

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
    return r.exit_code(), r.steps


# Pinned deliberately. Bump it in the same commit that adds or removes a case,
# so the number is a reviewed claim rather than a readout of whatever survived.
EXPECTED_TOTAL = 69

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


@case("carrier declaring an invented third role", 1, {"1": "PASS"})
def _unknown_carrier_kind():
    # Role substitution. carrier_kind is a CLOSED choice; steps 2 and 3 each
    # filter for one exact spelling, so a misspelling fell through both and step
    # 4 admitted the row on its triple alone — role gone, path unverified, PASS.
    s, l = faithful()
    c = dict(next(x for x in l["carriers"] if x["carrier_kind"] == REF))
    c["carrier_kind"], c["path"] = "artifactref", "synthetic::nowhere"
    l["carriers"].append(c)
    return s, l


@case("the same carrier occurrence declared twice", 1, {"1": "PASS"})
def _duplicate_ref_row():
    # Multiplicity collapse. Step 3 unions domains and steps 4/5 compare sets,
    # so a byte-for-byte duplicate reduced to one observation and a seventy-row
    # ledger passed against a sixty-nine-edge graph. Several carriers for one
    # edge stay legal; two declarations of ONE field occurrence do not.
    s, l = faithful()
    l["carriers"].append(dict(next(x for x in l["carriers"]
                                   if x["carrier_kind"] == REF)))
    return s, l


@case("schema declaring one event kind twice", 1, {"1": "PASS"})
def _duplicate_event_kind():
    # Step 1's norm is SET equality, so `set(...)` matched the norm — but only
    # after silently normalising an observation that says the schema defines one
    # event kind twice. Well-formedness first, then the comparison.
    s, l = faithful()
    s["event_kinds"] = s["event_kinds"] + [s["event_kinds"][0]]
    return s, l


@case("two contradictory schema facts for one field path", 1, {"1": "PASS"})
def _contradictory_field_facts():
    # Step 3 indexes facts by path, so the LAST row won: with the correct fact
    # last, a schema containing a contradictory claim passed all five steps.
    # The verdict depended on list order, which is silent resolution rather
    # than detection.
    s, l = faithful()
    bad = dict(s["artifact_ref_fields"][0])
    bad["allowed_concrete_target_kinds"] = ["ScopeContract"]
    s["artifact_ref_fields"] = [bad] + s["artifact_ref_fields"]
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

# A locator that resolves: the frozen Phase G blob, and a string occurring in it
# exactly once. `path_hint` is decoration by design — a blob does not carry its
# own path, so this layer cannot check it and does not pretend to.
VALID_LOCATOR = {
    "blob": "450380ff0d1f8ec08f783968f08bc6b3942f44a5",
    "path_hint": "docs/tasks/a1-f-v2-phase-g.md",
    "anchor": "FROZEN \u2014 the realization contract",
}
SCHEMA_PATH = REPO / "docs/tasks/a1-f-v2-schema-facts.json"
LEDGER_PATH = REPO / "docs/tasks/a1-f-v2-realization-ledger.json"
GRAPH_EXTRACTOR = REPO / "tools/a1_v2_extract_graph.py"
PHASE_G = REPO / "docs/tasks/a1-f-v2-phase-g.md"


def gate_env(**extra: str) -> dict[str, str]:
    """Environment for a gate child spawned by this corpus.

    `run_real` reproduces the gate-invocation path rather than owning an
    independent one, so it is held to the property EC-R1 installs there instead
    of quietly running under an inherited environment. HOME is present because
    the gate projects it onward to the graph extractor, which needs it to read
    `safe.directory`; nothing else is.
    """
    env = {k: os.environ[k] for k in ("HOME",) if k in os.environ}
    env.update(extra)
    return env


def run_real(graph: Path, schema: Path, env: dict[str, str] | None = None,
             isolated: bool = True) -> tuple[int, str]:
    ledger = REPO / "docs/tasks/a1-f-v2-realization-ledger.json"
    argv = [sys.executable] + (["-I"] if isolated else []) + [
        str(GATE), "--graph", str(graph), "--schema", str(schema),
        "--ledger", str(ledger)]
    p = subprocess.run(argv, capture_output=True, text=True, encoding="utf-8",
                       errors="replace", env=gate_env() if env is None else env)
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
        out.append(("forged graph via --graph is ERROR, not FAIL",
                    code == 2 and "preflight: graph extract" in txt
                    and " ERROR " in txt.split("preflight: graph extract")[1].split("\n")[0]
                    and "was NOT judged" in txt
                    and "1. event-kind" not in txt))

        # 2. Hand-filled schema facts: the middle term must come from an
        #    extractor, not a keyboard.
        forged_schema = json.loads(SCHEMA_PATH.read_text())
        forged_schema["extracted_from"] = "wishful://thinking"
        forged_schema["event_kinds"] = list(GRAPH["event_kinds"])
        fs = d / "forged-schema.json"
        fs.write_text(json.dumps(forged_schema, indent=2) + "\n")
        code, txt = run_real(GRAPH_PATH, fs)
        out.append(("hand-filled schema facts are ERROR, not FAIL",
                    code == 2 and "preflight: schema extract" in txt
                    and " ERROR " in txt.split("preflight: schema extract")[1].split("\n")[0]))

        # 3. The Phase G source itself moving: the extractor must notice that the
        #    authority it claims to derive from is no longer the pinned blob.
        moved = d / "phase-g-moved.md"
        moved.write_text(PHASE_G.read_text() + "\n<!-- authority drifted -->\n")
        p = subprocess.run([sys.executable, str(GRAPH_EXTRACTOR), "--check",
                            "--source", str(moved), "--out", str(GRAPH_PATH)],
                           capture_output=True, text=True, encoding="utf-8", errors="replace")
        out.append(("Phase G source blob mismatch is rejected", p.returncode == 1))

        # 3a. A re-pin that arrives WITHOUT its evidence. This is the shape the
        #     ceremony exists to stop: a --check failure tempts exactly one
        #     one-token edit, and that edit must not be enough on its own.
        sys.path.insert(0, str(REPO / "tools"))
        import a1_v2_extract_graph as xg  # noqa: E402
        gate_xg = xg
        out.append(("bare PINNED_BLOB edit is rejected as unevidenced",
                    xg.pin_chain_defect("f" * 40, []) is not None))

        # 3b. Evidence filed for a pin that never moved, and a chain that does
        #     not run from the original blob. Both are paperwork without a fact.
        orphan = [{"old_blob": xg.ORIGINAL_BLOB, "new_blob": "a" * 40,
                   "superseding_authority": VALID_LOCATOR}]
        broken = [{"old_blob": "b" * 40, "new_blob": "a" * 40,
                   "superseding_authority": VALID_LOCATOR}]
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
                           capture_output=True, text=True, encoding="utf-8", errors="replace",
                           env={**os.environ, "A1_V2_GATE_HARNESS": "1"})
        out.append(("A1_V2_GATE_HARNESS=1 no longer bypasses the preflight",
                    p.returncode == 2 and "preflight: graph extract" in p.stdout
                    and " ERROR " in p.stdout.split("preflight: graph extract")[1].split("\n")[0]))

        # 3e. An authority shape step 3's key cannot represent must be DECLARED
        #     as a verifier premise failure, never silently accepted and never
        #     reported as a defect of the authority.
        widened = json.loads(GRAPH_PATH.read_text())
        e0 = dict(widened["edges"][0])
        e0["class"] = "Causal" if e0["class"] == "Intra" else "Intra"
        widened["edges"].append(e0)
        schema, ledger = faithful()
        rep = gate.run_steps(widened, schema, ledger)
        line = next((x for x in rep.lines if "premise" in x), "")
        out.append(("an authority shape step 3 cannot represent is ERROR, not FAIL",
                    # step 5 also fails here (the widened graph has a 70th
                    # edge nothing carries) — ERROR must DOMINATE, because no
                    # step verdict is trustworthy once the premise is void.
                    rep.errored and rep.exit_code() == 2
                    and " ERROR " in line and "VERIFIER PREMISE INVALIDATED" in line
                    and "Do NOT alter the authority" in line))

        # 3f. Artifact loading through the EXECUTABLE path. An unreadable,
        #     unparseable or wrongly-shaped artifact used to raise, printing no
        #     RESULT and exiting 1 — the code this contract assigns to FAIL, so
        #     a consumer read "measured and lost" for a run that never began.
        bad = d / "bad.json"
        bad.write_text("{")
        shaped = d / "array.json"
        shaped.write_text("[]\n")
        rows = d / "rows.json"
        rows.write_text('{"carriers": "not a list"}\n')
        empty_row = d / "emptyrow.json"
        empty_row.write_text('{"carriers": [{}]}\n')
        partial = d / "partial.json"
        partial.write_text('{"carriers": [{"source": "WorkOrder", "target": "ScopeContract",'
                           ' "class": "Intra", "carrier_kind": "artifact_ref"}]}\n')
        mistyped = d / "mistyped.json"
        mistyped.write_text('{"carriers": [{"source": 1, "target": "ScopeContract",'
                            ' "class": "Intra", "carrier_kind": "artifact_ref",'
                            ' "path": "p"}]}\n')
        nokey = d / "nokey.json"
        nokey.write_text("{}\n")
        utf8 = d / "utf8.json"
        utf8.write_bytes(b'{"carriers":[]}\xff')
        dupe = d / "dupe.json"
        dupe.write_text('{"carriers": [{"source": "WorkOrder", "target": "ScopeContract",'
                        ' "class": "Intra", "carrier_kind": "artifact_ref",'
                        ' "path": "p"}], "carriers": []}\n')
        nonstd = d / "nonstd.json"
        nonstd.write_text('{"carriers": [], "x": NaN}\n')
        surro = d / "surro.json"
        surro.write_text('{"carriers": [{"source": "\\ud800", "target": "ScopeContract",'
                         ' "class": "Intra", "carrier_kind": "nonsense",'
                         ' "path": "p"}]}\n')
        deep = d / "deep.json"
        deep.write_text("[" * 2000 + "]" * 2000)
        for label, ledger_arg in (("malformed JSON", bad), ("missing file", d / "nope.json"),
                                  ("no `carriers` key at all", nokey),
                                  ("invalid UTF-8", utf8),
                                  ("a duplicate `carriers` member", dupe),
                                  ("a non-standard JSON constant", nonstd),
                                  ("an unpaired surrogate in a value", surro),
                                  ("nesting deeper than the parser reads", deep),
                                  ("non-object top level", shaped),
                                  ("carriers not a list of rows", rows),
                                  ("a row with no fields at all", empty_row),
                                  ("a row missing `path`", partial),
                                  ("a row whose `source` is not a string", mistyped)):
            p = subprocess.run(
                [sys.executable, str(GATE), "--graph", str(GRAPH_PATH),
                 "--schema", str(SCHEMA_PATH), "--ledger", str(ledger_arg)],
                capture_output=True, text=True, encoding="utf-8", errors="replace")
            out.append((f"ledger {label} is ERROR with a RESULT line",
                        p.returncode == 2 and "RESULT: ERROR" in p.stdout
                        and " ERROR " in p.stdout))

        # 3g. The report must not depend on the codec stdout happens to carry.
        #     Under LC_ALL=C every verdict string raised UnicodeEncodeError on
        #     its em dash: the run died mid-line with a truncated `RESULT: ` and
        #     Python's exit 1, which this contract reserves for FAIL. The gate's
        #     own prose decided the verdict, not the evidence.
        ascii_env = {**os.environ, "LC_ALL": "C", "LANG": "C", "PYTHONUTF8": "0"}
        p = subprocess.run(
            [sys.executable, str(GATE), "--graph", str(GRAPH_PATH),
             "--schema", str(SCHEMA_PATH), "--ledger", str(LEDGER_PATH)],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            env=ascii_env)
        out.append(("an ASCII stdout codec still yields a full RESULT line",
                    p.returncode == 1 and "RESULT: FAIL" in p.stdout
                    and "Envelope v2 is not realized" in p.stdout))

        p = subprocess.run(
            [sys.executable, str(GATE), "--graph", str(GRAPH_PATH),
             "--schema", str(SCHEMA_PATH), "--ledger", str(bad)],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            env=ascii_env)
        out.append(("ERROR survives an ASCII stdout codec",
                    p.returncode == 2 and "RESULT: ERROR" in p.stdout))

        # ---- EB-R1: referential validation of superseding_authority -----------
        # The locator answers ONE question: can the cited bytes, and one place
        # inside them, be identified? It must never answer "do these bytes
        # authorize the re-pin?" — hence case 3n below, which is meant to look
        # slightly blasphemous.
        head_commit = subprocess.run([GIT, "-C", str(REPO), "rev-parse", "HEAD"],
                                     capture_output=True, text=True,
                                     env=git_env()).stdout.strip()

        # 3h. Malformed locator: wrong type, wrong field set, wrong oid syntax.
        out.append(("malformed authority locator is rejected",
                    all(xg.authority_ref_defect(r) is not None and
                        "MALFORMED LOCATOR" in xg.authority_ref_defect(r)
                        for r in ("a string",
                                  {"blob": xg.PINNED_BLOB, "anchor": "x"},
                                  {**VALID_LOCATOR, "extra": "x"},
                                  {**VALID_LOCATOR, "blob": "not-a-hex-oid"},
                                  {**VALID_LOCATOR, "anchor": ""}))))

        # 3i. Syntactically valid oid that git cannot supply HERE. The diagnostic
        #     must not claim the object is absent from the repository — under the
        #     shallow clone CI uses by default, a real historical blob is simply
        #     not present, and proving absence needs the network.
        absent = xg.authority_ref_defect({**VALID_LOCATOR, "blob": "d" * 40})
        out.append(("unresolvable oid says CHECKOUT, not absence",
                    absent is not None
                    and "UNRESOLVABLE IN THIS CHECKOUT" in absent
                    and "does NOT assert the object is absent" in absent))

        # 3j. Right oid, wrong object type.
        wrong_type = xg.authority_ref_defect({**VALID_LOCATOR, "blob": head_commit})
        out.append(("an oid resolving to a non-blob is rejected",
                    wrong_type is not None and "not a blob" in wrong_type))

        # 3k. Anchor absent from the cited bytes.
        gone = xg.authority_ref_defect({**VALID_LOCATOR, "anchor": "no such text here"})
        out.append(("an anchor absent from the blob is rejected",
                    gone is not None and "ANCHOR NOT FOUND" in gone))

        # 3l. Anchor present more than once: a locator must select ONE place.
        many = xg.authority_ref_defect({**VALID_LOCATOR, "anchor": "one level down"})
        out.append(("an ambiguous anchor is rejected",
                    many is not None and "ANCHOR AMBIGUOUS" in many))

        # 3l-bis. Overlapping occurrences. `str.count` reports non-overlapping
        #     matches, so an anchor can occupy two distinct start positions and
        #     still "count" once. This witness is drawn from the frozen blob
        #     itself: a run of 51 spaces, in which a 50-space anchor counted
        #     once and started twice, and therefore resolved as unique.
        overlap = xg.authority_ref_defect({**VALID_LOCATOR, "anchor": " " * 50})
        out.append(("an anchor overlapping itself is rejected",
                    overlap is not None and "ANCHOR AMBIGUOUS" in overlap))

        # 3m. Normal positive: a real, authority-shaped, unique anchor.
        out.append(("a well-formed authority locator resolves",
                    xg.authority_ref_defect(VALID_LOCATOR) is None))

        # 3n. NEGATIVE CONTROL, and the most important case here. The anchor
        #     resolves and is unique, and authorizes precisely nothing — it is a
        #     note about a git tag. Referential validation MUST pass, because
        #     legitimacy is not its floor. If this ever starts failing, the
        #     resolver has quietly appointed itself Phase G.
        out.append(("a resolvable but normatively inert anchor still PASSES",
                    xg.authority_ref_defect({**VALID_LOCATOR,
                        "anchor": "Created by the maintainer as a plain ref; "
                                  "no release object was"}) is None))

        # 3o. NO LAZY FETCH. In a partial clone `git cat-file` will fetch a
        #     promised object it does not have; GIT_TERMINAL_PROMPT does not stop
        #     it. This builds a real promisor clone over file:// and asks for a
        #     blob genuinely absent from its object database — the `old_blob`
        #     case, since after a supersede that is exactly the object that has
        #     left the tree. The control arm proves the hazard is present, so
        #     the witness cannot pass vacuously.
        lazy = d / "lazy"
        origin, clone = lazy / "origin", lazy / "clone"
        origin.mkdir(parents=True)
        def g(*args, cwd, **kw):
            return subprocess.run([GIT, *args], cwd=str(cwd), capture_output=True,
                                  text=True, encoding="utf-8", errors="replace",
                                  env=git_env(), **kw)
        g("init", "-q", ".", cwd=origin)
        for k, v in (("user.email", "t@t"), ("user.name", "t"),
                     ("uploadpack.allowFilter", "true"),
                     ("uploadpack.allowAnySHA1InWant", "true")):
            g("config", k, v, cwd=origin)
        (origin / "f.txt").write_text("SUPERSEDED-AUTHORITY-TEXT\n")
        g("add", "f.txt", cwd=origin); g("commit", "-qm", "old", cwd=origin)
        old_blob = g("hash-object", "f.txt", cwd=origin).stdout.strip()
        (origin / "f.txt").write_text("CURRENT-AUTHORITY-TEXT\n")
        g("add", "f.txt", cwd=origin); g("commit", "-qm", "new", cwd=origin)
        # TWO clones. The control arm lazily fetches, which would leave the blob
        # local and make the witness pass for the wrong reason — the same
        # ordering mistake that made the first hand-run of this experiment
        # report nothing.
        for name in ("clone", "control"):
            g("clone", "-q", "--filter=blob:none", "--no-local",
              f"file://{origin}", str(lazy / name), cwd=lazy)
        control = lazy / "control"

        present = g("cat-file", "--batch-all-objects", "--batch-check=%(objectname)",
                    cwd=clone).stdout
        absent_locally = bool(old_blob) and old_blob not in present
        # control, on its OWN clone: without the guard git reaches the promisor
        # remote and succeeds — proving the hazard is real in this fixture.
        fetched = subprocess.run([GIT, "-C", str(control), "cat-file", "-t", old_blob],
                                 capture_output=True, text=True,
                                 env=git_env(GIT_TERMINAL_PROMPT="0"))
        # witness, on an untouched clone: the extractor refuses without fetching.
        refused = gate_xg._local_object(old_blob, repo=clone) is None
        out.append(("a promised-but-absent object is refused, not fetched",
                    absent_locally and fetched.returncode == 0 and refused))

        # An object database that is NOT this repository's, holding bytes this
        # repository has never seen. It is the bait for 3r.
        alien = d / "alien"
        alien.mkdir()
        g("init", "-q", ".", cwd=alien)
        (alien / "a.txt").write_text("ALIEN AUTHORITY BYTES\n")
        alien_oid = g("hash-object", "-w", "a.txt", cwd=alien).stdout.strip()
        alien_objects = alien / ".git/objects"

        # 3p..3r. THE CHILD ENVIRONMENT IS CONSTRUCTED, NOT INHERITED. Copying
        #     `os.environ` into the two `git cat-file` calls made every ambient
        #     variable an undeclared input to this extractor. Two hazards rode
        #     on that, and they need separate witnesses because closing one by
        #     name would leave the other open.
        HOSTILE = {"ARLIAI_API_KEY": "sk-secret-witness",
                   "GIT_DIR": str(alien / ".git"),
                   "GIT_OBJECT_DIRECTORY": str(alien_objects),
                   "GIT_ALTERNATE_OBJECT_DIRECTORIES": str(alien_objects)}
        saved = {k: os.environ.get(k) for k in HOSTILE}
        os.environ.update(HOSTILE)
        try:
            # 3p. The constructed mapping carries the allowlist and the guards,
            #     and nothing else — asserted as an exact key set, so a later
            #     `env.update(os.environ)` cannot slip back in unnoticed.
            #     `PATH` is present but SYNTHESIZED — it is the directory of the
            #     pinned git binary, never the caller's. Asserting that it
            #     differs from the ambient one is the point: an inherited PATH
            #     chose the executable that reported the bytes.
            built = xg._git_env()
            allowed = ({k for k in xg.GIT_ENV_ALLOW if k in os.environ}
                       | set(xg.GIT_ENV) | {"PATH"})
            out.append(("git's environment is built from an allowlist",
                        set(built) == allowed
                        and all(built[k] == v for k, v in xg.GIT_ENV.items())
                        and built["PATH"] == str(Path(xg._git_binary()).parent)
                        and built["PATH"] != os.environ.get("PATH")))

            # 3q. What the CHILD PROCESS actually receives. 3p alone would pass
            #     vacuously if `_git_env` were written and never wired in, which
            #     is precisely the shape of the defect it replaces. The shim is
            #     /bin/sh, not python: a python child sets `LC_CTYPE` on itself
            #     via PEP 538 legacy-locale coercion, which would look like a
            #     leak and is not one.
            #
            #     The shim is installed by overriding GIT_CANDIDATES, not by
            #     prepending to `PATH`. It used to go on `PATH` — which is how
            #     the reviewer who read this witness saw that `PATH` selected
            #     the executable, and made 3u necessary. A probe that reaches
            #     its target through the hole under test cannot survive the
            #     patch, and should not: 3u now owns that question.
            #
            #     `export -p`, not `env`: the child's PATH is now the git
            #     directory alone, so the shim cannot reach coreutils. A probe
            #     that needs the surface it is auditing is not a probe.
            shim, dump = d / "shim", d / "child-env.txt"
            shim.mkdir()
            (shim / "git").write_text("#!/bin/sh\nexport -p > '%s'\nexit 1\n" % dump)
            (shim / "git").chmod(0o755)
            candidates_saved = xg.GIT_CANDIDATES
            xg.GIT_CANDIDATES = (str(shim / "git"),)
            try:
                xg._local_object("0" * 40)
            finally:
                xg.GIT_CANDIDATES = candidates_saved
            child = {line[len("export "):].split("=", 1)[0]
                     for line in dump.read_text().splitlines()
                     if line.startswith("export ") and "=" in line}
            out.append(("no ambient variable reaches the git child process",
                        bool(child) and not (child & set(HOSTILE))))

            # 3u. WHICH EXECUTABLE REPORTS THE BYTES. While `PATH` was
            #     inherited and the command was the unqualified `git`, a
            #     counterfeit git first on `PATH` — printing the genuine
            #     published blob — satisfied the object-id check for
            #     `repo=/nonexistent`. Recomputing the id proves the bytes match
            #     the name; it cannot prove they came from an object database,
            #     when the process reporting them is the caller's.
            #
            #     Control arm: the same counterfeit, reached through
            #     GIT_CANDIDATES instead, DOES run and IS accepted. So the
            #     witness separates "this counterfeit cannot fool the check"
            #     (false, and not the claim) from "`PATH` cannot install it"
            #     (the claim).
            #     The counterfeit is a python script under an absolute
            #     interpreter, for the same reason 3q dropped `env`: with the
            #     child's PATH narrowed to the git directory, a /bin/sh shim
            #     can reach neither `touch` nor `cat`.
            fake, ran = d / "fake", d / "fake-ran"
            fake.mkdir()
            real = xg._local_object(xg.PINNED_BLOB)
            (fake / "payload").write_bytes(real[1])
            (fake / "git").write_text(
                "#!%s\n"
                "import sys, pathlib\n"
                "pathlib.Path(%r).write_text('ran')\n"
                "if '-t' in sys.argv: sys.stdout.write('blob\\n')\n"
                "else: sys.stdout.buffer.write(pathlib.Path(%r).read_bytes())\n"
                % (sys.executable, str(ran), str(fake / "payload")))
            (fake / "git").chmod(0o755)
            path_saved = os.environ["PATH"]
            os.environ["PATH"] = f"{fake}:{path_saved}"
            try:
                via_path = xg._local_object(xg.PINNED_BLOB, repo=d / "nonexistent")
                executed = ran.exists()
                still_works = xg._local_object(xg.PINNED_BLOB)
                xg.GIT_CANDIDATES = (str(fake / "git"),)
                try:
                    control = xg._local_object(xg.PINNED_BLOB, repo=d / "nonexistent")
                finally:
                    xg.GIT_CANDIDATES = candidates_saved
            finally:
                os.environ["PATH"] = path_saved
            out.append(("PATH cannot choose the git that answers for the repo",
                        via_path is None and not executed
                        and still_works is not None
                        and control is not None and ran.exists()))

            # 3r. The hazard closed at the level of the ANSWER, not of a
            #     variable listing. `GIT_ALTERNATE_OBJECT_DIRECTORIES` makes
            #     `git -C <repo> cat-file` resolve objects that are not in
            #     <repo> at all — so whoever invokes the extractor could choose
            #     which bytes answer a provenance question. The control arm runs
            #     the same lookup with the ambient variable honoured and MUST
            #     succeed, or the fixture would be proving nothing; and the
            #     pinned blob must still resolve, or the fix would have bought
            #     its determinism by breaking resolution.
            #
            #     The control arm is the CONSTRUCTED environment plus exactly
            #     one inherited variable, so the single difference between the
            #     two arms is the pass-through itself. Handing it all of
            #     `os.environ` instead made it fail on the unrelated `GIT_DIR`
            #     above — a control that fails for the wrong reason witnesses
            #     nothing, the same way a control that passes for the wrong
            #     reason does not.
            leaked = subprocess.run(
                [GIT, "-C", str(REPO), "cat-file", "-t", alien_oid],
                capture_output=True, text=True, encoding="utf-8", errors="replace",
                env={**xg._git_env(),
                     "GIT_ALTERNATE_OBJECT_DIRECTORIES": str(alien_objects)})
            redirected = xg._local_object(alien_oid)
            pinned = xg._local_object(xg.PINNED_BLOB)
            out.append(("an ambient object-store redirect cannot answer for the repo",
                        leaked.returncode == 0 and redirected is None
                        and pinned is not None and pinned[0] == "blob"))
        finally:
            for k, v in saved.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v

        # 3s..3t. SUBSTITUTED OBJECT CONTENTS. `refs/replace/<oid>` makes
        #     `cat-file` return different bytes for the requested oid — for the
        #     type query as well as the content query — so an anchor can
        #     validate against bytes whose id is not the locator's `blob`, and
        #     `blob` stops being the identity of anything. Replace refs do not
        #     arrive over a default clone or fetch (verified), so the reach is
        #     a local checkout: the same reach as an ambient variable.
        subst = d / "subst"
        subst.mkdir()
        g("init", "-q", ".", cwd=subst)
        def _hash(text):
            return subprocess.run([GIT, "hash-object", "-w", "--stdin"],
                                  cwd=str(subst), input=text, capture_output=True,
                                  text=True, encoding="utf-8",
                                  env=git_env()).stdout.strip()
        authentic = _hash("AUTHENTIC AUTHORITY — anchor lives here\n")
        forged = _hash("FORGED AUTHORITY — anchor lives here\n")
        g("replace", "-f", authentic, forged, cwd=subst)

        # 3s. Control arm proves the fixture poses the hazard: with replacement
        #     honoured, git hands back the forged bytes for the authentic oid.
        #     Guarded, the authentic bytes come back.
        swapped = subprocess.run(
            [GIT, "-C", str(subst), "cat-file", "blob", authentic],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            env={k: v for k, v in xg._git_env().items()
                 if k != "GIT_NO_REPLACE_OBJECTS"})
        guarded = xg._local_object(authentic, repo=subst)
        out.append(("a replacement ref cannot answer for the object it replaces",
                    "FORGED" in swapped.stdout
                    and guarded is not None
                    and guarded[1].decode().startswith("AUTHENTIC")))

        # 3t. The guard and the identity check are INDEPENDENT, and this is the
        #     one that matters. Each GIT_ENV guard closes one way git can answer
        #     with something other than the cited object, and each was found one
        #     at a time, by a reviewer, after the previous one shipped —
        #     lazy fetch, then object-store redirect, then replacement refs.
        #     Recomputing the object id closes the FAMILY: whatever substitutes
        #     the bytes, the substitute does not hash to the name. So this case
        #     removes the guard deliberately and requires the abort anyway. If
        #     it ever starts passing only with the guard present, the extractor
        #     is back to enumerating known tricks.
        saved_guard = xg.GIT_ENV.pop("GIT_NO_REPLACE_OBJECTS")
        try:
            try:
                xg._local_object(authentic, repo=subst)
                caught = ""
            except xg.ExtractDefect as e:
                caught = str(e)
        finally:
            xg.GIT_ENV["GIT_NO_REPLACE_OBJECTS"] = saved_guard
        out.append(("substituted object bytes abort even with the guard removed",
                    "OBJECT IDENTITY VIOLATED" in caught
                    and "do NOT edit the locator" in caught))

        # 3v. THIS CORPUS'S OWN GIT CHILDREN. Four review rounds of witnesses
        #     about credentials and object stores added six git call sites here,
        #     four of them inheriting `os.environ` — the corpus was committing
        #     the defect it exists to detect.
        #
        #     Checking one sample call would prove nothing about the fifth, and
        #     a `git_env` helper that exists is not a helper that is used. So
        #     this reads THIS FILE'S OWN SOURCE and requires, of every
        #     `subprocess.run` in it that spawns git: the executable is the
        #     pinned `GIT`, never the ambient string "git", and `env=` is
        #     passed. A call site added later fails this without anyone
        #     remembering the rule — which is the only version of the rule that
        #     survives the next round.
        import ast
        tree = ast.parse(Path(__file__).read_text())
        offenders = []
        for node in ast.walk(tree):
            if not (isinstance(node, ast.Call)
                    and isinstance(node.func, ast.Attribute)
                    and node.func.attr == "run"
                    and isinstance(node.func.value, ast.Name)
                    and node.func.value.id == "subprocess"):
                continue
            argv = node.args[0] if node.args else None
            if not isinstance(argv, (ast.List, ast.Tuple)) or not argv.elts:
                continue
            head = argv.elts[0]
            spawns_git = ((isinstance(head, ast.Name) and head.id == "GIT")
                          or (isinstance(head, ast.Constant)
                              and head.value == "git"))
            if not spawns_git:
                continue
            names = {k.arg for k in node.keywords}
            if not (isinstance(head, ast.Name) and "env" in names):
                offenders.append(node.lineno)
        # The control is the count: if the walk matched nothing, an empty
        # offender list would read exactly like compliance.
        found = sum(1 for n in ast.walk(tree)
                    if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute)
                    and n.func.attr == "run" and n.args
                    and isinstance(n.args[0], (ast.List, ast.Tuple))
                    and n.args[0].elts
                    and isinstance(n.args[0].elts[0], ast.Name)
                    and n.args[0].elts[0].id == "GIT")
        out.append(("every git child of this corpus gets a constructed env",
                    found >= 6 and not offenders))

        # 3w. And the construction excludes the credential while one is
        #     exported — the property, not just the shape of the call.
        os.environ["ARLIAI_API_KEY"] = "sk-secret-witness"
        try:
            built = git_env()
            leak_free = "ARLIAI_API_KEY" not in built
            named = git_env(GIT_TERMINAL_PROMPT="0")
        finally:
            os.environ.pop("ARLIAI_API_KEY", None)
        out.append(("the corpus's git environment is constructed, not inherited",
                    leak_free and set(built) <= {"PATH", "HOME"}
                    and named["GIT_TERMINAL_PROMPT"] == "0"))

        # EC-R1 --------------------------------------------------------------
        # The gate parent spawns two python extractor children. Until now it
        # handed them the whole parent environment. Three witnesses, split so
        # that one does not have to prove everything: capability, call site,
        # execution. The split is the lesson from this branch's failed
        # harnesses — a probe that must both reach its target and prove a
        # negative can stop running and report success by silence.

        # 5a. CAPABILITY, proved constructively rather than by absence. Two
        #     secret-shaped variables, not one: if only `ARLIAI_API_KEY` were
        #     checked, an implementation that removed that single name would
        #     pass while remaining a credential denylist. The schema extractor
        #     projects to the EMPTY set — it imports argparse/json/sys/pathlib,
        #     spawns nothing, reads no environment — and an unknown tool
        #     projects empty too, so a third extractor cannot inherit the parent
        #     by omission.
        secrets = {"ARLIAI_API_KEY": "sk-sentinel",
                   "SOME_OTHER_TOKEN": "also-secret",
                   "PYTHONPATH": str(d / "evil"), "TMPDIR": str(d / "evil"),
                   "VIRTUAL_ENV": str(d / "evil")}
        saved_env = {k: os.environ.get(k) for k in secrets}
        os.environ.update(secrets)
        try:
            g_env = gate.preflight_env(gate.GRAPH_EXTRACTOR, os.environ)
            s_env = gate.preflight_env(gate.SCHEMA_EXTRACTOR, os.environ)
            u_env = gate.preflight_env(Path("a1_v2_extract_future.py"), os.environ)
            out.append(("preflight children get only declared capabilities",
                        not (set(g_env) & set(secrets))
                        and g_env == {} and s_env == {} and u_env == {}))
        finally:
            for k, v in saved_env.items():
                if v is None:
                    os.environ.pop(k, None)
                else:
                    os.environ[k] = v

        # 5b. CALL SITE. 5a describes a function; this requires the gate to use
        #     it. A projection helper that exists and is never passed is the
        #     exact failure this branch already shipped once, one layer down.
        #     Read from the gate's own source: the preflight spawn must pass
        #     `env=preflight_env(...)` and must carry `-I`.
        import ast
        gate_src = ast.parse(GATE.read_text())
        spawns = [n for n in ast.walk(gate_src)
                  if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute)
                  and n.func.attr == "run" and n.args
                  and isinstance(n.args[0], (ast.List, ast.Tuple))
                  and any(isinstance(e, ast.Attribute) and e.attr == "executable"
                          for e in n.args[0].elts)]
        wired = []
        for call in spawns:
            argv_flags = {e.value for e in call.args[0].elts
                          if isinstance(e, ast.Constant)}
            env_kw = next((k.value for k in call.keywords if k.arg == "env"), None)
            wired.append({"-I", "-S"} <= argv_flags
                         and isinstance(env_kw, ast.Call)
                         and isinstance(env_kw.func, ast.Name)
                         and env_kw.func.id == "preflight_env")
        out.append(("the gate spawns through that projection, isolated and site-free",
                    len(spawns) >= 1 and all(wired)))

        # 5c. EXECUTION, and the preservation half of the contract. Removing
        #     ambient state must not change the preflight result for the
        #     committed artifacts. The control arm proves the fixture is
        #     genuinely hostile: the SAME PYTHONPATH, handed to an extractor run
        #     without isolation, hijacks it outright — a planted `json.py` is
        #     imported before the extractor's first line.
        evil = d / "evil"
        evil.mkdir(exist_ok=True)
        (evil / "json.py").write_text('raise SystemExit("HIJACKED")\n')
        hostile = {**gate_env(), "ARLIAI_API_KEY": "sk-sentinel",
                   "PYTHONPATH": str(evil), "TMPDIR": str(evil),
                   "VIRTUAL_ENV": str(evil)}
        hijacked = subprocess.run(
            [sys.executable, str(REPO / "tools/a1_v2_extract_schema.py"),
             "--check", "--out", str(SCHEMA_PATH)],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            env={**hostile})
        code, txt = run_real(GRAPH_PATH, SCHEMA_PATH, env=hostile)
        out.append(("preflights still PASS with ambient state removed",
                    "HIJACKED" in hijacked.stdout + hijacked.stderr
                    and txt.count(" PASS ") >= 2 and code == 1))

        # 5d. THE GRANT IS SCOPED, AND NOTHING AMBIENT IS REQUIRED. `HOME` used
        #     to be the graph extractor's one declared capability, justified by
        #     a foreign-owned checkout that git refuses without `safe.directory`.
        #     Review's objection was that the grant is far wider than its
        #     justification: a home directory carries every global git setting,
        #     credential-helper configuration and arbitrary `include.path`. The
        #     requirement now travels in the git command line, scoped to one
        #     path, and `GIT_ENV_ALLOW` is empty.
        #
        #     The foreign-ownership arm itself needs `chown`, so it is recorded
        #     in §2.3.1 as an explicit command rather than carried here, where it
        #     would either fail for a non-root developer or be skipped into a
        #     vacuous pass. What IS carried is the pair that catches a
        #     regression: nothing ambient is consulted, and the scoped grant is
        #     really on the command line.
        argv_seen = d / "argv.txt"
        probe = d / "argvshim"
        probe.mkdir()
        (probe / "git").write_text(
            "#!%s\nimport sys, pathlib\n"
            "pathlib.Path(%r).write_text(repr(sys.argv))\n"
            "sys.exit(1)\n" % (sys.executable, str(argv_seen)))
        (probe / "git").chmod(0o755)
        saved_cand, saved_environ = xg.GIT_CANDIDATES, dict(os.environ)
        try:
            xg.GIT_CANDIDATES = (str(probe / "git"),)
            xg._local_object("0" * 40, repo=REPO)
            argv = argv_seen.read_text()
            os.environ.clear()          # nothing ambient at all
            xg.GIT_CANDIDATES = saved_cand
            stripped = xg._local_object(xg.PINNED_BLOB, repo=REPO)
            env_built = xg._git_env()
        finally:
            xg.GIT_CANDIDATES = saved_cand
            os.environ.clear()
            os.environ.update(saved_environ)
        out.append(("the safe.directory grant is scoped and nothing is inherited",
                    f"safe.directory={REPO.resolve()}" in argv
                    and xg.GIT_ENV_ALLOW == ()
                    and stripped is not None and stripped[0] == "blob"
                    and set(env_built) == {"PATH"} | set(xg.GIT_ENV)))

        # 5f. AN EMPTY ENVIRONMENT DOES NOT EMPTY GIT'S CONFIGURATION. The
        #     system scope is read from a fixed path and owes nothing to the
        #     environment, so `env -i` leaves `/etc/gitconfig` fully in effect —
        #     reproduced directly, and the reason the "only the scoped grant"
        #     claim was false as first written. Both scopes are now pinned.
        #
        #     Witnessed through GIT_CONFIG_SYSTEM/GIT_CONFIG_GLOBAL rather than
        #     by writing to /etc: a corpus that edits machine-level
        #     configuration to make a point is a worse idea than the defect. The
        #     control arms hand git the hostile file WITHOUT each guard and
        #     require it to be read, so neither arm can pass by the file simply
        #     being unreachable.
        hostile_cfg = d / "hostile.gitconfig"
        hostile_cfg.write_text("[user]\n\tname = HOSTILE-CONFIG-STATE\n")
        def _cfg(env_extra):
            return subprocess.run(
                [GIT, "-C", str(REPO), "config", "--get", "user.name"],
                capture_output=True, text=True, encoding="utf-8",
                errors="replace",
                env={"PATH": str(Path(GIT).parent), **env_extra}).stdout.strip()
        sys_leaks = _cfg({"GIT_CONFIG_SYSTEM": str(hostile_cfg)})
        sys_guarded = _cfg({"GIT_CONFIG_SYSTEM": str(hostile_cfg),
                            "GIT_CONFIG_NOSYSTEM": "1"})
        glob_leaks = _cfg({"GIT_CONFIG_GLOBAL": str(hostile_cfg)})
        glob_guarded = _cfg({"GIT_CONFIG_GLOBAL": os.devnull})
        out.append(("git's config scopes are pinned, not merely unset",
                    sys_leaks == "HOSTILE-CONFIG-STATE" and sys_guarded == ""
                    and glob_leaks == "HOSTILE-CONFIG-STATE" and glob_guarded == ""
                    and xg.GIT_ENV.get("GIT_CONFIG_NOSYSTEM") == "1"
                    and xg.GIT_ENV.get("GIT_CONFIG_GLOBAL") == os.devnull))

        # 5e. `-I` DOES NOT IMPLY `-S`. It implies -E, -P and -s: PYTHON*
        #     variables, the script directory, and the USER site directory. The
        #     SYSTEM site directory still initializes, so a `.pth` file or a
        #     `sitecustomize` there runs arbitrary code before the extractor's
        #     first line — an import hook, an exit, a changed result. Confirmed
        #     against a real `.pth` planted in system site-packages: under `-I`
        #     it printed; under `-I -S` it did not.
        #
        #     Carried here as the property rather than the plant, since a corpus
        #     that writes into system site-packages to prove a point is a worse
        #     idea than the defect. The control arm is the same probe without
        #     `-S`, which must show site loaded — otherwise this passes on an
        #     installation where there was nothing to disable.
        site_probe = ("import sys;"
                      "print('site' in sys.modules,"
                      "any('packages' in p for p in sys.path))")
        with_S = subprocess.run([sys.executable, "-I", "-S", "-c", site_probe],
                                capture_output=True, text=True, env={})
        without_S = subprocess.run([sys.executable, "-I", "-c", site_probe],
                                   capture_output=True, text=True, env={})
        out.append(("preflight children start with site initialization disabled",
                    with_S.stdout.strip() == "False False"
                    and without_S.stdout.strip() == "True True"))

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
    if total != EXPECTED_TOTAL:
        print(f"  [XX] CORPUS SIZE {total} != {EXPECTED_TOTAL}. `total` is derived "
              "from the lists, so a deleted case would otherwise print a clean "
              "N/N and exit 0 — the suite shrinking without a failing check.")
        failures += 1
    print("=" * 76)
    print(f"{total - failures}/{total} cases behaved as specified")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
