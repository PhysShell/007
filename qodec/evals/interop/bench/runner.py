"""Execute a case's pipeline and persist every artifact, hashed.

produce -> non-qodec transforms -> qodec (terminal) -> measure -> save. One
qodec arm per case (named by the tool feeding qodec: rtk / codegraph / raw).
Unsupported transforms short-circuit to an explicit `unsupported` record.
"""

from __future__ import annotations

import json
from pathlib import Path

from . import execution, metrics, producers, qodec, transforms
from .artifacts import ArtifactDir
from .lockfiles import Repo, Tool
from .manifest import Case


def _roundtrip_ok(original: str, decoded: str, is_json: bool) -> bool:
    if decoded == original:
        return True
    if is_json:
        try:
            return json.loads(decoded) == json.loads(original)
        except json.JSONDecodeError:
            return False
    return False


def run_case(case: Case, tools: dict[str, Tool], repos: dict[str, Repo],
             run_dir: Path, *, codec: str, meter: str) -> dict:
    unsupported = case.unsupported
    if unsupported is not None:
        return {
            "id": case.id, "arm": case.arm, "pipeline_id": case.pipeline_id,
            "status": "unsupported",
            "reason": transforms.unsupported_reason(unsupported, tools),
        }

    try:
        produced, baseline = producers.produce(case, tools, repos)
    except execution.ExecutionError as exc:
        return {"id": case.id, "arm": case.arm, "pipeline_id": case.pipeline_id,
                "status": "error", "reason": str(exc)}

    text = produced.text
    stages = [produced.provenance()]
    try:
        for t in case.transforms[:-1]:
            if t.type == "rtk":
                ex = transforms.apply_rtk(text, t, tools)
                text = ex.text
                stages.append(ex.provenance())
    except (execution.ExecutionError, transforms.UnsupportedTransform) as exc:
        return {"id": case.id, "arm": case.arm, "pipeline_id": case.pipeline_id,
                "status": "error", "reason": str(exc)}

    tool_only_text = text
    qcodec = case.transforms[-1].raw.get("codec", codec)
    is_json = bool(case.raw.get("json", False))

    env = qodec.encode(tool_only_text, codec=qcodec, meter=meter, passthrough=True)
    # Honor the encoded flag: a passthrough (encoded=false) is already the
    # plaintext original and must not be run back through the decoder, or a
    # container-shaped passthrough would be wrongly unwrapped.
    decoded, decode_ms = qodec.decode_envelope(env)
    rt = _roundtrip_ok(tool_only_text, decoded, is_json)

    # The cold prompt a reader actually receives: for an encoded artifact, the
    # notation brief + artifact (measured together); for a passthrough, the
    # plaintext body (no brief). Saved as an artifact for the canonical record.
    if env.is_fallback:
        cold_prompt_text = env.content
        cold_prompt_tokens = env.tokens_out
    else:
        cold_prompt_text = qodec.probe(tool_only_text, codec=qcodec, meter=meter)
        cold_prompt_tokens = qodec.count(cold_prompt_text, meter=meter)
    m = metrics.build(env, cold_prompt_tokens=cold_prompt_tokens,
                      decode_ms=decode_ms, roundtrip_ok=rt)

    # Persist every artifact + provenance.
    ad = ArtifactDir(run_dir, case.id, case.arm)
    ad.write("producer.txt", produced.text)
    ad.write("transformed.txt", tool_only_text)
    envelope = {
        "encoded": env.encoded, "codec": env.codec, "content": env.content,
        "tokens_in": env.tokens_in, "tokens_out": env.tokens_out, "meter": env.meter,
    }
    ad.write("qodec-envelope.json", json.dumps(envelope, indent=2) + "\n")
    ad.write("qodec-content.txt", env.content)
    ad.write("cold-prompt.txt", cold_prompt_text)
    ad.write("decoded.txt", decoded)

    baseline_info = None
    if baseline is not None:
        baseline_info = {
            **baseline.provenance(),
            "tokens": qodec.count(baseline.text, meter=meter),
        }
        ad.write("baseline.txt", baseline.text)

    # For pipe transforms, the producer text (pre-rtk) differs from the
    # tool-only text (post-rtk): record the producer tokens and RTK's own
    # upstream reduction, so scoring sees producer -> tool -> qodec in full.
    producer_tokens = None
    upstream_reduction = None
    if produced.text != tool_only_text:
        producer_tokens = qodec.count(produced.text, meter=meter)
        if producer_tokens:
            upstream_reduction = round(1 - m.tool_only_tokens / producer_tokens, 4)

    meta = {
        "case": case.id,
        "arm": case.arm,
        "pipeline_id": case.pipeline_id,
        "qodec_version": qodec.version(),
        "codec": qcodec,
        "meter": meter,
        "is_json": is_json,
        "pipeline": stages,
        "baseline": baseline_info,
        "upstream_reduction": upstream_reduction,
        "tokens": {
            "producer": producer_tokens,
            "tool_only": m.tool_only_tokens,
            "qodec_content": m.qodec_content_tokens,
            "cold_prompt": m.cold_prompt_tokens,
            "warm_payload": m.warm_payload_tokens,
        },
        "incremental_qodec_gain": {
            "cold": round(m.gain(warm=False), 4),
            "warm": round(m.gain(warm=True), 4),
        },
        "qodec_codec": m.codec,
        "is_fallback": m.is_fallback,
        "encode_ms": round(m.encode_ms, 2),
        "decode_ms": round(m.decode_ms, 2),
        "upstream_tool_ms": round(sum(s["wall_ms"] for s in stages), 2),
        "roundtrip_ok": m.roundtrip_ok,
    }
    ad.write_meta(meta)

    return {
        "id": case.id,
        "arm": case.arm,
        "pipeline_id": case.pipeline_id,
        "status": "ok",
        "producer_tokens": producer_tokens,
        "upstream_reduction": upstream_reduction,
        "tool_only_tokens": m.tool_only_tokens,
        "cold_prompt_tokens": m.cold_prompt_tokens,
        "warm_payload_tokens": m.warm_payload_tokens,
        "cold_gain": round(m.gain(warm=False), 4),
        "warm_gain": round(m.gain(warm=True), 4),
        "codec": m.codec,
        "is_fallback": m.is_fallback,
        "verdict_cold": m.verdict(warm=False),
        "verdict_warm": m.verdict(warm=True),
        "encode_ms": round(m.encode_ms, 2),
        "decode_ms": round(m.decode_ms, 2),
        "upstream_tool_ms": round(sum(s["wall_ms"] for s in stages), 2),
        "roundtrip_ok": m.roundtrip_ok,
        "baseline": baseline_info,
        "artifact_dir": str(ad.dir.relative_to(run_dir)),
    }
