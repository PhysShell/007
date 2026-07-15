#!/usr/bin/env python3
"""N2-C scoring + quota-aware selection: thin wrappers that call frozen N2-B
`scorer.py`/`quota_planner.py` completely unchanged (section 12).

`rank_all` pre-maps every candidate into N2-B's field shape via
n2b_bridge.to_n2b_shape (candidate_id and origin_kind survive the mapping
unchanged), then calls frozen `scorer.rank_candidates` directly — so the
feature computation, weighting, sort, and tie-break formula are all
untouched N2-B code.

`plan_selection` is re-exported with NO wrapping at all: quota_planner.py has
no ecosystem/origin-kind restriction, so it runs on N2-C's original
(non-shaped) candidate dicts as-is, reading whatever quota dimension field
names the caller's quota contract names (ecosystem, primary_family,
origin_kind, expected_size_bucket are all present directly on N2-C
candidates with those exact names).
"""
from __future__ import annotations

import sys
from pathlib import Path

SOURCE_FREEZE_DIR = Path(__file__).resolve().parents[1]
N2_DIR = SOURCE_FREEZE_DIR.parent
MINER_TOOLS = N2_DIR / "miner" / "tools"

sys.path.insert(0, str(Path(__file__).resolve().parent))
import n2b_bridge  # noqa: E402

# Reuse n2b_bridge's collision-safe importlib loader — avoids loading
# scorer.py twice under two different sys.modules names.
n2b_scorer = n2b_bridge.n2b_scorer
n2b_quota_planner = n2b_bridge._load_frozen_module("n2b_frozen_quota_planner", MINER_TOOLS / "quota_planner.py")

plan_selection = n2b_quota_planner.plan_selection  # frozen, unmodified, re-exported


def load_policy() -> dict:
    import json
    return json.loads((SOURCE_FREEZE_DIR / "candidate-selection-policy.json").read_text())


def rank_all(candidates: list[dict], policy: dict | None = None) -> list[dict]:
    policy = policy or load_policy()
    shaped = [n2b_bridge.to_n2b_shape(c) for c in candidates]
    return n2b_scorer.rank_candidates(shaped, policy)
