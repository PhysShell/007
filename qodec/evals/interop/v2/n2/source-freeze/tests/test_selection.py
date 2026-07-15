"""Section 23 tests for selection.py: quota satisfaction, determinism,
alternate ordering, and the "no manual override" contract."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import registry  # noqa: E402
import eligibility  # noqa: E402
import selection  # noqa: E402

SOURCE_FREEZE_DIR = Path(__file__).resolve().parents[1]


def _eligible_candidates():
    reg = registry.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
    reports = eligibility.evaluate_registry(reg)
    eligible_ids = {r["candidate_id"] for r in reports if r["eligible"]}
    return [c for c in reg["candidates"] if c["candidate_id"] in eligible_ids]


class TestRealSelectionSatisfiesAllQuotas(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.eligible = _eligible_candidates()
        cls.result = selection.run_selection(cls.eligible)

    def test_status_is_final(self):
        self.assertEqual(self.result["status"], "FINAL")

    def test_exactly_17_primary_cases(self):
        self.assertEqual(len(self.result["primary_case_ids"]), 17)

    def test_at_least_8_alternates(self):
        self.assertGreaterEqual(self.result["alternate_count"], 8)
        self.assertTrue(self.result["minimum_alternates_satisfied"])

    def test_origin_kind_targets_exactly_met(self):
        self.assertEqual(self.result["unmet_origin_kind_targets"], {})

    def test_primary_family_targets_exactly_met(self):
        self.assertEqual(self.result["unmet_primary_family_targets"], {})

    def test_ecosystem_minimums_met(self):
        self.assertEqual(self.result["unmet_ecosystem_minimums"], {})

    def test_n2a_reference_not_in_selection(self):
        # N2-A's own case (miner-canary-dotnet-001) is not a candidate in
        # this registry at all — it is accounted for only via the
        # quota-contract's reduced ("remaining") targets.
        self.assertNotIn("miner-canary-dotnet-001", self.result["primary_case_ids"])


class TestDeterminism(unittest.TestCase):
    def test_two_independent_runs_are_byte_identical(self):
        import json
        eligible = _eligible_candidates()
        result_a = selection.run_selection(eligible)
        result_b = selection.run_selection(eligible)
        self.assertEqual(json.dumps(result_a, sort_keys=True), json.dumps(result_b, sort_keys=True))

    def test_alternate_order_is_deterministic(self):
        eligible = _eligible_candidates()
        result_a = selection.run_selection(eligible)
        result_b = selection.run_selection(eligible)
        self.assertEqual(result_a["alternate_case_ids"], result_b["alternate_case_ids"])


class TestQuotaEnforcement(unittest.TestCase):
    def test_primary_without_quota_assignment_impossible_by_construction(self):
        # Every candidate in primary_case_ids was placed there because it
        # matched an origin_family_group bucket with remaining capacity —
        # there is no code path that adds a candidate to primary_case_ids
        # without going through plan_selection's quota bookkeeping or the
        # deterministic top-up/reconciliation steps, both of which only
        # select FROM the ranked eligible pool.
        eligible = _eligible_candidates()
        result = selection.run_selection(eligible)
        by_id = {c["candidate_id"]: c for c in eligible}
        quota_contract = selection.load_quota_contract()
        valid_buckets = set(quota_contract["quotas"]["origin_family_group"])
        for cid in result["primary_case_ids"]:
            self.assertIn(by_id[cid]["origin_family_group"], valid_buckets)

    def test_every_alternate_gets_a_positive_fallback_priority(self):
        import generate_manifests
        eligible = _eligible_candidates()
        result = selection.run_selection(eligible)
        by_id = {c["candidate_id"]: c for c in eligible}
        for priority, cid in enumerate(result["alternate_case_ids"], start=1):
            manifest = generate_manifests.build_source_manifest(by_id[cid], "alternate", priority, [])
            self.assertEqual(manifest["fallback_priority"], priority)
            self.assertGreater(manifest["fallback_priority"], 0)


if __name__ == "__main__":
    unittest.main()
