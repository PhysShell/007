"""run_reader._run_manifest carries the full decision-relevant run identity.

Guards that the manifest actually POPULATES every identity field the resume
comparison relies on — execution identity (endpoint, budget, runtime), the
per-case encoded artifact hash, and a grammar pinned by content, not a bool.
"""

import sys
import types
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import run_reader  # noqa: E402
from bench import reader  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]

REQUIRED = [
    "manifest_version", "model_requested", "model_reported", "model_source",
    "model_gguf_sha256", "model_file_size_bytes", "quantization", "tokenizer_sha256",
    "tokenizer_config_sha256", "chat_template_sha256", "qodec_binary_sha256", "codec",
    "tasks_snapshot_sha256", "l1_run", "l1_tool_only_sha256", "encoded_artifact_sha256",
    "notation_brief_sha256", "system_prompt_sha256", "effective_contract",
    "reader_url", "max_tokens", "repeats", "runtime", "determinism", "arms",
]


def _pf(grammar=None):
    return {
        "tokenizer": {"path": "/no/such/tokenizer.json", "sha256": "b" * 64,
                      "tokenizer_config_sha256": "c" * 64},
        "model_identity": {"model_source": "repo@rev", "model_file_sha256": "a" * 64,
                           "model_file_size_bytes": 4_100_000_000, "quantization": "q4_k_m",
                           "llama_cpp_python_version": "0.3.34", "n_ctx": 8192,
                           "threads": 8, "batch": 512},
        "models": {"model_reported": "m"},
        "effective": {"stream": False, "send_seed": True, "include_usage": False,
                      "response_format": {"type": "json_object"}, "grammar": bool(grammar)},
        "_effective_obj": {"grammar": grammar},
        "determinism": {"seed_sent": 0},
    }


class RunManifest(unittest.TestCase):
    def _manifest(self, grammar=None):
        cfg = reader.ReaderConfig(url="http://x/v1", model="m", tokenizer="hf:/t.json",
                                  max_tokens=128, seed=0, model_source="repo@rev")
        args = types.SimpleNamespace(codec="squeeze", tasks=ROOT / "tasks" / "reader" / "tasks.json",
                                     l1_run=Path("results/x"), repeats=3)
        ctx = {"caseA": {"tool_only": "hello", "artifact": "%q1 ...\nbody"},
               "caseB": {"tool_only": "world", "artifact": "%q1 ...\nother"}}
        return run_reader._run_manifest(cfg, _pf(grammar), args, tasks=[], brief="BRIEF", ctx=ctx)

    def test_all_identity_fields_present(self):
        m = self._manifest()
        for key in REQUIRED:
            self.assertIn(key, m, f"manifest missing {key}")

    def test_execution_identity_and_artifacts(self):
        m = self._manifest()
        self.assertEqual((m["reader_url"], m["max_tokens"], m["repeats"]), ("http://x/v1", 128, 3))
        self.assertEqual(m["runtime"]["n_ctx"], 8192)
        self.assertEqual(m["model_file_size_bytes"], 4_100_000_000)
        self.assertEqual(set(m["encoded_artifact_sha256"]), {"caseA", "caseB"})
        self.assertNotEqual(m["encoded_artifact_sha256"]["caseA"],
                            m["encoded_artifact_sha256"]["caseB"])

    def test_grammar_pinned_by_content_not_bool(self):
        c = self._manifest()["effective_contract"]
        self.assertIn("grammar_sha256", c)
        self.assertNotIn("grammar", c)   # the bare bool must not stand in for the grammar
        self.assertIsNone(c["grammar_sha256"])
        # Two different grammars are two different contracts.
        a = self._manifest(grammar="root ::= a")["effective_contract"]["grammar_sha256"]
        b = self._manifest(grammar="root ::= b")["effective_contract"]["grammar_sha256"]
        self.assertTrue(a and b and a != b)


if __name__ == "__main__":
    unittest.main()
