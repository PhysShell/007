"""Shared SYNTHETIC fixtures for the offline test suite (no private RAW).

Everything here is constructed in-process. Nothing reads the CAS, so the whole
suite except the single integration test runs on any checkout.

The synthetic gold state is deliberately built so that two tasks with different
selectors must select different subsets — the acceptance invariant of the
corrective round (`#issuecomment-5182632187`).
"""
from __future__ import annotations

import copy

TOPIC_VOCABULARY = ["alpha", "beta", "shared"]


def _obs(oid, kind, statement, authority, status, topics, provenance, **extra):
    o = {
        "observation_id": oid,
        "kind": kind,
        "topics": list(topics),
        "statement": statement,
        "authority": authority,
        "status": status,
        "provenance": provenance,
    }
    o.update(extra)
    return o


GOLD = {
    "schema": "o7.b1.state-observables/v0",
    "fixture_id": "syn",
    "cutoff": {"identity": "syn-cutoff"},
    "topic_vocabulary": TOPIC_VOCABULARY,
    "observations": [
        _obs("obs-truth-topology", "evidence", "X and Y are one conversation.",
             "platform_capture", "current", ["beta", "shared"],
             [{"source_id": "cap-xy", "digest": "sha256:" + "a" * 64}]),
        _obs("obs-goal", "goal", "the goal.", "controller_or_repo_record", "current",
             ["alpha"], [{"source_id": "readme", "source_pointer": "README#goal"}]),
        _obs("obs-alpha-constraint", "constraint", "alpha must stay read-only.",
             "controller_or_repo_record", "current", ["alpha", "shared"],
             [{"source_id": "readme", "source_pointer": "README#constraint"}]),
        _obs("obs-alpha-next", "next_action", "do the alpha next step.",
             "controller_or_repo_record", "current", ["alpha"],
             [{"source_id": "readme", "source_pointer": "README#next"}]),
        _obs("obs-beta-status", "status", "beta capture is interim.",
             "controller_or_repo_record", "current", ["beta"],
             [{"source_id": "readme", "source_pointer": "README#beta"}]),
        _obs("obs-claim-separate", "evidence", "belief: X and Y are separate.",
             "agent_claim", "superseded", ["beta"],
             [{"source_id": "nc-recon", "digest": "sha256:" + "b" * 64}],
             superseded_by="obs-truth-topology",
             structured_value={"contrary_markers": ["separate conversation"]}),
    ],
    "relations": [
        {"kind": "supersedes", "from": "obs-truth-topology", "to": "obs-claim-separate"},
        {"kind": "contradicts", "from": "obs-claim-separate", "to": "obs-truth-topology"},
    ],
}

#: Task ALPHA — wants alpha scope only.
TASK_A = {
    "schema": "o7.b1.task/v0",
    "task_id": "syn-task-alpha",
    "fixture_id": "syn",
    "statement": "advance alpha.",
    "selectors": {
        "required_topics": ["alpha"],
        "preferred_topics": [],
        "excluded_topics": [],
        "required_kinds": ["goal", "constraint", "next_action"],
        "anchor_observation_ids": [],
        "anchor_rationale": {},
    },
}

#: Task BETA — wants beta scope only. Overlaps A only via `shared`, which A does
#: not request and B does, so neither selection is a superset of the other.
TASK_B = {
    "schema": "o7.b1.task/v0",
    "task_id": "syn-task-beta",
    "fixture_id": "syn",
    "statement": "audit beta.",
    "selectors": {
        "required_topics": ["beta"],
        "preferred_topics": ["shared"],
        "excluded_topics": [],
        "required_kinds": ["status", "evidence"],
        "anchor_observation_ids": [],
        "anchor_rationale": {},
    },
}

QUESTIONS_A = {"questions": [
    {"id": "qa-goal", "required_observation_ids": ["obs-goal"]},
    {"id": "qa-constraint", "required_observation_ids": ["obs-alpha-constraint"]},
    {"id": "qa-next", "required_observation_ids": ["obs-alpha-next"]},
]}
QUESTIONS_B = {"questions": [
    {"id": "qb-topology", "required_observation_ids": ["obs-truth-topology"]},
    {"id": "qb-status", "required_observation_ids": ["obs-beta-status"]},
]}

SOURCE_SELECTORS = {"sessions": [{"session_id": "sess-xy", "raw_source": "cap-xy"}]}

MANIFEST = {
    "raw_sources": [
        {"id": "cap-xy", "digest": "sha256:" + "a" * 64, "byte_length": 10,
         "native_conversation_id": "native-xy"},
    ],
    "negative_control": [
        {"id": "nc-recon", "digest": "sha256:" + "b" * 64, "byte_length": 20,
         "known_divergence_already_observed": "assumed X and Y separate"}],
}

#: NC that references no captured native id -> corrective evidence absent, and
#: whose text DOES carry the contrary marker -> contrary claim detectable.
NC_OBJ = {"events": [{"seq": 0, "role": "assistant",
                      "gist": "they were a separate conversation"}],
          "subject": "session recall without native ids"}

#: NC that DOES reference the corrective capture -> corrective evidence present.
NC_OBJ_CORRECTED = {"events": [{"seq": 0, "role": "assistant",
                                "gist": "see native-xy, one conversation"}],
                    "subject": "corrected recall"}

BUDGET = {"byte_budget": 20000, "record_budget": 32, "unit": "utf8_bytes+records"}


def gold() -> dict:
    return copy.deepcopy(GOLD)


def task_a() -> dict:
    return copy.deepcopy(TASK_A)


def task_b() -> dict:
    return copy.deepcopy(TASK_B)
