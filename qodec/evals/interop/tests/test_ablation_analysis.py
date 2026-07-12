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
    def _verdict(self, pattern):
        recs = question("c", "q", pattern)
        man = _man([("c", "q")], [])
        qs = A.factorial(recs, man)
        return qs[0]["verdict"]

    def test_alias_main_effect(self):
        v = self._verdict({"R": True, "I": True, "M": False, "F": True, "MF": True, "GF": True})
        self.assertTrue(any("alias main effect" in s for s in v))

    def test_structural_main_effect(self):
        v = self._verdict({"R": True, "I": True, "M": True, "F": False, "MF": True, "GF": True})
        self.assertTrue(any("structural main effect" in s for s in v))

    def test_interaction_requires_same_question(self):
        v = self._verdict({"R": True, "I": True, "M": True, "F": True, "MF": False, "GF": True})
        self.assertTrue(any("interaction" in s for s in v))
        self.assertTrue(any("lexical aliasing implicated" in s for s in v))

    def test_framing_regression(self):
        v = self._verdict({"R": True, "I": False, "M": False, "F": False, "MF": False, "GF": False})
        self.assertTrue(any("framing regression" in s for s in v))
        self.assertTrue(any("framing implicated" in s for s in v))

    def test_unstable_arm_is_not_a_clean_pass(self):
        recs = question("c", "q", ALLPASS)
        # M flips across repeats → unstable → not counted as pass for M
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
        recs += question("L", "l0", {"R": True, "I": True, "M": False, "F": True, "MF": False, "GF": False})
        recs += question("L", "l1", {"R": True, "I": True, "M": True, "F": False, "MF": False, "GF": False})
        recs += question("L", "l2", {"R": True, "I": True, "M": True, "F": True, "MF": False, "GF": True})
        recs += question("L", "l3", {"R": True, "I": True, "M": False, "F": True, "MF": False, "GF": True})
        recs += question("L", "l4", {"R": True, "I": True, "M": True, "F": False, "MF": False, "GF": True})
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
        # MF fails every loss → 0/5; GF rescues l2,l3,l4 → 3/5; none is 5/5
        self.assertEqual(gate["MF"]["losses_rescued"], "0/5")
        self.assertEqual(gate["GF"]["losses_rescued"], "3/5")
        self.assertIn("C:c0", gate["M"]["control_regressions"])
        self.assertFalse(any(g["advances_to_full_rerun"] for g in gate.values()))

    def test_pareto_orders_by_quality_first(self):
        recs, man = self._run()
        qs = A.factorial(recs, man)
        par = A.pareto(qs)
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
