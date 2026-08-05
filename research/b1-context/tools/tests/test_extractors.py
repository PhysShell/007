"""Extractor tests — run entirely on synthetic fixtures; no private RAW needed.

They pin the inclusion/exclusion policy (chain-of-thought never leaves, tool
plumbing dropped, hidden/off-path messages dropped) and byte-level determinism.
"""
import json
import os
import unittest

from o7b1.canonical import jsonl_bytes
from o7b1.extractors import ChatGPTBackendApiV0, ClaudeCodeV0

HERE = os.path.dirname(__file__)
FIX = os.path.join(HERE, "fixtures")


class ChatGPTExtractorTest(unittest.TestCase):
    def setUp(self):
        with open(os.path.join(FIX, "chatgpt_min.json"), encoding="utf-8") as fh:
            self.raw = json.load(fh)

    def extract(self):
        return ChatGPTBackendApiV0.extract(
            self.raw, source_id="syn-a", raw_source_digest="sha256:deadbeef"
        )

    def test_visible_thread_only(self):
        recs, pol = self.extract()
        # visible thread: user m2 + assistant m5 (multimodal, text part only)
        self.assertEqual([r["native_message_or_event_id"] for r in recs], ["m2", "m5"])
        self.assertEqual([r["role"] for r in recs], ["user", "assistant"])
        self.assertEqual(recs[1]["visible_text"], "here is the answer")

    def test_excludes_cot_tool_hidden_offpath(self):
        recs, _ = self.extract()
        texts = " ".join(r["visible_text"] for r in recs)
        self.assertNotIn("secret chain of thought", texts)       # thoughts
        self.assertNotIn("tool_call", texts)                     # tool-directed code
        self.assertNotIn("hidden from conversation", texts)      # hidden flag
        self.assertNotIn("OFF-PATH", texts)                      # edit branch off current_node
        self.assertNotIn("helpful assistant", texts)             # system

    def test_multimodal_non_text_counted_omitted(self):
        _, pol = self.extract()
        self.assertEqual(pol["stats"]["non_text_parts_omitted"], 1)

    def test_sequence_and_determinism(self):
        recs, _ = self.extract()
        self.assertEqual([r["sequence"] for r in recs], [0, 1])
        b1 = jsonl_bytes(recs)
        b2 = jsonl_bytes(self.extract()[0])
        self.assertEqual(b1, b2)

    def test_no_current_node_ambiguous_fails_closed(self):
        raw = json.loads(json.dumps(self.raw))
        raw["current_node"] = None  # two leaves (n5, n6, nH) -> ambiguous
        from o7b1.extractors.chatgpt_backend_api_v0 import ExtractorError
        with self.assertRaises(ExtractorError):
            ChatGPTBackendApiV0.extract(raw, source_id="x", raw_source_digest="sha256:x")


class ClaudeCodeExtractorTest(unittest.TestCase):
    def setUp(self):
        with open(os.path.join(FIX, "claude_min.jsonl"), "rb") as fh:
            self.raw = fh.read()

    def extract(self):
        return ClaudeCodeV0.extract(
            self.raw, source_id="syn-c", raw_source_digest="sha256:cafe"
        )

    def test_dialogue_text_only(self):
        recs, pol = self.extract()
        self.assertEqual(
            [r["native_message_or_event_id"] for r in recs], ["u1", "a1", "a3"]
        )
        self.assertEqual([r["visible_text"] for r in recs],
                         ["please review the change", "Looks good, one nit.", "Done."])
        self.assertEqual(pol["stats"]["sidechain_skipped"], 1)

    def test_excludes_cot_tool_sidechain(self):
        recs, _ = self.extract()
        texts = " ".join(r["visible_text"] for r in recs)
        self.assertNotIn("internal reasoning", texts)     # thinking block
        self.assertNotIn("file bytes here", texts)        # tool_result-only user turn
        self.assertNotIn("subagent internal", texts)      # isSidechain
        # the thinking signature must never appear anywhere in output
        self.assertNotIn("signature", jsonl_bytes(recs).decode("utf-8"))

    def test_parent_and_determinism(self):
        recs, _ = self.extract()
        self.assertEqual(recs[1]["parent_id"], "u1")
        self.assertEqual(jsonl_bytes(recs), jsonl_bytes(self.extract()[0]))


if __name__ == "__main__":
    unittest.main()
