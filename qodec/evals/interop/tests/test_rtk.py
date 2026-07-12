"""RTK invocation: the real interface the harness was built against.

Integration — skips when the rtk binary is not resolvable (RTK_BIN / PATH).
Pins the two facts the harness depends on: `log` is a stdin filter that reduces
a repetitive log, and the pinned version is what doctor expects.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import execution, lockfiles, qodec, transforms  # noqa: E402
from bench.manifest import Transform  # noqa: E402

_RTK = lockfiles.tools().get("rtk")
_RTK_BIN = _RTK.resolve_bin() if _RTK else None

_REPETITIVE_LOG = "".join(
    f"[2026-07-12T10:00:{i:02d}] ERROR worker failed: connection reset by peer\n"
    for i in range(30)
)


class RtkContract(unittest.TestCase):
    def test_log_is_declared_stdin_filter(self):
        self.assertIn("log", _RTK.stdin_filters)

    @unittest.skipUnless(_RTK_BIN, "rtk not resolvable")
    def test_pinned_version_matches(self):
        self.assertEqual(_RTK.detected_version(), _RTK.pinned_version)

    @unittest.skipUnless(_RTK_BIN, "rtk not resolvable")
    def test_log_filter_reduces_tokens_via_stdin(self):
        ex = transforms.apply_rtk(_REPETITIVE_LOG, Transform("rtk", {"filter": "log"}),
                                  lockfiles.tools())
        self.assertEqual(ex.exit_code, 0)
        before = qodec.count(_REPETITIVE_LOG)
        after = qodec.count(ex.text)
        self.assertLess(after, before, "rtk log must compress a repetitive log")

    @unittest.skipUnless(_RTK_BIN, "rtk not resolvable")
    def test_command_runner_filter_rejected_as_transform(self):
        # `grep` proxies a native command; used as a stdin transform it would
        # ignore the input, so apply_rtk must refuse it.
        with self.assertRaises(transforms.UnsupportedTransform):
            transforms.apply_rtk("x\n", Transform("rtk", {"filter": "grep"}), lockfiles.tools())


if __name__ == "__main__":
    unittest.main()
