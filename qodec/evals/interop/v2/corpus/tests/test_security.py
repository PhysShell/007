"""Security invariants: no shell, no path escape, no env injection, external
sources must pin an immutable revision and a license."""
import os
import tempfile
import unittest
from pathlib import Path

import capture
import corpus_tool as ct
import corpus_testutil as U


class TestSecurity(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = U.make_temp_corpus(Path(self.tmp.name))
        self.bundle = self.root / "examples" / U.DEMO_ID

    def tearDown(self):
        self.tmp.cleanup()

    def _recipe(self):
        return self.bundle / "capture-recipe.json"

    def test_shell_string_command_fails(self):
        p = self._recipe()
        r = U.load(p)
        r["native"] = {"command": "python fixture/demo_tool.py | rtk pipe"}
        U.dump(p, r)
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "schema"))

    def test_bash_dash_c_fails(self):
        p = self._recipe()
        r = U.load(p)
        r["native"]["argv"] = ["bash", "-c", "echo hi"]
        U.dump(p, r)
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "shell"))

    def test_pipe_metachar_in_argv_fails(self):
        with self.assertRaises(capture.CaptureError):
            capture.assert_argv_no_shell(["python3", "x.py", "|", "rtk"])

    def test_absolute_cwd_fails(self):
        with self.assertRaises(capture.CaptureError):
            capture.safe_join(self.bundle, "/etc/passwd")

    def test_path_traversal_fails(self):
        with self.assertRaises(capture.CaptureError):
            capture.safe_join(self.bundle, "../../etc/passwd")

    def test_symlink_escape_fails(self):
        link = self.bundle / "snapshots" / "escape"
        link.symlink_to("/etc/passwd")
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "path"))

    def test_mutable_external_revision_fails(self):
        p = self.bundle / "provenance.json"
        prov = U.load(p)
        prov["origin_kind"] = "external-sanitized"
        prov["upstream_revision"] = "main"
        prov["upstream_license"] = "MIT"
        U.dump(p, prov)
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "mutable-revision"))

    def test_external_source_without_license_fails(self):
        p = self.bundle / "provenance.json"
        prov = U.load(p)
        prov["origin_kind"] = "external-sanitized"
        prov["upstream_revision"] = "a" * 40
        U.dump(p, prov)  # no upstream_license
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "license"))

    def test_environment_variable_injection_fails(self):
        p = self._recipe()
        r = U.load(p)
        r["environment_allowlist"] = ["PATH", "GITHUB_TOKEN"]
        U.dump(p, r)
        v = U.run_validate()
        self.assertTrue(U.has_code(v, "env-injection"))

    def test_child_env_strips_credentials(self):
        os.environ["GITHUB_TOKEN"] = "secret-should-not-pass"
        try:
            env = capture.build_child_env(["PATH", "GITHUB_TOKEN"],
                                          {"locale": "C.UTF-8", "timezone": "UTC", "source_date_epoch": 0},
                                          "/tmp/home")
            self.assertNotIn("GITHUB_TOKEN", env)
        finally:
            del os.environ["GITHUB_TOKEN"]


if __name__ == "__main__":
    unittest.main()
