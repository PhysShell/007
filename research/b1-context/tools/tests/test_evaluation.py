"""Evaluation-engine tests on SYNTHETIC state (no private RAW).

They pin the core measured behaviours: the projection carries required
observations with provenance; the negative control *contradicts* (not merely
omits) a captured fact; and the scoring is deterministic.
"""
import unittest

from o7b1 import evaluate as ev
from o7b1 import projector as pj
from o7b1.canonical import canonical_bytes

# A minimal synthetic gold state: one captured truth, one superseded agent_claim
# belief that contradicts it, plus a couple of repo-record observations.
GOLD = {
    "schema": "o7.b1.state-observables/v0",
    "fixture_id": "syn",
    "cutoff": {"identity": "syn-cutoff"},
    "observations": [
        {"observation_id": "obs-truth-topology", "kind": "evidence",
         "statement": "X and Y are one conversation.", "authority": "platform_capture",
         "status": "current",
         "provenance": [{"source_id": "cap-xy", "digest": "sha256:" + "a" * 64}]},
        {"observation_id": "obs-goal", "kind": "goal", "statement": "the goal.",
         "authority": "controller_or_repo_record", "status": "current",
         "provenance": [{"source_id": "readme"}]},
        {"observation_id": "obs-claim-separate", "kind": "evidence",
         "statement": "belief: X and Y are separate.", "authority": "agent_claim",
         "status": "superseded", "superseded_by": "obs-truth-topology",
         "provenance": [{"source_id": "nc-recon", "digest": "sha256:" + "b" * 64}]},
    ],
    "relations": [
        {"kind": "supersedes", "from": "obs-truth-topology", "to": "obs-claim-separate"},
        {"kind": "contradicts", "from": "obs-claim-separate", "to": "obs-truth-topology"},
    ],
}
QUESTIONS = {"questions": [
    {"id": "q-topology", "required_observation_ids": ["obs-truth-topology"]},
    {"id": "q-goal", "required_observation_ids": ["obs-goal"]},
]}
SOURCE_SELECTORS = {"sessions": [
    {"session_id": "sess-xy", "raw_source": "cap-xy"},
]}
MANIFEST = {
    "raw_sources": [
        {"id": "cap-xy", "digest": "sha256:" + "a" * 64, "byte_length": 10,
         "native_conversation_id": "native-xy"},
        {"id": "capture-main-dialog", "digest": "sha256:" + "c" * 64, "byte_length": 10,
         "native_conversation_id": "native-bd"},
        {"id": "capture-compaction", "digest": "sha256:" + "d" * 64, "byte_length": 10,
         "native_conversation_id": "native-e"},
    ],
    "negative_control": [
        {"id": "nc-recon", "digest": "sha256:" + "b" * 64, "byte_length": 20,
         "known_divergence_already_observed": "assumed X and Y separate; unaware of E"}],
}
# NC object that does NOT reference the captured native ids -> divergence detected.
NC_OBJ = {"events": [{"seq": 0, "role": "assistant", "gist": "reconstruction"}],
          "subject": "session recall without native ids"}


class EvaluationTest(unittest.TestCase):
    def _run(self):
        sel = pj.select(GOLD, byte_budget=20000, record_budget=32)
        ctx_bytes = 1234
        derived_summary = {"total_bytes": 99999, "total_records": 50}
        return ev.evaluate(GOLD, QUESTIONS, SOURCE_SELECTORS, MANIFEST, NC_OBJ,
                           derived_summary, sel, ctx_bytes), sel

    def test_projection_full_recall_and_coverage(self):
        result, _ = self._run()
        C = result["arms"]["projection"]
        self.assertEqual(C["required_observation_recall"], 1.0)
        self.assertEqual(C["question_support_coverage"], 1.0)
        self.assertEqual(C["contradiction_count"], 0)
        self.assertEqual(C["provenance_coverage"], 1.0)

    def test_negative_control_contradicts_not_just_omits(self):
        result, _ = self._run()
        B = result["arms"]["negative_control"]
        self.assertEqual(B["required_observation_recall"], 0.0)
        self.assertEqual(B["contradiction_count"], 1)
        self.assertEqual(B["supersession_error_count"], 1)
        self.assertIn("obs-truth-topology", B["contradicted_required_observation_ids"])

    def test_full_derived_carries_capture_backed_obs(self):
        result, _ = self._run()
        A = result["arms"]["full_derived"]
        # obs-truth-topology is provenance-satisfiable from the transcript -> carried
        self.assertIn("obs-truth-topology", A["carried_required_observation_ids"])
        # obs-goal cites a repo record not in the transcript -> not carried
        self.assertNotIn("obs-goal", A["carried_required_observation_ids"])

    def test_divergence_detection_from_bytes(self):
        div = ev.detect_nc_divergences(GOLD, NC_OBJ, MANIFEST)
        # NC does not reference native-xy (the corrective capture) -> belief asserted
        self.assertTrue(div["obs-claim-separate"]["asserted_by_negative_control"])

    def test_deterministic(self):
        r1, _ = self._run()
        r2, _ = self._run()
        self.assertEqual(canonical_bytes(r1), canonical_bytes(r2))


class IntegrationTest(unittest.TestCase):
    """Full pipeline — runs only if the local CAS with the private blobs exists."""

    def test_full_run_if_cas_available(self):
        import os
        import tempfile
        data_root = os.path.expanduser("~/.local/share/o7-research")
        cas_root = os.path.join(data_root, "cas")
        # requires the owner's private RAW blobs; skip cleanly if absent
        probe = os.path.join(cas_root, "sha256", "6a",
                             "86185402fd69d2fe4aad425a257b06c9bcfc4b28decc0b690d9b314f4066b9")
        if not os.path.isfile(probe):
            self.skipTest("local CAS with private RAW blobs not present")
        import run_case
        with tempfile.TemporaryDirectory() as td:
            result = run_case.run("case-0001", data_root, td)
            self.assertEqual(result["report"]["status"]["development_result"], "PASS")


if __name__ == "__main__":
    unittest.main()
