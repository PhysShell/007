"""Tests for the N2-A MinimalSanitizer."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
from sanitizer import sanitize  # noqa: E402


class TestSanitizer(unittest.TestCase):
    def test_replaces_tmp_root(self):
        raw = b"building in /tmp/tmp.ABC123/work\n"
        out, report = sanitize(raw, tmp_root="/tmp/tmp.ABC123")
        self.assertEqual(out, b"building in <TMP>/work\n")
        self.assertEqual(report["rules_applied"][0]["rule"], "tmp_root")

    def test_replaces_workspace_root(self):
        raw = b"loaded from /home/runner/work/007/007/qodec\n"
        out, _ = sanitize(raw, workspace_root="/home/runner/work/007/007")
        self.assertEqual(out, b"loaded from <WORKSPACE>/qodec\n")

    def test_replaces_iso_timestamp(self):
        raw = b"Build started 2026-07-14T16:40:12Z\n"
        out, report = sanitize(raw)
        self.assertEqual(out, b"Build started <TIMESTAMP>\n")
        self.assertEqual(report["rules_applied"], [{"rule": "iso_timestamp", "replacements": 1}])

    def test_replaces_pid(self):
        raw = b"worker started pid=12345 ready\n"
        out, _ = sanitize(raw)
        self.assertEqual(out, b"worker started pid=<PID> ready\n")

    def test_replaces_dotnet_time_elapsed_line(self):
        raw = b"Build succeeded.\n\nTime Elapsed 00:00:01.23\n"
        out, _ = sanitize(raw)
        self.assertEqual(out, b"Build succeeded.\n\nTime Elapsed <ELAPSED>\n")

    def test_replaces_msbuild_build_started_banner(self):
        # Regression test for a real N2-A reproducibility mismatch: two
        # otherwise-identical captures differed only in this MSBuild banner's
        # seconds field, which the ISO-8601 timestamp rule doesn't match
        # (different format: MM/DD/YYYY HH:MM:SS).
        raw = b"Build started 07/14/2026 18:39:30.\nsomething else\n"
        out, _ = sanitize(raw)
        self.assertEqual(out, b"Build started <TIMESTAMP>.\nsomething else\n")

    def test_replaces_msbuild_process_id(self):
        # Regression test for the same real mismatch: MSBuild's out-of-process
        # node banner ("process id NNNN") isn't matched by the pid=/pid: rule.
        raw = b"Successfully created process with process id 3395\n"
        out, _ = sanitize(raw)
        self.assertEqual(out, b"Successfully created process with process id <PID>\n")

    def test_strips_ansi_csi_sequences(self):
        raw = b"\x1b[32mBuild succeeded.\x1b[0m\n"
        out, report = sanitize(raw)
        self.assertEqual(out, b"Build succeeded.\n")
        self.assertEqual(report["rules_applied"], [{"rule": "ansi_csi", "replacements": 2}])

    def test_normalizes_crlf(self):
        raw = b"line one\r\nline two\r\n"
        out, _ = sanitize(raw)
        self.assertEqual(out, b"line one\nline two\n")

    def test_untouched_text_is_byte_identical(self):
        raw = b"0 Warning(s)\n0 Error(s)\n"
        out, report = sanitize(raw)
        self.assertEqual(out, raw)
        self.assertEqual(report["rules_applied"], [])
        self.assertEqual(report["original_sha256"], report["sanitized_sha256"])

    def test_never_rewrites_error_message_text(self):
        # Negative test: the sanitizer must not touch the semantic content of
        # an error message, only the explicitly-listed volatile fields.
        raw = b"error CS0246: The type or namespace name 'Foo' could not be found\n"
        out, _ = sanitize(raw)
        self.assertEqual(out, raw)

    def test_deterministic_across_runs(self):
        raw = b"pid=999 at 2026-07-14T16:40:12Z in /tmp/tmp.X/work\n"
        out1, report1 = sanitize(raw, tmp_root="/tmp/tmp.X")
        out2, report2 = sanitize(raw, tmp_root="/tmp/tmp.X")
        self.assertEqual(out1, out2)
        self.assertEqual(report1, report2)

    def test_empty_tmp_root_is_a_noop_for_that_rule(self):
        raw = b"nothing volatile here\n"
        out, report = sanitize(raw, tmp_root="")
        self.assertEqual(out, raw)
        self.assertEqual(report["rules_applied"], [])


if __name__ == "__main__":
    unittest.main()
