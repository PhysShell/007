"""Transforms — rewrite text -> text. Only RTK stdin filters and qodec here.

Headroom and FastContext are deliberately NOT executable transforms (item 7 of
the increment): the Headroom adapter used an unverified return contract, and
FastContext is a served model, not a `fastcontext.brief()` package. A case that
names them gets an explicit `unsupported` arm, never a silent skip that reads as
"just waiting for install".
"""

from __future__ import annotations

from . import execution, lockfiles
from .manifest import Transform


class UnsupportedTransform(RuntimeError):
    pass


def apply_rtk(text: str, transform: Transform, tools: dict[str, lockfiles.Tool]) -> execution.Executed:
    """Pipe text through an RTK stdin filter (e.g. `rtk log`)."""
    rtk = tools["rtk"]
    b = rtk.resolve_bin()
    if not b:
        raise execution.ExecutionError("rtk not resolvable (RTK_BIN / PATH)")
    flt = transform.raw["filter"]
    if flt not in rtk.stdin_filters:
        raise UnsupportedTransform(
            f"rtk filter {flt!r} is not a stdin filter {rtk.stdin_filters}"
        )
    return execution.run([b, flt], stdin=text, tool="rtk", version=rtk.detected_version())


def unsupported_reason(transform: Transform, tools: dict[str, lockfiles.Tool]) -> str:
    tool = tools.get(transform.type)
    if tool and tool.reason:
        return f"unsupported: {tool.reason}"
    return "unsupported: adapter not validated"
