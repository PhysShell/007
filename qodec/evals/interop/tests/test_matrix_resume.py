"""Integration test for the real matrix orchestration + resume path.

Drives `bench.matrix.run_matrix` with a fake deterministic reader (no endpoint,
no qodec) and a crash injected mid pass-2. Exercises every guarantee end to end:
pass 1 survives the crash, resume re-runs no completed key and finishes without
duplicates, a further resume is idempotent, a changed identity is refused before
any request, and a fresh run into a populated dir is refused.
"""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import durability, matrix  # noqa: E402

TASKS = [{"case": "c", "id": f"q{i}", "category": "locator", "q": "where?"} for i in range(4)]
MAN = {
    "manifest_version": 2,
    "model_requested": "m", "model_reported": "m",
    "model_source": "repo@rev", "model_gguf_sha256": "a" * 64,
    "model_file_size_bytes": 123, "quantization": "q4_k_m",
    "tokenizer_sha256": "b" * 64, "codec": "squeeze", "tasks_snapshot_sha256": "c" * 64,
    "encoded_artifact_sha256": {"c": "e" * 64},
    "effective_contract": {"stream": False, "send_seed": True,
                           "response_format": {"type": "json_object"}, "grammar_sha256": None},
    "reader_url": "http://127.0.0.1:8000/v1",
    "max_tokens": 128, "repeats": 3,
    "runtime": {"llama_cpp_python_version": "0.3.34", "n_ctx": 8192, "threads": 8, "batch": 512},
    "determinism": {"temperature": 0, "seed": 0},
    "arms": list(matrix.ARMS),
}


def make_reader(calls):
    """Deterministic: q0's encoded+brief is a codec loss (so q0 is flagged for
    pass 2); everything else is clean. Records every call it receives."""
    def run_one(case, q, arm, repeat):
        calls.append((case, q["id"], arm, repeat))
        correct = not (q["id"] == "q0" and arm == "encoded+brief")
        return {"case": case, "question": q["id"], "category": q["category"],
                "arm": arm, "repeat": repeat, "correct": correct,
                "format_compliant": True, "malformed": False,
                "invalid_identifiers": [], "alias_leaks": []}
    return run_one


def crash_after(n_pass2):
    def hook(phase, n):
        if phase == "pass2" and n == n_pass2:
            raise RuntimeError("simulated crash")
    return hook


class MatrixResume(unittest.TestCase):
    def setUp(self):
        self.d = Path(tempfile.mkdtemp()) / "run"

    def _keys_on_disk(self):
        out = []
        for line in (self.d / "records.jsonl").read_text().splitlines():
            if line.strip():
                r = json.loads(line)
                out.append((r["case"], r["question"], r["arm"], r["repeat"]))
        return out

    def test_crash_in_pass2_then_resume_completes_without_duplicates(self):
        calls1 = []
        with self.assertRaises(RuntimeError):
            matrix.run_matrix(self.d, manifest=MAN, tasks=TASKS,
                              run_one=make_reader(calls1), resume=False, repeats=3,
                              crash_hook=crash_after(2))
        # Pass 1 finished and its receipt was written before pass 2 started.
        receipt = json.loads((self.d / "pass1-complete.json").read_text())
        self.assertEqual((receipt["expected"], receipt["actual"], receipt["duplicates"]), (12, 12, 0))
        self.assertEqual((receipt["missing"], receipt["n_primary"]), ([], 12))
        self.assertEqual(len(receipt["records_prefix_sha256"]), 64)
        self.assertEqual(sum(1 for k in calls1 if k[3] == 0), 12)   # 12 primary ran
        self.assertEqual(sum(1 for k in calls1 if k[3] > 0), 2)     # 2 pass-2 ran, then crash

        # Resume: no completed key re-invoked, matrix completes, no duplicates.
        calls2 = []
        matrix.run_matrix(self.d, manifest=MAN, tasks=TASKS,
                          run_one=make_reader(calls2), resume=True, repeats=3)
        self.assertTrue(set(calls2).isdisjoint(set(calls1)), "resume re-ran a completed key")
        keys = self._keys_on_disk()
        self.assertEqual(len(keys), len(set(keys)), "duplicate records after resume")
        self.assertEqual(len(keys), 18, "matrix did not complete (12 primary + 6 pass-2)")
        st = json.loads((self.d / "run-state.json").read_text())
        self.assertEqual((st["phase"], st["completed"], st["expected"]), ("complete", 18, 18))

        # A further resume is idempotent.
        calls3 = []
        before = (self.d / "records.jsonl").read_text()
        matrix.run_matrix(self.d, manifest=MAN, tasks=TASKS,
                          run_one=make_reader(calls3), resume=True, repeats=3)
        self.assertEqual(calls3, [], "a completed matrix must re-run nothing")
        self.assertEqual((self.d / "records.jsonl").read_text(), before)

    def test_resume_refuses_changed_identity_before_any_request(self):
        matrix.run_matrix(self.d, manifest=MAN, tasks=TASKS,
                          run_one=make_reader([]), resume=False, repeats=1)
        base_contract = MAN["effective_contract"]
        stream_changed = {**base_contract, "stream": True}
        grammar_changed = {**base_contract, "grammar_sha256": "aa" * 32}  # different grammar, same shape
        n_ctx_changed = {**MAN["runtime"], "n_ctx": 4096}
        for label, mutate in (("model", {"model_gguf_sha256": "d" * 64}),
                              ("tokenizer", {"tokenizer_sha256": "d" * 64}),
                              ("tasks", {"tasks_snapshot_sha256": "d" * 64}),
                              ("codec", {"codec": "mosaic"}),
                              ("max_tokens", {"max_tokens": 256}),
                              ("repeats", {"repeats": 5}),
                              ("reader_url", {"reader_url": "http://127.0.0.1:9000/v1"}),
                              ("encoded artifact", {"encoded_artifact_sha256": {"c": "f" * 64}}),
                              ("runtime n_ctx", {"runtime": n_ctx_changed}),
                              ("effective stream", {"effective_contract": stream_changed}),
                              ("grammar content", {"effective_contract": grammar_changed})):
            calls = []
            with self.assertRaises(matrix.ManifestMismatch):
                matrix.run_matrix(self.d, manifest={**MAN, **mutate}, tasks=TASKS,
                                  run_one=make_reader(calls), resume=True, repeats=1)
            self.assertEqual(calls, [], f"{label} change must be refused before any request")

    def test_fresh_run_into_populated_dir_is_refused(self):
        matrix.run_matrix(self.d, manifest=MAN, tasks=TASKS,
                          run_one=make_reader([]), resume=False, repeats=1)
        calls = []
        with self.assertRaises(matrix.DirectoryPolicyError):
            matrix.run_matrix(self.d, manifest=MAN, tasks=TASKS,
                              run_one=make_reader(calls), resume=False, repeats=1)
        self.assertEqual(calls, [])

    def test_resume_without_manifest_is_refused(self):
        self.d.mkdir(parents=True)   # exists but empty — nothing to resume
        with self.assertRaises(matrix.DirectoryPolicyError):
            matrix.run_matrix(self.d, manifest=MAN, tasks=TASKS,
                              run_one=make_reader([]), resume=True, repeats=1)


