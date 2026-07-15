"""Unit tests for generic_capture.py's argv resolution: erratum lookup plus
the argv0_override mechanism that must be applied AFTER erratum resolution,
never before it.
"""
import json
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
REPO_ROOT = Path(__file__).resolve().parents[7]
sys.path.insert(0, str(TOOLS))
import generic_capture as gc  # noqa: E402

REAL_ERRATA_PATH = REPO_ROOT / "qodec/evals/interop/v2/n2/d1-identity-lock/execution-plan-errata.json"


class TestResolveEffectiveArgv(unittest.TestCase):
    def _write_errata(self, tmp_path, case_id, original_argv, corrected_argv):
        entry = {
            "case_id": case_id,
            "status": "AUTHORIZED_ERRATUM",
            "original_frozen_argv": original_argv,
            "corrected_effective_argv": corrected_argv,
        }
        body = {"entries": [entry], "errata_sha256": "test-only-not-a-real-hash"}
        path = tmp_path / "errata.json"
        path.write_text(json.dumps(body))
        return path

    def test_no_erratum_returns_frozen_argv_unchanged(self, tmp_path=None):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write_errata(Path(tmp), "some-other-case", ["a"], ["b"])
            argv, resolution, erratum_sha = gc.resolve_effective_argv("repo-hyperfine", ["cargo", "run"], path)
        self.assertEqual(argv, ["cargo", "run"])
        self.assertEqual(resolution, "frozen")
        self.assertIsNone(erratum_sha)

    def test_matching_erratum_applies_correction(self):
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write_errata(Path(tmp), "repo-pyflakes", ["python", "-m", "pyflakes", "src/"],
                                       ["python", "-m", "pyflakes", "pyflakes/"])
            argv, resolution, erratum_sha = gc.resolve_effective_argv(
                "repo-pyflakes", ["python", "-m", "pyflakes", "src/"], path
            )
        self.assertEqual(argv, ["python", "-m", "pyflakes", "pyflakes/"])
        self.assertEqual(resolution, "authorized-n2d1b-erratum")
        self.assertIsNotNone(erratum_sha)

    def test_stale_erratum_raises_when_frozen_argv_already_substituted(self):
        # This is exactly the real bug: a caller that pre-substitutes a venv
        # python path into argv[0] BEFORE calling resolve_effective_argv
        # breaks the exact-match comparison against the erratum's recorded
        # original_frozen_argv.
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write_errata(Path(tmp), "repo-pyflakes", ["python", "-m", "pyflakes", "src/"],
                                       ["python", "-m", "pyflakes", "pyflakes/"])
            already_substituted = ["/home/runner/work/_temp/venv-repo-pyflakes/bin/python", "-m", "pyflakes", "src/"]
            with self.assertRaises(gc.GenericCaptureFailure):
                gc.resolve_effective_argv("repo-pyflakes", already_substituted, path)

    def test_pure_frozen_argv_plus_post_hoc_override_reproduces_correct_final_argv(self):
        # The fixed calling convention: pass the PURE frozen argv in, get the
        # erratum-corrected argv out, THEN apply argv0_override -- matching
        # what run_one_capture now does internally.
        import tempfile
        with tempfile.TemporaryDirectory() as tmp:
            path = self._write_errata(Path(tmp), "repo-pyflakes", ["python", "-m", "pyflakes", "src/"],
                                       ["python", "-m", "pyflakes", "pyflakes/"])
            effective_argv, resolution, _ = gc.resolve_effective_argv(
                "repo-pyflakes", ["python", "-m", "pyflakes", "src/"], path
            )
            venv_python = "/home/runner/work/_temp/venv-repo-pyflakes/bin/python"
            final_argv = [venv_python, *effective_argv[1:]]
        self.assertEqual(final_argv, [venv_python, "-m", "pyflakes", "pyflakes/"])
        self.assertEqual(resolution, "authorized-n2d1b-erratum")

    def test_against_real_committed_errata_file(self):
        self.assertTrue(REAL_ERRATA_PATH.is_file())
        argv, resolution, erratum_sha = gc.resolve_effective_argv(
            "repo-pyflakes", ["python", "-m", "pyflakes", "src/"], REAL_ERRATA_PATH
        )
        self.assertEqual(argv, ["python", "-m", "pyflakes", "pyflakes/"])
        self.assertEqual(resolution, "authorized-n2d1b-erratum")
        self.assertIsNotNone(erratum_sha)


if __name__ == "__main__":
    unittest.main()
