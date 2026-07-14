"""Tests for quota_planner.py — provisional, quota-aware selection planning."""
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import registry  # noqa: E402
import scorer  # noqa: E402
import quota_planner  # noqa: E402

EXAMPLE_PATH = MINER_DIR / "candidate-registry.example.json"


class TestQuotaPlanner(unittest.TestCase):
    def setUp(self):
        reg = registry.load_registry(EXAMPLE_PATH)
        self.candidates = registry.eligible_candidates(reg)
        self.by_id = {c["candidate_id"]: c for c in self.candidates}
        self.ranked = scorer.rank_candidates(self.candidates)

    def test_plan_is_marked_provisional_not_a_freeze(self):
        plan = quota_planner.plan_selection(self.ranked, self.by_id, {"ecosystem": {"dotnet": 1}})
        self.assertEqual(plan["status"], "PROVISIONAL")
        self.assertIn("NOT A CORPUS FREEZE", plan["notes"])
        self.assertIn("NO QODEC/RTK EVALUATION PERFORMED", plan["notes"])

    def test_unfilled_quotas_reported_as_deficits(self):
        # No swift/ecosystem exists among the eligible candidates at all.
        plan = quota_planner.plan_selection(self.ranked, self.by_id, {"ecosystem": {"swift": 1}})
        self.assertIn("ecosystem", plan["unfilled_quotas"])
        self.assertEqual(plan["unfilled_quotas"]["ecosystem"], {"swift": 1})
        self.assertEqual(plan["proposed_selection"], [])

    def test_satisfiable_quota_leaves_nothing_unfilled(self):
        plan = quota_planner.plan_selection(self.ranked, self.by_id, {"ecosystem": {"dotnet": 1}})
        self.assertEqual(plan["unfilled_quotas"], {})
        self.assertIn("synthetic-dotnet-eligible", plan["proposed_selection"] + [plan["proposed_selection"][0]])

    def test_selection_trace_is_deterministic(self):
        quotas = {"ecosystem": {"dotnet": 1, "rust": 1, "python": 1}}
        plan_a = quota_planner.plan_selection(self.ranked, self.by_id, quotas)
        plan_b = quota_planner.plan_selection(self.ranked, self.by_id, quotas)
        self.assertEqual(plan_a["selection_trace"], plan_b["selection_trace"])

    def test_eligible_alternatives_are_never_proposed(self):
        quotas = {"ecosystem": {"dotnet": 1}}
        plan = quota_planner.plan_selection(self.ranked, self.by_id, quotas)
        overlap = set(plan["proposed_selection"]) & set(plan["eligible_alternatives"])
        self.assertEqual(overlap, set())


if __name__ == "__main__":
    unittest.main()
