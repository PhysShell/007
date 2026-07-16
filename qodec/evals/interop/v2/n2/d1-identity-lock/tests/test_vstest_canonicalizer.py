"""Unit tests for vstest_canonicalizer.py -- the strict, case-specific
canonicalizer authorized (2026-07-16) for repo-kubeops-generator only, in
response to real CI evidence (run 29466573023, pair-verify artifact
8363205429) showing capture-a/capture-b raw stdout differ in exactly one
line -- VSTest's own completion banner's wall-clock "Duration: N s" field.
"""
import hashlib
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import vstest_canonicalizer as vc  # noqa: E402


def _banner(*, failed="0", passed="61", skipped="0", total="61", duration="2",
            tail="KubeOps.Generator.Test.dll (net10.0)") -> bytes:
    return (
        f"Passed!  - Failed:     {failed}, Passed:    {passed}, Skipped:     {skipped}, "
        f"Total:    {total}, Duration: {duration} s - {tail}\n"
    ).encode("utf-8")


class TestVstestDurationRule(unittest.TestCase):
    def test_replaces_only_the_duration(self):
        raw = _banner(duration="2")
        out, report = vc.canonicalize_stream(raw)
        self.assertIn(b"<ELAPSED>", out)
        self.assertNotIn(b"Duration: 2 s", out)
        self.assertIn(b"Failed:     0, Passed:    61, Skipped:     0, Total:    61", out)
        self.assertIn(b"KubeOps.Generator.Test.dll (net10.0)", out)
        self.assertEqual(report["rule_match_counts"]["vstest_duration"], 1)
        self.assertEqual(report["replacement_count"], 1)

    def test_duration_2s_and_1s_canonicalize_identically(self):
        # The exact real evidence from run 29466573023: identical 61/61 pass
        # results, differing only in wall-clock duration.
        capture_a = _banner(duration="2")
        capture_b = _banner(duration="1")
        self.assertNotEqual(capture_a, capture_b)
        canon_a, report_a = vc.canonicalize_stream(capture_a)
        canon_b, report_b = vc.canonicalize_stream(capture_b)
        self.assertEqual(canon_a, canon_b)
        self.assertEqual(report_a["replacement_count"], 1)
        self.assertEqual(report_b["replacement_count"], 1)

    def test_changed_pass_fail_total_count_does_not_canonicalize_identically(self):
        # A real regression (a genuinely different test result) must never
        # be masked -- only the duration field is ever touched.
        capture_a = _banner(passed="61", total="61", duration="2")
        capture_b = _banner(passed="60", total="61", duration="1")
        canon_a, _ = vc.canonicalize_stream(capture_a)
        canon_b, _ = vc.canonicalize_stream(capture_b)
        self.assertNotEqual(canon_a, canon_b)


class TestStructuralPreservation(unittest.TestCase):
    def _real_shaped_stream(self, *, duration="2") -> bytes:
        lines = [
            "Test run for /work/KubeOps.Generator.Test.dll (net10.0)",
            "",
            "Starting test execution, please wait...",
            "",
            _banner(duration=duration).decode().rstrip("\n"),
        ]
        return ("\n".join(lines) + "\n").encode("utf-8")

    def test_line_count_preserved(self):
        raw = self._real_shaped_stream()
        out, report = vc.canonicalize_stream(raw)
        self.assertEqual(len(raw.splitlines()), len(out.splitlines()))
        self.assertEqual(report["line_count_in"], report["line_count_out"])

    def test_trailing_newline_preserved_when_present(self):
        raw = self._real_shaped_stream()
        self.assertTrue(raw.endswith(b"\n"))
        out, report = vc.canonicalize_stream(raw)
        self.assertTrue(out.endswith(b"\n"))
        self.assertTrue(report["trailing_newline_preserved"])

    def test_trailing_newline_preserved_when_absent(self):
        raw = self._real_shaped_stream().rstrip(b"\n")
        self.assertFalse(raw.endswith(b"\n"))
        out, report = vc.canonicalize_stream(raw)
        self.assertFalse(out.endswith(b"\n"))
        self.assertTrue(report["trailing_newline_preserved"])

    def test_no_line_removed_including_blank_lines(self):
        raw = self._real_shaped_stream()
        out, _ = vc.canonicalize_stream(raw)
        self.assertIn("", out.decode().splitlines())

    def test_untouched_lines_are_byte_identical(self):
        raw = self._real_shaped_stream()
        out, _ = vc.canonicalize_stream(raw)
        raw_lines = raw.decode().splitlines()
        out_lines = out.decode().splitlines()
        for i, line in enumerate(raw_lines):
            if "Duration:" not in line:
                self.assertEqual(raw_lines[i], out_lines[i])


