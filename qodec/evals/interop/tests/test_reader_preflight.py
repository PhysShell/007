"""Capability negotiation: preflight discovers the honored contract, and the
matrix uses only that — a rejected parameter is never re-sent.

Integration against a configurable mock endpoint (skips if qodec is absent, since
preflight encodes a probe). The mock rejects `seed` (HTTP 400) and never streams
usage but returns it non-stream, and accepts response_format=json_object.
"""

import json
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from bench import preflight, reader  # noqa: E402


def _qodec_ok():
    from bench import qodec
    try:
        qodec.binary(); return True
    except Exception:
        return False


ANSWER = '{"facts":[],"files":[],"symbols":[],"call_path":[],"answer":"ok"}'


class _Handler(BaseHTTPRequestHandler):
    reject_seed = True
    stream_usage = False  # never emit usage in a stream

    def log_message(self, *a):
        pass

    def do_GET(self):
        if self.path.endswith("/models"):
            self._json(200, {"object": "list", "data": [{"id": "mock-model"}, {"id": "other"}]})
        else:
            self._json(404, {})

    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(n))
        if self.reject_seed and "seed" in body:
            self._json(400, {"error": {"message": "seed is not supported"}})
            return
        usage = {"prompt_tokens": 42, "completion_tokens": 7, "total_tokens": 49}
        if body.get("stream"):
            self.send_response(200)
            self.send_header("Content-Type", "text/event-stream")
            self.end_headers()
            self._sse({"choices": [{"delta": {"content": ANSWER}, "finish_reason": "stop"}]})
            if self.stream_usage:
                self._sse({"choices": [], "usage": usage})
            self.wfile.write(b"data: [DONE]\n\n")
        else:
            self._json(200, {"choices": [{"message": {"content": ANSWER}, "finish_reason": "stop"}],
                             "usage": usage})

    def _json(self, code, obj):
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(obj).encode())

    def _sse(self, obj):
        self.wfile.write(f"data: {json.dumps(obj)}\n\n".encode())


@unittest.skipUnless(_qodec_ok(), "qodec binary not built")
class Negotiation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = HTTPServer(("127.0.0.1", 0), _Handler)
        cls.port = cls.server.server_address[1]
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()

    def _cfg(self):
        return reader.ReaderConfig(url=f"http://127.0.0.1:{self.port}/v1", model="mock-model", seed=0)

    def test_effective_drops_seed_and_falls_back_to_nonstream(self):
        pf = preflight.run(self._cfg(), "o200k")
        eff = preflight.effective_from(pf)
        self.assertFalse(eff.send_seed, "seed rejected → must be dropped")
        # No streaming usage → matrix must not stream (usage comes from non-stream).
        self.assertFalse(eff.stream)
        self.assertTrue(pf["ready"])

    def test_model_reported_matches_requested_not_first(self):
        pf = preflight.run(self._cfg(), "o200k")
        # /models lists [mock-model, other]; requested mock-model must be reported.
        self.assertEqual(pf["models"]["model_reported"], "mock-model")

    def test_matrix_request_omits_rejected_seed(self):
        eff = preflight.effective_from(preflight.run(self._cfg(), "o200k"))
        res = reader.chat(self._cfg(), [{"role": "user", "content": "hi"}], eff)
        self.assertIsNone(res.http_error, "must not resend the rejected seed")
        self.assertNotIn("seed", res.request)

    def test_structured_json_detected(self):
        pf = preflight.run(self._cfg(), "o200k")
        self.assertIn(pf["structured_json"]["mode"], ("json_schema", "json_object", "text"))


if __name__ == "__main__":
    unittest.main()
