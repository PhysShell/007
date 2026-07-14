#!/usr/bin/env python3
"""N2-B CandidateRegistry: schema validation + selection-status state machine.

The registry is a versioned, explicitly-provided input file (never a crawler
output — external discovery is N2-C's job, not this framework's). This module
validates structure (via the frozen, dependency-free jsonschema_mini shared
with N2-A) and enforces the status transition contract from the N2-B
addendum section 4: most transitions are fixed; three specific regressions
(ineligible->selected, frozen->discovered, frozen->eligible) are blocked
unless accompanied by an explicit override carrying both a reason and a
registry_version bump.
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

SCHEMA_PATH = MINER_DIR / "schemas" / "candidate-registry.schema.json"

ALLOWED_TRANSITIONS = {
    ("discovered", "inspected"),
    ("inspected", "eligible"),
    ("inspected", "ineligible"),
    ("eligible", "selected"),
    ("selected", "frozen"),
    ("selected", "rejected-after-selection"),
}

# Regressions the addendum explicitly names as forbidden *by default* — they
# may only occur with a documented override AND a registry_version bump at
# that step, never silently.
RESTRICTED_TRANSITIONS = {
    ("ineligible", "selected"),
    ("frozen", "discovered"),
    ("frozen", "eligible"),
}


def load_schema() -> dict:
    return json.loads(SCHEMA_PATH.read_text())


def validate_schema(registry: dict) -> list[str]:
    return jsonschema_mini.validate(registry, load_schema())


def validate_transition_step(prev_status: str, prev_version: str, next_status: str, next_version: str,
                              override: dict | None) -> str | None:
    """Returns an error string, or None if the step is valid."""
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
    """`status_history` is the complete ordered list of statuses the
    candidate has occupied, ending with its CURRENT status — its last
    entry must equal `selection_status` (the two are not independent
    facts to be reconciled; status_history is the source of truth and
    selection_status is a redundant, checked mirror of its tail)."""
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


def validate_registry(registry: dict) -> list[str]:
    errors = validate_schema(registry)
    if errors:
        return errors
    for candidate in registry.get("candidates", []):
        errors.extend(validate_candidate_transitions(candidate))
    return errors


def load_registry(path: Path) -> dict:
    return json.loads(Path(path).read_text())


def eligible_candidates(registry: dict) -> list[dict]:
    return [c for c in registry["candidates"] if c["selection_status"] in ("eligible", "selected", "frozen")]


if __name__ == "__main__":
    example = MINER_DIR / "candidate-registry.example.json"
    errs = validate_registry(load_registry(example))
    print(json.dumps({"errors": errs, "valid": not errs}, indent=2))
