#!/usr/bin/env python3
"""Bridges N2-C candidates into frozen N2-B's exact eligibility/scoring shape,
so those modules run completely UNCHANGED (section 12: "use frozen N2-B
EligibilityPolicy, CandidateScorer, QuotaAwareSelectionPlanner, stable SHA256
tie-break").

Reuse boundary (documented here because it is load-bearing, not incidental):

- N2-B's frozen `eligibility.py` has exactly one ecosystem-restricted rule,
  `supported_build_entry_point`, which only recognizes the five ecosystems
  N2-B's ToolAdapters cover (dotnet, rust, python, jvm-maven, jvm-gradle).
  N2-C adds a sixth ecosystem, "infrastructure-or-language-neutral", that
  N2-B was never designed to inspect, plus four origin kinds
  (native-upstream-ci-log, public-runtime-dataset, kernel-or-infrastructure-bot,
  reproducible-research-corpus) that have no "commit_sha"/git-repository shape
  at all. Rather than editing frozen N2-B code to widen its ecosystem enum or
  invent a notion of "commit" for a dataset DOI, N2-C runs frozen
  `eligibility.evaluate` UNCHANGED only where it actually applies —
  repository-miner candidates in one of N2-B's five supported ecosystems —
  and uses its own `eligibility_extended.py` (new N2-C code, same hard-reject
  design pattern) for everything N2-B's frozen scope never covered.
- N2-B's frozen `scorer.py` (`score_candidate`/`rank_candidates`/
  `tie_break_value`) and `quota_planner.py` (`plan_selection`) have NO
  ecosystem or origin-kind restriction at all — every feature is a generic
  dict-field read with a safe default. These run UNCHANGED for every N2-C
  candidate regardless of origin kind or ecosystem; no bridging is needed
  for scoring, only field-name projection (this module's `to_n2b_shape`).
"""
from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

SOURCE_FREEZE_DIR = Path(__file__).resolve().parents[1]
N2_DIR = SOURCE_FREEZE_DIR.parent
MINER_TOOLS = N2_DIR / "miner" / "tools"


def _load_frozen_module(unique_name: str, file_path: Path):
    """Loads a frozen N2-B module under a private module name via
    importlib, bypassing sys.path/sys.modules name collisions entirely —
    N2-C's own tools/eligibility.py and tools/scorer.py would otherwise
    shadow N2-B's identically-named modules the moment either is imported
    first, regardless of sys.path insertion order (sys.modules caches by
    bare name, not by resolved file path)."""
    if unique_name in sys.modules:
        return sys.modules[unique_name]
    spec = importlib.util.spec_from_file_location(unique_name, file_path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[unique_name] = module
    spec.loader.exec_module(module)
    return module


n2b_eligibility = _load_frozen_module("n2b_frozen_eligibility", MINER_TOOLS / "eligibility.py")
n2b_scorer = _load_frozen_module("n2b_frozen_scorer", MINER_TOOLS / "scorer.py")

N2B_SUPPORTED_ECOSYSTEMS = {"dotnet", "rust", "python", "jvm-maven", "jvm-gradle"}


def n2b_eligibility_applies(candidate: dict) -> bool:
    """True exactly when N2-B's frozen eligibility rules were designed to
    cover this candidate: a repository-miner source in one of N2-B's five
    supported ecosystems."""
    return (
        candidate.get("origin_kind") in ("repository-miner", "n2a-reference-only")
        and candidate.get("ecosystem") in N2B_SUPPORTED_ECOSYSTEMS
        and candidate.get("source_identity", {}).get("identity_kind") == "git-commit"
    )


def to_n2b_shape(candidate: dict) -> dict:
    """Projects an N2-C candidate into exactly the field names N2-B's frozen
    eligibility.py/scorer.py read. Pure field renaming/reshaping — no new
    semantics, no relaxed values."""
    ident = candidate.get("source_identity", {})
    return {
        "candidate_id": candidate.get("candidate_id"),
        "license": {
            "status": candidate.get("license", {}).get("status"),
            "spdx": candidate.get("license", {}).get("spdx"),
        },
        "commit_sha": ident.get("commit_sha"),
        "tree_sha": ident.get("tree_sha"),
        "ecosystem": candidate.get("ecosystem"),
        "expected_log_family": candidate.get("primary_family"),
        "origin_kind": candidate.get("origin_kind"),
        "project": candidate.get("project", {}),
        "toolchain_request": candidate.get("toolchain_request", {}),
        "dependency_lock": candidate.get("dependency_lock", {}),
        "network_requirements": candidate.get("network_requirements", {}),
        "submodule_status": candidate.get("submodule_status", "none"),
        "git_lfs_status": candidate.get("git_lfs_status", "none"),
        "private_feed_status": candidate.get("private_feed_status", "none"),
        "external_service_requirements": candidate.get("external_service_requirements", []),
        "container_requirements": candidate.get("container_requirements", []),
        "estimated_resource_class": candidate.get("estimated_resource_class"),
        "expected_capture_command_class": candidate.get("expected_capture_command_class", ""),
        "security_flags": candidate.get("security_flags", []),
        "reproducibility_class": candidate.get("reproducibility_class"),
        "evidence_references": candidate.get("evidence_references", []),
    }


def evaluate_via_n2b(candidate: dict) -> dict:
    """Runs frozen N2-B eligibility.evaluate unchanged. Caller must have
    already checked n2b_eligibility_applies(candidate)."""
    return n2b_eligibility.evaluate(to_n2b_shape(candidate))


def score_via_n2b(candidate: dict, policy: dict) -> dict:
    """Runs frozen N2-B scorer.score_candidate unchanged — valid for ANY
    N2-C candidate (no ecosystem/origin-kind restriction in scorer.py), so
    this is called for every eligible candidate, not just n2b-shaped ones.
    `to_n2b_shape` already carries every field score_candidate's
    `_feature_values` reads (license, commit_sha, tree_sha, ecosystem,
    expected_log_family, origin_kind, project, dependency_lock,
    network_requirements, estimated_resource_class, security_flags,
    reproducibility_class, evidence_references) — including origin_kind,
    which is this policy's quota_group_dimension, so no further merging is
    needed. The N2-C-owned `policy` (candidate-selection-policy.json under
    source-freeze/) supplies only weights/quota-group-dimension; frozen
    N2-B code contributes the feature-computation and tie-break formula."""
    return n2b_scorer.score_candidate(to_n2b_shape(candidate), policy)
