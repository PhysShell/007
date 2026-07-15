"""N2-D1b: contract tests for capture-canonicalization-policy.json --
the D1b-authorized (2026-07-16) Maven canonicalization profile for
repo-docker-java-parser only.
"""
import hashlib
import json
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import maven_canonicalizer as mc  # noqa: E402

POLICY_PATH = Path(__file__).resolve().parents[1] / "capture-canonicalization-policy.json"


def load_policy() -> dict:
    return json.loads(POLICY_PATH.read_text())


class TestCaptureCanonicalizationPolicy(unittest.TestCase):
    def test_policy_self_hash_is_stable(self):
        body = load_policy()
        recorded = body.pop("policy_sha256")
        canonical = json.dumps(body, indent=2, sort_keys=True) + "\n"
        recomputed = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
        self.assertEqual(recorded, recomputed)

    def test_applies_only_to_repo_docker_java_parser(self):
        body = load_policy()
        self.assertEqual(body["applicable_case_ids"], ["repo-docker-java-parser"])

    def test_selected_source_stream_is_stdout(self):
        body = load_policy()
        self.assertEqual(body["selected_source_stream"], "stdout")

    def test_exactly_five_rules_match_the_five_authorized_fields(self):
        body = load_policy()
        rule_names = {r["rule_name"] for r in body["rules"]}
        self.assertEqual(rule_names, {
            "buildnumber_timestamp", "scala_compile_duration", "surefire_time_elapsed",
            "maven_total_time", "maven_finished_at",
        })

    def test_rules_regex_text_matches_the_actual_canonicalizer_code(self):
        """The JSON's documented regex must never drift from the code that
        actually runs -- this policy is built FROM maven_canonicalizer.RULES,
        never hand-duplicated."""
        body = load_policy()
        by_name = {r["rule_name"]: r for r in body["rules"]}
        for rule in mc.RULES:
            self.assertEqual(by_name[rule.name]["anchored_regex"], rule.pattern.pattern)
            self.assertEqual(by_name[rule.name]["trigger_substring"], rule.trigger)
            self.assertEqual(by_name[rule.name]["placeholder"], rule.placeholder)

    def test_prohibited_transformations_include_no_faking_wall_clock_and_no_argv_change(self):
        body = load_policy()
        joined = " ".join(body["prohibited_transformations"]).lower()
        self.assertIn("wall-clock", joined)
        self.assertIn("frozen maven execution argv", joined)

    def test_prohibited_transformations_include_structural_invariants(self):
        body = load_policy()
        joined = " ".join(body["prohibited_transformations"]).lower()
        for phrase in ("deduplicating lines", "removing lines", "reordering lines"):
            self.assertIn(phrase, joined)

    def test_approving_decision_identity_present(self):
        body = load_policy()
        self.assertTrue(body["approving_decision_identity"])

    def test_evidence_run_references_the_real_ci_run_and_artifacts(self):
        body = load_policy()
        evidence = body["evidence_run"]
        self.assertEqual(evidence["workflow_run_id"], 29436883023)
        self.assertTrue(evidence["both_captures_independently_content_accepted"])

    def test_evidence_bounded_diff_contains_all_five_field_names(self):
        body = load_policy()
        diff = body["evidence_bounded_diff"]
        for marker in ("Storing buildNumber", "compile in", "Time elapsed", "Total time", "Finished at"):
            self.assertIn(marker, diff)

    def test_utf8_and_line_ending_policy_present(self):
        body = load_policy()
        self.assertIn("UTF-8", body["utf8_and_line_ending_policy"])


if __name__ == "__main__":
    unittest.main()
