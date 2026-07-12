"""Manifest parsing: the pipeline model and its guardrails.

Pure — no tools needed. Verifies that a case can never quietly ignore its input
(qodec must be terminal; an rtk *transform* must be a real stdin filter, not a
command-runner) and that arm naming follows the tool feeding qodec.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import manifest  # noqa: E402

PIPE_FILTERS = {"log", "grep", "git-diff", "cargo-test"}


def parse(obj):
    return manifest.parse_case(obj, pipe_filters=PIPE_FILTERS)


class ArmNaming(unittest.TestCase):
    def test_fixture_qodec_is_raw(self):
        c = parse({"id": "x", "producer": {"type": "fixture", "path": "p"}, "transforms": ["qodec"]})
        self.assertEqual(c.arm, "raw")

    def test_rtk_transform_is_rtk(self):
        c = parse({"id": "x", "producer": {"type": "fixture", "path": "p"},
                   "transforms": [{"type": "rtk", "filter": "log"}, "qodec"]})
        self.assertEqual(c.arm, "rtk")

    def test_rtk_command_producer_is_rtk(self):
        c = parse({"id": "x", "producer": {"type": "rtk-command", "repo": "clap", "argv": ["rg", "x"]},
                   "transforms": ["qodec"]})
        self.assertEqual(c.arm, "rtk")

    def test_codegraph_producer_is_codegraph(self):
        c = parse({"id": "x", "producer": {"type": "codegraph", "repo": "clap", "query": "q"},
                   "transforms": ["qodec"]})
        self.assertEqual(c.arm, "codegraph")


class Guardrails(unittest.TestCase):
    def test_transforms_must_end_with_qodec(self):
        with self.assertRaises(ValueError):
            parse({"id": "x", "producer": {"type": "fixture", "path": "p"},
                   "transforms": [{"type": "rtk", "filter": "log"}]})

    def test_qodec_only_terminal(self):
        with self.assertRaises(ValueError):
            parse({"id": "x", "producer": {"type": "fixture", "path": "p"},
                   "transforms": ["qodec", "qodec"]})

    def test_rtk_transform_must_be_pinned_pipe_filter(self):
        # `grep` is a valid pipe filter; a native command-runner like `rg` is
        # not — used as a pipe transform it must be rejected.
        parse({"id": "ok", "producer": {"type": "fixture", "path": "p"},
               "transforms": [{"type": "rtk", "filter": "grep"}, "qodec"]})
        with self.assertRaises(ValueError):
            parse({"id": "bad", "producer": {"type": "fixture", "path": "p"},
                   "transforms": [{"type": "rtk", "filter": "rg"}, "qodec"]})

    def test_rtk_transform_needs_filter(self):
        with self.assertRaises(ValueError):
            parse({"id": "x", "producer": {"type": "fixture", "path": "p"},
                   "transforms": [{"type": "rtk"}, "qodec"]})

    def test_producer_required_fields(self):
        for bad in [
            {"type": "fixture"},
            {"type": "rtk-command", "repo": "clap"},
            {"type": "rtk-command", "argv": ["rg"]},
            {"type": "codegraph", "repo": "clap"},
        ]:
            with self.assertRaises(ValueError):
                parse({"id": "x", "producer": bad, "transforms": ["qodec"]})

    def test_unknown_producer(self):
        with self.assertRaises(ValueError):
            parse({"id": "x", "producer": {"type": "wat"}, "transforms": ["qodec"]})


class Unsupported(unittest.TestCase):
    def test_headroom_parses_but_flags_unsupported(self):
        c = parse({"id": "x", "producer": {"type": "fixture", "path": "p"},
                   "transforms": [{"type": "headroom"}, "qodec"]})
        self.assertIsNotNone(c.unsupported)
        self.assertEqual(c.unsupported.type, "headroom")


class RealManifests(unittest.TestCase):
    def test_shipped_manifests_parse(self):
        base = Path(__file__).resolve().parents[1] / "manifests"
        for name in ("corpus.json", "rtk.json", "codegraph.json"):
            cases = manifest.load(base / name, pipe_filters=PIPE_FILTERS)
            self.assertTrue(cases, f"{name} produced no cases")


if __name__ == "__main__":
    unittest.main()
