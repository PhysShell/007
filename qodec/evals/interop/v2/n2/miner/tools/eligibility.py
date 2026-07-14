#!/usr/bin/env python3
"""N2-B EligibilityPolicy.

Hard rejection rules, applied to every candidate BEFORE scoring (section 5).
Never consults QODEC, RTK, or token-count fields — those aren't in the
candidate schema at all, so a candidate carrying one is itself a schema
violation the caller should reject before this runs (see
test_eligibility.test_qodec_field_in_candidate_is_rejected et al., which
assert the *registry* schema has no such properties).
"""
from __future__ import annotations

RULES = [
    "has_explicit_license",
    "has_immutable_commit",
    "not_floating_ref",
    "redistribution_basis_clear",
    "no_private_credentials_required",
    "no_private_package_feed_required",
    "no_docker_socket_required",
    "no_privileged_execution_required",
    "no_uncontrolled_network_during_untrusted_execution",
    "no_mandatory_external_service",
    "no_mandatory_database_without_local_fixture",
    "no_unbounded_runtime_download",
    "no_ineliminable_pii_or_secret_exposure",
    "supported_build_entry_point",
]

_SUPPORTED_ECOSYSTEMS = {"dotnet", "rust", "python", "jvm-maven", "jvm-gradle"}


def _evaluate_rule(rule: str, candidate: dict) -> tuple[bool, str]:
    license_ = candidate.get("license", {})
    if rule == "has_explicit_license":
        ok = license_.get("status") == "clear" and bool(license_.get("spdx"))
        return ok, f"license.status={license_.get('status')!r} spdx={license_.get('spdx')!r}"
    if rule == "has_immutable_commit":
        sha = candidate.get("commit_sha", "")
        ok = bool(sha) and len(sha) == 40
        return ok, f"commit_sha={sha!r}"
    if rule == "not_floating_ref":
        # A candidate whose commit_sha field actually holds a branch/tag name
        # (not a full 40-hex SHA) is exactly the floating-ref case; the schema
        # already constrains the field's *shape*, this rule re-asserts intent.
        sha = candidate.get("commit_sha", "")
        ok = bool(sha) and len(sha) == 40 and all(c in "0123456789abcdef" for c in sha)
        return ok, f"commit_sha={sha!r}"
    if rule == "redistribution_basis_clear":
        ok = license_.get("status") == "clear"
        return ok, f"license.status={license_.get('status')!r}"
    if rule == "no_private_credentials_required":
        flags = candidate.get("security_flags", [])
        ok = "requires-private-credentials" not in flags
        return ok, f"security_flags={flags}"
    if rule == "no_private_package_feed_required":
        ok = candidate.get("private_feed_status") == "none"
        return ok, f"private_feed_status={candidate.get('private_feed_status')!r}"
    if rule == "no_docker_socket_required":
        reqs = candidate.get("container_requirements", [])
        ok = "docker-socket" not in reqs
        return ok, f"container_requirements={reqs}"
    if rule == "no_privileged_execution_required":
        flags = candidate.get("security_flags", [])
        ok = "requires-privileged-execution" not in flags
        return ok, f"security_flags={flags}"
    if rule == "no_uncontrolled_network_during_untrusted_execution":
        net = candidate.get("network_requirements", {})
        ok = not net.get("required_during_untrusted_execution", False)
        return ok, f"network_requirements={net}"
    if rule == "no_mandatory_external_service":
        svcs = candidate.get("external_service_requirements", [])
        ok = len(svcs) == 0
        return ok, f"external_service_requirements={svcs}"
    if rule == "no_mandatory_database_without_local_fixture":
        flags = candidate.get("security_flags", [])
        ok = "requires-database-without-local-fixture" not in flags
        return ok, f"security_flags={flags}"
    if rule == "no_unbounded_runtime_download":
        flags = candidate.get("security_flags", [])
        ok = "unbounded-runtime-download" not in flags
        return ok, f"security_flags={flags}"
    if rule == "no_ineliminable_pii_or_secret_exposure":
        flags = candidate.get("security_flags", [])
        ok = "ineliminable-pii-or-secret-exposure" not in flags
        return ok, f"security_flags={flags}"
    if rule == "supported_build_entry_point":
        entry = candidate.get("project", {}).get("entry_point")
        ambiguous = candidate.get("project", {}).get("ambiguous", False)
        ok = candidate.get("ecosystem") in _SUPPORTED_ECOSYSTEMS and bool(entry) and not ambiguous
        return ok, f"ecosystem={candidate.get('ecosystem')!r} entry_point={entry!r} ambiguous={ambiguous}"
    raise ValueError(f"unknown eligibility rule {rule!r}")


def evaluate(candidate: dict) -> dict:
    """Returns an eligibility report: candidate_id, per-rule pass/fail with
    evidence, final result, and exact rejection reason (first failing rule,
    in declared rule order — deterministic)."""
    rule_results = []
    first_failure = None
    for rule in RULES:
        passed, evidence = _evaluate_rule(rule, candidate)
        rule_results.append({"rule": rule, "pass": passed, "evidence": evidence})
        if not passed and first_failure is None:
            first_failure = rule
    return {
        "candidate_id": candidate.get("candidate_id"),
        "rules": rule_results,
        "eligible": first_failure is None,
        "rejection_reason": first_failure,
    }


def evaluate_registry(registry: dict) -> list[dict]:
    return [evaluate(c) for c in registry["candidates"]]


if __name__ == "__main__":
    import json
    import sys as _sys
    from pathlib import Path as _Path

    registry = json.loads((_Path(__file__).resolve().parents[1] / "candidate-registry.example.json").read_text())
    print(json.dumps(evaluate_registry(registry), indent=2), file=_sys.stderr)
