"""OpenAI-compatible reader client for the Level-2 comprehension benchmark.

Configured by environment so the served model and its runtime are pinned
variables, not guesses:

    QODEC_READER_URL          e.g. http://127.0.0.1:8000/v1
    QODEC_READER_MODEL        the served model id
    QODEC_READER_TOKENIZER    hf:<tokenizer.json> for the local token meter
    QODEC_READER_MODEL_FILE   path to the GGUF (recorded by SHA-256 + size)
    QODEC_READER_MODEL_SOURCE model repo/revision/quantization string
    QODEC_READER_SERVER_ARGV  the exact server launch argv (recorded verbatim)

Requests carry an `Effective` capability contract negotiated by preflight — the
matrix never re-sends a parameter the endpoint rejected. Stdlib only (urllib).
"""

from __future__ import annotations

import json
import os
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field


class ReaderUnavailable(RuntimeError):
    """No endpoint configured/reachable — a real reader run cannot proceed."""


@dataclass
class ReaderConfig:
    url: str
    model: str
    tokenizer: str | None = None
    max_tokens: int = 512
    seed: int | None = 0
    model_file: str | None = None
    model_source: str | None = None
    server_argv: str | None = None

    @classmethod
    def from_env(cls) -> "ReaderConfig":
        url = os.environ.get("QODEC_READER_URL")
        model = os.environ.get("QODEC_READER_MODEL")
        if not url or not model:
            raise ReaderUnavailable("set QODEC_READER_URL and QODEC_READER_MODEL to run the reader")
        return cls(
            url=url.rstrip("/"),
            model=model,
            tokenizer=os.environ.get("QODEC_READER_TOKENIZER"),
            max_tokens=int(os.environ.get("QODEC_READER_MAX_TOKENS", "512")),
            model_file=os.environ.get("QODEC_READER_MODEL_FILE"),
            model_source=os.environ.get("QODEC_READER_MODEL_SOURCE"),
            server_argv=os.environ.get("QODEC_READER_SERVER_ARGV"),
        )


@dataclass
class Effective:
    """The request contract preflight found the endpoint actually honors."""

    stream: bool = True
    send_seed: bool = True
    include_usage: bool = True
    response_format: dict | None = None  # json_schema / json_object / None
    grammar: str | None = None           # llama.cpp GBNF, when used instead

    def to_dict(self) -> dict:
        return {"stream": self.stream, "send_seed": self.send_seed,
                "include_usage": self.include_usage,
                "response_format": self.response_format,
                "grammar": bool(self.grammar)}


@dataclass
class ReaderResult:
    text: str
    request: dict
    response_meta: dict
    ttft_ms: float | None
    total_ms: float
    usage: dict = field(default_factory=dict)
    http_error: str | None = None


def _body(cfg: ReaderConfig, messages: list[dict], eff: Effective) -> dict:
    body: dict = {
        "model": cfg.model, "messages": messages,
        "temperature": 0, "max_tokens": cfg.max_tokens, "stream": eff.stream,
    }
    if eff.send_seed and cfg.seed is not None:
        body["seed"] = cfg.seed
    if eff.stream and eff.include_usage:
        body["stream_options"] = {"include_usage": True}
    if eff.response_format is not None:
        body["response_format"] = eff.response_format
    if eff.grammar is not None:
        body["grammar"] = eff.grammar
    return body


def chat(cfg: ReaderConfig, messages: list[dict], eff: Effective | None = None) -> ReaderResult:
    """One deterministic chat completion under the effective contract."""
    eff = eff or Effective()
    body = _body(cfg, messages, eff)
    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        f"{cfg.url}/chat/completions", data=data,
        headers={"Content-Type": "application/json",
                 "Authorization": f"Bearer {os.environ.get('QODEC_READER_KEY', 'none')}"},
        method="POST",
    )
    started = time.perf_counter()
    try:
        resp = urllib.request.urlopen(req, timeout=1200)
    except urllib.error.HTTPError as exc:
        # 4xx/5xx — capture the body so preflight can see WHY a param was rejected.
        detail = ""
        try:
            detail = exc.read().decode("utf-8", "replace")[:400]
        except Exception:  # noqa: BLE001
            pass
        return ReaderResult(text="", request=body, response_meta={},
                            ttft_ms=None, total_ms=(time.perf_counter() - started) * 1000,
                            http_error=f"HTTP {exc.code}: {detail}")
    except urllib.error.URLError as exc:
        raise ReaderUnavailable(f"reader endpoint {cfg.url} unreachable: {exc}") from exc

    if not eff.stream:
        obj = json.loads(resp.read())
        total = (time.perf_counter() - started) * 1000
        text = obj["choices"][0]["message"]["content"]
        return ReaderResult(text=text, request=body, response_meta=obj,
                            ttft_ms=None, total_ms=total, usage=obj.get("usage", {}) or {})

    text_parts: list[str] = []
    ttft_ms: float | None = None
    usage: dict = {}
    finish_reason = None
    for raw in resp:
        line = raw.decode("utf-8").strip()
        if not line.startswith("data:"):
            continue
        payload = line[len("data:"):].strip()
        if payload == "[DONE]":
            break
        try:
            chunk = json.loads(payload)
        except json.JSONDecodeError:
            continue
        if chunk.get("usage"):
            usage = chunk["usage"]
        for ch in chunk.get("choices", []):
            delta = ch.get("delta", {}).get("content")
            if delta:
                if ttft_ms is None:
                    ttft_ms = (time.perf_counter() - started) * 1000
                text_parts.append(delta)
            if ch.get("finish_reason"):
                finish_reason = ch["finish_reason"]
    total = (time.perf_counter() - started) * 1000
    return ReaderResult(text="".join(text_parts), request=body,
                        response_meta={"finish_reason": finish_reason},
                        ttft_ms=ttft_ms, total_ms=total, usage=usage)
