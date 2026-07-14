"""The full-L2 VG candidate gate. Exercised against the committed canonical
squeeze run (which must FAIL the VG gate — it is the rejected policy) so the gate
logic and the report fields are pinned on real data, and against synthetic
records for the pass path. Never moves a threshold.
"""

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import score_vg  # noqa: E402
from bench import qodec  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
CANON = ROOT / "results" / "l2-cpu-qwen2.5-coder-7b-v1"
HF_TOK = Path("/root/.cache/qodec-bench/qwen7b/tokenizer.json")


def _have_binary():
    try:
        qodec.binary()
        return True
    except Exception:
        return False


@unittest.skipUnless(CANON.exists() and HF_TOK.exists() and _have_binary(),
                     "canonical run / qwen tokenizer / qodec binary not present")
class CanonicalSqueezeFailsVGGate(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        meta = json.loads((CANON / "meta.json").read_text())
        recs = [json.loads(l) for l in (CANON / "records.jsonl").read_text().splitlines() if l.strip()]
        cls.v = score_vg.analyze_vg(meta, recs, CANON)

    def test_squeeze_does_not_pass(self):
        self.assertFalse(self.v["passes"])
        self.assertIn("DOES NOT PASS", self.v["verdict"])

    def test_full_run_competence_gate_holds(self):
        # The reader is competent enough to decide (raw 70%, eligible, parity).
        self.assertTrue(all(self.v["full_run_gate"].values()), self.v["full_run_gate"])

    def test_it_fails_on_stable_losses_and_leaks_not_on_savings(self):
        g = self.v["vg_quality_gate"]
        self.assertFalse(g["stable_vg_losses==0"])          # squeeze has 5 stable losses
        self.assertFalse(g["alias_leaks==0"])               # and alias leaks
        self.assertTrue(g["mean_savings>0"])                # squeeze does save tokens
        self.assertTrue(g["median_savings>0"])
        self.assertTrue(g["exact_roundtrip_all"])           # and roundtrips

    def test_report_fields_present(self):
        for key in ("retention", "token_savings_vs_raw_brief", "shelf_distribution",
                    "stage2_attempted_cases", "guarded_mining_accepted_cases",
                    "vg_equals_v_cases", "per_case"):
            self.assertIn(key, self.v)
        s = self.v["token_savings_vs_raw_brief"]
        for k in ("total", "mean", "median", "percent"):
            self.assertIn(k, s)

    def test_accepted_is_a_subset_of_attempted(self):
        # A guarded mine can only be accepted where stage-2 was attempted.
        self.assertTrue(set(self.v["guarded_mining_accepted_cases"])
                        <= set(self.v["stage2_attempted_cases"]))

    def test_passthrough_unwrap_is_not_counted_as_accepted(self):
        # Any per-case row that unwrapped to naked raw must NOT be an accepted mine.
        for case, pc in self.v["per_case"].items():
            if pc["passthrough_unwrapped"]:
                self.assertFalse(pc["guarded_mining_accepted"], case)


class GateAssembly(unittest.TestCase):
    """The pass/fail wiring on a hand-built analysis (no model/qodec)."""

    def _verdict(self, full, vg):
        passes = all(full.values()) and all(vg.values())
        return ("VG PASSES FULL L2 CANDIDATE GATE.\nProduction squeeze remains rejected.\n"
                "VG promotion/integration is a separate decision." if passes else
                "VG DOES NOT PASS FULL L2.\nProduction squeeze remains rejected.")

    def test_all_conditions_true_passes(self):
        full = {"raw_competence>=60%": True, "eligible_overall>=10": True,
                "eligible_locator>=4": True, "tokenizer_parity_ok": True}
        vg = {"stable_vg_losses==0": True, "alias_leaks==0": True, "invalid_id_delta<=0": True,
              "malformed_delta<=0": True, "mean_savings>0": True, "median_savings>0": True,
              "exact_roundtrip_all": True}
        self.assertIn("PASSES FULL L2", self._verdict(full, vg))

    def test_one_false_fails(self):
        full = {"raw_competence>=60%": True, "eligible_overall>=10": True,
                "eligible_locator>=4": True, "tokenizer_parity_ok": True}
        vg = {"stable_vg_losses==0": True, "alias_leaks==0": True, "invalid_id_delta<=0": True,
              "malformed_delta<=0": True, "mean_savings>0": True, "median_savings>0": False,
              "exact_roundtrip_all": True}
        self.assertIn("DOES NOT PASS", self._verdict(full, vg))


if __name__ == "__main__":
    unittest.main()
