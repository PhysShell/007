#!/usr/bin/env python3
"""N2-B CandidateScorer.

Applies only to eligible candidates (eligibility.evaluate must have already
passed — see section 6). Fully deterministic: every feature value is derived
from static candidate fields, never from QODEC/RTK output, token counts, or
which benchmark arm "wins" — those aren't observable inputs here at all.
`assert_no_forbidden_markers` is a defensive scan so a poisoned candidate
(carrying e.g. a `qodec_token_reduction` field) fails loudly instead of
silently influencing the ranking.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path

MINER_DIR = Path(__file__).resolve().parents[1]
POLICY_PATH = MINER_DIR / "candidate-selection-policy.json"

_SIZE_BUCKET_VALUE = {"small": 1.0, "medium": 0.6, "large": 0.3}
_REPRO_VALUE = {
    "expected-byte-reproducible": 1.0,
    "expected-semantically-reproducible": 0.6,
    "unknown": 0.0,
}


def load_policy() -> dict:
    return json.loads(POLICY_PATH.read_text())


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


def assert_no_forbidden_markers(candidate: dict, policy: dict) -> None:
    markers = [m.lower() for m in policy["forbidden_feature_markers"]]
    for key in _flatten_keys(candidate):
        low = key.lower()
        for marker in markers:
            if marker in low:
                raise ValueError(
                    f"candidate {candidate.get('candidate_id')!r} carries forbidden scoring input "
                    f"{key!r} (matches marker {marker!r}) — QODEC/RTK/token/popularity signals "
                    f"must never reach the scorer"
                )


def _feature_values(candidate: dict) -> dict:
    license_ = candidate.get("license", {})
    project = candidate.get("project", {})
    net = candidate.get("network_requirements", {})
    size_value = _SIZE_BUCKET_VALUE.get(candidate.get("estimated_resource_class"), 0.0)
    return {
        "license_clarity": 1.0 if license_.get("status") == "clear" else 0.0,
        "immutable_source_completeness": (
            1.0 if candidate.get("commit_sha") and candidate.get("tree_sha")
            else 0.5 if candidate.get("commit_sha") else 0.0
        ),
        "offline_execution_feasibility": 0.0 if net.get("required_during_untrusted_execution") else 1.0,
        "dependency_lock_quality": 1.0 if candidate.get("dependency_lock", {}).get("present") else 0.3,
        "build_detector_confidence": 0.0 if project.get("ambiguous") else 1.0,
        "expected_family_coverage": 1.0 if candidate.get("expected_log_family") else 0.0,
        "expected_ecosystem_coverage": 1.0 if candidate.get("ecosystem") else 0.0,
        "expected_size_bucket": size_value,
        "security_risk_inverse": max(0.0, 1.0 - 0.25 * len(candidate.get("security_flags", []))),
        "estimated_ci_resource_cost_inverse": size_value,
        "reproducibility_expectation": _REPRO_VALUE.get(candidate.get("reproducibility_class"), 0.0),
        "source_provenance_completeness": (
            1.0 if candidate.get("origin_kind") and candidate.get("evidence_references") else 0.5
        ),
    }


def tie_break_value(candidate: dict) -> str:
    payload = f"{candidate.get('candidate_id', '')}{candidate.get('commit_sha', '')}".encode()
    return hashlib.sha256(payload).hexdigest()


def score_candidate(candidate: dict, policy: dict | None = None) -> dict:
    policy = policy or load_policy()
    assert_no_forbidden_markers(candidate, policy)
    values = _feature_values(candidate)
    breakdown = []
    total_weight = 0.0
    weighted_sum = 0.0
    for feature in policy["features"]:
        key, weight = feature["key"], feature["weight"]
        value = values[key]
        breakdown.append({"feature": key, "value": value, "weight": weight, "contribution": value * weight})
        weighted_sum += value * weight
        total_weight += weight
    final_score = weighted_sum / total_weight if total_weight else 0.0
    return {
        "candidate_id": candidate.get("candidate_id"),
        "policy_version": policy["policy_version"],
        "breakdown": breakdown,
        "final_score": final_score,
        "quota_group": candidate.get(policy["quota_group_dimension"]),
        "tie_break": tie_break_value(candidate),
    }


def rank_candidates(candidates: list[dict], policy: dict | None = None) -> list[dict]:
    """Eligible candidates only — caller must pre-filter via eligibility.evaluate.
    Ranking is a pure function of (registry, policy): same inputs always
    produce a byte-identical ordering, because ties break on a deterministic
    hash rather than input order or any wall-clock/random value."""
    policy = policy or load_policy()
    scored = [score_candidate(c, policy) for c in candidates]
    scored.sort(key=lambda s: (-s["final_score"], s["tie_break"]))
    groups: dict[str, int] = {}
    for entry in scored:
        group = entry["quota_group"]
        groups[group] = groups.get(group, 0) + 1
        entry["rank_within_group"] = groups[group]
    return scored


if __name__ == "__main__":
    import sys

    from registry import load_registry, eligible_candidates  # noqa: E402

    registry = load_registry(MINER_DIR / "candidate-registry.example.json")
    ranking = rank_candidates(eligible_candidates(registry))
    print(json.dumps(ranking, indent=2), file=sys.stderr)
