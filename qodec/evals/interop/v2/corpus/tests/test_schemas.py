"""Schema + happy-path validation of the committed demonstration bundle."""
import tempfile
import unittest
from pathlib import Path

import corpus_testutil as U


class TestSchemas(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        U.make_temp_corpus(Path(self.tmp.name))

    def tearDown(self):
        self.tmp.cleanup()

    def test_valid_demonstration_bundle_passes(self):
        v = U.run_validate()
        self.assertEqual(v, [], "committed demonstration bundle must validate:\n" + "\n".join(v))

    def test_manifest_declares_zero_benchmark_cases(self):
        import corpus_tool as ct
        m = U.load(ct.MANIFEST_PATH)
        self.assertEqual(m["benchmark_cases"], [])
        self.assertIn("deterministic-log-demo", m["demonstration_cases"])

    def test_all_schema_files_are_loadable_json(self):
        import corpus_tool as ct
        for name in ["case-bundle", "capture-recipe", "provenance", "execution-receipt",
                     "snapshot-manifest", "evidence-map", "corpus-manifest"]:
            s = U.load(ct.SCHEMAS_DIR / f"{name}.schema.json")
            self.assertEqual(s.get("type"), "object")


if __name__ == "__main__":
    unittest.main()
