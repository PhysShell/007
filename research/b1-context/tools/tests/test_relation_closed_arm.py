"""Synthetic tests for the relation-closure control producer (Q1 cell C1).

No case literal here — a small invented gold graph and B0 context. Proves the
closure materializes exactly the demanded sources/targets, adds only exact gold
edges, keeps stale edge_witness targets relation-only, and fails closed on an
ineligible source or a contract/gold target disagreement.
"""
import unittest

from o7b1 import relation_closed_arm as rca


def _obs(oid, status, authority, kind="status"):
    return {"observation_id": oid, "status": status, "authority": authority, "kind": kind,
            "topics": ["t"], "provenance": [{"source_id": "s", "digest": "sha256:" + "0" * 64}],
            "statement": "stmt " + oid}


GOLD = {
    "observations": [
        _obs("obs-src-1", "current", "repo_record"),
        _obs("obs-src-2", "current", "repo_record"),
        _obs("obs-cur-1", "current", "repo_record"),
        _obs("obs-stale-1", "superseded", "repo_record"),
        _obs("obs-claim", "pending", "agent_claim"),
        _obs("obs-keep", "current", "repo_record"),
    ],
    "relations": [
        {"from": "obs-src-1", "kind": "supersedes", "to": "obs-stale-1"},
        {"from": "obs-src-2", "kind": "depends_on", "to": "obs-cur-1"},
        {"from": "obs-claim", "kind": "supersedes", "to": "obs-stale-1"},
    ],
}


def _b0():
    return {"task_id": "t-syn", "selected": [dict(_obs("obs-keep", "current", "repo_record"),
                                                  selection_reason="r", selection_score=1.0)],
            "omitted": [{"observation_id": "obs-src-1", "reason": "not relevant"},
                        {"observation_id": "obs-src-2", "reason": "not relevant"},
                        {"observation_id": "obs-cur-1", "reason": "not relevant"}],
            "relations": []}


REQS = [
    {"question": "q1", "from": "obs-src-1", "kind": "supersedes", "direction": "outgoing",
     "depth": 1, "match": "all", "endpoint_policy": "edge_witness",
     "gold_derived_targets": ["obs-stale-1"], "target_status": ["superseded"]},
    {"question": "q2", "from": "obs-src-2", "kind": "depends_on", "direction": "outgoing",
     "depth": 1, "match": "all", "endpoint_policy": "all_current_targets_materialized",
     "gold_derived_targets": ["obs-cur-1"], "target_status": ["current"]},
]


class RelationClosureTest(unittest.TestCase):
    def setUp(self):
        self.idx = rca._gold_index(GOLD)

    def test_materializes_sources_and_current_targets(self):
        res = rca.close_task("t-syn", REQS, self.idx, _b0())
        # rule 12: requirement order, then observation id. Req0 -> obs-src-1;
        # Req1 -> {obs-src-2, obs-cur-1} ordered by id -> obs-cur-1, obs-src-2.
        self.assertEqual(res["materialized"], ["obs-src-1", "obs-cur-1", "obs-src-2"])
        sel_ids = {o["observation_id"] for o in res["context"]["selected"]}
        self.assertIn("obs-src-1", sel_ids)
        self.assertIn("obs-cur-1", sel_ids)
        # stale edge_witness target is NOT selected (relation-only)
        self.assertNotIn("obs-stale-1", sel_ids)
        # original selection preserved
        self.assertIn("obs-keep", sel_ids)

    def test_adds_only_exact_gold_edges_no_fabrication(self):
        res = rca.close_task("t-syn", REQS, self.idx, _b0())
        edges = {(r["from"], r["kind"], r["to"]) for r in res["context"]["relations"]}
        self.assertIn(("obs-src-1", "supersedes", "obs-stale-1"), edges)
        self.assertIn(("obs-src-2", "depends_on", "obs-cur-1"), edges)
        # no edge to a non-gold target
        for (f, k, t) in edges:
            self.assertIn(t, {"obs-stale-1", "obs-cur-1"})

    def test_removes_materialized_from_omitted(self):
        res = rca.close_task("t-syn", REQS, self.idx, _b0())
        om = {o["observation_id"] for o in res["context"]["omitted"]}
        self.assertNotIn("obs-src-1", om)
        self.assertNotIn("obs-cur-1", om)

    def test_materialized_records_keep_exact_gold_fields(self):
        res = rca.close_task("t-syn", REQS, self.idx, _b0())
        src = next(o for o in res["context"]["selected"] if o["observation_id"] == "obs-src-1")
        self.assertEqual(src["status"], "current")
        self.assertEqual(src["authority"], "repo_record")
        self.assertEqual(src["statement"], "stmt obs-src-1")

    def test_fail_closed_on_agent_claim_source(self):
        reqs = [{"question": "q1", "from": "obs-claim", "kind": "supersedes", "direction": "outgoing",
                 "depth": 1, "match": "all", "endpoint_policy": "edge_witness",
                 "gold_derived_targets": ["obs-stale-1"], "target_status": ["superseded"]}]
        with self.assertRaises(rca.ClosureError):
            rca.close_task("t-syn", reqs, self.idx, _b0())

    def test_fail_closed_on_target_disagreement(self):
        reqs = [dict(REQS[0], gold_derived_targets=["obs-cur-1"])]  # wrong target
        with self.assertRaises(rca.ClosureError):
            rca.close_task("t-syn", reqs, self.idx, _b0())

    def test_no_requirements_is_identity(self):
        res = rca.close_task("t-syn", [], self.idx, _b0())
        self.assertEqual(res["materialized"], [])
        self.assertEqual(res["context"]["relations"], [])
        self.assertEqual(len(res["context"]["selected"]), 1)

    def test_producer_identity_not_selector_v0(self):
        res = rca.close_task("t-syn", REQS, self.idx, _b0())
        self.assertEqual(res["context"]["selector"]["selector_id"], "o7.b1.relation-closed-control/v0")
        self.assertNotEqual(res["context"]["selector"]["selector_id"], "o7.b1.selector/v0")


if __name__ == "__main__":
    unittest.main()
