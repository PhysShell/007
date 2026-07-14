"""Static + integrity tests for the Scope N1 pilot corpus.

Snapshot-dependent tests (hashes, receipts, anchors, four-arm) skip when the
canonical snapshots have not yet been captured by the pinned toolchain, so the
suite is green on authoring inputs and fully exercised in CI where the committed
snapshots exist.
"""
import json
import os
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import pilot_lib as pl  # noqa: E402
import pilot_validate as pv  # noqa: E402

MANIFEST = pl.load_json(pl.MANIFEST_PATH)
CASES = MANIFEST["cases"]


def _has_snapshots():
    return all((pl.bundle_dir(c) / pl.snap.RAW_STDOUT).exists() for c in CASES)


class TestManifest(unittest.TestCase):
    def test_exactly_ten_cases(self):
        self.assertEqual(len(CASES), 10)
        self.assertEqual(MANIFEST["case_count"], 10)

    def test_unique_case_ids(self):
        self.assertEqual(len(set(CASES)), 10)

    def test_non_gating_and_public_dev_only(self):
        self.assertTrue(MANIFEST["non_gating"])
        self.assertEqual(MANIFEST["split_policy"], "public-development-only")

    def test_manifest_schema(self):
        self.assertEqual(pl.js.validate(MANIFEST, pl.load_schema("pilot-manifest.schema.json")), [])


class TestCorpusRules(unittest.TestCase):
    def setUp(self):
        self.cases = {c: pl.load_json(pl.bundle_dir(c) / "case.json") for c in CASES}

    def test_every_case_public_development(self):
        for c, j in self.cases.items():
            self.assertEqual(j["split"], "public-development", c)

    def test_zero_validation_or_sealed_cases(self):
        for c, j in self.cases.items():
            self.assertNotIn(j["split"], ("public-validation", "sealed-heldout"), c)
            for m in j["markers"]:
                self.assertNotIn(m, ("PUBLIC-VALIDATION", "SEALED-HELDOUT", "HELD-OUT", "SEALED"), c)

    def test_no_payload_hand_authored(self):
        for c, j in self.cases.items():
            self.assertIs(j["hand_authored"], False, c)

    def test_family_and_ecosystem_diversity(self):
        fams = {j["family"] for j in self.cases.values()}
        ecos = {j["ecosystem"] for j in self.cases.values()}
        self.assertGreaterEqual(len(fams), 4)
        self.assertGreaterEqual(len(ecos), 3)

    def test_every_case_has_provenance_and_license(self):
        for c in CASES:
            prov = pl.load_json(pl.bundle_dir(c) / "provenance.json")
            self.assertTrue(prov.get("license"), c)
            self.assertTrue(prov.get("secret_review"), c)
            self.assertTrue(prov.get("pii_review"), c)
            self.assertEqual(pl.js.validate(prov, pv._corpus_schema("provenance.schema.json")), [], c)

    def test_case_and_anchor_schemas(self):
        for c, j in self.cases.items():
            self.assertEqual(pl.js.validate(j, pl.load_schema("pilot-case.schema.json")), [], c)
            anc = pl.load_json(pl.bundle_dir(c) / "anchors.json")
            self.assertEqual(pl.js.validate(anc, pl.load_schema("pilot-anchors.schema.json")), [], c)

    def test_validate_inputs_passes(self):
        rc = pv.main(["--inputs-only"])
        self.assertEqual(rc, 0)


@unittest.skipUnless(_has_snapshots(), "canonical snapshots not captured yet")
class TestCommittedSnapshots(unittest.TestCase):
    def test_snapshot_hashes_validate(self):
        rc = pv.main([])   # full validation incl. hashes, receipts, anchors-in-raw
        self.assertEqual(rc, 0)

    def test_raw_and_rtk_hashes_match_manifest(self):
        for c in CASES:
            b = pl.bundle_dir(c)
            case = pl.load_json(b / "case.json")
            sm = pl.load_json(b / case["snapshot_manifest_path"])
            self.assertEqual(pl.verify_snapshot_manifest(b, case, sm), [], c)

    def test_rtk_receipt_not_failed(self):
        for c in CASES:
            r = pl.load_json(pl.bundle_dir(c) / pl.snap.RTK_RECEIPT)
            self.assertEqual(r["exit_code"], 0, c)
            self.assertNotEqual(r["rtk_classification"], "failed", c)

    def test_anchors_present_in_raw(self):
        for c in CASES:
            b = pl.bundle_dir(c)
            for a in pl.load_json(b / "anchors.json")["anchors"]:
                if a["stream"].startswith("raw"):
                    data = (b / pl.STREAM_FILE[a["stream"]]).read_text("utf-8", "replace")
                    self.assertIn(a["value"], data, f"{c}:{a['anchor_id']}")


if __name__ == "__main__":
    unittest.main()
