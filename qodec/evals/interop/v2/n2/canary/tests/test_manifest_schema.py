"""Schema + static-content checks for the N2-A source manifest."""
import json
import sys
import unittest
from pathlib import Path

CANARY_DIR = Path(__file__).resolve().parents[1]
CORPUS_TOOLS = CANARY_DIR.parents[1] / "corpus" / "tools"
sys.path.insert(0, str(CORPUS_TOOLS))
import jsonschema_mini  # noqa: E402

MANIFEST_PATH = CANARY_DIR / "source-manifest.json"
SCHEMA_PATH = CANARY_DIR / "schemas" / "source-manifest.schema.json"


class TestManifestSchema(unittest.TestCase):
    def setUp(self):
        self.manifest = json.loads(MANIFEST_PATH.read_text())
        self.schema = json.loads(SCHEMA_PATH.read_text())

    def test_manifest_validates_against_schema(self):
        errors = jsonschema_mini.validate(self.manifest, self.schema)
        self.assertEqual(errors, [])

    def test_approved_commit_is_full_immutable_sha(self):
        sha = self.manifest["repository"]["approved_commit_sha"]
        self.assertRegex(sha, r"^[0-9a-f]{40}$")

    def test_repository_is_not_this_sessions_own_repos(self):
        # N2-A mines exactly one explicitly-approved third-party repository;
        # it must never point at one of the repos this session already has
        # write access to (that would defeat the trust-boundary purpose).
        url = self.manifest["repository"]["url"].lower()
        for forbidden in ("physshell/007", "physshell/own.net", "physshell/ownaudit"):
            self.assertNotIn(forbidden, url)

    def test_zero_dependency_expectation(self):
        self.assertEqual(self.manifest["project"]["expected_package_reference_count"], 0)
        self.assertEqual(self.manifest["project"]["expected_project_reference_count"], 0)

    def test_build_argv_uses_no_restore(self):
        self.assertIn("--no-restore", self.manifest["build"]["argv"])

    def test_license_is_mit(self):
        self.assertEqual(self.manifest["license"]["spdx"], "MIT")

    def test_schema_rejects_missing_required_field(self):
        broken = dict(self.manifest)
        del broken["license"]
        errors = jsonschema_mini.validate(broken, self.schema)
        self.assertTrue(any("license" in e for e in errors))

    def test_schema_rejects_non_full_sha(self):
        broken = json.loads(json.dumps(self.manifest))
        broken["repository"]["approved_commit_sha"] = "ee3f780"  # short/floating-looking
        errors = jsonschema_mini.validate(broken, self.schema)
        self.assertTrue(any("approved_commit_sha" in e for e in errors))


if __name__ == "__main__":
    unittest.main()
