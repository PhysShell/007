#!/usr/bin/env python3
"""N2-B toolchain identity contract (section 10).

Mandated directly by the N2-A finding: a workflow requested `8.0.x` but the
build actually executed under `10.0.301`, silently, because nothing compared
the *requested* range against what was *resolved* and what was *executed*.
This module keeps those three stages distinct and produces one of exactly
four classifications. `identity-missing` is always a hard failure — a setup
action's input string is a request, never evidence of what actually ran.
"""
from __future__ import annotations

import re

CLASSIFICATIONS = ("exact-match", "compatible-resolution", "unexpected-resolution", "identity-missing")


def _range_matches(requested: str, resolved: str) -> bool:
    """`requested` may be a floating range like "8.0.x" or "8.0.*"; `resolved`
    is the concrete version actually resolved. Exact equality is handled by
    the caller before this is reached. Built character-by-character (rather
    than re.escape then substring-replace) because re.escape does not escape
    a bare "x", so a post-escape ``\\x`` search never matches anything."""
    if "x" not in requested and "*" not in requested:
        return False
    pattern = "^" + "".join(r"\d+" if ch in ("x", "*") else re.escape(ch) for ch in requested) + "$"
    return bool(re.match(pattern, resolved))


def classify(*, requested_version_or_range: str, resolved_version: str | None,
             runtime_identifier: str | None, resolved_executable_path: str | None,
             executed_binary_absolute_path: str | None, executed_binary_sha256: str | None) -> str:
    required_present = [resolved_version, runtime_identifier, resolved_executable_path,
                         executed_binary_absolute_path, executed_binary_sha256]
    if any(not v for v in required_present):
        return "identity-missing"
    if resolved_version == requested_version_or_range:
        return "exact-match"
    if _range_matches(requested_version_or_range, resolved_version):
        return "compatible-resolution"
    return "unexpected-resolution"


def build_toolchain_identity(*, requested_version_or_range: str, resolver_mechanism: str,
                              resolved_version: str | None, runtime_identifier: str | None,
                              resolved_executable_path: str | None,
                              executed_binary_absolute_path: str | None,
                              executed_binary_sha256: str | None,
                              executed_argv0: str | None = None) -> dict:
    classification = classify(
        requested_version_or_range=requested_version_or_range,
        resolved_version=resolved_version,
        runtime_identifier=runtime_identifier,
        resolved_executable_path=resolved_executable_path,
        executed_binary_absolute_path=executed_binary_absolute_path,
        executed_binary_sha256=executed_binary_sha256,
    )
    return {
        "toolchain_requested": {
            "version_or_range": requested_version_or_range,
            "resolver_mechanism": resolver_mechanism,
        },
        "toolchain_resolved": {
            "resolved_version": resolved_version,
            "runtime_identifier": runtime_identifier,
            "resolved_executable_path": resolved_executable_path,
        },
        "toolchain_executed": {
            "executed_argv0": executed_argv0,
            "executed_binary_absolute_path": executed_binary_absolute_path,
            "executed_binary_sha256": executed_binary_sha256,
            "classification": classification,
        },
    }


def is_hard_failure(classification: str) -> bool:
    return classification == "identity-missing"
