"""Producers — the generic `command` producer runs argv in a pinned repo.

Regression for the `p["repo"]` bug (a Producer is not subscriptable): a command
producer with a repo must resolve the repo dir and run there. Skips when the
clap clone is absent.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import lockfiles, producers  # noqa: E402
from bench.manifest import parse_case  # noqa: E402

_REPOS = lockfiles.repos()
_CLAP = _REPOS.get("clap")
_CLAP_CLONED = _CLAP is not None and (_CLAP.clone_dir() / ".git").exists()


@unittest.skipUnless(_CLAP_CLONED, "clap clone absent (run manage.py sync)")
class CommandProducer(unittest.TestCase):
    def _case(self, argv):
        return parse_case(
            {"id": "cmd", "producer": {"type": "command", "repo": "clap", "argv": argv},
             "transforms": ["qodec"]},
            pipe_filters=set(),
        )

    def test_command_runs_in_pinned_repo(self):
        case = self._case(["git", "rev-parse", "HEAD"])
        produced, baseline = producers.produce(case, lockfiles.tools(), _REPOS)
        self.assertIsNone(baseline)
        self.assertEqual(produced.text.strip(), _CLAP.rev, "command must run in the clap clone")
        self.assertEqual(produced.exit_code, 0)
        self.assertEqual(produced.repo_sha, _CLAP.rev)

    def test_command_captures_provenance(self):
        case = self._case(["git", "rev-parse", "--abbrev-ref", "HEAD"])
        produced, _ = producers.produce(case, lockfiles.tools(), _REPOS)
        prov = produced.provenance()
        self.assertEqual(prov["argv"], ["git", "rev-parse", "--abbrev-ref", "HEAD"])
        self.assertTrue(prov["cwd"].endswith("clap"))


if __name__ == "__main__":
    unittest.main()
