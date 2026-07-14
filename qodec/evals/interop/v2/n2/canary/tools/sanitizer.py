#!/usr/bin/env python3
"""N2-A MinimalSanitizer.

Handles ONLY the volatile fields actually observed in the canary build's
stdout/stderr: temporary root paths, the workspace path, timestamps, process
IDs, and ANSI/terminal control sequences. Every replacement is explicit,
deterministic, and listed in `RULES` — no general log-normalization DSL. Per
the N2 addendum: minimal, deterministic, documented, and testable; never
rewrite error messages, reorder or deduplicate lines, or otherwise touch
anything the rules below don't explicitly name.
"""
from __future__ import annotations

import hashlib
import re


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


# Each rule: (name, compiled pattern, replacement). Applied in order, once,
# to a decoded text (errors="replace") view of the captured bytes.
def _rules(tmp_root: str, workspace_root: str) -> list[tuple[str, re.Pattern, str]]:
    rules = []
    if workspace_root:
        rules.append(("workspace_root", re.compile(re.escape(workspace_root)), "<WORKSPACE>"))
    if tmp_root:
        rules.append(("tmp_root", re.compile(re.escape(tmp_root)), "<TMP>"))
    rules.extend(
        [
            (
                "iso_timestamp",
                re.compile(r"\b\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})?\b"),
                "<TIMESTAMP>",
            ),
            (
                "elapsed_time",
                re.compile(r"\b\d+(?:\.\d+)?\s*(?:ms|s|Sec|sec)\b"),
                "<ELAPSED>",
            ),
            (
                "dotnet_time_elapsed_line",
                re.compile(r"^Time Elapsed \d{2}:\d{2}:\d{2}\.\d+[ \t]*$", re.MULTILINE),
                "Time Elapsed <ELAPSED>",
            ),
            ("pid_bracket", re.compile(r"\bpid[:=]\s*\d+\b", re.IGNORECASE), "pid=<PID>"),
            ("ansi_csi", re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]"), ""),
            ("ansi_osc", re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)"), ""),
            ("cr", re.compile(r"\r\n?"), "\n"),
        ]
    )
    return rules


def sanitize(raw: bytes, *, tmp_root: str = "", workspace_root: str = "") -> tuple[bytes, dict]:
    """Returns (sanitized_bytes, report) where report lists which rules
    actually matched (and how many times), plus original/sanitized hashes."""
    text = raw.decode("utf-8", errors="replace")
    applied = []
    for name, pattern, replacement in _rules(tmp_root, workspace_root):
        text, count = pattern.subn(replacement, text)
        if count:
            applied.append({"rule": name, "replacements": count})
    sanitized = text.encode("utf-8")
    report = {
        "original_sha256": sha256_bytes(raw),
        "sanitized_sha256": sha256_bytes(sanitized),
        "rules_applied": applied,
    }
    return sanitized, report


if __name__ == "__main__":
    import sys

    data = sys.stdin.buffer.read()
    out, report = sanitize(data, tmp_root=sys.argv[1] if len(sys.argv) > 1 else "")
    sys.stdout.buffer.write(out)
    print(report, file=sys.stderr)
