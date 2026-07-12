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
            "id": case.id, "arm": case.arm, "status": "unsupported",
            "reason": transforms.unsupported_reason(unsupported, tools),
        }

    try:
        produced, baseline = producers.produce(case, tools, repos)
    except execution.ExecutionError as exc:
        return {"id": case.id, "arm": case.arm, "status": "error", "reason": str(exc)}

    text = produced.text
    stages = [produced.provenance()]
    try:
        for t in case.transforms[:-1]:
            if t.type == "rtk":
                ex = transforms.apply_rtk(text, t, tools)
                text = ex.text
                stages.append(ex.provenance())
    except (execution.ExecutionError, transforms.UnsupportedTransform) as exc:
        return {"id": case.id, "arm": case.arm, "status": "error", "reason": str(exc)}

    tool_only_text = text
    qcodec = case.transforms[-1].raw.get("codec", codec)
    is_json = bool(case.raw.get("json", False))

    env = qodec.encode(tool_only_text, codec=qcodec, meter=meter, passthrough=True)
    decoded, decode_ms = qodec.decode(env.content)
    rt = _roundtrip_ok(tool_only_text, decoded, is_json)
    m = metrics.measure(tool_only_text, env, codec=qcodec, meter=meter,
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
    ad.write("decoded.txt", decoded)

    baseline_info = None
    if baseline is not None:
        baseline_info = {
            **baseline.provenance(),
            "tokens": qodec.count(baseline.text, meter=meter),
        }
        ad.write("baseline.txt", baseline.text)

    meta = {
        "case": case.id,
        "arm": case.arm,
        "qodec_version": qodec.version(),
        "codec": qcodec,
        "meter": meter,
        "is_json": is_json,
        "pipeline": stages,
        "baseline": baseline_info,
        "tokens": {
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
        "status": "ok",
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
