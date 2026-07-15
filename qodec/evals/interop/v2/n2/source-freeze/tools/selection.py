#!/usr/bin/env python3
"""N2-C selection orchestrator (section 12-13): calls frozen N2-B
quota_planner.plan_selection unchanged against ONE combined
`origin_family_group` dimension (see quota-contract.json's
combined_dimension_rationale for why three separate dimensions overshoot the
greedy algorithm's exact-17 target), then verifies origin_kind/
primary_family/ecosystem distributions post-hoc against the same contract.
Deterministic top-up and alternate-freezing both walk the SAME rank order
the frozen scorer already produced (section 12's "two independent selection
runs ... byte-identical" requirement) — no manual reordering.
"""
from __future__ import annotations

import json
from collections import Counter
from pathlib import Path

import scoring

SOURCE_FREEZE_DIR = Path(__file__).resolve().parent.parent


def load_quota_contract() -> dict:
    return json.loads((SOURCE_FREEZE_DIR / "quota-contract.json").read_text())


def _post_hoc_distribution(candidates_by_id: dict, case_ids: list, field: str) -> dict:
    return dict(Counter(candidates_by_id[cid][field] for cid in case_ids))


def _unmet(actual: dict, targets: dict, exact: bool) -> dict:
    unmet = {}
    for key, target in targets.items():
        got = actual.get(key, 0)
        if (exact and got != target) or (not exact and got < target):
            unmet[key] = {"target": target, "actual": got}
    return unmet


def _reconcile_ecosystem_minimums(primary_ids: list, alternate_ids: list, by_id: dict,
                                   ranked: list, ecosystem_minimums: dict) -> tuple[list, list, list]:
    """Collapsing origin_kind+family into one combined quota dimension
    (see quota-contract.json) guarantees exactly 17 primaries with exact
    origin/family distributions, but ecosystem minimums are then a separate,
    unenforced post-hoc fact — the frozen scorer's generic features don't
    know about ecosystem balance, so a bucket can rank a same-bucket
    candidate of an already-abundant ecosystem above one from a deficient
    ecosystem. This performs a fully deterministic, rule-based fix — not a
    manual reorder — for each ecosystem still short of its minimum: take the
    highest-ranked not-yet-selected same-`origin_family_group`-bucket
    alternate of that ecosystem, and swap out the LOWEST-ranked current
    primary in that same bucket whose own ecosystem has slack (i.e.
    removing it would not itself violate that ecosystem's minimum) —
    preferring to draw slack from whichever ecosystem has the most surplus
    above its minimum, so the swap can never trade one deficit for another.
    Preserves origin_kind/primary_family exactness (swaps stay within a
    bucket) and total primary count (one-for-one swap)."""
    rank_index = {entry["candidate_id"]: i for i, entry in enumerate(ranked)}
    primary_ids = list(primary_ids)
    alternate_ids = list(alternate_ids)
    swaps = []

    actual = Counter(by_id[cid]["ecosystem_quota_group"] for cid in primary_ids)
    deficits = {eco: target - actual.get(eco, 0) for eco, target in ecosystem_minimums.items()
                if actual.get(eco, 0) < target}

    for eco in sorted(deficits, key=lambda e: (-deficits[e], e)):  # largest deficit first, then alphabetical (stable)
        needed = deficits[eco]
        for _ in range(needed):
            candidates_for_eco = sorted(
                (cid for cid in alternate_ids if by_id[cid]["ecosystem_quota_group"] == eco),
                key=lambda cid: rank_index[cid],
            )
            swapped = False
            for cand_id in candidates_for_eco:
                bucket = by_id[cand_id]["origin_family_group"]
                # Prefer a victim whose ecosystem has the MOST slack (spend
                # abundant slack before scarce slack, so an earlier swap in
                # this loop never forecloses a later one), breaking ties by
                # worst rank first.
                same_bucket_primaries = sorted(
                    (pid for pid in primary_ids if by_id[pid]["origin_family_group"] == bucket),
                    key=lambda pid: (
                        -(actual.get(by_id[pid]["ecosystem_quota_group"], 0)
                          - ecosystem_minimums.get(by_id[pid]["ecosystem_quota_group"], 0)),
                        -rank_index[pid],
                    ),
                )
                for victim_id in same_bucket_primaries:
                    victim_eco = by_id[victim_id]["ecosystem_quota_group"]
                    if victim_eco == eco:
                        continue  # swapping like-for-like fixes nothing
                    slack = actual.get(victim_eco, 0) - ecosystem_minimums.get(victim_eco, 0)
                    if slack > 0:
                        primary_ids.remove(victim_id)
                        primary_ids.append(cand_id)
                        alternate_ids.remove(cand_id)
                        alternate_ids.append(victim_id)
                        actual[victim_eco] -= 1
                        actual[eco] = actual.get(eco, 0) + 1
                        swaps.append({
                            "removed_case_id": victim_id, "added_case_id": cand_id,
                            "bucket": bucket, "reason": f"ecosystem-minimum reconciliation for {eco!r}",
                        })
                        swapped = True
                        break
                if swapped:
                    break
    return primary_ids, alternate_ids, swaps


