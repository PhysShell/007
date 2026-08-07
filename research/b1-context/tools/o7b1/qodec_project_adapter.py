"""B1 <-> qodec-project protocol adapter (Q2).

007 owns the B1 meaning; Qodec owns the generic projection. This adapter is the
only bridge, and it bridges through BYTES: it converts frozen B1 semantics
(gold observations, task selectors, the frozen contract's relation requirements,
a baseline B0 context) into a `qodec-project-request-v0` document, and converts a
`qodec-project-result-v0` document back into a standard B1 `context.json`.

It imports NO Qodec source module and relies on NO Qodec repository path — the
Qodec binary path is supplied by the caller. It does NOT use the C1 control output
as an input (C1 is the comparison control, not an oracle): the request is built
from B0 + gold + contract only, and Qodec computes the closure independently.

B1 eligibility (in-force AND not an agent claim) is computed HERE and handed to
Qodec as a bare `eligible: bool` plus opaque `caller_evidence`. Qodec enforces that
decision; it never sees "current", "agent_claim", topics or authority classes.
"""
from __future__ import annotations

import os

from .canonical import canonical_bytes, sha256_hex

ADAPTER_ID = "o7.b1.qodec-project-arm/v0"
IN_FORCE = ("current", "pending")


def adapter_impl_digest() -> str:
    with open(os.path.abspath(__file__), "rb") as fh:
        return "sha256:" + sha256_hex(fh.read())


def _eligible(obs: dict) -> bool:
    """Frozen B1 admission: in-force and not an agent claim."""
    return obs.get("status") in IN_FORCE and obs.get("authority") != "agent_claim"


def build_request(task_id: str, gold: dict, task_spec: dict, b0_context: dict,
                  byte_budget: int, record_budget: int, request_id: str) -> dict:
    """Map frozen B1 state for one task into a generic qodec-project request.

    Records = every gold observation (candidate universe). base_selected marks the
    B0 baseline. Relations = the gold relation graph. relation_requirements come
    verbatim from the contract (from/kind/endpoint_policy/expected_targets).
    """
    obs_by_id = {o["observation_id"]: o for o in gold["observations"]}
    b0_selected = {o["observation_id"] for o in b0_context.get("selected", []) or []}

    records = []
    for oid in sorted(obs_by_id):
        o = obs_by_id[oid]
        records.append({
            "id": oid,
            "payload": o,
            "eligible": _eligible(o),
            "base_selected": oid in b0_selected,
            "relevance_score": 0.0,
            "caller_evidence": {"status": o.get("status"), "authority": o.get("authority")},
        })

    relations = [{"from": r["from"], "kind": r["kind"], "to": r["to"]}
                 for r in gold.get("relations", [])]

    reqs = []
    for rr in task_spec.get("relation_requirements", []):
        reqs.append({
            "requirement_id": "%s::%s::%s" % (rr["question"], rr["from"], rr["kind"]),
            "from": rr["from"], "kind": rr["kind"],
            "direction": "outgoing", "depth": 1, "match": "all",
            "endpoint_policy": rr["endpoint_policy"],
            "expected_targets": list(rr["gold_derived_targets"]),
        })

    return {
        "schema": "qodec-project-request-v0",
        "request_id": request_id,
        "input_digest": "sha256:" + sha256_hex(b"adapter-informational"),
        "budget": {"semantic_byte_limit": byte_budget, "record_limit": record_budget},
        "records": records,
        "relations": relations,
        "relation_requirements": reqs,
        "required_record_ids": [],
    }


def reconstruct_context(task_id: str, b0_context: dict, result: dict,
                        qodec_producer: dict) -> dict:
    """Turn a qodec-project result back into a standard B1 context.json."""
    selected = []
    for s in sorted(result.get("selected", []), key=lambda x: x["id"]):
        rec = dict(s["payload"])  # exact observation payload
        rec["selection_reason"] = "qodec-project: " + ",".join(s["selection_reason_codes"])
        rec["selection_score"] = 1.0
        selected.append(rec)
    # deterministic: keep selected in the observation order the projector emitted
    selected.sort(key=lambda o: o["observation_id"])
    omitted = [{"observation_id": o["id"], "reason": o["reason_code"]}
               for o in sorted(result.get("omitted", []), key=lambda x: x["id"])]
    relations = [{"from": r["from"], "kind": r["kind"], "to": r["to"]}
                 for r in result.get("relations", [])]
    out = dict(b0_context)
    out["task_id"] = task_id
    out["selected"] = selected
    out["omitted"] = omitted
    out["relations"] = relations
    out["selector"] = {
        "selector_id": ADAPTER_ID,
        "qodec_project_version": qodec_producer.get("version"),
        "qodec_implementation_sha256": qodec_producer.get("implementation_sha256"),
        "adapter_impl_digest": adapter_impl_digest(),
    }
    out["produced_by"] = ADAPTER_ID
    return out


def canonical_context_bytes(context: dict) -> bytes:
    return canonical_bytes(context) + b"\n"
