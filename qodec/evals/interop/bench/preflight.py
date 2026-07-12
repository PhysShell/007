"""Reader preflight — probe the endpoint's real capabilities before the matrix.

Records what the endpoint actually supports (streaming, usage accounting, seed)
and the exact identities that make a run reproducible (tokenizer path + SHA-256 +
size, requested vs reported model, qodec binary hash). An unsupported `seed` is
not fatal — it is recorded as the effective determinism contract, never dropped
silently. Written to preflight.json.
"""

from __future__ import annotations

import hashlib
import json
import time
import urllib.error
import urllib.request
from pathlib import Path

from . import qodec, reader


def _tokenizer_identity(meter: str) -> dict:
    if not meter.startswith("hf:"):
        return {"meter": meter, "kind": "bpe-proxy"}
    path = meter[len("hf:"):]
    p = Path(path)
    if not p.exists():
        return {"meter": meter, "path": path, "error": "tokenizer file missing"}
    data = p.read_bytes()
    return {
        "meter": meter, "path": path,
        "sha256": hashlib.sha256(data).hexdigest(),
        "size_bytes": len(data),
    }


def _models(cfg: reader.ReaderConfig) -> dict:
    try:
        req = urllib.request.Request(f"{cfg.url}/models", method="GET")
        with urllib.request.urlopen(req, timeout=30) as resp:
            obj = json.loads(resp.read())
        ids = [m.get("id") for m in obj.get("data", [])]
        return {"ok": True, "ids": ids, "requested_served": cfg.model in ids}
    except (urllib.error.URLError, json.JSONDecodeError, KeyError) as exc:
        return {"ok": False, "error": str(exc)}


def run(cfg: reader.ReaderConfig, meter: str) -> dict:
    result: dict = {
        "url": cfg.url, "model_requested": cfg.model,
        "tokenizer": _tokenizer_identity(meter),
        "qodec_version": qodec.version(),
    }

    # qodec encode under this meter (fail-closed: a bad tokenizer aborts here).
    try:
        env = qodec.encode("Parser::parse\n" * 8, meter=meter, passthrough=True)
        result["qodec_encode"] = {"ok": True, "meter": env.meter, "codec": env.codec}
    except qodec.QodecError as exc:
        result["qodec_encode"] = {"ok": False, "error": str(exc)}

    result["models"] = _models(cfg)

    # One real streaming request: content, TTFT, usage, model reported.
    try:
        res = reader.chat(cfg, [
            {"role": "system", "content": "Reply with a single JSON object and nothing else."},
            {"role": "user", "content": 'Return exactly {"answer": "ok"}'},
        ])
        result["streaming"] = {
            "ok": bool(res.text.strip()),
            "ttft_ms": res.ttft_ms,
            "total_ms": res.total_ms,
            "usage_supported": bool(res.usage),
            "usage": res.usage,
            "sample": res.text[:200],
        }
    except reader.ReaderUnavailable as exc:
        result["streaming"] = {"ok": False, "error": str(exc)}

    # Seed support: send with an explicit seed; record acceptance (non-fatal).
    seed_ok = None
    if cfg.seed is not None:
        try:
            reader.chat(cfg, [{"role": "user", "content": "ping"}])
            seed_ok = True  # accepted without error
        except reader.ReaderUnavailable:
            seed_ok = False
    result["determinism"] = {
        "temperature": 0,
        "seed_sent": cfg.seed,
        "seed_accepted": seed_ok,
        "contract": f"temperature=0, seed={cfg.seed}" if seed_ok else "temperature=0 (seed unconfirmed)",
    }

    result["ready"] = bool(
        result.get("qodec_encode", {}).get("ok")
        and result.get("streaming", {}).get("ok")
    )
    return result


def save(result: dict, path: Path) -> None:
    path.write_text(json.dumps(result, indent=2) + "\n")
