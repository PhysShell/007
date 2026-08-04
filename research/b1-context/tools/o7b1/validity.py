"""Projection validity checks — metrics that CAN fail.

Corrective round `#issuecomment-5182632187` §3/§6: the previous evaluator
computed `unsupported_claim_count` / `contradiction_count` /
`supersession_error_count` from a helper that returned an empty list for every
arm except the negative control. The projection's zeroes were therefore
invariants of arm-specific control flow, not measurements — a projection that
presented an `agent_claim` as authoritative would still have scored zero.

Everything here is computed from the SELECTED RECORDS THEMSELVES, so a
malformed projection produces non-zero counts no matter which arm produced it.
`tests/test_validity.py` builds deliberately malformed projections and asserts
each counter rises.

These are projection-validity metrics. They are deliberately NOT merged into the
structural-coverage family in `evaluate.py`: coverage answers "is this state
represented at all", validity answers "is what we present well-formed". Mixing
them is what let a structural zero read as a quality result.
"""
from __future__ import annotations


def _prov_refs(o: dict) -> list[dict]:
    p = o.get("provenance")
    return p if isinstance(p, list) else []


def _prov_complete(o: dict) -> bool:
    """A record's provenance is complete iff it has at least one ref and every
    ref names a source and pins either a digest or an explicit source pointer."""
    refs = _prov_refs(o)
    if not refs:
        return False
    for pr in refs:
        if not isinstance(pr, dict) or not pr.get("source_id"):
            return False
        if not pr.get("digest") and not pr.get("source_pointer"):
            return False
    return True


def _contradiction_pairs(gold: dict, selected_ids: set[str]) -> list[list[str]]:
    """Both sides of a contradiction present in the same projection.

    Two independent sources of contradiction are checked:
      * supersession: a superseded observation AND its superseder both selected;
      * an explicit `contradicts` relation with both endpoints selected.
    """
    pairs: set[tuple[str, str]] = set()
    for o in gold["observations"]:
        oid = o["observation_id"]
        sb = o.get("superseded_by")
        if o.get("status") == "superseded" and sb:
            if oid in selected_ids and sb in selected_ids:
                pairs.add(tuple(sorted((oid, sb))))
    for rel in gold.get("relations", []):
        if rel.get("kind") == "contradicts":
            a, b = rel.get("from"), rel.get("to")
            if a in selected_ids and b in selected_ids:
                pairs.add(tuple(sorted((a, b))))
    return [list(p) for p in sorted(pairs)]


def check_projection(gold: dict, sel_spec: dict, selected: list[dict],
                     used_bytes: int, budget: dict) -> dict:
    """Return projection-validity findings for an arbitrary selected set.

    `selected` is whatever the projection actually presents — this function does
    not trust it, which is the entire point.
    """
    selected_ids = {o["observation_id"] for o in selected}

    agent_claim_records = sorted(
        o["observation_id"] for o in selected if o.get("authority") == "agent_claim")
    superseded_records = sorted(
        o["observation_id"] for o in selected if o.get("status") == "superseded")
    not_in_force_records = sorted(
        o["observation_id"] for o in selected
        if o.get("status") not in ("current", "pending"))
    incomplete_provenance_records = sorted(
        o["observation_id"] for o in selected if not _prov_complete(o))
    contradiction_pairs = _contradiction_pairs(gold, selected_ids)

    present_kinds = {o.get("kind") for o in selected}
    missing_required_kinds = sorted(
        k for k in sel_spec.get("required_kinds", []) if k not in present_kinds)

    present_topics: set[str] = set()
    for o in selected:
        for t in (o.get("topics") or []):
            present_topics.add(t)
    missing_required_topics = sorted(
        t for t in sel_spec.get("required_topics", []) if t not in present_topics)

    over_records = len(selected) > budget["record_budget"]
    over_bytes = used_bytes > budget["byte_budget"]

    findings = {
        "agent_claim_presented_count": len(agent_claim_records),
        "agent_claim_presented_ids": agent_claim_records,
        "superseded_presented_count": len(superseded_records),
        "superseded_presented_ids": superseded_records,
        "not_in_force_presented_count": len(not_in_force_records),
        "contradiction_pair_count": len(contradiction_pairs),
        "contradiction_pairs": contradiction_pairs,
        "incomplete_provenance_count": len(incomplete_provenance_records),
        "incomplete_provenance_ids": incomplete_provenance_records,
        "missing_required_kinds": missing_required_kinds,
        "missing_required_topics": missing_required_topics,
        "budget_record_overflow": over_records,
        "budget_byte_overflow": over_bytes,
        "presented_record_count": len(selected),
    }
    findings["valid"] = (
        findings["agent_claim_presented_count"] == 0
        and findings["superseded_presented_count"] == 0
        and findings["not_in_force_presented_count"] == 0
        and findings["contradiction_pair_count"] == 0
        and findings["incomplete_provenance_count"] == 0
        and not findings["missing_required_kinds"]
        and not findings["missing_required_topics"]
        and not over_records
        and not over_bytes
    )
    return findings


def presented_provenance_ratio(selected: list[dict]) -> float | None:
    """Fraction of PRESENTED records carrying complete provenance.

    Denominator: the records this projection actually presents. Returns None for
    an empty projection, where the ratio would be undefined rather than 1.0 —
    the old binary `1.0 if carried else 0.0` reported full coverage for an arm
    that had carried a single observation out of sixteen.
    """
    if not selected:
        return None
    good = sum(1 for o in selected if _prov_complete(o))
    return round(good / len(selected), 6)
