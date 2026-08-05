"""A failed --update-expectations must leave committed artifacts untouched.

Runs offline: it drives run_case.run_update against a NONEXISTENT CAS, so the
generation is incomplete and the freeze must refuse *before* any swap or
expectation write. No private RAW is needed.
"""
import os
import tempfile
import unittest

import run_case


class TransactionalUpdateTest(unittest.TestCase):
    def _fixture_expectation_state(self):
        fdir = os.path.normpath(os.path.join(
            os.path.dirname(__file__), "..", "..", "fixtures", "case-0001"))
        path = os.path.join(fdir, run_case.EXPECTED_REPORT_FILENAME)
        if not os.path.isfile(path):
            return None
        with open(path, "rb") as fh:
            return fh.read()

    def test_failed_update_does_not_replace_results_or_expectation(self):
        before_expectation = self._fixture_expectation_state()
        with tempfile.TemporaryDirectory() as tmp:
            target = os.path.join(tmp, "results")
            os.makedirs(target)
            sentinel = os.path.join(target, "SENTINEL.txt")
            with open(sentinel, "w") as fh:
                fh.write("committed content that must survive a failed update")

            with self.assertRaises(run_case.FailClosed):
                run_case.run_update("case-0001", "/definitely/not/a/cas", target)

            # target untouched (no partial replacement)
            self.assertTrue(os.path.isfile(sentinel))
            with open(sentinel) as fh:
                self.assertIn("must survive", fh.read())
            # temp/bak siblings cleaned up
            for suffix in (".update-tmp1", ".update-tmp2", ".update-bak"):
                self.assertFalse(os.path.exists(target + suffix))

        # the committed fixture expectation is byte-for-byte unchanged
        self.assertEqual(self._fixture_expectation_state(), before_expectation)


if __name__ == "__main__":
    unittest.main()
