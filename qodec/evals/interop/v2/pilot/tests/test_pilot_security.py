"""Security tests: the pilot inherits the N0 capture protections — no shell, no
credential forwarding, no bundle path escape. These assert the protections are
active for every committed recipe and as unit behaviour.
"""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import pilot_lib as pl  # noqa: E402

CASES = pl.case_ids()


class TestRecipeSafety(unittest.TestCase):
    def test_no_recipe_uses_a_shell(self):
        for c in CASES:
            recipe = pl.load_json(pl.bundle_dir(c) / "capture-recipe.json")
            for step in [recipe["native"], {"argv": recipe["rtk"]["argv"]}]:
                pl.capture.assert_argv_no_shell(step["argv"])  # raises on violation

    def test_no_recipe_allowlists_a_credential(self):
        for c in CASES:
            recipe = pl.load_json(pl.bundle_dir(c) / "capture-recipe.json")
            for name in recipe["environment_allowlist"]:
                self.assertFalse(pl.capture.env_name_is_forbidden(name), f"{c}:{name}")

    def test_network_disabled_everywhere(self):
        for c in CASES:
            recipe = pl.load_json(pl.bundle_dir(c) / "capture-recipe.json")
            self.assertEqual(recipe["network_policy"], "disabled", c)


class TestCaptureGuards(unittest.TestCase):
    def test_shell_interpreter_rejected(self):
        with self.assertRaises(pl.capture.CaptureError):
            pl.capture.assert_argv_no_shell(["bash", "-c", "echo hi"])

    def test_shell_metachar_rejected(self):
        with self.assertRaises(pl.capture.CaptureError):
            pl.capture.assert_argv_no_shell(["rg", "x | y"])

    def test_credential_env_names_forbidden(self):
        for name in ("GITHUB_TOKEN", "AWS_SECRET_ACCESS_KEY", "MY_API_KEY", "DB_PASSWORD"):
            self.assertTrue(pl.capture.env_name_is_forbidden(name), name)

    def test_path_escape_rejected(self):
        base = pl.PILOT_DIR
        with self.assertRaises(pl.capture.CaptureError):
            pl.capture.safe_join(base, "../../../etc/passwd")
        with self.assertRaises(pl.capture.CaptureError):
            pl.capture.safe_join(base, "/etc/passwd")

    def test_credential_stripped_from_child_env(self):
        import os
        os.environ["SNEAKY_TOKEN"] = "leak"
        try:
            recipe = {"locale": "C.UTF-8", "timezone": "UTC", "source_date_epoch": 1700000000}
            env = pl.capture.build_child_env(["PATH", "SNEAKY_TOKEN"], recipe, "/tmp")
            self.assertNotIn("SNEAKY_TOKEN", env)
        finally:
            os.environ.pop("SNEAKY_TOKEN", None)


if __name__ == "__main__":
    unittest.main()
