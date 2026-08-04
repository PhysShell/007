"""Task-conditioned projection v0.

Selection is a pure, deterministic function of (gold state, task, budget):

  select an observation iff it is IN FORCE (status current or pending) AND
  AUTHORITATIVE (authority != agent_claim).

That rule alone drops the negative control's superseded beliefs and every
agent_claim, so a superseded observation can never enter the current projection
as if in force. Selected observations are ordered deterministically (by a fixed
kind order, then observation_id) and then subjected to explicit byte and record
budgets. Every omission is recorded with a reason — nothing is dropped silently.

The renderer (context.md) emits only what the structured projection already
contains: no new facts are authored here.
"""
from __future__ import annotations

from .canonical import canonical_bytes, canonical_file_bytes, sha256_hex

PROJECTOR_VERSION = "0"

# Deterministic display order for kinds (does not imply priority of truth).
_KIND_ORDER = [
    "goal",
    "status",
    "decision",
    "evidence",
    "constraint",
    "risk",
    "work_item",
    "unresolved_question",
    "next_action",
]
_KIND_RANK = {k: i for i, k in enumerate(_KIND_ORDER)}


def _in_force_authoritative(o: dict) -> bool:
    return o["status"] in ("current", "pending") and o["authority"] != "agent_claim"


def select(gold: dict, *, byte_budget: int, record_budget: int) -> dict:
    """Return a selection dict: selected records, omitted-with-reasons, budget use."""
    candidates = []
    omitted = []
    for o in gold["observations"]:
        if _in_force_authoritative(o):
            candidates.append(o)
        else:
            if o["authority"] == "agent_claim":
                reason = "authority agent_claim is never authoritative"
            elif o["status"] == "superseded":
                reason = "superseded_by %s" % o.get("superseded_by")
            else:
                reason = "status %s not in force" % o["status"]
            omitted.append({"observation_id": o["observation_id"], "reason": reason})

    candidates.sort(key=lambda o: (_KIND_RANK.get(o["kind"], 99), o["observation_id"]))

    selected = []
    used_bytes = 0
    for o in candidates:
        rec_bytes = len(canonical_bytes(o)) + 1
        if len(selected) + 1 > record_budget or used_bytes + rec_bytes > byte_budget:
            omitted.append({
                "observation_id": o["observation_id"],
                "reason": "budget exceeded (byte_budget=%d, record_budget=%d)"
                          % (byte_budget, record_budget),
            })
            continue
        selected.append(o)
        used_bytes += rec_bytes

    selected_ids = {o["observation_id"] for o in selected}
    relations = [
        r for r in gold["relations"]
        if r["from"] in selected_ids or r["to"] in selected_ids
    ]
    return {
        "selected": selected,
        "selected_ids": sorted(selected_ids),
        "omitted": sorted(omitted, key=lambda x: x["observation_id"]),
        "relations": relations,
        "used_bytes": used_bytes,
    }


def build_context_json(gold: dict, task: dict, sel: dict) -> dict:
    obs_by_id = {o["observation_id"]: o for o in gold["observations"]}
    selected_records = []
    for o in sel["selected"]:
        selected_records.append({
            "observation_id": o["observation_id"],
            "kind": o["kind"],
            "statement": o["statement"],
            "authority": o["authority"],
            "status": o["status"],
            "provenance": o["provenance"],
            "selection_reason": "in-force (%s) authoritative (%s)" % (o["status"], o["authority"]),
        })
    return {
        "schema": "o7.b1.context/v0",
        "fixture_id": gold["fixture_id"],
        "task_id": task["task_id"],
        "cutoff": gold["cutoff"],
        "selected": selected_records,
        "relations": sel["relations"],
        "omitted": sel["omitted"],
    }


def _render_lines(selected, kinds):
    out = []
    for o in selected:
        if o["kind"] in kinds:
            out.append("- %s  _(%s, authority: %s, status: %s)_"
                       % (o["statement"], o["observation_id"], o["authority"], o["status"]))
    return out


def build_context_md(gold: dict, task: dict, sel: dict) -> str:
    selected = sel["selected"]
    lines: list[str] = []
    lines.append("# 007 B1 Task Context — case-0001")
    lines.append("")
    lines.append("> Rendered only from the structured projection. No new facts are authored here.")
    lines.append("")
    lines.append("**Task:** %s" % task["statement"].strip())
    lines.append("")
    lines.append("**Cutoff:** %s" % gold["cutoff"]["identity"])
    lines.append("")

    sections = [
        ("Goal", ["goal"]),
        ("Current state", ["status", "decision", "evidence"]),
        ("Constraints & forbidden actions", ["constraint"]),
        ("Risks", ["risk"]),
        ("Open work / unresolved", ["work_item", "unresolved_question"]),
        ("Next permitted action", ["next_action"]),
    ]
    for title, kinds in sections:
        rendered = _render_lines(selected, kinds)
        if not rendered:
            continue
        lines.append("## %s" % title)
        lines.extend(rendered)
        lines.append("")

    lines.append("## Evidence pointers")
    for o in selected:
        refs = []
        for pr in o["provenance"]:
            ref = pr["source_id"]
            if pr.get("digest"):
                ref += " " + pr["digest"]
            elif pr.get("source_pointer"):
                ref += " " + pr["source_pointer"]
            refs.append(ref)
        lines.append("- %s: %s" % (o["observation_id"], "; ".join(refs)))
    lines.append("")

    if sel["omitted"]:
        lines.append("## Omitted (with reasons)")
        for om in sel["omitted"]:
            lines.append("- %s: %s" % (om["observation_id"], om["reason"]))
        lines.append("")
    return "\n".join(lines)


def build_context_meta(gold: dict, task_digest: str, questions_digest: str,
                       schema_digest: str, sel: dict, budget: dict,
                       artifact_digests: dict, input_observation_digest: str) -> dict:
    return {
        "schema": "o7.b1.context-meta/v0",
        "fixture_id": gold["fixture_id"],
        "cutoff_identity": gold["cutoff"]["identity"],
        "task_digest": task_digest,
        "questions_digest": questions_digest,
        "schema_version": "o7.b1.state-observables/v0",
        "schema_digest": schema_digest,
        "projector_version": PROJECTOR_VERSION,
        "input_observation_digest": input_observation_digest,
        "budget": budget,
        "selected_count": len(sel["selected"]),
        "omitted_count": len(sel["omitted"]),
        "used_bytes": sel["used_bytes"],
        "output_artifact_digests": artifact_digests,
    }
