"""Passthrough-accounting normalization of realized-stage receipts.

The run-time audit receipt records stage-2 transform_applied = (final SHA changed
vs stage-1). Under --passthrough-on-no-gain, a no-gain VG pipeline unwraps the
%q1 container to the naked raw payload, and that unwrap flips the SHA without any
mining. ablation_policies.normalize_realized must not count that as a guarded
mine, while still detecting a real accepted mine. Pinned on synthetic receipts
(no model, no qodec binary) so the rule is independent of the run.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import ablation_policies as ap  # noqa: E402
import score_reader as sr  # noqa: E402

RAW_SHA = "a" * 64
S1_SHA = "b" * 64
MINE_SHA = "c" * 64


def _receipt(stage1_codec, stage1_leg, s2_attempted, final_codec, final_sha, added, in_sha):
    """Build a receipt in the exact shape ap._realized emits."""
    return {
        "arm": "VG",
        "stage1": {"attempted": stage1_codec is not None, "selected_codec": stage1_codec,
                   "transform_applied": stage1_codec not in (None, "raw", "identity"),
                   "alias_entries": stage1_leg,
                   "artifact_sha256": S1_SHA if stage1_codec else None,
                   "tokens": 100},
        "stage2": {"attempted": s2_attempted,
                   "selected_miner": ("guarded-mine-or-deep" if (added and s2_attempted) else None),
                   "transform_applied": (final_sha != in_sha) if s2_attempted else False,
                   "alias_entries_added": added,
                   "input_artifact_sha256": in_sha if s2_attempted else None,
                   "artifact_sha256": final_sha if s2_attempted else None, "tokens": 90},
        "final": {"outer_codec": final_codec, "artifact_sha256": final_sha, "tokens": 90},
        "overall_alias_entries": stage1_leg + added,
        "alias_applied": (stage1_leg + added) > 0,
        "structural_applied": stage1_codec not in (None, "raw", "identity"),
    }


class RawContainerPassthroughIsNotMining(unittest.TestCase):
    def test_container_to_naked_raw_unwrap_not_counted(self):
        # stage-1 = %q1 raw container (SHA b…), final = naked raw (SHA a…, outer None),
        # zero aliases added. The audit receipt says transform_applied=True; the
        # normalized receipt must say NO candidate accepted and flag the unwrap.
        rec = _receipt("raw", 0, True, None, RAW_SHA, 0, S1_SHA)
        self.assertTrue(rec["stage2"]["transform_applied"])          # audit: SHA changed
        n = ap.normalize_realized(rec, RAW_SHA)
        self.assertFalse(n["stage2"]["candidate_accepted"])
        self.assertFalse(n["stage2"]["transform_applied"])
        self.assertIsNone(n["stage2"]["selected_miner"])
        self.assertTrue(n["final"]["passthrough_unwrapped"])
        # audit receipt untouched
        self.assertTrue(rec["stage2"]["transform_applied"])

    def test_grep_container_no_mine_is_not_unwrap_and_not_accepted(self):
        # stage-1 = grep container, final == stage-1 (no mine ran). Not a passthrough
        # unwrap (final is still a %q1 grep container), and not an accepted mine.
        rec = _receipt("grep", 0, True, "grep", S1_SHA, 0, S1_SHA)
        n = ap.normalize_realized(rec, RAW_SHA)
        self.assertFalse(n["stage2"]["candidate_accepted"])
        self.assertFalse(n["final"]["passthrough_unwrapped"])


class AcceptedGuardedMineIsDetected(unittest.TestCase):
    def test_real_mine_with_added_aliases_is_accepted(self):
        rec = _receipt("raw", 0, True, "mine", MINE_SHA, 2, S1_SHA)
        n = ap.normalize_realized(rec, RAW_SHA)
        self.assertTrue(n["stage2"]["candidate_accepted"])
        self.assertTrue(n["stage2"]["transform_applied"])
        self.assertIsNotNone(n["stage2"]["selected_miner"])
        self.assertFalse(n["final"]["passthrough_unwrapped"])


class TmplStage1LegendIsNotAStage2Alias(unittest.TestCase):
    def test_stage1_legend_does_not_make_stage2_accepted(self):
        # A tmpl/diag stage-1 carries its own legend (alias_entries=3) but the mine
        # added nothing (alias_entries_added=0). The stage-1 legend must NOT be read
        # as accepted stage-2 mining.
        rec = _receipt("tmpl", 3, True, "tmpl", S1_SHA, 0, S1_SHA)
        n = ap.normalize_realized(rec, RAW_SHA)
        self.assertEqual(n["stage1"]["alias_entries"], 3)
        self.assertFalse(n["stage2"]["candidate_accepted"])
        self.assertEqual(n["stage2"]["alias_entries_added"], 0)


class StabilityUsesFullSignature(unittest.TestCase):
    def _rec(self, correct=True, malformed=False, leaks=(), inv=()):
        return {"correct": correct, "malformed": malformed, "format_compliant": not malformed,
                "alias_leaks": list(leaks), "invalid_identifiers": list(inv)}

    def test_leak_flip_at_equal_correctness_is_unstable(self):
        a = self._rec(correct=False, leaks=["码"])
        b = self._rec(correct=False, leaks=["帧"])           # same count, different glyph
        self.assertEqual(a["correct"], b["correct"])
        self.assertNotEqual(sr.stability_signature(a), sr.stability_signature(b))

    def test_invalid_id_flip_is_unstable(self):
        a = self._rec(inv=["Foo"])
        b = self._rec(inv=["Bar"])
        self.assertNotEqual(sr.stability_signature(a), sr.stability_signature(b))

    def test_same_set_reordered_is_stable(self):
        a = self._rec(leaks=["码", "帧"], inv=["Foo", "Bar"])
        b = self._rec(leaks=["帧", "码"], inv=["Bar", "Foo"])
        self.assertEqual(sr.stability_signature(a), sr.stability_signature(b))

    def test_format_flip_is_unstable(self):
        a = self._rec(malformed=False)
        b = self._rec(malformed=True)
        self.assertNotEqual(sr.stability_signature(a), sr.stability_signature(b))


if __name__ == "__main__":
    unittest.main()
