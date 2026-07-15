#!/usr/bin/env python3
"""N2-C CandidateRegistry: schema validation + selection-status state machine.

Structurally mirrors N2-B's frozen registry.py (qodec/evals/interop/v2/n2/miner/tools/registry.py)
but validates against N2-C's own, broader schema (5 origin kinds, not just
repository-miner) and its own status vocabulary (selected-primary /
selected-alternate, distinguishing the two selection roles the N2-C addendum
requires — N2-B never needed that distinction). Reuses the frozen,
dependency-free jsonschema_mini validator shared with N2-A/N2-B, read-only.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

SOURCE_FREEZE_DIR = Path(__file__).resolve().parents[1]
N2_DIR = SOURCE_FREEZE_DIR.parent
V2_DIR = N2_DIR.parent
CORPUS_TOOLS = V2_DIR / "corpus" / "tools"
sys.path.insert(0, str(CORPUS_TOOLS))
import jsonschema_mini  # noqa: E402

SCHEMA_PATH = SOURCE_FREEZE_DIR / "schemas" / "candidate-registry.schema.json"

ALLOWED_TRANSITIONS = {
    ("discovered", "inspected"),
    ("inspected", "eligible"),
    ("inspected", "ineligible"),
    ("eligible", "selected-primary"),
    ("eligible", "selected-alternate"),
    ("selected-primary", "frozen"),
    ("selected-alternate", "frozen"),
    ("selected-primary", "rejected-after-selection"),
    ("selected-alternate", "rejected-after-selection"),
}

RESTRICTED_TRANSITIONS = {
    ("ineligible", "selected-primary"),
    ("ineligible", "selected-alternate"),
    ("frozen", "discovered"),
    ("frozen", "eligible"),
}


def load_schema() -> dict:
    return json.loads(SCHEMA_PATH.read_text())


def validate_schema(registry: dict) -> list[str]:
    return jsonschema_mini.validate(registry, load_schema())


def validate_transition_step(prev_status: str, prev_version: str, next_status: str, next_version: str,
                              override: dict | None) -> str | None:
    pair = (prev_status, next_status)
    if pair in ALLOWED_TRANSITIONS:
        return None
    if pair in RESTRICTED_TRANSITIONS:
        if override and override.get("reason") and next_version != prev_version:
            return None
        return (f"restricted transition {prev_status!r} -> {next_status!r} requires an override "
                f"with a documented reason AND a new registry_version (got override={override!r}, "
                f"prev_version={prev_version!r}, next_version={next_version!r})")
    return f"undefined transition {prev_status!r} -> {next_status!r}"


def validate_candidate_transitions(candidate: dict) -> list[str]:
    errors = []
    candidate_id = candidate.get("candidate_id")
    history = candidate.get("status_history", [])
    if not history:
        errors.append(f"candidate {candidate_id!r}: status_history must contain at least one entry")
        return errors
    if history[-1]["status"] != candidate.get("selection_status"):
        errors.append(
            f"candidate {candidate_id!r}: status_history's last entry "
            f"({history[-1]['status']!r}) does not match selection_status "
            f"({candidate.get('selection_status')!r})"
        )
    for i in range(1, len(history)):
        prev, cur = history[i - 1], history[i]
        err = validate_transition_step(
            prev["status"], prev["registry_version"],
            cur["status"], cur.get("registry_version", prev["registry_version"]),
            cur.get("override"),
        )
        if err:
            errors.append(f"candidate {candidate_id!r}: {err}")
    return errors


_FORBIDDEN_FIELD_MARKERS = (
    "raw_tokens", "qodec_tokens", "rtk_tokens", "hybrid_tokens", "token_savings",
    "compression_ratio", "winner", "preferred_arm", "reader_score", "model_score",
    "qodec", "rtk",
)


def _flatten_keys(obj, prefix="") -> list[str]:
    keys = []
    if isinstance(obj, dict):
        for k, v in obj.items():
            keys.append(f"{prefix}{k}")
            keys.extend(_flatten_keys(v, f"{prefix}{k}."))
    elif isinstance(obj, list):
        for item in obj:
            keys.extend(_flatten_keys(item, prefix))
    return keys


def validate_no_forbidden_fields(registry: dict) -> list[str]:
    """Section 19 sealing check: no benchmark-output fields anywhere in the
    candidate registry, even as null. Applied at validation time so a poisoned
    field is rejected before it ever reaches eligibility/scoring."""
    errors = []
    for candidate in registry.get("candidates", []):
        for key in _flatten_keys(candidate):
            low = key.lower()
            for marker in _FORBIDDEN_FIELD_MARKERS:
                if marker in low:
                    errors.append(
                        f"candidate {candidate.get('candidate_id')!r} carries forbidden field "
                        f"{key!r} (matches marker {marker!r})"
                    )
    return errors


def validate_registry(registry: dict) -> list[str]:
    errors = validate_schema(registry)
    if errors:
        return errors
    errors.extend(validate_no_forbidden_fields(registry))
    if errors:
        return errors
    for candidate in registry.get("candidates", []):
        errors.extend(validate_candidate_transitions(candidate))
    return errors


def load_registry(path: Path) -> dict:
    return json.loads(Path(path).read_text())


def eligible_candidates(registry: dict) -> list[dict]:
    return [c for c in registry["candidates"]
            if c["selection_status"] in ("eligible", "selected-primary", "selected-alternate", "frozen")]


if __name__ == "__main__":
    registry_path = SOURCE_FREEZE_DIR / "candidate-registry.json"
    errs = validate_registry(load_registry(registry_path))
    print(json.dumps({"errors": errs, "valid": not errs}, indent=2))
