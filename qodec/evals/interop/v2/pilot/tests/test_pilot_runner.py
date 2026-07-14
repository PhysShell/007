"""Four-arm runner tests.

Integrity-negative tests (a nonzero RTK exit, an empty RTK output for a non-empty
raw, or a missing snapshot manifest must never pass) run on synthetic bundles and
need no tools. The positive four-arm / roundtrip tests need the pinned qodec
binary and the committed snapshots, and skip otherwise.
"""
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import pilot_lib as pl  # noqa: E402
import pilot_validate as pv  # noqa: E402
import pilot_run as pr  # noqa: E402

CASES = pl.case_ids()
HAVE_QODEC = Path(pl.resolve_exe("qodec")).exists()
HAVE_SNAP = all((pl.bundle_dir(c) / pl.snap.RAW_STDOUT).exists() for c in CASES)


def _synth_bundle(root: Path, cid="synth", rtk_exit=0, rtk_stdout=b"reduced\n",
                  raw_stdout=b"real tool line\nsecond line\n"):
    b = root / cid
    (b / "snapshots").mkdir(parents=True)
    (b / "receipts").mkdir(parents=True)
    (b / "fixture").mkdir(parents=True)
    (b / "fixture" / "in.txt").write_bytes(b"x\n")
    pl.rcpt.write_json  # ensure import
    case = {"case_id": cid, "split": "public-development", "family": "search-listing",
            "ecosystem": "language-neutral", "tool": "t", "primary_stream": "raw.stdout",
            "origin_kind": "generated-first-party", "capture_mode": "reproducible-command",
            "rtk_mode": "pipe-filter", "rtk_filter": "grep", "snapshot_policy": "raw-and-rtk",
            "outcome": "success-clean", "hand_authored": False,
            "provenance_path": "provenance.json", "capture_recipe_path": "capture-recipe.json",
            "anchors_path": "anchors.json", "snapshot_manifest_path": "snapshot-manifest.json",
            "markers": list(pv.REQUIRED_MARKERS), "tags": [], "notes": "synthetic"}
    pl.rcpt.write_json(b / "case.json", case)
    (b / "snapshots" / "raw.stdout").write_bytes(raw_stdout)
    (b / "snapshots" / "raw.stderr").write_bytes(b"")
    (b / "snapshots" / "rtk.stdout").write_bytes(rtk_stdout)
    (b / "snapshots" / "rtk.stderr").write_bytes(b"")
    recipe = {"native": {"argv": ["rg", "x", "fixture"], "cwd": ".", "stdin_path": None},
              "rtk": {"argv": ["rtk", "pipe", "--filter", "grep"]},
              "environment_allowlist": ["PATH"], "expected_exit_code_class": "exact",
              "expected_exit_code": 0, "timeout_s": 60, "network_policy": "disabled",
              "locale": "C.UTF-8", "timezone": "UTC", "source_date_epoch": 1700000000}
    identity = pl.rcpt.assemble_identity(pl.REPO_ROOT)

    def step(argv, out, code):
        return {"argv": argv, "cwd": ".", "stdin_bytes": b"", "stdout": out,
                "stderr": b"", "exit_code": code, "wall_time_s": 0.0, "timed_out": False}
    pl.rcpt.write_json(b / "receipts" / "native.json",
                       pl.rcpt.build_receipt(cid, "native", step(["rg"], raw_stdout, 0),
                                             recipe, identity, "rg", None))
    rtk_extra = {"rtk_source_sha": None, "rtk_argv": ["rtk", "pipe", "--filter", "grep"],
                 "rtk_classification": "failed" if rtk_exit else "reduced",
                 "payload_changed": True, "never_worse_returned_raw": False}
    pl.rcpt.write_json(b / "receipts" / "rtk.json",
                       pl.rcpt.build_receipt(cid, "rtk", step(["rtk"], rtk_stdout, rtk_exit),
                                             recipe, identity, "rtk", None, rtk_extra))
    pl.rcpt.write_json(b / "provenance.json", {
        "origin_kind": "generated-first-party", "source_description": "synthetic",
        "source_revision": "x", "source_sha256": "0" * 64, "license": "in-repo",
        "sanitization": "none-required", "generator_identity": "rg", "created_by_scope": "N1"})
    pl.rcpt.write_json(b / "capture-recipe.json", recipe)
    pl.rcpt.write_json(b / "anchors.json", {"case_id": cid, "primary_stream": "raw.stdout",
                       "anchors": [{"anchor_id": "a", "kind": "exact", "value": "real tool line",
                                    "stream": "raw.stdout"}]})
    sm = pl.build_snapshot_manifest(b, case)
    pl.rcpt.write_json(b / "snapshot-manifest.json", sm)
    return b


class TestIntegrityNegatives(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self._orig = pl.CASES_DIR

    def tearDown(self):
        pl.CASES_DIR = self._orig
        pv.pl.CASES_DIR = self._orig
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _point_at(self, root):
        pl.CASES_DIR = root
        pv.pl.CASES_DIR = root

    def test_nonzero_rtk_exit_cannot_pass(self):
        _synth_bundle(self.tmp, "synth", rtk_exit=1)
        self._point_at(self.tmp)
        res = pv.Result()
        pv.validate_case("synth", res, inputs_only=False)
        self.assertTrue(any("rtk-failed" in v for v in res.violations), res.violations)

    def test_missing_snapshot_manifest_cannot_pass(self):
        b = _synth_bundle(self.tmp, "synth2")
        (b / "snapshot-manifest.json").unlink()
        self._point_at(self.tmp)
        res = pv.Result()
        pv.validate_case("synth2", res, inputs_only=False)
        self.assertTrue(any("missing-snapshot-manifest" in v for v in res.violations), res.violations)

    def test_tampered_snapshot_hash_cannot_pass(self):
        b = _synth_bundle(self.tmp, "synth3")
        (b / "snapshots" / "raw.stdout").write_bytes(b"TAMPERED\n")  # manifest now stale
        self._point_at(self.tmp)
        res = pv.Result()
        pv.validate_case("synth3", res, inputs_only=False)
        self.assertTrue(any("hash" in v for v in res.violations), res.violations)

    @unittest.skipUnless(HAVE_QODEC, "pinned qodec required")
    def test_empty_rtk_for_nonempty_raw_fails_invariant(self):
        _synth_bundle(self.tmp, "synth4", rtk_stdout=b"")
        self._point_at(self.tmp)
        result = pr.run_case("synth4", pl.resolve_exe("qodec"))
        empty_inv = next(i for i in result["invariants"] if "non-empty" in i["invariant"])
        self.assertFalse(empty_inv["ok"])
        self.assertFalse(result["invariants_ok"])


@unittest.skipUnless(HAVE_QODEC and HAVE_SNAP, "pinned qodec + committed snapshots required")
class TestFourArm(unittest.TestCase):
    def test_all_four_arms_present_and_roundtrip(self):
        qbin = pl.resolve_exe("qodec")
        for c in CASES:
            r = pr.run_case(c, qbin)
            self.assertEqual(set(r["arms"]), set(pr.ARMS), c)
            self.assertTrue(r["arms"]["QODEC"]["roundtrip_ok"], f"{c} qodec(raw)")
            self.assertTrue(r["arms"]["RTK+QODEC"]["roundtrip_ok"], f"{c} qodec(rtk)")
            self.assertTrue(r["invariants_ok"], f"{c}: {[i for i in r['invariants'] if not i['ok']]}")


if __name__ == "__main__":
    unittest.main()
