"""RTK invocation via `rtk pipe --filter <name>` — the interface v0.42.4 ships.

Integration — skips when the rtk binary is not resolvable (RTK_BIN / PATH).
Pins the facts the harness depends on: the pinned filters are pipe filters, the
pinned version + SHA match, `log`/`grep` reduce a repetitive payload over stdin,
and a native command-runner name is refused as a pipe transform.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import lockfiles, qodec, transforms  # noqa: E402
from bench.manifest import Transform  # noqa: E402

_RTK = lockfiles.tools().get("rtk")
_RTK_BIN = _RTK.resolve_bin() if _RTK else None

_REPETITIVE_LOG = "".join(
    f"[2026-07-12T10:00:{i:02d}] ERROR worker failed: connection reset by peer\n"
    for i in range(30)
)
_GREP_OUTPUT = "".join(f"src/lib.rs:{i}:    let parser = Parser::new();\n" for i in range(40))


class RtkContract(unittest.TestCase):
    def test_pinned_pipe_filters(self):
        self.assertIn("log", _RTK.pipe_filters)
        self.assertIn("grep", _RTK.pipe_filters)

    @unittest.skipUnless(_RTK_BIN, "rtk not resolvable")
    def test_pinned_version_matches(self):
        self.assertEqual(_RTK.detected_version(), _RTK.pinned_version)

    @unittest.skipUnless(_RTK_BIN, "rtk not resolvable")
    def test_pinned_sha256_matches(self):
        if _RTK.pinned_sha256:
            self.assertEqual(_RTK.actual_sha256(), _RTK.pinned_sha256)

    @unittest.skipUnless(_RTK_BIN, "rtk not resolvable")
    def test_pipe_log_reduces_tokens(self):
        ex = transforms.apply_rtk(_REPETITIVE_LOG, Transform("rtk", {"filter": "log"}),
                                  lockfiles.tools())
        self.assertEqual(ex.exit_code, 0)
        self.assertIn("pipe", ex.argv)  # ran `rtk pipe --filter log`
        self.assertLess(qodec.count(ex.text), qodec.count(_REPETITIVE_LOG))

    @unittest.skipUnless(_RTK_BIN, "rtk not resolvable")
    def test_pipe_grep_reduces_tokens(self):
        ex = transforms.apply_rtk(_GREP_OUTPUT, Transform("rtk", {"filter": "grep"}),
                                  lockfiles.tools())
        self.assertEqual(ex.exit_code, 0)
        self.assertLess(qodec.count(ex.text), qodec.count(_GREP_OUTPUT))

    def test_native_command_name_refused_as_pipe_transform(self):
        # `rg` is a command-runner (rtk rg), never a pipe filter.
        with self.assertRaises(transforms.UnsupportedTransform):
            transforms.apply_rtk("x\n", Transform("rtk", {"filter": "rg"}), lockfiles.tools())


if __name__ == "__main__":
    unittest.main()
