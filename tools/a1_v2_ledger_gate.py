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
from another. There is NO bypass of any kind - no flag, no environment variable.
The corpus imports run_steps() instead of asking the executable to disarm.

Usage:  python3 tools/a1_v2_ledger_gate.py [--graph P] [--schema P] [--ledger P]
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
GRAPH_EXTRACTOR = REPO / "tools/a1_v2_extract_graph.py"
SCHEMA_EXTRACTOR = REPO / "tools/a1_v2_extract_schema.py"


Key = tuple[str, str, str]


class Report:
    """Three distinct verdicts, per AGENTS.md rule 2.

    FAIL   the gate ran and the target failed it.
    ERROR  the gate could not obtain a trustworthy answer at all.

    The distinction is not decoration. A preflight failure means the ruler is
    not the authority, and an invalidated premise means this verifier cannot
    represent the authority it was handed — in neither case has the realization
    been judged. Reporting either as FAIL says "Envelope v2 is not realized",
    which blames the target for a defect of the machinery: the same demotion
    E-R10 removed from the premise's diagnostic text, still present in the
    verdict vocabulary one floor up.

    TWO RULES THAT MUST SURVIVE ANY REFACTOR OF THIS CLASS:

    1. ERROR DOMINATES FAIL. A failed sub-check under an invalidated ruler is
       evidence about EXECUTION, not evidence that the target failed the gate.
       The observation stays in the report — it is true, and diagnostics need
       it — but it may not contribute to the terminal verdict, because the
       premise licensing the measurement is void. Let FAIL win here and ERROR
       becomes a decorative side-channel while FAIL keeps exercising authority
       it no longer has.

    2. ERROR IS NOT AN INFRASTRUCTURE-ERROR BUCKET. Do not later merge it with
       crashes, I/O failures or malformed input on the grounds that "something
       went wrong". Those may deserve different diagnostics, but if none of
       them permits a valid judgement they share the terminal semantics that
       matter to a caller: NO AUTHORITATIVE CLAIM ABOUT REALIZATION WAS
       PRODUCED — do not repair the target on the strength of this result.
       ERROR is an epistemic state, not a process outcome.
    """

    def __init__(self) -> None:
        self.failed = False
        self.errored = False
        self.lines: list[str] = []
        self.steps: dict[str, str] = {}

    def add(self, label: str, ok: bool, detail: str, owed: bool = False,
            error: bool = False) -> None:
        if not ok:
            if error:
                self.errored = True
            else:
                self.failed = True
        status = "PASS" if ok else ("ERROR" if error else ("OWED" if owed else "FAIL"))
        self.lines.append(f"  {label:<37} {status}   {detail}")
        n = label.split(".")[0]
        if n.isdigit():
            self.steps[n] = status

    def exit_code(self) -> int:
        """0 only on PASS. ERROR takes 2 so callers can tell the two apart."""
        return 2 if self.errored else (1 if self.failed else 0)

    def emit(self) -> int:
        print("A1-F v2 wire realization gate")
        print("=" * 76)
        for line in self.lines:
            print(line)
        print("=" * 76)
        if self.errored:
            verdict = ("ERROR — no trustworthy answer; the realization was NOT "
                       "judged. Repair the verifier, not the target.")
        elif self.failed:
            verdict = "FAIL — Envelope v2 is not realized"
        else:
            verdict = "PASS"
        print("RESULT:", verdict)
        return self.exit_code()


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


