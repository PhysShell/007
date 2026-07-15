"""Unit tests (synthetic, no network) for derive_raw_input.py."""
import json
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import derive_raw_input as d  # noqa: E402

SOURCE_FREEZE_TOOLS = Path(__file__).resolve().parents[2] / "source-freeze" / "tools"
sys.path.insert(0, str(SOURCE_FREEZE_TOOLS))
import archive_security  # noqa: E402


def _manifest(case_id: str, recipe: dict) -> dict:
    return {"case_id": case_id, "execution_expectation": {"extraction_recipe": recipe}}


def _write_sfm(root: Path, entries: list[dict]):
    (root / "source-file-manifest.json").write_text(json.dumps(entries))


class TestVerifySourceFileManifest(unittest.TestCase):
    def test_matching_files_produce_no_problems(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "source").mkdir()
            (root / "source" / "a.txt").write_bytes(b"hello")
            sfm = [{"path": "source/a.txt", "sha256": d.sha256_bytes(b"hello")}]
            self.assertEqual(d.verify_source_file_manifest(root, sfm), [])

    def test_hash_mismatch_detected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "source").mkdir()
            (root / "source" / "a.txt").write_bytes(b"hello")
            sfm = [{"path": "source/a.txt", "sha256": "wrong"}]
            problems = d.verify_source_file_manifest(root, sfm)
            self.assertEqual(len(problems), 1)
            self.assertEqual(problems[0]["problem"], "sha256 mismatch")

    def test_missing_file_detected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sfm = [{"path": "source/missing.txt", "sha256": "whatever"}]
            problems = d.verify_source_file_manifest(root, sfm)
            self.assertEqual(len(problems), 1)
            self.assertIn("missing", problems[0]["problem"])


class TestResolveInputFileIdentity(unittest.TestCase):
    def test_explicit_identity_used_verbatim(self):
        recipe = {"input_file_identity": "source/x.csv"}
        self.assertEqual(d.resolve_input_file_identity(recipe, []), "source/x.csv")

    def test_null_identity_with_exactly_one_file_resolves(self):
        recipe = {"input_file_identity": None}
        sfm = [{"path": "source/only.log", "sha256": "h"}]
        self.assertEqual(d.resolve_input_file_identity(recipe, sfm), "source/only.log")

    def test_null_identity_with_multiple_files_is_an_error(self):
        recipe = {"input_file_identity": None}
        sfm = [{"path": "source/a.log", "sha256": "h1"}, {"path": "source/b.log", "sha256": "h2"}]
        with self.assertRaises(d.DerivationError):
            d.resolve_input_file_identity(recipe, sfm)

    def test_null_identity_with_zero_files_is_an_error(self):
        recipe = {"input_file_identity": None}
        with self.assertRaises(d.DerivationError):
            d.resolve_input_file_identity(recipe, [])


class TestApplyByteRange(unittest.TestCase):
    def test_full_range_when_under_max(self):
        self.assertEqual(d.apply_byte_range(b"hello", 0, 4096), b"hello")

    def test_truncates_at_max(self):
        self.assertEqual(d.apply_byte_range(b"abcdefgh", 0, 4), b"abcd")

    def test_offset_applied(self):
        self.assertEqual(d.apply_byte_range(b"abcdefgh", 2, 3), b"cde")


