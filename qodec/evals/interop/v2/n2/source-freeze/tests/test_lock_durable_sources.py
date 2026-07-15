"""Section 23/9 tests for lock_durable_sources.py: real acquired bytes get
staged into a durable repo path for expiring sources (CI logs, bot output),
while DOI-bound datasets/research-corpus are exempt (already durable via
their own versioned publisher record + checksum)."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import lock_durable_sources  # noqa: E402


def _registry(*candidates):
    return {"candidates": list(candidates)}


class TestStageDurableSources(unittest.TestCase):
    def setUp(self):
        import tempfile
        self._tmpdir = tempfile.TemporaryDirectory()
        self.acq_root = Path(self._tmpdir.name) / "acquisition"
        self.acq_root.mkdir()
        # MUST be an explicit temp dir, never the real FROZEN_SOURCES_DIR
        # default — otherwise running this test suite would write real
        # files into the working tree.
        self.dest_root = Path(self._tmpdir.name) / "frozen-sources"

    def tearDown(self):
        self._tmpdir.cleanup()

    def _stage(self, reg):
        return lock_durable_sources.stage_durable_sources(self.acq_root, reg, dest_root=self.dest_root)

    def test_ci_log_content_is_staged(self):
        case_dir = self.acq_root / "ci-log-c1" / "source"
        case_dir.mkdir(parents=True)
        (case_dir / "job-1.log").write_bytes(b"real log bytes")
        reg = _registry({
            "candidate_id": "ci-log-c1", "source_kind": "ci-run-artifact",
            "origin_kind": "native-upstream-ci-log", "source_identity": {},
        })
        report = self._stage(reg)
        self.assertEqual(report["staged"], ["ci-log-c1"])
        ident = reg["candidates"][0]["source_identity"]
        self.assertTrue(ident["durable_object_identity"].endswith("frozen-sources/ci-log-c1"))
        self.assertTrue(ident["durable_sha256"])

    def test_bot_output_content_is_staged(self):
        case_dir = self.acq_root / "bot-c1" / "source"
        case_dir.mkdir(parents=True)
        (case_dir / "response.html").write_bytes(b"<html>real bytes</html>")
        reg = _registry({
            "candidate_id": "bot-c1", "source_kind": "bot-output-artifact",
            "origin_kind": "kernel-or-infrastructure-bot", "source_identity": {},
        })
        report = self._stage(reg)
        self.assertEqual(report["staged"], ["bot-c1"])

    def test_doi_bound_dataset_is_exempt_not_staged(self):
        case_dir = self.acq_root / "dataset-c1" / "source"
        case_dir.mkdir(parents=True)
        (case_dir / "file.tar.gz").write_bytes(b"dataset bytes")
        reg = _registry({
            "candidate_id": "dataset-c1", "source_kind": "dataset-artifact",
            "origin_kind": "public-runtime-dataset", "source_identity": {},
        })
        report = self._stage(reg)
        self.assertEqual(report["skipped_exempt"], ["dataset-c1"])
        self.assertIsNone(reg["candidates"][0]["source_identity"].get("durable_object_identity"))

    def test_research_corpus_is_exempt_not_staged(self):
        case_dir = self.acq_root / "corpus-c1" / "source"
        case_dir.mkdir(parents=True)
        (case_dir / "file.zip").write_bytes(b"corpus bytes")
        reg = _registry({
            "candidate_id": "corpus-c1", "source_kind": "research-corpus-artifact",
            "origin_kind": "reproducible-research-corpus", "source_identity": {},
        })
        report = self._stage(reg)
        self.assertEqual(report["skipped_exempt"], ["corpus-c1"])

    def test_repository_execution_is_skipped_entirely(self):
        reg = _registry({
            "candidate_id": "repo-c1", "source_kind": "repository-execution",
            "origin_kind": "repository-miner", "source_identity": {},
        })
        report = self._stage(reg)
        self.assertNotIn("repo-c1", report["staged"])
        self.assertNotIn("repo-c1", report["skipped_exempt"])
        self.assertNotIn("repo-c1", report["skipped_no_content"])

    def test_missing_content_is_recorded_not_silently_skipped(self):
        reg = _registry({
            "candidate_id": "ci-log-missing", "source_kind": "ci-run-artifact",
            "origin_kind": "native-upstream-ci-log", "source_identity": {},
        })
        report = self._stage(reg)
        self.assertEqual(report["skipped_no_content"], ["ci-log-missing"])


if __name__ == "__main__":
    unittest.main()
