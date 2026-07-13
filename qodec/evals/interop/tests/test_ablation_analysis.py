"""Factorial analysis of the ablation run: per-question truth tables, evidence
verdicts, the candidate gate, and Pareto ranking — validated on synthetic
records so the interpretation rules are pinned independently of the model."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import analyze_ablation as A  # noqa: E402


def rec(case, q, arm, repeat, correct, leaks=0, inv=0, malformed=False, ptok=100, ms=1000.0):
    return {"case": case, "question": q, "category": "locator", "arm": arm, "repeat": repeat,
            "correct": correct, "format_compliant": not malformed, "malformed": malformed,
            "alias_leaks": ["码"] * leaks, "invalid_identifiers": ["x"] * inv,
            "server_prompt_tokens": ptok, "total_ms": ms, "answer_parsed": {}, "finish_reason": "stop"}


def question(case, q, pattern, **kw):
    # pattern: dict arm->correct. repeat 0 only unless overridden.
    return [rec(case, q, arm, 0, ok, **kw) for arm, ok in pattern.items()]


ALLPASS = {a: True for a in A.ARMS}


def _man(losses, controls, weak=()):
    return {"losses": [f"{c}:{q}" for c, q in losses],
            "controls": [f"{c}:{q}" for c, q in controls],
            "weakly_matched": [f"{c}:{q}" for c, q in weak],
            "model_requested": "qwen2.5-coder-7b-instruct", "qodec_binary_sha256": "ab" * 32}


class Verdicts(unittest.TestCase):
    def _verdict(self, pattern, mf_codec="mine"):
        recs = question("c", "q", pattern)
        for r in recs:
            r["arm_receipt"] = {"format_codec": mf_codec}
        qs = A.factorial(recs, _man([("c", "q")], []))
        return qs[0]["verdict"]

    def test_alias_main_effect_confirmed(self):
        v = self._verdict({"R": True, "I": True, "M": False, "F": True, "MF": False, "VG": True})
        self.assertIn("alias main effect confirmed", v["causal"])

    def test_production_stage_mine_interaction_when_mf_mined(self):
        v = self._verdict({"R": True, "I": True, "M": True, "F": True, "MF": False, "VG": True}, mf_codec="mine")
        self.assertIn("production-stage / mine interaction unresolved", v["causal"])
        # VG rescue is reported as candidate-policy, NOT causal
        self.assertIn("VG", v["candidate_policy_rescue"])

    def test_production_stage_effect_when_mf_structural(self):
        v = self._verdict({"R": True, "I": True, "M": True, "F": True, "MF": False, "VG": True}, mf_codec="tmpl")
        self.assertIn("production-stage effect unresolved", v["causal"])

    def test_no_lexical_guard_claim_from_mf_fail_vg_pass(self):
        # The old bug: MF fail + VG pass must NOT be stated as a lexical-guard effect.
        v = self._verdict({"R": True, "I": True, "M": True, "F": True, "MF": False, "VG": True})
        self.assertNotIn("lexical", v["causal"].lower())
        self.assertNotIn("guard", v["causal"].lower())

    def test_framing_implicated(self):
        v = self._verdict({"R": True, "I": False, "M": False, "F": False, "MF": False, "VG": False})
        self.assertIn("framing implicated", v["causal"])

    def test_unstable_arm_is_not_a_clean_pass(self):
        recs = question("c", "q", ALLPASS)
        recs += [rec("c", "q", "M", 1, False), rec("c", "q", "M", 2, True)]
        recs = [r for r in recs if not (r["arm"] == "M" and r["repeat"] == 0)] + [rec("c", "q", "M", 0, True)]
        qs = A.factorial(recs, _man([("c", "q")], []))
        self.assertFalse(A._ok(qs[0]["cells"]["M"]))


class GateAndPareto(unittest.TestCase):
    def _run(self):
        losses = [("L", f"l{i}") for i in range(5)]
        controls = [("C", f"c{i}") for i in range(5)]
        recs = []
        # loss patterns: none rescued by a single arm across all five
        recs += question("L", "l0", {"R": True, "I": True, "M": False, "F": True, "MF": False, "VG": False})
        recs += question("L", "l1", {"R": True, "I": True, "M": True, "F": False, "MF": False, "VG": False})
        recs += question("L", "l2", {"R": True, "I": True, "M": True, "F": True, "MF": False, "VG": True})
        recs += question("L", "l3", {"R": True, "I": True, "M": False, "F": True, "MF": False, "VG": True})
        recs += question("L", "l4", {"R": True, "I": True, "M": True, "F": False, "MF": False, "VG": True})
        for i in range(5):
            # controls all pass except c0 regresses under M
            pat = dict(ALLPASS)
            if i == 0:
                pat["M"] = False
            recs += question("C", f"c{i}", pat, ptok=200)
        return recs, _man(losses, controls)

    def test_gate_no_arm_rescues_all(self):
        recs, man = self._run()
        qs = A.factorial(recs, man)
        gate = A.candidate_gate(qs)
        # MF fails every loss → 0/5; VG rescues l2,l3,l4 → 3/5; none is 5/5
        self.assertEqual(gate["MF"]["losses_rescued"], "0/5")
        self.assertEqual(gate["VG"]["losses_rescued"], "3/5")
        self.assertIn("C:c0", gate["M"]["control_regressions"])
        self.assertFalse(any(g["advances_to_full_rerun"] for g in gate.values()))

    def test_priority_ranking_orders_by_quality_first(self):
        recs, man = self._run()
        qs = A.factorial(recs, man)
        par = A.priority_ranking(qs)
        # R passes all 10; MF passes fewest → R ranks ahead of MF
        self.assertLess(par["ranked"].index("R"), par["ranked"].index("MF"))

    def test_build_files_byte_stable(self):
        recs, man = self._run()
        f1 = {k: v for k, v in A.build_files(Path("."), recs, man).items() if k != "_meta" and not k.endswith((".jsonl",))}
        f2 = {k: v for k, v in A.build_files(Path("."), recs, man).items() if k != "_meta" and not k.endswith((".jsonl",))}
        # factorial.json + per-question + README are deterministic
        self.assertEqual(f1["factorial.json"], f2["factorial.json"])


if __name__ == "__main__":
    unittest.main()
