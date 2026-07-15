"""Unit tests (synthetic, no network) for build_durable_manifest.py."""
import json
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import build_durable_manifest as m  # noqa: E402


def _fetched_ci_log(candidate_id):
    return {
        "artifact_name": f"acquisition-{candidate_id}", "artifact_id": 1, "api_reported_size_in_bytes": 100,
        "api_reported_digest_sha256": "digest1", "workflow_run_id_of_artifact": "29404265568",
        "contained_files": [
            {"path": f"{candidate_id}/normalized-source.tar", "sha256": "tarhash", "size": 50},
            {"path": f"{candidate_id}/acquisition-receipt.json", "sha256": "receipthash", "size": 20},
        ],
    }


class TestBuildN2cEntries(unittest.TestCase):
    def test_primary_role_assigned(self):
        fetch_report = {"fetched": [_fetched_ci_log("ci-log-nlog")]}
        quota = {"primary_case_ids": ["ci-log-nlog"], "alternate_case_ids": []}
        entries = m.build_n2c_entries(fetch_report, quota, {}, {"ci-log-nlog": "ci-run-artifact"}, {}, "2026-07-15T00:00:00Z")
        self.assertEqual(len(entries), 1)
        self.assertEqual(entries[0]["role"], "primary")
        self.assertEqual(entries[0]["canonical_benchmark_input_path"], "ci-log-nlog/normalized-source.tar")
        self.assertEqual(entries[0]["canonical_benchmark_input_sha256"], "tarhash")

    def test_alternate_role_assigned(self):
        fetch_report = {"fetched": [_fetched_ci_log("ci-log-nlohmann-json")]}
        quota = {"primary_case_ids": [], "alternate_case_ids": ["ci-log-nlohmann-json"]}
        entries = m.build_n2c_entries(fetch_report, quota, {}, {"ci-log-nlohmann-json": "ci-run-artifact"}, {}, "t")
        self.assertEqual(entries[0]["role"], "alternate")

    def test_release_asset_identity_attached(self):
        fetch_report = {"fetched": [_fetched_ci_log("ci-log-nlog")]}
        quota = {"primary_case_ids": ["ci-log-nlog"], "alternate_case_ids": []}
        assets = {"acquisition-ci-log-nlog": {"release_tag": "n2d0-v1", "asset_name": "n2c-acquisition-ci-log-nlog.zip",
                                               "asset_sha256": "assetsha"}}
        entries = m.build_n2c_entries(fetch_report, quota, assets, {"ci-log-nlog": "ci-run-artifact"}, {}, "t")
        self.assertEqual(entries[0]["durable_release_tag"], "n2d0-v1")
        self.assertEqual(entries[0]["durable_release_asset_sha256"], "assetsha")

    def test_non_acquisition_artifacts_excluded(self):
        fetch_report = {"fetched": [{"artifact_name": "n2c-artifact-index"}]}
        quota = {"primary_case_ids": [], "alternate_case_ids": []}
        entries = m.build_n2c_entries(fetch_report, quota, {}, {}, {}, "t")
        self.assertEqual(entries, [])

    def test_entries_sorted_by_logical_id(self):
        fetch_report = {"fetched": [_fetched_ci_log("zzz"), _fetched_ci_log("aaa")]}
        quota = {"primary_case_ids": ["zzz", "aaa"], "alternate_case_ids": []}
        entries = m.build_n2c_entries(fetch_report, quota, {}, {"zzz": "x", "aaa": "x"}, {}, "t")
        self.assertEqual([e["logical_id"] for e in entries], ["aaa", "zzz"])


class TestCanonicalizeAndHash(unittest.TestCase):
    def test_deterministic_across_key_order(self):
        a = {"b": 1, "a": 2}
        b = {"a": 2, "b": 1}
        text_a, hash_a = m.canonicalize_and_hash(a)
        text_b, hash_b = m.canonicalize_and_hash(b)
        self.assertEqual(hash_a, hash_b)
        self.assertEqual(text_a, text_b)

    def test_different_content_different_hash(self):
        _, hash_a = m.canonicalize_and_hash({"x": 1})
        _, hash_b = m.canonicalize_and_hash({"x": 2})
        self.assertNotEqual(hash_a, hash_b)


class TestBuildManifest(unittest.TestCase):
    def test_manifest_self_hash_is_present_and_stable(self):
        n2c_entries = [{"logical_id": "a", "role": "primary"}]
        n2a_entry = {"logical_id": "miner-canary-dotnet-001", "source_workflow_run_id": "29384147131",
                     "accepted_head_sha": "9c755db"}
        result = m.build_manifest(n2c_entries, n2a_entry, ["a"], "2026-07-15T00:00:00Z")
        self.assertIn("manifest_sha256", result["body"])
        self.assertEqual(result["manifest_sha256"], result["body"]["manifest_sha256"])
        # Re-serializing the exact same body (minus the self-hash key) must
        # reproduce the same hash — a real, checkable determinism property.
        body_copy = json.loads(json.dumps(result["body"]))
        del body_copy["manifest_sha256"]
        _, recomputed = m.canonicalize_and_hash(body_copy)
        self.assertEqual(recomputed, result["manifest_sha256"])

    def test_primary_and_alternate_counts(self):
        n2c_entries = [{"logical_id": "a", "role": "primary"}, {"logical_id": "b", "role": "alternate"},
                       {"logical_id": "c", "role": "alternate"}]
        n2a_entry = {"logical_id": "n2a", "source_workflow_run_id": "r", "accepted_head_sha": "h"}
        result = m.build_manifest(n2c_entries, n2a_entry, ["b", "c"], "t")
        self.assertEqual(result["body"]["primary_case_count"], 1)
        self.assertEqual(result["body"]["alternate_case_count"], 2)


if __name__ == "__main__":
    unittest.main()
