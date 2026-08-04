"""Task-dependent projection v0.

Selection is a pure deterministic function of

    gold state + task + selector contract/version + budget

and the task materially drives it: filtering (topic relevance and exclusion),
ranking (required/preferred topic weight and required kinds) and therefore what
survives the budget. Two different tasks over the SAME gold state produce
different projections — `tests/test_task_dependence.py` asserts exactly that,
and the committed per-task artifacts under `results/` show it on the real
fixture.

All selection rules, the fail-closed vocabulary checks and the anchor ceiling
live in `selector.py` (`o7.b1.selector/v0`); this module wires them to the
rendered artifacts and records the contract's digest so a projection can be
traced back to the exact logic that produced it.

History: before the corrective round recorded in `#issuecomment-5182632187`,
`select()` took only (gold, byte_budget, record_budget) while documenting itself
as task-conditioned. It selected every in-force authoritative observation, so
recall was tautological. That is fixed here; the metric naming and the validity
checks were fixed alongside it in `evaluate.py` and `validity.py`.

The renderer (context.md) emits only what the structured projection already
contains: no new facts are authored here.
"""
from __future__ import annotations

from . import selector as sl
from . import validity as vl
from .canonical import canonical_bytes

PROJECTOR_VERSION = "1"


def select(gold: dict, task: dict, *, byte_budget: int, record_budget: int) -> dict:
    """Return a selection dict: selected records, omitted-with-reasons, budget use.

    Raises `selector.SelectorError` if the task's selector block cannot be
    interpreted exactly (unknown field, unknown topic, bad anchor, ...).
    """
    spec = sl.parse_selectors(task, gold)
    candidates, omitted = sl.classify(gold, spec)

    selected: list[dict] = []
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

    sl.enforce_anchor_ratio(selected, spec)

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
        "selector_spec": spec,
        "selector_ref": sl.selector_contract_ref(task),
        "scores": {o["observation_id"]: sl.score(o, spec) for o in selected},
    }


def validate(gold: dict, sel: dict, *, byte_budget: int, record_budget: int) -> dict:
    """Projection-validity findings for this selection (see `validity.py`)."""
    return vl.check_projection(
        gold, sel["selector_spec"], sel["selected"], sel["used_bytes"],
        {"byte_budget": byte_budget, "record_budget": record_budget})


def build_context_json(gold: dict, task: dict, sel: dict) -> dict:
    spec = sel["selector_spec"]
    selected_records = []
    for o in sel["selected"]:
        topics = set(o.get("topics") or [])
        why = []
        matched_req = sorted(topics & set(spec["required_topics"]))
        matched_pref = sorted(topics & set(spec["preferred_topics"]))
        if matched_req:
            why.append("required topic(s) %s" % ",".join(matched_req))
        if matched_pref:
            why.append("preferred topic(s) %s" % ",".join(matched_pref))
        if o["kind"] in spec["required_kinds"]:
            why.append("required kind %s" % o["kind"])
        if o["observation_id"] in spec["anchor_observation_ids"]:
            why.append("explicit anchor: %s" % spec["anchor_rationale"][o["observation_id"]])
        selected_records.append({
            "observation_id": o["observation_id"],
            "kind": o["kind"],
            "statement": o["statement"],
            "authority": o["authority"],
            "status": o["status"],
            "topics": sorted(topics),
            "provenance": o["provenance"],
            "selection_score": sel["scores"][o["observation_id"]],
            "selection_reason": "; ".join(why) if why else "eligible",
        })
    return {
        "schema": "o7.b1.context/v0",
        "fixture_id": gold["fixture_id"],
        "task_id": task["task_id"],
        "cutoff": gold["cutoff"],
        "selector": sel["selector_ref"],
        "selectors": spec,
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
    spec = sel["selector_spec"]
    lines: list[str] = []
    lines.append("# 007 B1 Task Context — %s" % gold["fixture_id"])
    lines.append("")
    lines.append("> Rendered only from the structured projection. No new facts are authored here.")
    lines.append("")
    lines.append("**Task:** %s" % task["statement"].strip())
    lines.append("")
    lines.append("**Task id:** %s" % task["task_id"])
    lines.append("")
    lines.append("**Selector:** %s (impl %s)"
                 % (sel["selector_ref"]["selector_id"],
                    sel["selector_ref"]["selector_impl_digest"]))
    lines.append("")
    lines.append("**Required topics:** %s" % (", ".join(spec["required_topics"]) or "-"))
    lines.append("")
    lines.append("**Preferred topics:** %s" % (", ".join(spec["preferred_topics"]) or "-"))
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


def build_context_meta(gold: dict, task: dict, task_digest: str, questions_digest: str,
                       schema_digest: str, sel: dict, budget: dict,
                       artifact_digests: dict, input_observation_digest: str,
                       validity: dict) -> dict:
    return {
        "schema": "o7.b1.context-meta/v0",
        "fixture_id": gold["fixture_id"],
        "task_id": task["task_id"],
        "cutoff_identity": gold["cutoff"]["identity"],
        "task_digest": task_digest,
        "questions_digest": questions_digest,
        "schema_version": "o7.b1.state-observables/v0",
        "schema_digest": schema_digest,
        "projector_version": PROJECTOR_VERSION,
        "selector": sel["selector_ref"],
        "input_observation_digest": input_observation_digest,
        "budget": budget,
        "selected_count": len(sel["selected"]),
        "omitted_count": len(sel["omitted"]),
        "used_bytes": sel["used_bytes"],
        "projection_validity": validity,
        "output_artifact_digests": artifact_digests,
    }
