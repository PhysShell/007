"""Headroom adapter — prompt-compression lane.

Headroom is itself a serious competitor: content router, JSON/code/text
compressors, reversible CCR, source retrieval. So Headroom + qodec is the most
interesting pairing — either qodec finds residual repetition, or double
notation only makes the payload harder to read (a neutral/harm result the bench
must be able to record).

Two contract points the design doc insists on:
  1. Use the Headroom *library* API `compress(messages)` with memory, learning
     and output shaping DISABLED. `headroom wrap` starts a proxy/MCP and can
     hand out RTK-like tools, so wrapping would silently benchmark
     Headroom+RTK+three hidden switches instead of Headroom alone.
  2. On the reverse order qodec(...)->Headroom (a negative control only),
     Headroom may treat `%q1` as prose and semantically rewrite a
     self-describing artifact. Never ship that ordering; the bench runs it only
     to demonstrate the failure.

`optimize()` calls the library API in a subprocess so a broken Headroom install
cannot take the harness down with it.
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys

from . import Availability, ToolUnavailable

NAME = "headroom"

# Library-API driver: compress one message with every side effect off.
_DRIVER = r"""
import json, sys
try:
    from headroom import compress
except Exception as exc:  # library missing or renamed
    sys.stderr.write("headroom import failed: %s" % exc); sys.exit(3)
text = sys.stdin.read()
out = compress(
    [{"role": "user", "content": text}],
    memory=False, learning=False, output_shaping=False,
)
# Normalize to a single string regardless of return shape.
if isinstance(out, list):
    out = "".join(m.get("content", "") if isinstance(m, dict) else str(m) for m in out)
sys.stdout.write(out if isinstance(out, str) else json.dumps(out))
"""


def available() -> Availability:
    if importlib.util.find_spec("headroom") is None:
        return Availability(False, "python package `headroom` not importable")
    return Availability(True, "headroom library API")


def optimize(text: str, **_: object) -> str:
    if not available().ok:
        raise ToolUnavailable("headroom library not installed")
    proc = subprocess.run(
        [sys.executable, "-c", _DRIVER],
        input=text,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise ToolUnavailable(f"headroom compress failed: {proc.stderr.strip()}")
    return proc.stdout
