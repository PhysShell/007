"""Evaluation-engine tests on SYNTHETIC state (no private RAW).

They pin the core measured behaviours after the corrective round: honest metric
names, a real provenance ratio, negative-control claims split into independently
supported fields, and deterministic scoring.
"""
import unittest

from o7b1 import evaluate as ev
from o7b1 import projector as pj
from o7b1.canonical import canonical_bytes

# `unittest discover -s tests` makes this directory the top-level package
# root, so the shared fixtures are imported as a plain sibling module.
import synth

GOLD = synth.GOLD
QUESTIONS = synth.QUESTIONS_B
SOURCE_SELECTORS = synth.SOURCE_SELECTORS
MANIFEST = synth.MANIFEST
NC_OBJ = synth.NC_OBJ


class EvaluationTest(unittest.TestCase):
    def _run(self, nc=NC_OBJ, questions=QUESTIONS):
        sel = pj.select(GOLD, synth.task_b(), byte_budget=20000, record_budget=32)
        derived_summary = {"total_bytes": 99999, "total_records": 50}
        return ev.evaluate(GOLD, questions, SOURCE_SELECTORS, MANIFEST, nc,
                           derived_summary, sel, 1234, synth.BUDGET), sel

    def test_projection_structural_coverage(self):
        result, _ = self._run()
        C = result["arms"]["projection"]
        self.assertEqual(C["compiled_observation_coverage"], 1.0)
        self.assertEqual(C["structural_question_support"], 1.0)
        self.assertTrue(C["has_carried_provenance"])
        self.assertEqual(C["presented_provenance_ratio"], 1.0)

    def test_old_misleading_metric_names_are_gone(self):
        """Renamed in the corrective round; nothing may reintroduce them."""
        result, _ = self._run()
        for arm in result["arms"].values():
            self.assertNotIn("required_observation_recall", arm)
            self.assertNotIn("question_support_coverage", arm)
            self.assertNotIn("provenance_coverage", arm)

    def test_metric_semantics_are_published_with_the_numbers(self):
        result, _ = self._run()
        sem = result["metric_semantics"]
        self.assertIn("NOT semantic recall", sem["compiled_observation_coverage"])
        self.assertIn("NOT evidence the question can be answered",
                      sem["structural_question_support"])

    def test_projection_validity_block_present_and_valid(self):
        result, _ = self._run()
        C = result["arms"]["projection"]
        self.assertIsNotNone(C["projection_validity"])
        self.assertTrue(C["projection_validity"]["valid"])

    def test_families_are_null_where_not_applicable(self):
        """No arm gets a flattering zero from a family that does not apply."""
        result, _ = self._run()
        A = result["arms"]["full_derived"]
        B = result["arms"]["negative_control"]
        C = result["arms"]["projection"]
        self.assertIsNone(A["projection_validity"])
        self.assertIsNone(A["negative_control_diagnostics"])
        self.assertIsNone(A["presented_provenance_ratio"])
        self.assertIsNone(B["projection_validity"])
        self.assertIsNotNone(B["negative_control_diagnostics"])
        self.assertIsNone(C["negative_control_diagnostics"])

    def test_negative_control_diagnostics(self):
        result, _ = self._run()
        B = result["arms"]["negative_control"]
        d = B["negative_control_diagnostics"]
        self.assertEqual(B["compiled_observation_coverage"], 0.0)
        self.assertEqual(d["stale_belief_count"], 1)
        self.assertIn("obs-claim-separate", d["stale_belief_ids"])
        self.assertIn("obs-truth-topology", d["required_superseders_missing"])

    def test_full_derived_carries_capture_backed_obs(self):
        result, _ = self._run()
        A = result["arms"]["full_derived"]
        self.assertIn("obs-truth-topology", A["carried_required_observation_ids"])
        self.assertNotIn("obs-beta-status", A["carried_required_observation_ids"])

    def test_deterministic(self):
        r1, _ = self._run()
        r2, _ = self._run()
        self.assertEqual(canonical_bytes(r1), canonical_bytes(r2))


class NegativeControlSplitTest(unittest.TestCase):
    """Corrective round §7: three propositions, three fields, no conflation."""

    def test_absent_corrective_reference_is_reported_separately(self):
        div = ev.detect_nc_divergences(GOLD, NC_OBJ, MANIFEST)["obs-claim-separate"]
        self.assertTrue(div["corrective_evidence_absent_from_negative_control"])
        self.assertTrue(div["fixture_expected_divergence"])
        self.assertIn("does not by itself prove", div["establishes"])

    def test_contrary_claim_detected_when_marker_present(self):
        div = ev.detect_nc_divergences(GOLD, NC_OBJ, MANIFEST)["obs-claim-separate"]
        self.assertIs(div["contrary_claim_detected"], True)
        self.assertIn("separate conversation", div["contrary_markers_found"])

    def test_contrary_claim_not_evaluated_without_markers(self):
        gold = synth.gold()
        for o in gold["observations"]:
            if o["observation_id"] == "obs-claim-separate":
                o.pop("structured_value", None)
        div = ev.detect_nc_divergences(gold, NC_OBJ, MANIFEST)["obs-claim-separate"]
        self.assertIsNone(div["contrary_claim_detected"])

    def test_absence_of_marker_does_not_imply_contrary_claim(self):
        """The NC lacks the corrective id AND lacks the contrary phrasing: the
        first is true, the second must be False — not silently inherited."""
        nc = {"events": [{"seq": 0, "gist": "no opinion on topology"}]}
        div = ev.detect_nc_divergences(GOLD, nc, MANIFEST)["obs-claim-separate"]
        self.assertTrue(div["corrective_evidence_absent_from_negative_control"])
        self.assertIs(div["contrary_claim_detected"], False)

    def test_corrected_negative_control_is_not_flagged_stale(self):
        div = ev.detect_nc_divergences(
            GOLD, synth.NC_OBJ_CORRECTED, MANIFEST)["obs-claim-separate"]
        self.assertFalse(div["corrective_evidence_absent_from_negative_control"])


class IntegrationTest(unittest.TestCase):
    """Full CAS-backed pipeline — runs only if the owner's local CAS exists."""

    def test_full_run_if_cas_available(self):
        import os
        import tempfile
        # O7B1_DATA_ROOT lets the offline suite deliberately point at a
        # nonexistent CAS so this one test skips (Part 2 offline behaviour).
        data_root = os.environ.get(
            "O7B1_DATA_ROOT", os.path.expanduser("~/.local/share/o7-research"))
        cas_root = os.path.join(data_root, "cas")
        probe = os.path.join(cas_root, "sha256", "6a",
                             "86185402fd69d2fe4aad425a257b06c9bcfc4b28decc0b690d9b314f4066b9")
        if not os.path.isfile(probe):
            self.skipTest("local CAS with private RAW blobs not present")
        fdir = os.path.normpath(os.path.join(
            os.path.dirname(__file__), "..", "..", "fixtures", "case-0001"))
        if not os.path.isfile(os.path.join(fdir, "expected-report-v1.json")):
            self.skipTest("expected-report-v1.json not yet frozen (pre-regeneration)")
        import run_case
        with tempfile.TemporaryDirectory() as td:
            result = run_case.run("case-0001", data_root, td)
            st = result["report"]["status"]
            self.assertEqual(st["development_result"], "PASS")
            self.assertEqual(st["task_conditioned_projection"], "IMPLEMENTED")
            self.assertTrue(result["report"]["task_dependence"]["projections_differ"])
            self.assertTrue(
                result["report"]["task_dependence"]["acceptance"]["accepted"])


if __name__ == "__main__":
    unittest.main()
