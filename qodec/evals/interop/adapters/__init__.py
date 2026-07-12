"""Producer adapters for the qodec interop bench.

Each optimizer the bench layers qodec *after* — Graphify, CodeGraph, RTK,
Headroom, FastContext — gets one adapter here. An adapter's job is narrow: say
whether the tool is installed (`available()`), and turn a raw producer input
into the tool's optimized output (`optimize()`). qodec itself is the terminal
adapter (`qodec.py`); it is always present.

The design contract (docs/token-codec.md "qodec interop bench"): the bench is a
*separate* evaluation harness, not tool code stuffed into the Rust crate. So an
adapter that cannot find its tool must fail loud and skippable, never fake a
result — a lane with no tool is reported as skipped, not silently passed.
"""

from __future__ import annotations

from dataclasses import dataclass


class ToolUnavailable(RuntimeError):
    """The optimizer is not installed / not on PATH in this environment."""


@dataclass(frozen=True)
class Availability:
    ok: bool
    detail: str


# Import order == the layering order the bench reports lanes in.
from . import codegraph, fastcontext, graphify, headroom, rtk  # noqa: E402

OPTIMIZERS = {
    "graphify": graphify,
    "codegraph": codegraph,
    "rtk": rtk,
    "headroom": headroom,
    "fastcontext": fastcontext,
}
