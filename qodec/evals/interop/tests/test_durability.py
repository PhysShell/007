"""Crash-durability + resume: pass 1 survives a pass-2 crash, resume never
duplicates a request, and a second --resume is idempotent.

Pure — exercises bench.durability directly (that is where the crash-safety
lives), simulating run_reader's completed-key skip loop.
"""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import durability  # noqa: E402

PASS1 = [("c", f"q{i}", arm, 0) for i in range(4) for arm in ("raw", "raw+brief", "encoded+brief")]
PASS2 = [("c", "q0", arm, r) for r in (1, 2) for arm in ("raw", "raw+brief", "encoded+brief")]
ALL = PASS1 + PASS2


def _rec(k):
    return {"case": k[0], "question": k[1], "arm": k[2], "repeat": k[3],
            "correct": True, "malformed": False, "alias_leaks": [], "invalid_identifiers": []}


def simulate(log, keys):
    """Mimic run_reader's `do`: skip completed keys, append the rest."""
    executed = []
    for k in keys:
        if log.has(k):
            continue
        log.append(_rec(k))
        executed.append(k)
    return executed


class Durability(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.path = self.dir / "records.jsonl"

    def _keys_on_disk(self):
        out = []
        for line in self.path.read_text().splitlines():
            if line.strip():
                r = json.loads(line)
                out.append((r["case"], r["question"], r["arm"], r["repeat"]))
        return out

    def test_atomic_write_leaves_no_tmp(self):
        p = self.dir / "meta.json"
        durability.atomic_write(p, '{"x":1}\n')
        self.assertEqual(json.loads(p.read_text())["x"], 1)
        self.assertFalse((self.dir / "meta.json.tmp").exists())

    def test_torn_last_line_is_dropped_and_truncated(self):
        # 3 whole lines + a torn partial (no trailing newline) from a crash.
        self.path.write_text('{"case":"c","question":"q0","arm":"raw","repeat":0}\n'
                             '{"case":"c","question":"q0","arm":"raw+brief","repeat":0}\n'
                             '{"case":"c","question":"q0","arm":"encoded+brief","repeat":0}\n'
                             '{"case":"c","question":"q1","arm":"raw","rep')  # torn
        log = durability.RecordLog(self.path)
        log.load_existing()
        self.assertEqual(len(log.completed), 3)
        # File truncated so subsequent appends stay well-formed (all lines parse).
        for line in self.path.read_text().splitlines():
            json.loads(line)

    def test_pass1_survives_pass2_crash_and_resume_has_no_duplicates(self):
        # Run pass 1 + 2 pass-2 keys, then simulate a crash mid-write.
        log = durability.RecordLog(self.path)
        log.open()
        simulate(log, PASS1 + PASS2[:2])
        log.close()
        with self.path.open("a") as fh:
            fh.write('{"case":"c","question":"q0","arm":"raw+bri')  # torn write

        # Resume: pass 1 must be intact, torn line dropped, no key runs twice.
        log2 = durability.RecordLog(self.path)
        log2.load_existing()
        for k in PASS1:
            self.assertIn(k, log2.completed, f"pass-1 key lost: {k}")
        log2.open()
        executed = simulate(log2, ALL)
        log2.close()
        # Only the not-yet-done keys ran again.
        self.assertNotIn(PASS1[0], executed)
        keys = self._keys_on_disk()
        self.assertEqual(len(keys), len(set(keys)), "duplicate records on resume")
        self.assertEqual(set(keys), set(ALL), "resume did not complete the matrix")

    def test_second_resume_is_idempotent(self):
        log = durability.RecordLog(self.path)
        log.open()
        simulate(log, ALL)
        log.close()
        before = self.path.read_text()

        log2 = durability.RecordLog(self.path)
        log2.load_existing()
        log2.open()
        executed = simulate(log2, ALL)
        log2.close()
        self.assertEqual(executed, [], "a completed run must re-run nothing")
        self.assertEqual(self.path.read_text(), before, "idempotent resume must not change the journal")


if __name__ == "__main__":
    unittest.main()
