"""Versioned selector contract `o7.b1.selector/v0`.

This module is the ONLY place that decides which observations a given task
sees. It exists because the previous projector selected every in-force
authoritative observation regardless of the task, while documenting itself as
task-conditioned (corrective round, `#issuecomment-5182632187` §1).

Separation of concerns
----------------------
* An observation carries `topics`: TASK-INDEPENDENT classification. A topic says
  what a statement is *about*, never how much it matters. No global `importance`
  is stored on observations — priority is always relative to a task.
* A task carries `selectors`: which topics/kinds that task requires, prefers or
  excludes. Priority lives here, and only here.

Selection is a pure deterministic function of

    gold state + task + selector contract/version + budget

Rules (v0), applied in this order
---------------------------------
1. ELIGIBILITY (task-independent). An observation is eligible iff its status is
   `current` or `pending` AND its authority is not `agent_claim`. Ineligible
   observations can never enter a projection, whatever the task asks for.
2. EXCLUSION. If the observation shares any topic with `excluded_topics` it is
   omitted, even if it also matches a required topic. Exclusion always wins, so
   a task can hard-deny a topic without having to know what else matches.
3. RELEVANCE. The observation is relevant iff it shares at least one topic with
   `required_topics` or `preferred_topics`. Anything else is omitted with an
   explicit reason — a task never silently drags in unrelated state.
4. ANCHORS. `anchor_observation_ids` may force specific eligible observations in
   regardless of topic match. Anchors are an escape hatch for genuinely
   id-specific needs: each one must carry a rationale in `anchor_rationale`, and
   the contract REFUSES to run if a task tries to drive selection primarily by
   id (see `MAX_ANCHOR_RATIO`) — that would reduce the "selector" to a hand-
   written answer key.
5. SCORE + ORDER. score = 2*|topics ∩ required| + 1*|topics ∩ preferred|
   + 1 if kind ∈ required_kinds else 0, +ANCHOR_BONUS for an anchor. Ties break
   on (kind order, observation_id) so the output is total-ordered and stable.
6. BUDGET. Records are admitted in that order until either budget is reached;
   every rejected record is reported with a budget reason. Nothing is dropped
   silently.

Fail-closed
-----------
Unknown selector keys, unknown topic identifiers, unknown kinds, anchors that
name a missing or ineligible observation, an anchor without a rationale, or an
anchor ratio above `MAX_ANCHOR_RATIO` all raise `SelectorError`. A task that
cannot be interpreted exactly is never approximated.
"""
from __future__ import annotations

import os

from .canonical import canonical_bytes, sha256_hex, sha256_of_file

SELECTOR_ID = "o7.b1.selector/v0"
SELECTOR_VERSION = "0"

#: Keys a task's `selectors` block may contain. Anything else fails closed.
ALLOWED_SELECTOR_KEYS = frozenset({
    "required_topics",
    "preferred_topics",
    "excluded_topics",
    "required_kinds",
    "anchor_observation_ids",
    "anchor_rationale",
})

#: Score added for an explicit anchor, so anchors sort above incidental matches
#: but never above a strong required-topic match plus required-kind.
ANCHOR_BONUS = 1

#: An anchor-driven task is not a selector. If more than this fraction of the
#: selected set comes in *only* because it was named by id, the contract
#: refuses: the task is an answer key, not a query.
MAX_ANCHOR_RATIO = 0.34

