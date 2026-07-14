"""Reader preflight — negotiate the endpoint's real contract, pin every identity.

Probes what the endpoint actually honors (streaming, usage accounting, seed,
structured-JSON), and records the identities that make a run reproducible: the
model GGUF hash + size + quantization + source, the llama-cpp-python / server
argv / n_ctx / threads / batch, CPU backend, and the tokenizer + config hashes.
The negotiated `effective` contract is what the whole matrix uses — a parameter
the endpoint rejected is never re-sent. Written to preflight.json.
"""

from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import urllib.error
import urllib.request
from pathlib import Path

from . import qodec, reader

# The fixed answer shape; used for structured-JSON negotiation.
ANSWER_JSON_SCHEMA = {
    "type": "object",
    "properties": {
        "facts": {"type": "array", "items": {"type": "string"}},
        "files": {"type": "array", "items": {"type": "string"}},
        "symbols": {"type": "array", "items": {"type": "string"}},
        "call_path": {"type": "array", "items": {"type": "string"}},
        "answer": {"type": "string"},
    },
    "required": ["facts", "files", "symbols", "call_path", "answer"],
    "additionalProperties": False,
}
_GBNF = None  # llama.cpp grammar left None; response_format is preferred when present


def _sha256_file(path: Path) -> str | None:
    if not path.exists():
        return None
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def _tokenizer_identity(meter: str) -> dict:
    if not meter.startswith("hf:"):
        return {"meter": meter, "kind": "bpe-proxy"}
    path = Path(meter[len("hf:"):])
    if not path.exists():
        return {"meter": meter, "path": str(path), "error": "tokenizer file missing"}
    ident = {"meter": meter, "path": str(path),
             "sha256": _sha256_file(path), "size_bytes": path.stat().st_size}
    cfg = path.parent / "tokenizer_config.json"
    if cfg.exists():
        ident["tokenizer_config_sha256"] = _sha256_file(cfg)
    return ident


def _model_identity(cfg: reader.ReaderConfig) -> dict:
    ident: dict = {
        "model_requested": cfg.model,
        "model_source": cfg.model_source,
        "server_argv": cfg.server_argv,
        "cpu": {"machine": platform.machine(), "cores": os.cpu_count(),
                "system": platform.system()},
    }
    if cfg.model_file:
        p = Path(cfg.model_file)
        ident["model_file"] = str(p)
        ident["model_file_sha256"] = _sha256_file(p)
        ident["model_file_size_bytes"] = p.stat().st_size if p.exists() else None
        hay = f"{p.name} {cfg.model_source or ''}".lower()
        m = re.search(r"(q\d[_a-z0-9]*|f16|bf16|iq\d\w*)", hay)
        ident["quantization"] = m.group(1) if m else None
    if cfg.server_argv:
        for key, flag in (("n_ctx", "n_ctx"), ("threads", "n_threads"), ("batch", "n_batch")):
            m = re.search(rf"--{flag}[= ]+(\d+)", cfg.server_argv)
            ident[key] = int(m.group(1)) if m else None
    try:
        import llama_cpp
        ident["llama_cpp_python_version"] = getattr(llama_cpp, "__version__", None)
    except Exception:  # noqa: BLE001 - client may not have the binding
        ident["llama_cpp_python_version"] = None
    return ident


def _models(cfg: reader.ReaderConfig) -> dict:
    try:
        with urllib.request.urlopen(f"{cfg.url}/models", timeout=30) as resp:
            obj = json.loads(resp.read())
        ids = [m.get("id") for m in obj.get("data", [])]
        # model_reported: the served id that matches the request, not just [0].
        reported = cfg.model if cfg.model in ids else (ids[0] if ids else None)
        return {"ok": True, "ids": ids, "model_reported": reported,
                "requested_served": cfg.model in ids}
    except (urllib.error.URLError, json.JSONDecodeError, KeyError) as exc:
        return {"ok": False, "error": str(exc), "model_reported": None}


def _probe(cfg: reader.ReaderConfig, eff: reader.Effective, msg: str) -> reader.ReaderResult:
    return reader.chat(cfg, [{"role": "user", "content": msg}], eff)


def _negotiate(cfg: reader.ReaderConfig) -> tuple[reader.Effective, list[dict]]:
    """Find the honored contract. Sequence: streaming+seed+usage → streaming+seed
    → streaming+usage → streaming → non-stream. A rejected parameter is dropped,
    never re-sent; nothing here is fatal."""
    trials: list[dict] = []

    def trial(stream, send_seed, include_usage):
        eff = reader.Effective(stream=stream, send_seed=send_seed, include_usage=include_usage)
        res = _probe(cfg, eff, "Return exactly {\"answer\": \"ok\"}")
        rec = {"stream": stream, "send_seed": send_seed, "include_usage": include_usage,
               "http_error": res.http_error, "content_ok": bool(res.text.strip()),
               "usage_present": bool(res.usage), "ttft_ms": res.ttft_ms}
        trials.append(rec)
        return res, rec

    # Which base params are accepted at all (seed)?
    _, t_full = trial(True, True, True)
    seed_ok = t_full["http_error"] is None
    if not seed_ok:
        _, t_noseed = trial(True, False, True)  # retry without seed
    usage_stream = any(t["content_ok"] and t["usage_present"] and t["stream"] for t in trials)

    # If streaming yields usage, stream the matrix (TTFT + usage together).
    # Otherwise stream=False so usage comes from the non-stream body (server
    # tokens are required); TTFT is taken from a preflight streaming sample.
    _, t_nonstream = trial(False, seed_ok, False)
    nonstream_usage = t_nonstream["content_ok"] and t_nonstream["usage_present"]

    eff = reader.Effective(
        stream=usage_stream,             # matrix streams only if that gives usage
        send_seed=seed_ok,
        include_usage=usage_stream,
        response_format=None,
    )
    if not usage_stream and not nonstream_usage:
        trials.append({"warning": "no usage from stream OR non-stream — server tokens unavailable"})
    return eff, trials


