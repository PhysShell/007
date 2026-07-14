#!/usr/bin/env python3
"""N2-B ReceiptContract + ReproducibilityContract (sections 16-ish).

`validate_receipt` enforces the versioned generic receipt schema (structural
non-emptiness of every mandatory identity field). `compare_receipts`
generalizes the N2-A reproducibility-gate fix: for a configurable set of
identity fields, "null == null" or "" == "" must never count as agreement —
a receipt missing an identity field is incomplete evidence, not evidence of
agreement between two captures.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
N2_DIR = MINER_DIR.parent
V2_DIR = N2_DIR.parent
CORPUS_TOOLS = V2_DIR / "corpus" / "tools"
sys.path.insert(0, str(CORPUS_TOOLS))
import jsonschema_mini  # noqa: E402

SCHEMA_PATH = MINER_DIR / "schemas" / "receipt-contract.schema.json"

# Dotted paths within a receipt that must never be treated as "in agreement"
# on the basis of both sides being empty/null — mirrors N2-A's
# REQUIRE_NON_EMPTY_FIELDS, generalized to the full generic receipt shape.
REQUIRE_NON_EMPTY_PATHS = frozenset({
    "source_identity.commit_sha",
    "source_identity.archive_sha256",
    "license_identity.sha256",
    "adapter_identity.name",
    "adapter_identity.version",
    "sandbox_identity.sandboy_commit_sha",
    "sandbox_identity.policy_sha256",
    "toolchain_resolved.resolved_version",
    "toolchain_resolved.runtime_identifier",
    "toolchain_executed.executed_binary_absolute_path",
    "toolchain_executed.executed_binary_sha256",
    "stdout_identity.sha256",
    "stderr_identity.sha256",
    "termination.exit_code",
})

DEFAULT_SEMANTIC_PATHS = tuple(sorted(REQUIRE_NON_EMPTY_PATHS))

# Fields whose presence is NOT "truthy" but "is an int, not a bool" — 0 is a
# perfectly valid, meaningful exit code, not missing evidence. Using
# `bool(value)` for this field (the original N2-A-derived gate) made
# exit_code=0 read as absent on both sides, so two receipts that both
# legitimately succeeded would fail the "is this field even present" check
# before ever comparing values.
_INT_PRESENCE_FIELDS = frozenset({"termination.exit_code"})


def load_schema() -> dict:
    return json.loads(SCHEMA_PATH.read_text())


def validate_receipt(receipt: dict) -> list[str]:
    return jsonschema_mini.validate(receipt, load_schema())


def _get_path(d: dict, path: str):
    cur = d
    for part in path.split("."):
        if not isinstance(cur, dict) or part not in cur:
            return None
        cur = cur[part]
    return cur


def _is_present(path: str, value) -> bool:
    """Type-aware presence check used for REQUIRE_NON_EMPTY_PATHS fields.

    String identity fields (commit SHAs, hashes, versions, ...): present iff
    the value is a string, and is not empty or whitespace-only.

    `termination.exit_code`: present iff the value is an int and NOT a bool
    (bool is a subclass of int in Python, so `isinstance(True, int)` is True
    and must be excluded explicitly) — any integer, including 0 and negative
    values, counts as present.
    """
    if path in _INT_PRESENCE_FIELDS:
        return isinstance(value, int) and not isinstance(value, bool)
    return isinstance(value, str) and value.strip() != ""


def compare_receipts(a: dict, b: dict, semantic_paths=DEFAULT_SEMANTIC_PATHS,
                      require_non_empty_paths=REQUIRE_NON_EMPTY_PATHS) -> list[dict]:
    rows = []
    for path in semantic_paths:
        va, vb = _get_path(a, path), _get_path(b, path)
        if path in require_non_empty_paths:
            equal = _is_present(path, va) and _is_present(path, vb) and va == vb
        else:
            equal = va == vb
        rows.append({"field": path, "value_a": va, "value_b": vb, "equal": equal})
    return rows


def overall_reproducible(rows: list[dict]) -> bool:
    return all(r["equal"] for r in rows)
