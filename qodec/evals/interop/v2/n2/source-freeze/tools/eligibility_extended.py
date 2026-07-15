#!/usr/bin/env python3
"""N2-C EligibilityPolicy extension — NEW code, not a modification of frozen
N2-B eligibility.py (see n2b_bridge.py for the exact reuse boundary).

Applies to:
  - repository-miner candidates whose ecosystem is
    "infrastructure-or-language-neutral" (N2-B's frozen rule set only
    recognizes the five ecosystems its ToolAdapters cover, so it cannot
    correctly evaluate these — not a bug to fix in N2-B, a scope N2-B was
    never given).
  - the four non-repository origin kinds: native-upstream-ci-log,
    public-runtime-dataset, kernel-or-infrastructure-bot,
    reproducible-research-corpus.

Same design as N2-B's eligibility.py: an ordered RULES list, per-rule
evaluation returning (pass, evidence), first-failure-wins rejection reason,
deterministic given the same candidate. Never consults QODEC/RTK/token-count
fields (schema forbids them outright — see registry.py's
validate_no_forbidden_fields).
"""
from __future__ import annotations

_INFRA_NEUTRAL_ECOSYSTEM = "infrastructure-or-language-neutral"

REPOSITORY_MINER_RULES = [
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
    "no_unbounded_runtime_download",
    "no_ineliminable_pii_or_secret_exposure",
    "has_recognizable_static_entry_point",
]

NON_REPOSITORY_RULES = [
    "has_explicit_license",
    "has_immutable_object_identity",
    "not_mutable_latest_url",
    "redistribution_basis_clear",
    "no_private_credentials_required",
    "no_ineliminable_pii_or_secret_exposure",
    "publisher_identity_present",
]


def _evaluate_repository_miner_rule(rule: str, candidate: dict) -> tuple[bool, str]:
    license_ = candidate.get("license", {})
    ident = candidate.get("source_identity", {})
    if rule == "has_explicit_license":
        ok = license_.get("status") == "clear" and bool(license_.get("spdx"))
        return ok, f"license.status={license_.get('status')!r} spdx={license_.get('spdx')!r}"
    if rule == "has_immutable_commit":
        sha = ident.get("commit_sha") or ""
        ok = bool(sha) and len(sha) == 40
        return ok, f"commit_sha={sha!r}"
    if rule == "not_floating_ref":
        sha = ident.get("commit_sha") or ""
        ok = bool(sha) and len(sha) == 40 and all(c in "0123456789abcdef" for c in sha)
        return ok, f"commit_sha={sha!r}"
    if rule == "redistribution_basis_clear":
        ok = license_.get("status") == "clear" and license_.get("redistribution_allowed") is True
        return ok, f"license.status={license_.get('status')!r} redistribution_allowed={license_.get('redistribution_allowed')!r}"
    if rule == "no_private_credentials_required":
        flags = candidate.get("security_flags", [])
        ok = "requires-private-credentials" not in flags
        return ok, f"security_flags={flags}"
    if rule == "no_private_package_feed_required":
        ok = candidate.get("private_feed_status", "none") == "none"
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
    if rule == "no_unbounded_runtime_download":
        flags = candidate.get("security_flags", [])
        ok = "unbounded-runtime-download" not in flags
        return ok, f"security_flags={flags}"
    if rule == "no_ineliminable_pii_or_secret_exposure":
        flags = candidate.get("security_flags", [])
        ok = "ineliminable-pii-or-secret-exposure" not in flags
        return ok, f"security_flags={flags}"
    if rule == "has_recognizable_static_entry_point":
        entry = candidate.get("project", {}).get("entry_point")
        ambiguous = candidate.get("project", {}).get("ambiguous", False)
        ok = candidate.get("ecosystem") == _INFRA_NEUTRAL_ECOSYSTEM and bool(entry) and not ambiguous
        return ok, f"ecosystem={candidate.get('ecosystem')!r} entry_point={entry!r} ambiguous={ambiguous}"
    raise ValueError(f"unknown repository-miner rule {rule!r}")


def _evaluate_non_repository_rule(rule: str, candidate: dict) -> tuple[bool, str]:
    license_ = candidate.get("license", {})
    ident = candidate.get("source_identity", {})
    publisher = candidate.get("publisher", {})
    if rule == "has_explicit_license":
        ok = license_.get("status") == "clear" and bool(license_.get("spdx"))
        return ok, f"license.status={license_.get('status')!r} spdx={license_.get('spdx')!r}"
    if rule == "has_immutable_object_identity":
        identity_kind = ident.get("identity_kind")
        has_id = bool(ident.get("object_id_or_doi") or ident.get("run_id") or ident.get("original_content_sha256"))
        ok = identity_kind in ("immutable-object-or-doi", "immutable-run-or-artifact") and has_id
        return ok, f"identity_kind={identity_kind!r} has_id={has_id}"
    if rule == "not_mutable_latest_url":
        url = candidate.get("public_canonical_url", "")
        low = url.lower()
        ok = bool(url) and "latest" not in low and not low.endswith("/head")
        return ok, f"public_canonical_url={url!r}"
    if rule == "redistribution_basis_clear":
        ok = license_.get("status") == "clear" and license_.get("redistribution_allowed") is True
        return ok, f"license.status={license_.get('status')!r} redistribution_allowed={license_.get('redistribution_allowed')!r}"
    if rule == "no_private_credentials_required":
        flags = candidate.get("security_flags", [])
        ok = "requires-private-credentials" not in flags
        return ok, f"security_flags={flags}"
    if rule == "no_ineliminable_pii_or_secret_exposure":
        flags = candidate.get("security_flags", [])
        personal = candidate.get("personal_data_review", {})
        ok = ("ineliminable-pii-or-secret-exposure" not in flags
              and personal.get("personal_data_present", False) is not True)
        return ok, f"security_flags={flags} personal_data_present={personal.get('personal_data_present')!r}"
    if rule == "publisher_identity_present":
        ok = bool(publisher.get("identity"))
        return ok, f"publisher.identity={publisher.get('identity')!r}"
    raise ValueError(f"unknown non-repository rule {rule!r}")


def evaluate(candidate: dict) -> dict:
    origin_kind = candidate.get("origin_kind")
    ecosystem = candidate.get("ecosystem")
    if origin_kind == "repository-miner" and ecosystem == _INFRA_NEUTRAL_ECOSYSTEM:
        rules, evaluator = REPOSITORY_MINER_RULES, _evaluate_repository_miner_rule
    elif origin_kind in ("native-upstream-ci-log", "public-runtime-dataset",
                         "kernel-or-infrastructure-bot", "reproducible-research-corpus"):
        rules, evaluator = NON_REPOSITORY_RULES, _evaluate_non_repository_rule
    else:
        raise ValueError(
            f"eligibility_extended.evaluate does not apply to origin_kind={origin_kind!r} "
            f"ecosystem={ecosystem!r} — use n2b_bridge.evaluate_via_n2b instead"
        )
    rule_results = []
    first_failure = None
    for rule in rules:
        passed, evidence = evaluator(rule, candidate)
        rule_results.append({"rule": rule, "pass": passed, "evidence": evidence})
        if not passed and first_failure is None:
            first_failure = rule
    return {
        "candidate_id": candidate.get("candidate_id"),
        "rules": rule_results,
        "eligible": first_failure is None,
        "rejection_reason": first_failure,
    }