# Deterministic display/tie-break order for kinds (not a priority of truth).
KIND_ORDER = [
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
_KIND_RANK = {k: i for i, k in enumerate(KIND_ORDER)}


class SelectorError(Exception):
    """A task's selector block cannot be interpreted exactly."""


def selector_impl_digest() -> str:
    """Digest of THIS file, so a report pins the exact selection logic used."""
    return "sha256:" + sha256_of_file(os.path.abspath(__file__))


def selector_contract_ref(task: dict) -> dict:
    """The digest-bound identity of the contract + the task's own selector block."""
    return {
        "selector_id": SELECTOR_ID,
        "selector_version": SELECTOR_VERSION,
        "selector_impl_digest": selector_impl_digest(),
        "selectors_digest": "sha256:" + sha256_hex(canonical_bytes(task.get("selectors", {}))),
    }


def eligible(o: dict) -> bool:
    """Task-independent eligibility: in force, and never an agent claim."""
    return o["status"] in ("current", "pending") and o["authority"] != "agent_claim"


def _topic_vocabulary(gold: dict) -> set[str]:
    vocab = gold.get("topic_vocabulary")
    if not isinstance(vocab, list) or not vocab:
        raise SelectorError(
            "gold state has no topic_vocabulary; the selector contract cannot "
            "validate topic identifiers and refuses to guess")
    return {str(t) for t in vocab}


def parse_selectors(task: dict, gold: dict) -> dict:
    """Validate + normalize a task's selector block. Raises SelectorError."""
    sel = task.get("selectors")
    if sel is None:
        raise SelectorError(
            "task %r has no `selectors` block; under %s a task must state what it "
            "requires — an absent selector is not treated as 'select everything'"
            % (task.get("task_id"), SELECTOR_ID))
    if not isinstance(sel, dict):
        raise SelectorError("task selectors must be a mapping")

    unknown = sorted(set(sel) - ALLOWED_SELECTOR_KEYS)
    if unknown:
        raise SelectorError(
            "unknown selector field(s) %s; %s fails closed rather than ignoring "
            "fields it does not implement" % (unknown, SELECTOR_ID))

    vocab = _topic_vocabulary(gold)
    out: dict = {}
    for key in ("required_topics", "preferred_topics", "excluded_topics"):
        vals = sel.get(key) or []
        if not isinstance(vals, list):
            raise SelectorError("%s must be a list" % key)
        bad = sorted({v for v in vals if v not in vocab})
        if bad:
            raise SelectorError(
                "%s references topic id(s) %s absent from the gold state's own "
                "topic_vocabulary" % (key, bad))
        out[key] = sorted(set(vals))

    kinds = sel.get("required_kinds") or []
    if not isinstance(kinds, list):
        raise SelectorError("required_kinds must be a list")
    bad_kinds = sorted({k for k in kinds if k not in _KIND_RANK})
    if bad_kinds:
        raise SelectorError("required_kinds references unknown kind(s) %s" % bad_kinds)
    out["required_kinds"] = sorted(set(kinds))

    if not out["required_topics"] and not out["preferred_topics"]:
        raise SelectorError(
            "task declares neither required_topics nor preferred_topics; that "
            "would select nothing, which is never a useful projection")

    anchors = sel.get("anchor_observation_ids") or []
    if not isinstance(anchors, list):
        raise SelectorError("anchor_observation_ids must be a list")
    anchors = sorted(set(anchors))
    rationale = sel.get("anchor_rationale") or {}
    if not isinstance(rationale, dict):
        raise SelectorError("anchor_rationale must be a mapping of observation_id -> reason")
    by_id = {o["observation_id"]: o for o in gold["observations"]}
    for a in anchors:
        if a not in by_id:
            raise SelectorError("anchor %s names no observation in this gold state" % a)
        if not eligible(by_id[a]):
            raise SelectorError(
                "anchor %s is not eligible (status %s, authority %s); anchors may "
                "not resurrect ineligible state"
                % (a, by_id[a]["status"], by_id[a]["authority"]))
        if not str(rationale.get(a, "")).strip():
            raise SelectorError(
                "anchor %s has no anchor_rationale; an id-level override must say "
                "why the topic vocabulary is insufficient" % a)
    out["anchor_observation_ids"] = anchors
    out["anchor_rationale"] = {a: str(rationale[a]).strip() for a in anchors}
    return out


def _topics_of(o: dict) -> set[str]:
    t = o.get("topics")
    if not isinstance(t, list) or not t:
        raise SelectorError(
            "observation %s carries no topics; under %s every observation must be "
            "classified or selection cannot be task-dependent"
            % (o.get("observation_id"), SELECTOR_ID))
    return set(t)


def score(o: dict, sel: dict) -> int:
    topics = _topics_of(o)
    s = 2 * len(topics & set(sel["required_topics"]))
    s += len(topics & set(sel["preferred_topics"]))
    if o["kind"] in sel["required_kinds"]:
        s += 1
    if o["observation_id"] in sel["anchor_observation_ids"]:
        s += ANCHOR_BONUS
    return s


def classify(gold: dict, sel: dict) -> tuple[list[dict], list[dict]]:
    """Split observations into (ranked candidates, omitted-with-reason)."""
    candidates: list[dict] = []
    omitted: list[dict] = []
    req = set(sel["required_topics"])
    pref = set(sel["preferred_topics"])
    excl = set(sel["excluded_topics"])
    anchors = set(sel["anchor_observation_ids"])

    for o in gold["observations"]:
        oid = o["observation_id"]
        if not eligible(o):
            if o["authority"] == "agent_claim":
                reason = "not eligible: authority agent_claim is never authoritative"
            elif o["status"] == "superseded":
                reason = "not eligible: superseded_by %s" % o.get("superseded_by")
            else:
                reason = "not eligible: status %s not in force" % o["status"]
            omitted.append({"observation_id": oid, "reason": reason})
            continue

        topics = _topics_of(o)
        hit_excluded = sorted(topics & excl)
        if hit_excluded:
            omitted.append({
                "observation_id": oid,
                "reason": "excluded by task selector on topic(s) %s" % ",".join(hit_excluded),
            })
            continue

        if oid in anchors:
            candidates.append(o)
            continue

        if not (topics & (req | pref)):
            omitted.append({
                "observation_id": oid,
                "reason": "not relevant to this task: topics %s share nothing with "
                          "required %s or preferred %s"
                          % (",".join(sorted(topics)),
                             ",".join(sel["required_topics"]) or "-",
                             ",".join(sel["preferred_topics"]) or "-"),
            })
            continue
        candidates.append(o)

    candidates.sort(key=lambda o: (-score(o, sel),
                                   _KIND_RANK.get(o["kind"], 99),
                                   o["observation_id"]))
    return candidates, omitted


def enforce_anchor_ratio(selected: list[dict], sel: dict) -> None:
    """Refuse a task that selects primarily by naming observation ids."""
    if not selected:
        return
    req = set(sel["required_topics"])
    pref = set(sel["preferred_topics"])
    anchors = set(sel["anchor_observation_ids"])
    anchor_only = [o for o in selected
                   if o["observation_id"] in anchors and not (_topics_of(o) & (req | pref))]
    ratio = len(anchor_only) / len(selected)
    if ratio > MAX_ANCHOR_RATIO:
        raise SelectorError(
            "%.0f%% of the selection (%d/%d) matched only by explicit anchor id, "
            "above the %.0f%% ceiling: this task is an answer key, not a selector"
            % (ratio * 100, len(anchor_only), len(selected), MAX_ANCHOR_RATIO * 100))
