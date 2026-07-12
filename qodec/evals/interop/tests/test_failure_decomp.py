"""Offline failure decomposition — losses derived from data, honest labels, and
a hermetic (no network / model / qodec) deterministic pipeline.

Runs against the committed canonical 7B record, plus synthetic mini-records for
the eligibility rules.
"""

import shutil
import subprocess
import sys
import tempfile
import unittest
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import analyze_codec_failures as A  # noqa: E402
from bench import failure_decomp as fd  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / "results" / "l2-cpu-qwen2.5-coder-7b-v1"

EXPECTED_LOSSES = {
    ("build-log-rtk-log", "n-warnings"), ("clap-derive-explore", "def-path"),
    ("clap-derive-explore", "top-symbol"), ("rtk-rg-derive-clap", "file"),
    ("rtk-rg-parser-clap", "file"),
}


def _rec(case, q, arm, rep, correct, leaks=None, inv=None, malformed=False):
    return {"case": case, "question": q, "arm": arm, "repeat": rep, "correct": correct,
            "format_compliant": not malformed, "malformed": malformed,
            "alias_leaks": leaks or [], "invalid_identifiers": inv or []}


class LossDetection(unittest.TestCase):
    def test_stable_losses_derived_from_records_not_hardcoded(self):
        canon = fd.load_canonical(RUN)
        losses = set(fd.stable_codec_losses(canon["records"]))
        self.assertEqual(losses, EXPECTED_LOSSES)
        # And they agree with the canonical report's stable-loss counts.
        fd.crosscheck_losses(list(losses), canon["tasks"], canon["report"])

    def test_exactly_five_and_crosscheck_guards_drift(self):
        canon = fd.load_canonical(RUN)
        losses = fd.stable_codec_losses(canon["records"])
        self.assertEqual(len(losses), 5)
        # A fabricated extra loss must trip the cross-check against the report.
        with self.assertRaises(fd.LossSetMismatch):
            fd.crosscheck_losses(losses + [("x", "y")], {**canon["tasks"], ("x", "y"): {"category": "locator"}},
                                 canon["report"])

    def test_unstable_codec_loss_excluded(self):
        recs = [_rec("c", "q", "raw", 0, True), _rec("c", "q", "raw+brief", 0, True),
                _rec("c", "q", "encoded+brief", 0, False),
                _rec("c", "q", "encoded+brief", 1, True)]   # repeats disagree → unstable
        self.assertNotIn(("c", "q"), fd.stable_codec_losses(recs))

    def test_rawbrief_incorrect_is_not_an_eligible_loss(self):
        recs = [_rec("c", "q", "raw", 0, True), _rec("c", "q", "raw+brief", 0, False),
                _rec("c", "q", "encoded+brief", 0, False)]
        self.assertNotIn(("c", "q"), fd.stable_codec_losses(recs))


class SpanFate(unittest.TestCase):
    def test_verbatim_and_alias_only(self):
        legend = {"码": "clap_builder/src/builder/"}
        artifact = "%q1 mine\n码=clap_builder/src/builder/\n%q1 body\n码value_parser.rs"
        decoded = fd.decode_via_legend("码value_parser.rs", legend)
        self.assertEqual(fd.span_fate("value_parser.rs", artifact, decoded, legend)["fate"],
                         "preserved_verbatim")
        full = fd.span_fate("clap_builder/src/builder/value_parser.rs", artifact, decoded, legend)
        self.assertEqual(full["fate"], "represented_by_alias")
        self.assertIn("码", full["aliases"])


class Controls(unittest.TestCase):
    def test_selection_is_deterministic_and_scored(self):
        canon = fd.load_canonical(RUN)
        losses = fd.stable_codec_losses(canon["records"])
        a = fd.select_controls(losses, canon["records"], canon["tasks"])
        b = fd.select_controls(losses, canon["records"], canon["tasks"])
        self.assertEqual(a, b)                                   # deterministic
        chosen = [(c["control"]["case"], c["control"]["question_id"]) for c in a]
        self.assertEqual(len(chosen), len(set(chosen)))          # no reuse
        pool = set(fd.both_correct_controls(canon["records"], canon["tasks"]))
        self.assertTrue(set(chosen) <= pool)                    # every control is both-correct
        for c in a:                                             # scores are recorded
            self.assertIn("candidate_pool_size", c["selection_score"])


class Pipeline(unittest.TestCase):
    def test_canonical_sha_mismatch_blocks_analysis(self):
        d = Path(tempfile.mkdtemp()) / "c"
        shutil.copytree(RUN, d)
        (d / "meta.json").write_text((d / "meta.json").read_text() + " ", encoding="utf-8")
        with self.assertRaises(fd.CanonicalMismatch):
            A.build_files(d, "x")

    def test_regeneration_is_byte_stable(self):
        f1 = {k: v for k, v in A.build_files(RUN, "0b76e64").items() if k != "_meta"}
        f2 = {k: v for k, v in A.build_files(RUN, "0b76e64").items() if k != "_meta"}
        self.assertEqual(f1, f2)
        self.assertIn("SHA256SUMS", f1)

    def test_no_network_model_or_qodec_subprocess(self):
        # failure_decomp is stdlib-only; assert it never shells out or hits the net.
        self.assertNotIn("qodec", dir(fd))
        orig_run, orig_open = subprocess.run, urllib.request.urlopen

        def boom(*a, **k):
            raise AssertionError("analyzer must not spawn a subprocess / touch the network")
        subprocess.run = boom
        urllib.request.urlopen = boom
        try:
            files = A.build_files(RUN, "test")
            self.assertIn("summary.json", files)
        finally:
            subprocess.run, urllib.request.urlopen = orig_run, orig_open


class Mechanisms(unittest.TestCase):
    def test_labels_and_conclusion_lock(self):
        # Regression lock: the evidence-based decomposition must not silently drift.
        meta = A.build_files(RUN, "0b76e64")["_meta"]
        primary = {(d["identity"]["case"], d["identity"]["question_id"]): d["mechanism"]["primary_mechanism"]
                   for d in meta["losses"]}
        self.assertEqual(primary[("build-log-rtk-log", "n-warnings")], "notation-ambiguity")
        self.assertEqual(primary[("clap-derive-explore", "def-path")], "identifier-or-path-aliasing")
        self.assertEqual(primary[("clap-derive-explore", "top-symbol")], "identifier-or-path-aliasing")
        self.assertEqual(primary[("rtk-rg-derive-clap", "file")], "mixed")
        self.assertEqual(primary[("rtk-rg-parser-clap", "file")], "grouping-or-boundary-loss")
        self.assertEqual(meta["summary"]["conclusion"], "evidence suggests fold × alias interaction")
        # No gold span was actually absent — qodec aliased/folded, it did not delete.
        self.assertEqual(meta["summary"]["gold_span_share_losses"]["absent"], 0)


if __name__ == "__main__":
    unittest.main()
