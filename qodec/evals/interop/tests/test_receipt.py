"""Receipt validation: doctor's structure and strict gate.

The qodec-health parts need the built binary; those skip if it is absent. The
strict-gate logic (unknown/unsupported tools fail) is pure and always runs.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import doctor, lockfiles  # noqa: E402


def _qodec_present() -> bool:
    from bench import qodec
    try:
        qodec.binary()
        return True
    except Exception:
        return False


class ReceiptShape(unittest.TestCase):
    def test_tools_include_unsupported_flags(self):
        r = doctor.build_receipt([])
        tools = r["tools"]
        for name in ("headroom", "fastcontext", "graphify"):
            self.assertEqual(tools[name]["kind"], "unsupported", name)
            self.assertTrue(tools[name].get("reason"), f"{name} needs a reason")

    def test_repos_pinned(self):
        # repos.lock.toml must pin every repo (no 'unset' in a reproducible run).
        repos = lockfiles.repos()
        self.assertIn("clap", repos)
        self.assertEqual(len(repos["clap"].rev), 40)


class StrictGate(unittest.TestCase):
    def test_unknown_tool_fails_strict(self):
        r = doctor.build_receipt(["no-such-tool"])
        self.assertFalse(r["strict_ok"])
        self.assertTrue(any("no-such-tool" in f for f in r["strict_failures"]))

    def test_unsupported_tool_cannot_be_required(self):
        r = doctor.build_receipt(["headroom"])
        self.assertFalse(r["strict_ok"])


class QodecHealth(unittest.TestCase):
    @unittest.skipUnless(_qodec_present(), "qodec binary not built")
    def test_qodec_roundtrips_in_receipt(self):
        r = doctor.build_receipt([])
        self.assertTrue(r["qodec"]["ok"])
        self.assertTrue(r["qodec"]["roundtrip"])
        self.assertNotEqual(r["qodec"]["meter"], "approx")


if __name__ == "__main__":
    unittest.main()
