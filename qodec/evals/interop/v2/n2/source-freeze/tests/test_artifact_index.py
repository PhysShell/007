"""Section 23/8 tests for artifact_index.py: real artifact_id/archive_digest
population, logical vs execution SHA distinction, self-exclusion, and the
required negative-test coverage (missing artifact_id/archive_digest/
logical_head_sha/per-file hash)."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import artifact_index  # noqa: E402


def _make_artifact_dir(root: Path, name: str, files: dict) -> None:
    d = root / name
    d.mkdir(parents=True, exist_ok=True)
    for rel, content in files.items():
        p = d / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(content.encode() if isinstance(content, str) else content)


class TestBuildIndex(unittest.TestCase):
    def setUp(self):
        import tempfile
        self._tmpdir = tempfile.TemporaryDirectory()
        self.root = Path(self._tmpdir.name)

    def tearDown(self):
        self._tmpdir.cleanup()

    def test_real_ids_and_digests_populated_from_api_response(self):
        _make_artifact_dir(self.root, "n2-source-freeze", {"receipt.json": "{}"})
        api_artifacts = [{"name": "n2-source-freeze", "id": 12345, "digest": "sha256:abcdef"}]
        index = artifact_index.build_index(
            self.root, api_artifacts, source_workflow_run="999",
            logical_head_sha="deadbeef" * 5, execution_sha="merge" + "0" * 35,
            self_artifact_name="n2c-artifact-index",
        )
        self.assertEqual(len(index), 1)
        entry = index[0]
        self.assertEqual(entry["artifact_id"], 12345)
        self.assertEqual(entry["archive_digest"], "sha256:abcdef")
        self.assertEqual(entry["logical_head_sha"], "deadbeef" * 5)
        self.assertEqual(entry["execution_sha"], "merge" + "0" * 35)
        self.assertNotEqual(entry["logical_head_sha"], entry["execution_sha"])
        self.assertEqual(len(entry["contained_files"]), 1)
        self.assertTrue(entry["contained_files"][0]["sha256"])

    def test_self_artifact_excluded_from_index(self):
        _make_artifact_dir(self.root, "n2c-artifact-index", {"n2c-artifact-index.json": "[]"})
        _make_artifact_dir(self.root, "other-artifact", {"f.json": "{}"})
        api_artifacts = [{"name": "other-artifact", "id": 1, "digest": "sha256:aa"}]
        index = artifact_index.build_index(
            self.root, api_artifacts, "1", "head", "exec", self_artifact_name="n2c-artifact-index",
        )
        names = [e["artifact_name"] for e in index]
        self.assertNotIn("n2c-artifact-index", names)
        self.assertIn("other-artifact", names)


class TestValidateArtifactIndex(unittest.TestCase):
    def _valid_entry(self, **overrides):
        entry = {
            "artifact_name": "a", "artifact_id": 1, "archive_digest": "sha256:aa",
            "contained_files": [{"path": "f.json", "sha256": "a" * 64}],
            "source_workflow_run": "1", "logical_head_sha": "h" * 40, "execution_sha": "e" * 40,
        }
        entry.update(overrides)
        return entry

    def test_valid_index_passes(self):
        self.assertEqual(artifact_index.validate_artifact_index([self._valid_entry()]), [])

    def test_missing_artifact_id_rejected(self):
        entry = self._valid_entry(artifact_id=None)
        errors = artifact_index.validate_artifact_index([entry])
        self.assertTrue(any("artifact_id" in e for e in errors))

    def test_missing_archive_digest_rejected(self):
        entry = self._valid_entry(archive_digest=None)
        errors = artifact_index.validate_artifact_index([entry])
        self.assertTrue(any("archive_digest" in e for e in errors))

    def test_missing_logical_head_sha_rejected(self):
        entry = self._valid_entry()
        del entry["logical_head_sha"]
        errors = artifact_index.validate_artifact_index([entry])
        self.assertTrue(any("logical_head_sha" in e for e in errors))

    def test_missing_per_file_hash_rejected(self):
        entry = self._valid_entry(contained_files=[{"path": "f.json", "sha256": ""}])
        errors = artifact_index.validate_artifact_index([entry])
        self.assertTrue(any("sha256" in e for e in errors))

    def test_logical_head_sha_without_execution_sha_rejected(self):
        entry = self._valid_entry()
        del entry["execution_sha"]
        errors = artifact_index.validate_artifact_index([entry])
        self.assertTrue(any("distinct fields" in e for e in errors))


if __name__ == "__main__":
    unittest.main()
