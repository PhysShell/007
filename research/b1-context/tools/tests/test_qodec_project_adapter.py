"""Synthetic tests for the B1 <-> qodec-project adapter (Q2).

No Qodec binary and no case literal: these exercise the pure request-building and
context-reconstruction logic. The frozen B1 admission rule (in-force AND not an
agent claim) is verified, and the reconstruction is checked to carry a producer
identity that is NOT selector v0.
"""
import unittest

from o7b1 import qodec_project_adapter as ad


def _obs(oid, status, authority):
    return {"observation_id": oid, "status": status, "authority": authority,
            "kind": "status", "topics": ["t"],
            "provenance": [{"source_id": "s", "digest": "sha256:" + "0" * 64}],
            "statement": "s " + oid}


GOLD = {
    "observations": [
        _obs("o-current", "current", "repo_record"),
        _obs("o-pending-claim", "pending", "agent_claim"),
        _obs("o-superseded", "superseded", "repo_record"),
        _obs("o-src", "current", "repo_record"),
        _obs("o-stale", "superseded", "repo_record"),
    ],
    "relations": [{"from": "o-src", "kind": "supersedes", "to": "o-stale"}],
}
TASK_SPEC = {
    "questions_file": "q.yaml",
    "relation_requirements": [
        {"question": "qx", "from": "o-src", "kind": "supersedes", "direction": "outgoing",
         "depth": 1, "match": "all", "endpoint_policy": "edge_witness",
         "gold_derived_targets": ["o-stale"], "target_status": ["superseded"]}],
}
B0 = {"task_id": "t", "selected": [_obs("o-current", "current", "repo_record")],
      "omitted": [], "relations": []}


class AdapterTest(unittest.TestCase):
    def test_eligibility_in_force_and_not_agent_claim(self):
        req = ad.build_request("t", GOLD, TASK_SPEC, B0, 20000, 32, "r1")
        elig = {r["id"]: r["eligible"] for r in req["records"]}
        self.assertTrue(elig["o-current"])          # current, repo_record
        self.assertFalse(elig["o-pending-claim"])    # pending but agent_claim -> ineligible
        self.assertFalse(elig["o-superseded"])       # not in force
        self.assertTrue(elig["o-src"])
        self.assertFalse(elig["o-stale"])            # superseded

    def test_base_selected_marks_b0(self):
        req = ad.build_request("t", GOLD, TASK_SPEC, B0, 20000, 32, "r1")
        base = {r["id"]: r["base_selected"] for r in req["records"]}
        self.assertTrue(base["o-current"])
        self.assertFalse(base["o-src"])

    def test_requirements_mapped_verbatim(self):
        req = ad.build_request("t", GOLD, TASK_SPEC, B0, 20000, 32, "r1")
        self.assertEqual(len(req["relation_requirements"]), 1)
        rr = req["relation_requirements"][0]
        self.assertEqual(rr["from"], "o-src")
        self.assertEqual(rr["endpoint_policy"], "edge_witness")
        self.assertEqual(rr["expected_targets"], ["o-stale"])
        self.assertEqual(rr["direction"], "outgoing")

    def test_caller_evidence_is_opaque_status_authority(self):
        req = ad.build_request("t", GOLD, TASK_SPEC, B0, 20000, 32, "r1")
        ev = {r["id"]: r["caller_evidence"] for r in req["records"]}
        self.assertEqual(ev["o-pending-claim"], {"status": "pending", "authority": "agent_claim"})

    def test_reconstruct_context_identity_and_fields(self):
        result = {
            "selected": [
                {"id": "o-current", "payload": _obs("o-current", "current", "repo_record"),
                 "canonical_bytes": 10, "selection_reason_codes": ["base_selected"]},
                {"id": "o-src", "payload": _obs("o-src", "current", "repo_record"),
                 "canonical_bytes": 10, "selection_reason_codes": ["edge_witness_source"]}],
            "relations": [{"from": "o-src", "kind": "supersedes", "to": "o-stale"}],
            "omitted": [{"id": "o-stale", "reason_code": "relation_only_target", "detail": None}],
            "requirements": [],
        }
        producer = {"version": "qodec.project/v0", "implementation_sha256": "sha256:" + "a" * 64}
        ctx = ad.reconstruct_context("t", B0, result, producer)
        self.assertEqual(sorted(o["observation_id"] for o in ctx["selected"]), ["o-current", "o-src"])
        self.assertEqual(ctx["relations"], [{"from": "o-src", "kind": "supersedes", "to": "o-stale"}])
        self.assertEqual(ctx["selector"]["selector_id"], "o7.b1.qodec-project-arm/v0")
        self.assertNotEqual(ctx["selector"]["selector_id"], "o7.b1.selector/v0")
        self.assertEqual(ctx["selector"]["qodec_project_version"], "qodec.project/v0")
        # payload preserved exactly
        src = next(o for o in ctx["selected"] if o["observation_id"] == "o-src")
        self.assertEqual(src["status"], "current")
        self.assertTrue(src["selection_reason"].startswith("qodec-project:"))

    def test_no_case_literal_in_adapter_source(self):
        import os
        path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(ad.__file__))),
                            "o7b1", "qodec_project_adapter.py")
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
        for tok in ["case-0002", "obs-round0-closed", "qa3-superseded"]:
            self.assertNotIn(tok, text)


if __name__ == "__main__":
    unittest.main()
