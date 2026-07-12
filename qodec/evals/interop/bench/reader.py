"""OpenAI-compatible reader client for the Level-2 comprehension benchmark.

Configured entirely by environment so the served model is a pinned variable, not
a hardcoded guess:

    QODEC_READER_URL        e.g. http://127.0.0.1:8000/v1  (vLLM / Colibrì / llama.cpp)
    QODEC_READER_MODEL      the served model id
    QODEC_READER_TOKENIZER  hf:<tokenizer.json> for the local token meter

Every call fixes temperature=0 (and seed when supported), a bounded max output,
and streams so time-to-first-token is measured. The full request and response
are returned for the record — nothing about a reader run is reconstructed from
memory. Stdlib only (urllib): Level 2 adds no Python runtime dependency.
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

    @classmethod
    def from_env(cls) -> "ReaderConfig":
        url = os.environ.get("QODEC_READER_URL")
        model = os.environ.get("QODEC_READER_MODEL")
        if not url or not model:
            raise ReaderUnavailable(
                "set QODEC_READER_URL and QODEC_READER_MODEL to run the reader"
            )
        return cls(
            url=url.rstrip("/"),
            model=model,
            tokenizer=os.environ.get("QODEC_READER_TOKENIZER"),
            max_tokens=int(os.environ.get("QODEC_READER_MAX_TOKENS", "512")),
        )


@dataclass
class ReaderResult:
    text: str
    request: dict
    response_meta: dict
    ttft_ms: float | None
    total_ms: float
    usage: dict = field(default_factory=dict)


def chat(cfg: ReaderConfig, messages: list[dict], *, stream: bool = True) -> ReaderResult:
    """One deterministic chat completion. Streams to measure TTFT."""
    body = {
        "model": cfg.model,
        "messages": messages,
        "temperature": 0,
        "max_tokens": cfg.max_tokens,
        "stream": stream,
    }
    if cfg.seed is not None:
        body["seed"] = cfg.seed
    if stream:
        body["stream_options"] = {"include_usage": True}

    data = json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        f"{cfg.url}/chat/completions", data=data,
        headers={"Content-Type": "application/json",
                 "Authorization": f"Bearer {os.environ.get('QODEC_READER_KEY', 'none')}"},
        method="POST",
    )
    started = time.perf_counter()
    try:
        resp = urllib.request.urlopen(req, timeout=600)
    except urllib.error.URLError as exc:
        raise ReaderUnavailable(f"reader endpoint {cfg.url} unreachable: {exc}") from exc

    if not stream:
        obj = json.loads(resp.read())
        total = (time.perf_counter() - started) * 1000
        text = obj["choices"][0]["message"]["content"]
        return ReaderResult(text=text, request=body, response_meta=obj,
                            ttft_ms=None, total_ms=total, usage=obj.get("usage", {}))

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
    return ReaderResult(
        text="".join(text_parts), request=body,
        response_meta={"finish_reason": finish_reason},
        ttft_ms=ttft_ms, total_ms=total, usage=usage,
    )