class TestIdempotence(unittest.TestCase):
    def test_applying_twice_changes_zero_bytes(self):
        raw = _banner(duration="2")
        once, _ = vc.canonicalize_stream(raw)
        twice, report_twice = vc.canonicalize_stream(once)
        self.assertEqual(once, twice)
        self.assertEqual(report_twice["replacement_count"], 0)


class TestStrictUtf8(unittest.TestCase):
    def test_invalid_utf8_raises(self):
        raw = b"\xff\xfe not valid utf-8 \x00\x01"
        with self.assertRaises(vc.CanonicalizerError):
            vc.canonicalize_stream(raw)


class TestNonMatchingSimilarTextUnchanged(unittest.TestCase):
    def test_line_without_trigger_substring_passes_through_unchanged(self):
        # "Duration without" has no "Duration: " (colon-space) trigger
        # substring, so it must never be touched.
        raw = b"Some unrelated line mentioning Duration without the banner grammar\n"
        out, report = vc.canonicalize_stream(raw)
        self.assertEqual(raw, out)
        self.assertEqual(report["replacement_count"], 0)


class TestUnexpectedGrammarFailsLoudly(unittest.TestCase):
    def test_malformed_duration_grammar_raises(self):
        # Trigger substring "Duration: " is present, but the banner's own
        # shape is malformed (missing the trailing " s - <suite>" tail) --
        # must raise, never silently pass through un-canonicalized.
        raw = b"Passed!  - Failed:     0, Passed:    61, Skipped:     0, Total:    61, Duration: 2\n"
        with self.assertRaises(vc.CanonicalizerError):
            vc.canonicalize_stream(raw)

    def test_non_numeric_duration_raises(self):
        raw = _banner().replace(b"Duration: 2 s", b"Duration: N/A s")
        with self.assertRaises(vc.CanonicalizerError):
            vc.canonicalize_stream(raw)


class TestReplacementRecordShape(unittest.TestCase):
    def test_replacement_records_rule_name_line_number_and_hashes(self):
        raw = _banner(duration="2")
        out, report = vc.canonicalize_stream(raw)
        self.assertEqual(len(report["replacements"]), 1)
        entry = report["replacements"][0]
        self.assertEqual(entry["rule_name"], "vstest_duration")
        self.assertEqual(entry["line_number"], 1)
        before_line = raw.decode().splitlines()[0]
        after_line = out.decode().splitlines()[0]
        self.assertEqual(entry["before_line_sha256"], hashlib.sha256(before_line.encode()).hexdigest())
        self.assertEqual(entry["after_line_sha256"], hashlib.sha256(after_line.encode()).hexdigest())


class TestRealEvidenceLineHashMatches(unittest.TestCase):
    def test_before_line_sha256_matches_real_ci_evidence(self):
        # The exact real capture-a raw line (run 29466573023) -- its
        # before_line_sha256 here must match the sha256 recorded against
        # "side: a" in the real pair-reproducibility-report.json
        # (artifact 8363205429)'s unmatched_raw_diff_lines.
        raw = _banner(duration="2")
        _, report = vc.canonicalize_stream(raw)
        self.assertEqual(
            report["replacements"][0]["before_line_sha256"],
            "8153ad7156fef14587d4dd3dfe21dc2c867c2ef4cd40149436b0e241676bb396",
        )


if __name__ == "__main__":
    unittest.main()
