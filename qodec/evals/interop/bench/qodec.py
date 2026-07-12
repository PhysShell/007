"""qodec — the terminal transform, and the only tokenizer the bench trusts.

Every lane ends by handing its producer/transform output to `encode` through
the adapter envelope (`encode --json --passthrough-on-no-gain`, see
`qodec/src/adapter.rs`). We shell the built binary rather than bind the crate:
the binary is what a PostToolUse hook or `o7` would call, so the bench measures
exactly what ships.

The binary is also the bench's token meter. Level 1 needs three token counts
the envelope does not carry directly — an arbitrary string's length, and the
*cold* prompt (notation brief + artifact) a reader actually pays. `count()` and
`probe()` get both from the same binary, so raw and encoded arms are measured
under one tokenizer with no Python-side approximation.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

# bench/ -> interop/ -> evals/ -> qodec/ (crate root: target/, Cargo.toml)
CRATE_ROOT = Path(__file__).resolve().parents[3]


class QodecError(RuntimeError):
    """qodec exited non-zero or emitted output we could not parse."""


def binary() -> Path:
    """Locate the built `qodec` binary. Honor QODEC_BIN, else prefer release
    (a debug binary reports honest ratios but slanders wall time)."""
    override = os.environ.get("QODEC_BIN")
    if override:
        p = Path(override)
        if p.exists():
            return p
        raise QodecError(f"QODEC_BIN={override} does not exist")
    for profile in ("release", "debug"):
        cand = CRATE_ROOT / "target" / profile / "qodec"
        if cand.exists():
            return cand
    which = shutil.which("qodec")
    if which:
        return Path(which)
    raise QodecError(
        f"no qodec binary under {CRATE_ROOT / 'target'} — "
        "run `cargo build --release` in the crate root"
    )


def version() -> str:
    """A stable identity for the receipt. The crate has no --version flag yet,
    so pin the binary's own content hash (short) as its version."""
    import hashlib

    data = binary().read_bytes()
    return "sha256:" + hashlib.sha256(data).hexdigest()[:12]


@dataclass
class Encoded:
    encoded: bool
    codec: str
    content: str
    tokens_in: int
    tokens_out: int
    meter: str
    encode_ms: float

    @property
    def gain(self) -> float:
        if self.tokens_in == 0:
            return 0.0
        return 1.0 - self.tokens_out / self.tokens_in

    @property
    def is_fallback(self) -> bool:
        return self.codec in ("raw", "passthrough")


def _run(argv: list[str], text: str) -> tuple[str, float]:
    started = time.perf_counter()
    proc = subprocess.run(argv, input=text, capture_output=True, text=True, check=False)
    ms = (time.perf_counter() - started) * 1000.0
    if proc.returncode != 0:
        raise QodecError(f"{argv[1] if len(argv) > 1 else argv} failed: {proc.stderr.strip()}")
    return proc.stdout, ms


def encode(text: str, *, codec: str = "squeeze", meter: str = "o200k",
           passthrough: bool = True) -> Encoded:
    argv = [str(binary()), "encode", "--codec", codec, "--meter", meter, "--json"]
    if passthrough:
        argv.append("--passthrough-on-no-gain")
    out, ms = _run(argv, text)
    try:
        env = json.loads(out)
    except json.JSONDecodeError as exc:
        raise QodecError(f"encode emitted non-JSON: {exc}") from exc
    return Encoded(
        encoded=env["encoded"], codec=env["codec"], content=env["content"],
        tokens_in=env["tokens_in"], tokens_out=env["tokens_out"],
        meter=env["meter"], encode_ms=ms,
    )


def decode(content: str) -> tuple[str, float]:
    """Invert a `%q1` artifact. NOTE: this always runs the decoder, so passing
    container-shaped *plaintext* here would wrongly unwrap it — route through
    `decode_envelope`, which respects the `encoded` flag."""
    return _run([str(binary()), "decode"], content)


def decode_envelope(env: Encoded) -> tuple[str, float]:
    """Recover the original text from an adapter envelope, honoring `encoded`.

    `encoded=false` means `content` is already the plaintext original (a
    passthrough) and must be returned verbatim — never fed to `qodec decode`.
    That matters when the passthrough text is itself container-shaped (a literal
    `%q1 …` string in tool output): decoding it would unwrap a container the
    codec never created. Only `encoded=true` content is a real artifact to
    invert."""
    if not env.encoded:
        return env.content, 0.0
    return decode(env.content)


def count(text: str, *, meter: str = "o200k") -> int:
    """Token length of arbitrary text under the meter. `fold` is a single
    linear pass and always reports the true input count in its envelope,
    whatever it decides to emit."""
    out, _ = _run([str(binary()), "encode", "--codec", "fold", "--meter", meter, "--json"], text)
    return json.loads(out)["tokens_in"]


def notation() -> str:
    """The notation brief (decoder instruction) verbatim — the reader's
    `raw + brief` control and `encoded + brief` preamble."""
    out, _ = _run([str(binary()), "notation"], "")
    return out


def probe(text: str, *, codec: str = "squeeze", meter: str = "o200k") -> str:
    """The cold prompt a reader actually receives for an encoded payload: the
    notation brief (the decoder instruction) followed by the artifact. `qodec
    probe` prepends exactly `ab::notation_brief()`, so cold accounting can never
    drift from the codec's own teaching text. Deterministic for a fixed codec,
    so its artifact equals `encode`'s content on the same input."""
    out, _ = _run([str(binary()), "probe", "--codec", codec, "--meter", meter], text)
    return out
