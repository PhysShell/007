"""Tests for scorer.py — deterministic CandidateScorer."""
import json
import sys
import unittest
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(MINER_DIR / "tools"))
import registry  # noqa: E402
import scorer  # noqa: E402

EXAMPLE_PATH = MINER_DIR / "candidate-registry.example.json"


def _load_eligible():
    reg = registry.load_registry(EXAMPLE_PATH)
    return registry.eligible_candidates(reg)


class TestDeterministicRanking(unittest.TestCase):
    def test_ranking_is_byte_identical_across_runs(self):
        candidates = _load_eligible()
        ranking_a = scorer.rank_candidates(candidates)
        ranking_b = scorer.rank_candidates(candidates)
        self.assertEqual(json.dumps(ranking_a, sort_keys=True), json.dumps(ranking_b, sort_keys=True))

    def test_ranking_is_byte_identical_from_a_freshly_reloaded_registry(self):
        ranking_a = scorer.rank_candidates(_load_eligible())
        ranking_b = scorer.rank_candidates(_load_eligible())
        self.assertEqual(json.dumps(ranking_a, sort_keys=True), json.dumps(ranking_b, sort_keys=True))

    def test_all_ranked_candidates_get_a_rank_within_their_quota_group(self):
        ranking = scorer.rank_candidates(_load_eligible())
        for entry in ranking:
            self.assertIn("rank_within_group", entry)
            self.assertGreaterEqual(entry["rank_within_group"], 1)

    def test_tie_break_is_a_pure_function_of_candidate_id_and_commit(self):
        candidate = {"candidate_id": "x", "commit_sha": "a" * 40}
        self.assertEqual(scorer.tie_break_value(candidate), scorer.tie_break_value(candidate))
        other = {"candidate_id": "y", "commit_sha": "a" * 40}
        self.assertNotEqual(scorer.tie_break_value(candidate), scorer.tie_break_value(other))

    def test_tie_break_matches_documented_sha256_formula_not_time_or_random(self):
        # section 6: "SHA256(candidate_id + commit SHA)" — never wall-clock or
        # random, which would make ranking non-reproducible across runs.
        import hashlib
        candidate = {"candidate_id": "x", "commit_sha": "a" * 40}
        expected = hashlib.sha256(b"x" + b"a" * 40).hexdigest()
        self.assertEqual(scorer.tie_break_value(candidate), expected)


class TestForbiddenScoringInputsRejected(unittest.TestCase):
    def _base_candidate(self):
        return {
            "candidate_id": "poisoned",
            "commit_sha": "a" * 40,
            "ecosystem": "dotnet",
            "license": {"status": "clear"},
            "project": {"ambiguous": False},
            "network_requirements": {},
            "dependency_lock": {"present": True},
            "expected_log_family": "build-success",
            "estimated_resource_class": "small",
            "security_flags": [],
            "reproducibility_class": "unknown",
            "origin_kind": "synthetic-first-party",
            "evidence_references": ["x"],
        }

    def test_qodec_field_rejected(self):
        candidate = self._base_candidate()
        candidate["qodec_token_reduction"] = 0.9
        with self.assertRaises(ValueError):
            scorer.score_candidate(candidate)

    def test_rtk_field_rejected(self):
        candidate = self._base_candidate()
        candidate["rtk_score"] = 0.5
        with self.assertRaises(ValueError):
            scorer.score_candidate(candidate)

    def test_token_savings_field_rejected(self):
        candidate = self._base_candidate()
        candidate["estimated_token_savings"] = 1234
        with self.assertRaises(ValueError):
            scorer.score_candidate(candidate)

    def test_github_stars_field_rejected(self):
        candidate = self._base_candidate()
        candidate["github_stars"] = 9001
        with self.assertRaises(ValueError):
            scorer.score_candidate(candidate)

    def test_nested_poisoned_field_rejected(self):
        candidate = self._base_candidate()
        candidate["evaluation"] = {"winning_arm": "qodec"}
        with self.assertRaises(ValueError):
            scorer.score_candidate(candidate)

    def test_clean_candidate_is_not_rejected(self):
        candidate = self._base_candidate()
        result = scorer.score_candidate(candidate)
        self.assertIn("final_score", result)


if __name__ == "__main__":
    unittest.main()
