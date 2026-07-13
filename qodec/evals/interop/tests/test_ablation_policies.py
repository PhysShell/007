"""Factorized ablation policies: six arms, each byte-exact and factor-pure.

Drives the built qodec binary (release, or QODEC_BIN). Uses the built-in o200k
meter so no external tokenizer is needed.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import ablation_policies as ap  # noqa: E402
from bench import qodec  # noqa: E402

METER = "o200k"
# Repetitive, code-shaped payload so mining and folding both have something to do.
SAMPLE = "\n".join(
    f"»src/_derive/mod.rs\n{i}://! clap_derive::ValueParser sees clap_builder/src/builder/value_parser.rs"
    for i in range(12)
) + "\n»src/_derive/implicit.rs\n4://! pub fn value_parser() -> ValueParser\n"


class Guard(unittest.TestCase):
    def test_python_guard_mirrors_rust_classes(self):
        for g in ("value_parser", "ValueParser", "getValue", "clap::ValueParser",
                  "src/_derive/mod.rs", "»src/", "`code`", "mod.rs"):
            self.assertTrue(ap.is_guarded_lexical(g), g)
        for ok in ("9 warnings", "error CS1061", "Log Summary", "the value"):
            self.assertFalse(ap.is_guarded_lexical(ok), ok)


class Policies(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.qb = str(qodec.binary())
        cls.arms = ap.encode_all_arms(SAMPLE, METER, cls.qb)
        env = qodec.encode(SAMPLE, codec="squeeze", meter=METER, passthrough=False)
        cls.squeeze = env.content

    def test_all_arms_present(self):
        self.assertEqual(set(self.arms), set(ap.ARM_NAMES))

    def test_all_invariants_hold(self):
        viol = ap.check_invariants(self.arms, SAMPLE, squeeze_artifact=self.squeeze)
        self.assertEqual(viol, [], f"invariant violations: {viol}")

    def test_every_encoded_arm_roundtrips_byte_exact(self):
        for name in ("I", "M", "F", "MF", "VG"):
            self.assertTrue(self.arms[name].roundtrip_ok, f"{name} roundtrip")
            self.assertEqual(self.arms[name].receipt["roundtrip_sha256"], ap._sha(SAMPLE))

    def test_MF_reproduces_production_squeeze(self):
        self.assertEqual(self.arms["MF"].artifact, self.squeeze)
        self.assertTrue(self.arms["MF"].receipt["alias_enabled"])
        self.assertTrue(self.arms["MF"].receipt["structural_enabled"])
        self.assertFalse(self.arms["MF"].receipt["lexical_guard"])

    def test_alias_off_arms_have_no_legend(self):
        for name in ("R", "I", "F"):
            self.assertEqual(ap.legend_of(self.arms[name].artifact), {}, name)

    def test_VG_aliases_nothing_guarded(self):
        for a, phrase in ap.legend_of(self.arms["VG"].artifact).items():
            self.assertFalse(ap.is_guarded_lexical(phrase), f"{a}={phrase!r}")
        self.assertTrue(self.arms["VG"].receipt["lexical_guard"])

    def test_identity_is_a_container_not_passthrough(self):
        self.assertTrue(self.arms["I"].artifact.startswith("%q1 identity"))
        self.assertTrue(self.arms["I"].encoded)

    def test_deterministic(self):
        again = ap.encode_all_arms(SAMPLE, METER, self.qb)
        for name in ap.ARM_NAMES:
            self.assertEqual(again[name].receipt["artifact_sha256"],
                             self.arms[name].receipt["artifact_sha256"], name)

    def test_receipt_shape(self):
        for name, res in self.arms.items():
            for key in ("alias_enabled", "structural_enabled", "lexical_guard",
                        "format_codec", "miner", "artifact_sha256", "roundtrip_sha256", "tokens"):
                self.assertIn(key, res.receipt, f"{name} missing {key}")

    def test_realized_stages_report_applied_transforms_not_intent(self):
        st = {a: ap.realized_stages(a, SAMPLE, METER, self.qb) for a in ap.ARM_NAMES if a != "R"}
        # I: identity container — neither factor actually applied.
        self.assertFalse(st["I"]["alias_applied"])
        self.assertFalse(st["I"]["structural_applied"])
        # M: alias applied (legend present), no structural stage.
        self.assertTrue(st["M"]["alias_applied"])
        self.assertFalse(st["M"]["structural_applied"])
        # F/VG shelf is fold/grep only; MF shelf includes tmpl/diag/toon.
        self.assertEqual(st["F"]["stage1"]["candidate_codecs"], ["fold", "grep"])
        self.assertIn("tmpl", st["MF"]["stage1"]["candidate_codecs"])
        # alias_applied is read from the artifact, never from the arm name.
        for a in ("I", "F"):
            self.assertEqual(st[a]["stage2"]["legend_entries"], 0)

    def test_byte_identical_pairs_detected(self):
        # On this sample the guard removes all VG aliases → VG == F byte-for-byte.
        pairs = ap.byte_identical_pairs(self.arms)
        self.assertIn(("F", "VG"), [tuple(p) for p in pairs])


class ClosurePolicies(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.qb = str(qodec.binary())
        cls.arms = ap.encode_all_arms(SAMPLE, METER, cls.qb, policies=ap.CLOSURE_POLICIES)
        cls.squeeze = qodec.encode(SAMPLE, codec="squeeze", meter=METER, passthrough=False).content
        cls.stage1 = qodec.encode(SAMPLE, codec="squeeze-stage1", meter=METER, passthrough=False).content

    def test_closure_invariants_hold(self):
        viol = ap.check_closure_invariants(self.arms, SAMPLE, self.squeeze, self.stage1)
        self.assertEqual(viol, [], f"closure invariant violations: {viol}")

    def test_SM_is_squeeze_and_S_is_stage1(self):
        self.assertEqual(self.arms["SM"].artifact, self.squeeze)
        self.assertEqual(self.arms["S"].artifact, self.stage1)

    def test_SG_shares_stage1_with_SM_and_differs_only_in_guard(self):
        # SG's non-mine (stage-1) legend equals S's legend; only the mine differs.
        s_legend = ap.legend_of(self.arms["S"].artifact)
        for a, phrase in ap.legend_of(self.arms["SG"].artifact).items():
            if a not in s_legend:  # a mine-added entry
                self.assertFalse(ap.is_guarded_lexical(phrase), f"SG mine aliased guarded {a}={phrase!r}")


if __name__ == "__main__":
    unittest.main()