class TestExtractArchiveMemberBytes(unittest.TestCase):
    def test_tar_gz_member_extracted(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            member_path = root / "inner.log"
            member_path.write_bytes(b"log content here")
            archive_path = root / "bundle.tar.gz"
            with tarfile.open(archive_path, "w:gz") as tf:
                tf.add(member_path, arcname="inner.log")
            data = d.extract_archive_member_bytes(archive_path, "inner.log", archive_security)
            self.assertEqual(data, b"log content here")

    def test_zip_member_extracted(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive_path = root / "bundle.zip"
            with zipfile.ZipFile(archive_path, "w") as zf:
                zf.writestr("dir/inner.log", "zip log content")
            data = d.extract_archive_member_bytes(archive_path, "dir/inner.log", archive_security)
            self.assertEqual(data, b"zip log content")

    def test_missing_member_raises(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive_path = root / "bundle.zip"
            with zipfile.ZipFile(archive_path, "w") as zf:
                zf.writestr("present.log", "x")
            with self.assertRaises(d.DerivationError):
                d.extract_archive_member_bytes(archive_path, "absent.log", archive_security)


class TestDeriveRawInputEndToEnd(unittest.TestCase):
    def test_direct_file_case_like_dataset_rtn_traffic_ids(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "source").mkdir()
            content = b"x" * 5_000_000  # over the 4 MiB cap
            (root / "source" / "big.csv").write_bytes(content)
            _write_sfm(root, [{"path": "source/big.csv", "sha256": d.sha256_bytes(content)}])
            manifest = _manifest("synthetic-direct", {
                "input_file_identity": "source/big.csv", "archive_member": None,
                "starting_line_or_byte_offset": 0, "maximum_extracted_source_bytes": 4194304,
            })
            result = d.derive_raw_input(root, manifest, archive_security)
            self.assertEqual(result["problems"], [])
            self.assertEqual(result["derived_byte_size"], 4194304)
            self.assertTrue(result["utf8_valid"])
            self.assertEqual(result["derived_raw_input_sha256"], d.sha256_bytes(content[:4194304]))

    def test_null_identity_case_like_ci_log(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "source").mkdir()
            content = b"build log line 1\nbuild log line 2\n"
            (root / "source" / "job-abc.log").write_bytes(content)
            _write_sfm(root, [{"path": "source/job-abc.log", "sha256": d.sha256_bytes(content)}])
            manifest = _manifest("synthetic-ci-log", {
                "input_file_identity": None, "archive_member": None,
                "starting_line_or_byte_offset": 0, "maximum_extracted_source_bytes": 4194304,
            })
            result = d.derive_raw_input(root, manifest, archive_security)
            self.assertEqual(result["problems"], [])
            self.assertEqual(result["input_file_identity"], "source/job-abc.log")
            self.assertEqual(result["derived_raw_input_sha256"], d.sha256_bytes(content))

    def test_nested_archive_member_case_like_loghub(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "source").mkdir()
            inner_content = b"proxifier log content\n"
            with tarfile.open(root / "source" / "Proxifier.tar.gz", "w:gz") as tf:
                inner_path = root / "Proxifier.log"
                inner_path.write_bytes(inner_content)
                tf.add(inner_path, arcname="Proxifier.log")
            outer_hash = d.sha256_file(root / "source" / "Proxifier.tar.gz")
            _write_sfm(root, [{"path": "source/Proxifier.tar.gz", "sha256": outer_hash}])
            manifest = _manifest("synthetic-nested", {
                "input_file_identity": "source/Proxifier.tar.gz", "archive_member": "Proxifier.log",
                "starting_line_or_byte_offset": 0, "maximum_extracted_source_bytes": 4194304,
            })
            result = d.derive_raw_input(root, manifest, archive_security)
            self.assertEqual(result["problems"], [])
            self.assertEqual(result["derived_raw_input_sha256"], d.sha256_bytes(inner_content))

    def test_invalid_utf8_is_reported_not_lossy_decoded(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "source").mkdir()
            content = b"\xff\xfe not valid utf-8"
            (root / "source" / "bad.bin").write_bytes(content)
            _write_sfm(root, [{"path": "source/bad.bin", "sha256": d.sha256_bytes(content)}])
            manifest = _manifest("synthetic-invalid-utf8", {
                "input_file_identity": "source/bad.bin", "archive_member": None,
                "starting_line_or_byte_offset": 0, "maximum_extracted_source_bytes": 4194304,
            })
            result = d.derive_raw_input(root, manifest, archive_security)
            self.assertFalse(result["utf8_valid"])
            self.assertEqual(len(result["problems"]), 1)
            # A real failure is still hash-recorded (bytes are preserved, not discarded)
            self.assertEqual(result["derived_raw_input_sha256"], d.sha256_bytes(content))

    def test_source_file_manifest_mismatch_short_circuits_before_extraction(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "source").mkdir()
            (root / "source" / "a.csv").write_bytes(b"data")
            _write_sfm(root, [{"path": "source/a.csv", "sha256": "deliberately-wrong"}])
            manifest = _manifest("synthetic-mismatch", {
                "input_file_identity": "source/a.csv", "archive_member": None,
                "starting_line_or_byte_offset": 0, "maximum_extracted_source_bytes": 4194304,
            })
            result = d.derive_raw_input(root, manifest, archive_security)
            self.assertEqual(len(result["problems"]), 1)
            self.assertEqual(result["problems"][0]["problem"], "sha256 mismatch")
            self.assertNotIn("derived_raw_input_sha256", result)


if __name__ == "__main__":
    unittest.main()
