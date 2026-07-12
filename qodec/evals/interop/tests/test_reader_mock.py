"""The reader client against a mock OpenAI-compatible SSE endpoint.

Proves the full client path — streaming, TTFT, usage, deterministic params —
without a real model, so the harness is verified end-to-end here and only the
model itself is missing at run time.
"""

import json
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import reader  # noqa: E402

_ANSWER = '{"facts": [], "files": ["SessionService.cs"], "symbols": [], "call_path": [], "answer": "20"}'


class _Handler(BaseHTTPRequestHandler):
    def log_message(self, *a):  # silence
        pass

    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length))
        # Echo back the request's determinism so the test can assert it.
        assert body["temperature"] == 0
        assert body["stream"] is True
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()

        def sse(obj):
            self.wfile.write(f"data: {json.dumps(obj)}\n\n".encode("utf-8"))

        # Two content deltas, then usage, then DONE.
        sse({"choices": [{"delta": {"content": _ANSWER[:20]}, "finish_reason": None}]})
        sse({"choices": [{"delta": {"content": _ANSWER[20:]}, "finish_reason": "stop"}]})
        sse({"choices": [], "usage": {"prompt_tokens": 123, "completion_tokens": 31}})
        self.wfile.write(b"data: [DONE]\n\n")


class ReaderClient(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = HTTPServer(("127.0.0.1", 0), _Handler)
        cls.port = cls.server.server_address[1]
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()

    def test_streaming_chat_records_everything(self):
        cfg = reader.ReaderConfig(url=f"http://127.0.0.1:{self.port}/v1", model="mock", seed=0)
        res = reader.chat(cfg, [{"role": "user", "content": "How many matches?"}])
        self.assertEqual(res.text, _ANSWER)
        self.assertIsNotNone(res.ttft_ms)
        self.assertEqual(res.usage["completion_tokens"], 31)
        self.assertEqual(res.request["temperature"], 0)
        self.assertEqual(res.request["seed"], 0)
        # And it parses+scores as the harness would.
        from bench import reader_tasks as rt
        ans = rt.parse_answer(res.text)
        self.assertEqual(ans["files"], ["SessionService.cs"])


if __name__ == "__main__":
    unittest.main()