def run_selection(eligible_candidates: list[dict], policy: dict | None = None,
                   quota_contract: dict | None = None) -> dict:
    policy = policy or scoring.load_policy()
    quota_contract = quota_contract or load_quota_contract()
    primary_target = quota_contract["total_new_primary_target"]
    min_alternates = quota_contract["minimum_alternates"]

    ranked = scoring.rank_all(eligible_candidates, policy)
    by_id = {c["candidate_id"]: c for c in eligible_candidates}

    base_plan = scoring.plan_selection(ranked, by_id, quota_contract["quotas"])

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

    ecosystem_swaps = []
    ecosystem_actual = _post_hoc_distribution(by_id, primary_ids, "ecosystem_quota_group")
    if _unmet(ecosystem_actual, quota_contract["ecosystem_minimums"], exact=False):
        primary_ids, alternate_ids, ecosystem_swaps = _reconcile_ecosystem_minimums(
            primary_ids, alternate_ids, by_id, ranked, quota_contract["ecosystem_minimums"],
        )

    origin_kind_actual = _post_hoc_distribution(by_id, primary_ids, "origin_kind")
    family_actual = _post_hoc_distribution(by_id, primary_ids, "primary_family")
    ecosystem_actual = _post_hoc_distribution(by_id, primary_ids, "ecosystem_quota_group")

    unmet_origin = _unmet(origin_kind_actual, quota_contract["origin_kind_targets"], exact=True)
    unmet_family = _unmet(family_actual, quota_contract["primary_family_targets"], exact=True)
    unmet_ecosystem = _unmet(ecosystem_actual, quota_contract["ecosystem_minimums"], exact=False)

    all_quotas_satisfied = (
        len(primary_ids) == primary_target
        and not unmet_origin and not unmet_family and not unmet_ecosystem
    )

    return {
        "status": "FINAL" if all_quotas_satisfied else "QUOTA_CHECK_FAILED",
        "ranked_order": [entry["candidate_id"] for entry in ranked],
        "base_quota_plan": base_plan,
        "primary_case_ids": primary_ids,
        "quota_deficit_topped_up_case_ids": topped_up_from,
        "ecosystem_minimum_reconciliation_swaps": ecosystem_swaps,
        "alternate_case_ids": alternate_ids,
        "alternate_count": len(alternate_ids),
        "minimum_alternates_satisfied": len(alternate_ids) >= min_alternates,
        "origin_kind_distribution": origin_kind_actual,
        "primary_family_distribution": family_actual,
        "ecosystem_distribution": ecosystem_actual,
        "unmet_origin_kind_targets": unmet_origin,
        "unmet_primary_family_targets": unmet_family,
        "unmet_ecosystem_minimums": unmet_ecosystem,
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
