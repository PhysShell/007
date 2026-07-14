#!/usr/bin/env python3
"""N2-B quota-aware selection planning (section 7).

Produces a PROVISIONAL selection proposal against declared quota targets. It
never fixes a final case set — that decision, and any QODEC/RTK evaluation
of the result, is explicitly out of scope for N2-B (it belongs to N2-C).
"""
from __future__ import annotations

QUOTA_DIMENSIONS = (
    "ecosystem",
    "expected_log_family",
    "origin_kind",
    "estimated_resource_class",
    "reproducibility_class",
)


def _candidate_dimension_value(candidate: dict, dimension: str):
    if dimension == "repository_miner":
        return "repository-miner"
    return candidate.get(dimension)


def plan_selection(ranked_candidates: list[dict], candidates_by_id: dict, quotas: dict) -> dict:
    """`ranked_candidates` is scorer.rank_candidates() output (already sorted
    deterministically). `quotas` maps a dimension name -> {value: target_count}.
    Selection walks the ranking in order and fills each dimension's quotas
    greedily; a candidate can fill more than one dimension's quota at once.
    The walk order is exactly the deterministic rank order, so the same
    (registry, policy, quotas) triple always yields a byte-identical trace.
    """
    remaining = {
        dim: dict(targets) for dim, targets in quotas.items()
    }
    proposed = []
    trace = []
    for entry in ranked_candidates:
        candidate = candidates_by_id[entry["candidate_id"]]
        contributes = {}
        for dim, targets in remaining.items():
            value = _candidate_dimension_value(candidate, dim)
            if value in targets and targets[value] > 0:
                contributes[dim] = value
        if contributes:
            for dim, value in contributes.items():
                remaining[dim][value] -= 1
            proposed.append(entry["candidate_id"])
            trace.append({
                "candidate_id": entry["candidate_id"],
                "action": "proposed",
                "final_score": entry["final_score"],
                "filled_quotas": contributes,
            })
        else:
            trace.append({
                "candidate_id": entry["candidate_id"],
                "action": "held-as-alternative",
                "final_score": entry["final_score"],
                "reason": "no unfilled quota dimension matches this candidate",
            })

    unfilled = {
        dim: {value: count for value, count in targets.items() if count > 0}
        for dim, targets in remaining.items()
    }
    unfilled = {dim: vals for dim, vals in unfilled.items() if vals}

    alternatives = [t["candidate_id"] for t in trace if t["action"] == "held-as-alternative"]

    return {
        "status": "PROVISIONAL",
        "notes": [
            "NOT A CORPUS FREEZE",
            "NO QODEC/RTK EVALUATION PERFORMED",
        ],
        "proposed_selection": proposed,
        "unfilled_quotas": unfilled,
        "eligible_alternatives": alternatives,
        "selection_trace": trace,
    }


if __name__ == "__main__":
    import json
    import sys
    from pathlib import Path

    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from registry import load_registry, eligible_candidates  # noqa: E402
    from scorer import rank_candidates  # noqa: E402

    miner_dir = Path(__file__).resolve().parents[1]
    registry = load_registry(miner_dir / "candidate-registry.example.json")
    elig = eligible_candidates(registry)
    ranked = rank_candidates(elig)
    by_id = {c["candidate_id"]: c for c in elig}
    quotas = {"ecosystem": {"dotnet": 1, "rust": 1, "python": 1, "jvm-maven": 1, "jvm-gradle": 1}}
    print(json.dumps(plan_selection(ranked, by_id, quotas), indent=2), file=sys.stderr)