def run_steps(graph: dict, schema: dict, ledger: dict, r: Report | None = None) -> Report:
    """Steps 1-5 over three already-loaded terms.

    The corpus imports and calls THIS, which is why the shipped gate carries no
    preflight bypass of any kind. The previous shape kept an A1_V2_GATE_HARNESS
    env var that skipped both preflights and added no report line, so a harness
    run was textually indistinguishable from a proven PASS - and an env var set
    once in a shell survives every later invocation in it. A test seam that can
    silently disarm the acceptance executable is not a test seam.
    """
    r = r or Report()

    # ---- premise: step 3's key must be as discriminating as the norm ---------
    # Step 3 keys field capability by (source, target domain) and never looks at
    # class. That coarser representation is lossless only while class is
    # functionally determined by (source, target). The frozen registry has always
    # satisfied this and nothing declared it, so the guard was a property of the
    # DATA rather than of the check.
    #
    # This is a COMPATIBILITY PREMISE OF THIS VERIFIER, not a constraint on
    # Phase G. A supersede admitting (X, Y, A) and (X, Y, B) is a legitimate
    # authoritative shape; what it invalidates is this representation. The check
    # therefore lives here and not in the extractor — extraction failing would
    # mean "the authority is invalid", which is the demotion this whole document
    # exists to prevent, arriving through a schema assumption instead of through
    # a derivability claim.
    pair_class: dict[tuple[str, str], set[str]] = {}
    for e in graph["edges"]:
        pair_class.setdefault((e["source"], e["target"]), set()).add(e["class"])
    ambiguous = sorted(p for p, cs in pair_class.items() if len(cs) > 1)
    if ambiguous:
        src, tgt = ambiguous[0]
        r.add("premise: step 3 key adequacy", False, error=True, detail=
              f"VERIFIER PREMISE INVALIDATED — {src} -> {tgt} carries "
              f"{sorted(pair_class[(src, tgt)])}; step 3 assumes class is "
              "functionally determined by (source, target). Revise step 3 or "
              "supersede the verifier contract. Do NOT alter the authority.")

    carriers = ledger.get("carriers", [])

    # ---- ledger well-formedness: closed role, distinct rows ------------------
    # Two information-losing operations lived here, one per remaining species of
    # the family named in 7 (E-R9).
    #
    # ROLE SUBSTITUTION. carrier_kind is a CLOSED choice in frozen 4.2.6. Steps 2
    # and 3 each filter for one exact spelling, so a misspelled or invented third
    # role fell through both, and step 4 admitted the row on its triple alone —
    # a carrier with no role, an unverified path, and a PASS.
    #
    # MULTIPLICITY COLLAPSE. Step 3 unions target domains and steps 4/5 compare
    # sets, so a byte-for-byte duplicate row reduced to one observation: a
    # seventy-row ledger for a sixty-nine-edge graph reported PASS. Frozen 4.2.6
    # allows several carriers for one edge (the NormalizedOutput case) — it does
    # not make two declarations of the SAME concrete field occurrence into two
    # carriers.
    known_kinds = {"artifact_ref", "event_payload_digest"}
    ill: list[str] = []
    row_count: dict[tuple, int] = {}
    for c in carriers:
        kind = c.get("carrier_kind")
        if kind not in known_kinds:
            ill.append(f'{c.get("source")} -> {c.get("target")}: carrier_kind '
                       f'{kind!r} is not one of {sorted(known_kinds)}')
        row = (c.get("source"), c.get("target"), c.get("class"), kind, c.get("path"))
        row_count[row] = row_count.get(row, 0) + 1
    for row, n in sorted(row_count.items(), key=lambda kv: str(kv[0])):
        if n > 1:
            ill.append(f"{row[0]} -{row[2]}-> {row[1]} at {row[4]!r}: declared "
                       f"{n} times as one carrier occurrence")
    # The same rule on the OTHER observed term. Step 1's norm is set equality and
    # step 3 indexes facts by path, so `set(...)` and `{f["path"]: f}` were
    # silently normalising the middle term: a kind defined twice collapsed to
    # one, and two contradictory facts for one path resolved by LIST ORDER —
    # correct fact last, gate green. Neither is a judgement about the authority;
    # both are malformed observations, and an observation that cannot be read
    # unambiguously must not be reduced to one that can.
    schema_ill: list[str] = []
    kind_n: dict[str, int] = {}
    for k in schema.get("event_kinds") or []:
        kind_n[k] = kind_n.get(k, 0) + 1
    schema_ill += [f"event kind {k!r} declared {n} times" for k, n in sorted(kind_n.items()) if n > 1]
    path_n: dict[str, int] = {}
    for f in schema.get("artifact_ref_fields") or []:
        path_n[f.get("path")] = path_n.get(f.get("path"), 0) + 1
    schema_ill += [f"field path {q!r} has {n} fact rows" for q, n in sorted(path_n.items(), key=str) if n > 1]
    if schema.get("event_kinds") or schema.get("artifact_ref_fields"):
        r.add("schema facts well-formedness", not schema_ill,
              f'{len(kind_n)} kinds, {len(path_n)} field paths, each declared once'
              if not schema_ill else "; ".join(schema_ill[:2]))

    if carriers:
        r.add("ledger well-formedness", not ill,
              f"{len(carriers)} rows, closed roles, no repeated occurrence" if not ill
              else "; ".join(ill[:2]))
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
        # Keyed by (source, TARGET). Counting by source alone bound the digest to
        # an event kind but not to its payload, and eleven of the twenty-one
        # kinds carry more than one frozen edge — so a ledger could move the
        # digest onto a sibling edge of the same source and keep the count at
        # one. `CampaignEvent(HumanCommandRejected)` has rows 56 and 64; swapping
        # which one carried the digest passed all five steps while the required
        # relation --event_payload_digest--> HumanCommandRejectedPayload did not
        # exist. The frozen requirement is a PAIR, so the key must be the pair.
        struct_count: dict[tuple[str, str], int] = {}
        for c in carriers:
            if c.get("carrier_kind") == "event_payload_digest":
                key = (c["source"], c["target"])
                struct_count[key] = struct_count.get(key, 0) + 1
        # The REQUIREMENT comes from the frozen presence map, never from the
        # schema. Frozen 4.2.6: `expected_payload(k) = P => EXACTLY ONE ledger
        # carrier CampaignEvent(k) --event_payload_digest--> P`. Deriving `want`
        # from schema.structural_commitments let the term under test lower its
        # own bar: a schema with a correct presence map that commits no
        # structural digests made zero carriers the expected count, and a ledger
        # relabelling all eleven payload edges as artifact_ref then passed steps
        # 3, 4 and 5 as well — a full green on a realization carrying none of
        # the eleven structural digests the contract requires.
        required_pairs = {(f"CampaignEvent({k})", v) for k, v in expected_payload.items() if v}
        required_sources = {s for s, _ in required_pairs}
        struct_decl = schema.get("structural_commitments") or {}
        schema_struct = {f"CampaignEvent({k})" for k in struct_decl}
        if schema_struct != required_sources:
            missing = sorted(s[len("CampaignEvent("):-1] for s in required_sources - schema_struct)
            extra = sorted(s[len("CampaignEvent("):-1] for s in schema_struct - required_sources)
            bad.append(f"schema commits structural digests for {len(schema_struct)} kinds, "
                       f"frozen map requires {len(required_sources)}: "
                       f"missing={missing[:3]} extra={extra[:3]}")
        # want=1 catches a required pair left uncarried; want=0 catches the
        # carrier kind appearing on any edge the frozen map does not name.
        for src, tgt in sorted(required_pairs | set(struct_count)):
            want = 1 if (src, tgt) in required_pairs else 0
            got = struct_count.get((src, tgt), 0)
            if got != want:
                bad.append(f"{src} --event_payload_digest--> {tgt}: {got} carriers, "
                           f"expected exactly {want}")
        # And the PATH. Steps 4 and 5 ignore paths and step 3 only ever looks at
        # artifact_ref rows, so a structural carrier's path was the one ledger
        # column nothing checked: every digest could name `does-not-exist` and
        # all five steps passed. Frozen 4.2.6 lists the concrete carrier path as
        # part of a ledger row, and a column nothing checks is not evidence.
        for c in carriers:
            if c.get("carrier_kind") != "event_payload_digest":
                continue
            src = c["source"]
            kind = src[len("CampaignEvent("):-1] if src.startswith("CampaignEvent(") else src
            want_path = struct_decl.get(kind)
            if want_path is None:
                bad.append(f"{src}: structural carrier for a kind the schema does not commit")
            elif c.get("path") != want_path:
                bad.append(f"{src}: structural path {c.get('path')!r} != schema {want_path!r}")
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

    return r


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--graph", default=str(GRAPH))
    ap.add_argument("--schema", default=str(SCHEMA))
    ap.add_argument("--ledger", default=str(LEDGER))
    args = ap.parse_args()

    r = Report()

    # ---- preflight: both non-ledger terms must be DERIVABLE, not merely present.
    # Unconditional. Each --check is bound to the very path the gate then reads;
    # checking the default artifact and then reading an overridden one would
    # re-open exactly the hole the preflight exists to close.
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
              else (p.stderr.strip().split("\n")[0] or "mismatch"),
              error=p.returncode != 0)
        if p.returncode != 0:
            # Every step below would be measured against a forged ruler.
            return r.emit()

    # ---- loading is part of obtaining a judgement, so its failures are ERROR --
    # An unreadable, unparseable or wrongly-shaped artifact produced an uncaught
    # exception: no RESULT line at all, and Python's default exit status of 1 —
    # which this contract assigns to "the gate ran and the realization failed
    # it". A consumer parsing the exit code was told the target had been
    # measured and had lost. Report.__doc__ rule 2 already said crashes, I/O
    # failures and malformed input share ERROR's terminal semantics; the rule
    # was written one revision before the path that needed it.
    loaded = {}
    for name, path in (("graph", args.graph), ("schema", args.schema),
                       ("ledger", args.ledger)):
        try:
            obj = json.loads(Path(path).read_text())
        except OSError as exc:
            r.add(f"load: {name}", False, f"unreadable: {exc}", error=True)
            return r.emit()
        except json.JSONDecodeError as exc:
            r.add(f"load: {name}", False, f"not JSON: {exc}", error=True)
            return r.emit()
        if not isinstance(obj, dict):
            r.add(f"load: {name}", False,
                  f"top level is {type(obj).__name__}, expected an object", error=True)
            return r.emit()
        loaded[name] = obj

    carriers = loaded["ledger"].get("carriers", [])
    if not isinstance(carriers, list) or not all(isinstance(c, dict) for c in carriers):
        r.add("load: ledger", False,
              "`carriers` must be a list of objects, one row per carrier", error=True)
        return r.emit()

    run_steps(loaded["graph"], loaded["schema"], loaded["ledger"], r)
    return r.emit()


if __name__ == "__main__":
    sys.exit(main())
