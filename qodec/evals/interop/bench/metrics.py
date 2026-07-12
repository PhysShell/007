"""Honest token accounting — cold and warm.

The design doc's warning: "do not call a combination a win if it only passes
after ignoring the mandatory decoder instruction." An encoded artifact is
unreadable without the qodec notation brief, so a fair cold measurement must
include that brief. Two figures per arm:

  cold_prompt_tokens   what a one-shot message pays: notation brief + artifact.
                       (passthrough pays only the plaintext — no brief needed.)
  warm_payload_tokens  what a protocol pays once the brief sits in a cached
                       prefix: the artifact body alone.

incremental_qodec_gain is reported against the tool-only tokens for both, so a
combination that wins warm but loses cold is visible as exactly that.
"""

from __future__ import annotations

from dataclasses import dataclass

from . import qodec


@dataclass
class ArmMetrics:
    tool_only_tokens: int          # tokens of the text handed to qodec
    qodec_content_tokens: int      # artifact / passthrough body (warm payload)
    cold_prompt_tokens: int        # notation brief + artifact (or plaintext)
    warm_payload_tokens: int       # == qodec_content_tokens
    codec: str
    is_fallback: bool
    encode_ms: float
    decode_ms: float
    roundtrip_ok: bool

    def gain(self, *, warm: bool) -> float:
        payload = self.warm_payload_tokens if warm else self.cold_prompt_tokens
        if self.tool_only_tokens == 0:
            return 0.0
        return 1.0 - payload / self.tool_only_tokens

    def verdict(self, *, warm: bool, threshold: float = 0.10) -> str:
        if self.is_fallback:
            return "passthrough"
        g = self.gain(warm=warm)
        if g >= threshold:
            return "win"
        if g > 0:
            return "marginal"
        return "loss"


def measure(tool_only_text: str, env: qodec.Encoded, *, codec: str, meter: str,
            decode_ms: float, roundtrip_ok: bool) -> ArmMetrics:
    """Build the cold/warm metrics for one qodec arm.

    cold: for an encoded artifact, count the notation brief + artifact together
    (the reader tokenizes them as one prompt). For a passthrough there is no
    brief — cold == warm == the plaintext body.
    """
    warm = env.tokens_out
    if env.is_fallback:
        cold = warm
    else:
        cold = qodec.count(qodec.probe(tool_only_text, codec=codec, meter=meter), meter=meter)
    return ArmMetrics(
        tool_only_tokens=env.tokens_in,
        qodec_content_tokens=env.tokens_out,
        cold_prompt_tokens=cold,
        warm_payload_tokens=warm,
        codec=env.codec,
        is_fallback=env.is_fallback,
        encode_ms=env.encode_ms,
        decode_ms=decode_ms,
        roundtrip_ok=roundtrip_ok,
    )
