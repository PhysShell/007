"""Deterministic relation-closure control producer (Q1 cell C1).

This is an EXPERIMENTAL producer, NOT selector v0. It takes a baseline projection
(B0 task contexts) plus the frozen gold state and the frozen evaluation contract's
explicit relation requirements, and closes the projection over exactly those
requirements: it materializes the requirement *sources* (and, for
`all_current_targets_materialized`, the current targets) and adds the exact gold
edges, so relation-aware evaluation can find the witnesses that plain relevance
selection dropped.

It is generic: there is no case literal here — every observation id, edge and
requirement comes from the supplied gold state and contract. The case-specific
consequence (which witnesses get added) is a *result* of running the algorithm,
never a hard-coded input.

Boundaries honoured:
  * selector v0 and projector v0 are NOT imported or modified;
  * no observation is added that is not demanded by a frozen relation requirement;
  * no fabricated edge is ever added;
  * a newly materialized observation must exist in gold, be current/pending, not be
    an agent_claim, and it keeps its exact statement/authority/status/topics/provenance;
  * for `edge_witness` a stale target is NOT selected — it stays a relation-only
    reference; the source is materialized;
  * additions are ordered deterministically (contract-requirement order, then id).
"""
from __future__ import annotations

import hashlib
import os

from .canonical import canonical_bytes, sha256_hex

PRODUCER_ID = "o7.b1.relation-closed-control/v0"
PRODUCER_VERSION = "0"
IN_FORCE = ("current", "pending")


def producer_impl_digest() -> str:
    with open(os.path.abspath(__file__), "rb") as fh:
        return "sha256:" + sha256_hex(fh.read())


def _gold_index(gold: dict) -> dict:
    obs = {o["observation_id"]: o for o in gold["observations"]}
    edges: dict[tuple[str, str], set] = {}
    for r in gold.get("relations", []):
        edges.setdefault((r["from"], r["kind"]), set()).add(r["to"])
    return {"obs": obs, "edges": edges}


def _eligible_to_materialize(o: dict | None) -> bool:
    return bool(o) and o.get("status") in IN_FORCE and o.get("authority") != "agent_claim"


class ClosureError(Exception):
    pass


def close_task(task_id: str, requirements: list[dict], gold_idx: dict,
               b0_context: dict) -> dict:
    """Return a relation-closed context for one task. `requirements` is the
    contract's `relation_requirements` for this task, in contract order."""
    obs = gold_idx["obs"]
    selected = list(b0_context.get("selected", []) or [])
    selected_ids = {o["observation_id"] for o in selected}
    omitted = list(b0_context.get("omitted", []) or [])
    relations = list(b0_context.get("relations", []) or [])
    rel_set = {(r["from"], r["kind"], r["to"]) for r in relations}

    additions = []          # (req_index, oid, reason), sorted below
    added_edges = []        # (from, kind, to) to add, in deterministic order
    seen_add = set()

    def queue_materialize(req_idx, oid, reason):
        if oid in selected_ids or oid in seen_add:
            return
        o = obs.get(oid)
        if not _eligible_to_materialize(o):
            raise ClosureError(
                "%s: cannot materialize %s (status=%s authority=%s)"
                % (task_id, oid, (o or {}).get("status"), (o or {}).get("authority")))
        seen_add.add(oid)
        additions.append((req_idx, oid, reason))

    for ridx, rr in enumerate(requirements):
        frm, kind, policy = rr["from"], rr["kind"], rr["endpoint_policy"]
        gold_targets = set(gold_idx["edges"].get((frm, kind), set()))
        # rule 2: declared targets must equal recomputed gold edges
        if set(rr["gold_derived_targets"]) != gold_targets:
            raise ClosureError("%s: requirement %s/%s targets disagree with gold"
                               % (task_id, frm, kind))
        # rule 11: only exact gold edges are added (never fabricated)
        for to in sorted(gold_targets):
            edge = (frm, kind, to)
            if edge not in rel_set:
                rel_set.add(edge)
                added_edges.append((ridx, edge))
        # rule 4/5: materialize the source observation
        queue_materialize(ridx, frm, "relation-closure: witness source for %s/%s" % (frm, kind))
        if policy == "all_current_targets_materialized":
            # rule 6: materialize every current/pending target; fail closed otherwise
            for to in sorted(gold_targets):
                t = obs.get(to)
                if t is None:
                    raise ClosureError("%s: all_current target %s absent from gold" % (task_id, to))
                if t.get("status") in IN_FORCE:
                    queue_materialize(ridx, to, "relation-closure: current target of %s/%s" % (frm, kind))
                # a target that is not in force under all_current is a contract/gold
                # disagreement; the evaluator's gate-10 flags it. We do not select it.
        # edge_witness: stale targets stay relation-only (not selected). Nothing to do.

    # rule 12: deterministic ordering by contract-requirement order, then observation ID.
    additions_sorted = sorted(additions, key=lambda a: (a[0], a[1]))
    new_selected = list(selected)
    add_ids = set()
    materialized_ids = []
    for _ridx, oid, reason in additions_sorted:
        rec = dict(obs[oid])  # exact gold fields
        rec["selection_reason"] = reason
        rec["selection_score"] = 1.0
        new_selected.append(rec)
        add_ids.add(oid)
        materialized_ids.append(oid)

    new_omitted = [o for o in omitted if o.get("observation_id") not in add_ids]
    added_edge_tuples = sorted({e for _, e in added_edges})
    existing_edge_tuples = [(r["from"], r["kind"], r["to"]) for r in relations]
    all_edges = existing_edge_tuples + [e for e in added_edge_tuples
                                        if e not in set(existing_edge_tuples)]
    new_relations = [{"from": f, "kind": k, "to": t} for (f, k, t) in all_edges]

    out = dict(b0_context)
    out["selected"] = new_selected
    out["omitted"] = new_omitted
    out["relations"] = new_relations
    out["selector"] = {"selector_id": PRODUCER_ID, "selector_version": PRODUCER_VERSION,
                       "selector_impl_digest": producer_impl_digest()}
    out["produced_by"] = PRODUCER_ID
    return {"context": out, "materialized": materialized_ids,
            "added_edges": added_edge_tuples,
            "used_bytes": sum(len(canonical_bytes(o)) + 1 for o in new_selected),
            "selected_count": len(new_selected)}


def render_md(context: dict) -> str:
    lines = ["# relation-closed context: %s" % context.get("task_id", ""),
             "producer: %s" % PRODUCER_ID, ""]
    lines.append("## selected (%d)" % len(context.get("selected", [])))
    for o in context.get("selected", []):
        lines.append("- %s [%s/%s] %s" % (o["observation_id"], o.get("status"),
                                          o.get("authority"), o.get("statement", "")))
    lines.append("")
    lines.append("## relations (%d)" % len(context.get("relations", [])))
    for r in context.get("relations", []):
        lines.append("- %s -%s-> %s" % (r["from"], r["kind"], r["to"]))
    lines.append("")
    return "\n".join(lines) + "\n"
