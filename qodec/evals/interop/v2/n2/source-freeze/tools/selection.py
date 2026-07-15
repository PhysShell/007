#!/usr/bin/env python3
"""N2-C selection orchestrator (section 12-13): calls frozen N2-B
quota_planner.plan_selection unchanged for the declared per-dimension
minimums, then deterministically tops up to the 17-primary target and
freezes >=8 alternates — both top-up and alternate ordering walk the SAME
rank order plan_selection already produced, so the whole pipeline is a pure
function of (registry, policy, quota-contract): identical inputs always
yield identical primary/alternate lists (section 12's "two independent
selection runs ... byte-identical" requirement).
"""
from __future__ import annotations

import json
from pathlib import Path

import scoring

SOURCE_FREEZE_DIR = Path(__file__).resolve().parent.parent


def load_quota_contract() -> dict:
    return json.loads((SOURCE_FREEZE_DIR / "quota-contract.json").read_text())


def run_selection(eligible_candidates: list[dict], policy: dict | None = None,
                   quota_contract: dict | None = None) -> dict:
    policy = policy or scoring.load_policy()
    quota_contract = quota_contract or load_quota_contract()
    quotas = quota_contract["quotas"]
    primary_target = quota_contract["total_new_primary_target"]
    min_alternates = quota_contract["minimum_alternates"]

    ranked = scoring.rank_all(eligible_candidates, policy)
    by_id = {c["candidate_id"]: c for c in eligible_candidates}

    base_plan = scoring.plan_selection(ranked, by_id, quotas)

    primary_ids = list(base_plan["proposed_selection"])
    remaining_in_rank_order = [cid for cid in base_plan["eligible_alternatives"] if cid not in primary_ids]

    topped_up_from = []
    i = 0
    while len(primary_ids) < primary_target and i < len(remaining_in_rank_order):
        candidate_id = remaining_in_rank_order[i]
        primary_ids.append(candidate_id)
        topped_up_from.append(candidate_id)
        i += 1

    alternate_ids = [cid for cid in remaining_in_rank_order if cid not in primary_ids]

    return {
        "status": "FINAL" if len(primary_ids) == primary_target else "INSUFFICIENT_CANDIDATES",
        "ranked_order": [entry["candidate_id"] for entry in ranked],
        "base_quota_plan": base_plan,
        "primary_case_ids": primary_ids,
        "quota_deficit_topped_up_case_ids": topped_up_from,
        "alternate_case_ids": alternate_ids,
        "alternate_count": len(alternate_ids),
        "minimum_alternates_satisfied": len(alternate_ids) >= min_alternates,
        "unfilled_quotas": base_plan["unfilled_quotas"],
    }


if __name__ == "__main__":
    import sys

    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import registry as registry_mod  # noqa: E402
    import eligibility as eligibility_mod  # noqa: E402

    reg = registry_mod.load_registry(SOURCE_FREEZE_DIR / "candidate-registry.json")
    reports = eligibility_mod.evaluate_registry(reg)
    eligible_ids = {r["candidate_id"] for r in reports if r["eligible"]}
    eligible = [c for c in reg["candidates"] if c["candidate_id"] in eligible_ids]
    result = run_selection(eligible)
    print(json.dumps(result, indent=2), file=sys.stderr)
