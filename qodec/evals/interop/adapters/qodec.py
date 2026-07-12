"""qodec adapter — the one adapter that is always present.

Every lane ends here: the producer (a raw command output, a Graphify subgraph,
a CodeGraph explore dump, an RTK-filtered log, a Headroom-shaped prompt, a
FastContext evidence appendix) is handed to `encode` and, in the protected/
blind arms, decoded back to prove losslessness.

We shell out to the built `qodec` binary rather than bind the Rust crate: the
binary is the artifact the rest of the world (a PostToolUse hook, `o7`) would
call, so the bench measures exactly what ships. The adapter envelope
(`encode --json`) is the contract — see `qodec/src/adapter.rs`.
"""

from __future__ import annotations

import json
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

# adapters/ -> interop/ -> evals/ -> qodec/ (the crate root: target/, Cargo.toml)
CRATE_ROOT = Path(__file__).resolve().parents[3]


class QodecError(RuntimeError):
    """qodec exited non-zero or produced output we could not parse."""


def binary() -> Path:
    """Locate the built `qodec` binary, preferring an optimized release.

    The bench reports encode/decode wall time, so a debug binary would slander
    the codec by 10-20x; `doctor.py` warns when only debug is available.
    """
    for profile in ("release", "debug"):
        cand = CRATE_ROOT / "target" / profile / "qodec"
        if cand.exists():
            return cand
    raise QodecError(
        f"no qodec binary under {CRATE_ROOT / 'target'} — "
        "run `cargo build --release` in the crate root first"
    )


@dataclass
class Encoded:
    """One `encode --json` envelope plus the wall time it cost."""

    encoded: bool
    codec: str
    content: str
    tokens_in: int
    tokens_out: int
    meter: str
    encode_ms: float

    @property
    def gain(self) -> float:
        """Fraction of input tokens removed. Negative means the artifact grew;
        with passthrough on, a no-gain payload reports exactly 0.0."""
        if self.tokens_in == 0:
            return 0.0
        return 1.0 - self.tokens_out / self.tokens_in

    @property
    def is_fallback(self) -> bool:
        return self.codec in ("raw", "passthrough")


def encode(
    text: str,
    *,
    codec: str = "squeeze",
    meter: str = "o200k",
    passthrough: bool = True,
    profile: Path | None = None,
) -> Encoded:
    """Run the producer output through qodec, returning the adapter envelope.

    `passthrough=True` is the interop default: applied blindly at the end of a
    lane, qodec must never add container tax to output that held no residual
    repetition (that is the whole point of --passthrough-on-no-gain)."""
    argv = [
        str(binary()),
        "encode",
        "--codec",
        codec,
        "--meter",
        meter,
        "--json",
    ]
    if passthrough:
        argv.append("--passthrough-on-no-gain")
    if profile is not None:
        argv += ["--profile", str(profile)]
    started = time.perf_counter()
    proc = subprocess.run(
        argv, input=text, capture_output=True, text=True, check=False
    )
    elapsed_ms = (time.perf_counter() - started) * 1000.0
    if proc.returncode != 0:
        raise QodecError(f"encode failed ({proc.returncode}): {proc.stderr.strip()}")
    try:
        env = json.loads(proc.stdout)
    except json.JSONDecodeError as exc:
        raise QodecError(f"encode emitted non-JSON: {exc}") from exc
    return Encoded(
        encoded=env["encoded"],
        codec=env["codec"],
        content=env["content"],
        tokens_in=env["tokens_in"],
        tokens_out=env["tokens_out"],
        meter=env["meter"],
        encode_ms=elapsed_ms,
    )


def decode(content: str, *, profile: Path | None = None) -> tuple[str, float]:
    """Invert `content` (an artifact or a passthrough) back to the producer
    output. Returns (text, decode_ms). A passthrough decodes to itself, so the
    caller can round-trip every envelope uniformly."""
    argv = [str(binary()), "decode"]
    started = time.perf_counter()
    proc = subprocess.run(
        argv, input=content, capture_output=True, text=True, check=False
    )
    elapsed_ms = (time.perf_counter() - started) * 1000.0
    if proc.returncode != 0:
        raise QodecError(f"decode failed ({proc.returncode}): {proc.stderr.strip()}")
    return proc.stdout, elapsed_ms


def roundtrip_ok(original: str, env: Encoded) -> bool:
    """Byte-exact for text codecs; the interop harness treats JSON specially
    upstream (Value-equal) — here we assert the strict property the container
    guarantees for everything except toon."""
    back, _ = decode(env.content)
    return back == original