class Pass1DurablePrefix(unittest.TestCase):
    """The pass-1 receipt pins a power-loss-durable prefix; a resume must prove
    that prefix is intact, not regenerate a truncated/corrupted journal."""

    def setUp(self):
        self.d = Path(tempfile.mkdtemp()) / "run"

    def _fresh(self):
        # 4 unique x 3 arms = 12 primary records — deliberately NOT a multiple of
        # RecordLog.sync_every (5), so sync() is what makes the receipt durable.
        matrix.run_matrix(self.d, manifest=MAN, tasks=TASKS,
                          run_one=make_reader([]), resume=False, repeats=1)

    def test_receipt_pins_full_durable_prefix(self):
        self._fresh()
        receipt = json.loads((self.d / "pass1-complete.json").read_text())
        self.assertEqual(receipt["records_prefix_bytes"], (self.d / "records.jsonl").stat().st_size)
        self.assertEqual(receipt["n_primary"], 12)

    def test_resume_refuses_truncated_prefix(self):
        self._fresh()
        data = (self.d / "records.jsonl").read_bytes()
        (self.d / "records.jsonl").write_bytes(data[: len(data) // 2])   # lose pass-1 bytes
        calls = []
        with self.assertRaises(durability.JournalCorruption):
            matrix.run_matrix(self.d, manifest=MAN, tasks=TASKS,
                              run_one=make_reader(calls), resume=True, repeats=1)
        self.assertEqual(calls, [], "a shortened pass-1 journal must abort, not be regenerated")

    def test_resume_refuses_corrupted_prefix(self):
        self._fresh()
        data = (self.d / "records.jsonl").read_bytes()
        corrupt = data.replace(b'"question": "q1"', b'"question": "qZ"', 1)   # same length, valid JSON
        self.assertNotEqual(corrupt, data)
        (self.d / "records.jsonl").write_bytes(corrupt)
        calls = []
        with self.assertRaises(durability.JournalCorruption):
            matrix.run_matrix(self.d, manifest=MAN, tasks=TASKS,
                              run_one=make_reader(calls), resume=True, repeats=1)
        self.assertEqual(calls, [])


class DiffManifest(unittest.TestCase):
    def test_reports_nested_mismatch_path(self):
        a = {"x": 1, "n": {"p": {"q": 1}}}
        b = {"x": 1, "n": {"p": {"q": 2}}}
        self.assertEqual(matrix.diff_manifest(a, b), ["n.p.q: 1 != 2"])

    def test_identical_is_empty(self):
        self.assertEqual(matrix.diff_manifest(MAN, dict(MAN)), [])

    def test_added_and_removed_keys_reported(self):
        self.assertEqual(len(matrix.diff_manifest({"a": 1}, {"b": 2})), 2)


if __name__ == "__main__":
    unittest.main()
