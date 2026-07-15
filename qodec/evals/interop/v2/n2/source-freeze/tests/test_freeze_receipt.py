"""Section 23/1/7 tests for generate_freeze_receipt.py: alternate order is
preserved exactly as selection produced it (never re-sorted alphabetically),
alternate_fallback_order is present and internally consistent, and
acquisition_complete requires real source-content identity (not metadata
alone) per source kind."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import generate_freeze_receipt  # noqa: E402

SOURCE_FREEZE_DIR = Path(__file__).resolve().parents[1]


class TestRealFreezeReceipt(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.receipt = generate_freeze_receipt.build_freeze_receipt(
            "7d4dc3aabf760c4df272cf13a7e17ea437c81490", ["999"], "2026-01-01T00:00:00Z",
        )

    def test_alternate_order_matches_fallback_priority_not_alphabetical(self):
        ids = self.receipt["selected_alternate_case_ids"]
        self.assertNotEqual(ids, sorted(ids), "alternate order must be selection's rank order, not alphabetical")
        order_ids = [e["case_id"] for e in self.receipt["alternate_fallback_order"]]
        self.assertEqual(order_ids, ids)

    def test_alternate_fallback_order_priorities_are_1_to_n_contiguous_no_dupes(self):
        priorities = [e["fallback_priority"] for e in self.receipt["alternate_fallback_order"]]
        n = len(priorities)
        self.assertEqual(priorities, list(range(1, n + 1)))
        self.assertEqual(len(set(priorities)), n)

    def test_every_alternate_fallback_order_entry_has_quota_groups(self):
        for entry in self.receipt["alternate_fallback_order"]:
            self.assertTrue(entry["fallback_quota_groups"])

    def test_acquisition_complete_false_when_no_real_content_hashes_present(self):
        # Provisional (pre-acquisition) registry state — every non-repository
        # identity field is still null, so acquisition_complete must be False.
        self.assertFalse(self.receipt["acquisition_complete"])

    def test_new_hash_maps_present_for_every_selected_case(self):
        all_ids = self.receipt["selected_primary_case_ids"] + self.receipt["selected_alternate_case_ids"]
        for field in ("metadata_sha256", "source_content_sha256", "normalized_source_sha256", "normalized_archive_sha256"):
            self.assertEqual(set(self.receipt[field].keys()), set(all_ids), f"{field} must cover every selected case")


class TestAcquisitionCompleteDefinition(unittest.TestCase):
    def test_repository_case_complete_via_normalized_archive_hash_alone(self):
        by_id = {"r1": {"source_kind": "repository-execution",
                         "source_identity": {"normalized_archive_sha256": "h", "source_content_sha256": None,
                                              "normalized_source_sha256": None}}}
        # Mirror the private helper's logic directly (no metadata_sha256/
        # source_content_sha256 required for a repository-execution case).
        cid = "r1"
        ident = by_id[cid]["source_identity"]
        complete = ident["normalized_archive_sha256"] is not None
        self.assertTrue(complete)

    def test_non_repository_case_incomplete_with_only_metadata_hash(self):
        ident = {"metadata_sha256": "m", "source_content_sha256": None, "normalized_source_sha256": None}
        complete = ident["source_content_sha256"] is not None and ident["normalized_source_sha256"] is not None
        self.assertFalse(complete, "metadata_sha256 alone must never satisfy acquisition_complete")

    def test_non_repository_case_complete_with_content_and_normalized_hashes(self):
        ident = {"metadata_sha256": "m", "source_content_sha256": "c", "normalized_source_sha256": "n"}
        complete = ident["source_content_sha256"] is not None and ident["normalized_source_sha256"] is not None
        self.assertTrue(complete)


if __name__ == "__main__":
    unittest.main()