def _structured_json(cfg: reader.ReaderConfig, eff: reader.Effective) -> dict:
    """Try, in order: json_schema, json_object, grammar, plain text. Set the
    first that returns parseable (schema-conforming) JSON on eff."""
    ask = 'Return the reader answer JSON for: how many? Put "1" in answer.'

    def works(rf=None, grammar=None) -> tuple[bool, str]:
        probe = reader.Effective(stream=eff.stream, send_seed=eff.send_seed,
                                 include_usage=eff.include_usage, response_format=rf, grammar=grammar)
        res = _probe(cfg, probe, ask)
        if res.http_error:
            return False, res.http_error
        m = re.search(r"\{.*\}", res.text, re.DOTALL)
        if not m:
            return False, "no json"
        try:
            obj = json.loads(m.group(0))
            return isinstance(obj, dict) and "answer" in obj, "ok"
        except json.JSONDecodeError as exc:
            return False, str(exc)

    attempts = {}
    schema_rf = {"type": "json_schema",
                 "json_schema": {"name": "reader_answer", "schema": ANSWER_JSON_SCHEMA, "strict": True}}
    ok, why = works(rf=schema_rf)
    attempts["json_schema"] = why
    if ok:
        eff.response_format = schema_rf
        return {"mode": "json_schema", "attempts": attempts}
    ok, why = works(rf={"type": "json_object"})
    attempts["json_object"] = why
    if ok:
        eff.response_format = {"type": "json_object"}
        return {"mode": "json_object", "attempts": attempts}
    if _GBNF is not None:
        ok, why = works(grammar=_GBNF)
        attempts["grammar"] = why
        if ok:
            eff.grammar = _GBNF
            return {"mode": "grammar", "attempts": attempts}
    ok, why = works()
    attempts["text"] = why
    return {"mode": "text", "attempts": attempts}


def run(cfg: reader.ReaderConfig, meter: str) -> dict:
    result: dict = {
        "url": cfg.url,
        "tokenizer": _tokenizer_identity(meter),
        "qodec_version": qodec.version(),
        "model_identity": _model_identity(cfg),
    }
    try:
        env = qodec.encode("Parser::parse\n" * 8, meter=meter, passthrough=True)
        result["qodec_encode"] = {"ok": True, "meter": env.meter, "codec": env.codec}
    except qodec.QodecError as exc:
        result["qodec_encode"] = {"ok": False, "error": str(exc)}

    result["models"] = _models(cfg)
    result["model_identity"]["model_reported"] = result["models"].get("model_reported")

    try:
        eff, trials = _negotiate(cfg)
    except reader.ReaderUnavailable as exc:
        result["negotiation"] = {"error": str(exc)}
        result["ready"] = False
        return result
    result["negotiation"] = trials
    result["structured_json"] = _structured_json(cfg, eff)
    result["effective"] = eff.to_dict()
    result["_effective_obj"] = {  # full object for the runner (response_format kept)
        "stream": eff.stream, "send_seed": eff.send_seed, "include_usage": eff.include_usage,
        "response_format": eff.response_format, "grammar": eff.grammar,
    }

    # One representative streaming sample for TTFT (even if the matrix is non-stream).
    ttft_eff = reader.Effective(stream=True, send_seed=eff.send_seed, include_usage=False)
    sample = _probe(cfg, ttft_eff, 'Return exactly {"answer": "ok"}')
    result["streaming_sample"] = {"ok": bool(sample.text.strip()), "ttft_ms": sample.ttft_ms,
                                  "total_ms": sample.total_ms, "content": sample.text[:120]}
    result["determinism"] = {
        "temperature": 0, "seed_sent": cfg.seed if eff.send_seed else None,
        "contract": f"temperature=0, seed={cfg.seed}" if eff.send_seed else "temperature=0 (seed unsupported/omitted)",
    }
    result["ready"] = bool(result.get("qodec_encode", {}).get("ok")
                           and result["streaming_sample"]["ok"])
    return result


def effective_from(pf: dict) -> reader.Effective:
    e = pf.get("_effective_obj") or {}
    return reader.Effective(
        stream=e.get("stream", False), send_seed=e.get("send_seed", True),
        include_usage=e.get("include_usage", False),
        response_format=e.get("response_format"), grammar=e.get("grammar"),
    )


def save(result: dict, path: Path) -> None:
    path.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
