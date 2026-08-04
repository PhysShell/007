"""Dependency-free validation of a gold state against state-observables v0.

We intentionally do not import a third-party JSON-Schema engine: this vertical
must run on a bare machine with no network and no pip step. The closed
vocabularies are read from the committed schema file so this validator cannot
drift from it, and on top of structural checks we enforce the B1 semantic
invariants that plain JSON Schema cannot express (referential integrity of
supersession/relations, and "agent_claim never stands as authoritative").
"""
from __future__ import annotations

import json
import os
import re
from typing import Any

_OBS_ID_RE = re.compile(r"^obs-[a-z0-9][a-z0-9-]*$")
_DIGEST_RE = re.compile(r"^sha256:[0-9a-f]{64}$")

SCHEMA_ID = "o7.b1.state-observables/v0"


class SchemaError(Exception):
    pass


def _load_enums(schema_path: str) -> dict[str, set[str]]:
    with open(schema_path, encoding="utf-8") as fh:
        schema = json.load(fh)
    defs = schema["$defs"]
    return {
        "authority": set(defs["authority"]["enum"]),
        "observation_kind": set(defs["observation_kind"]["enum"]),
        "observation_status": set(defs["observation_status"]["enum"]),
        "relation_kind": set(defs["relation_kind"]["enum"]),
    }


def default_schema_path() -> str:
    here = os.path.dirname(__file__)
    return os.path.normpath(
        os.path.join(here, "..", "..", "schema", "state-observables-v0.schema.json")
    )


def validate_gold_state(state: dict[str, Any], schema_path: str | None = None) -> None:
    """Raise SchemaError on the first violation; return None if valid."""
    schema_path = schema_path or default_schema_path()
    enums = _load_enums(schema_path)

    if state.get("schema") != SCHEMA_ID:
        raise SchemaError("top-level schema must be %r" % SCHEMA_ID)
    for key in ("fixture_id", "cutoff", "observations", "relations"):
        if key not in state:
            raise SchemaError("missing top-level key: %s" % key)
    if not isinstance(state["cutoff"], dict) or not state["cutoff"].get("identity"):
        raise SchemaError("cutoff.identity is required")

    obs_ids: set[str] = set()
    for o in state["observations"]:
        oid = o.get("observation_id", "")
        if not _OBS_ID_RE.match(oid):
            raise SchemaError("bad observation_id: %r" % oid)
        if oid in obs_ids:
            raise SchemaError("duplicate observation_id: %s" % oid)
        obs_ids.add(oid)
        if o.get("kind") not in enums["observation_kind"]:
            raise SchemaError("%s: bad kind %r" % (oid, o.get("kind")))
        if o.get("authority") not in enums["authority"]:
            raise SchemaError("%s: bad authority %r" % (oid, o.get("authority")))
        if o.get("status") not in enums["observation_status"]:
            raise SchemaError("%s: bad status %r" % (oid, o.get("status")))
        if not isinstance(o.get("statement"), str) or not o["statement"].strip():
            raise SchemaError("%s: statement must be non-empty text" % oid)
        prov = o.get("provenance")
        if not isinstance(prov, list) or not prov:
            raise SchemaError("%s: at least one provenance ref required" % oid)
        for pr in prov:
            if not isinstance(pr, dict) or not pr.get("source_id"):
                raise SchemaError("%s: provenance ref needs source_id" % oid)
            dg = pr.get("digest")
            if dg is not None and not _DIGEST_RE.match(dg):
                raise SchemaError("%s: bad provenance digest %r" % (oid, dg))

    # Referential + semantic invariants.
    for o in state["observations"]:
        oid = o["observation_id"]
        if o["status"] == "superseded":
            sb = o.get("superseded_by")
            if not sb:
                raise SchemaError("%s: superseded observation must set superseded_by" % oid)
            if sb not in obs_ids:
                raise SchemaError("%s: superseded_by -> unknown %s" % (oid, sb))
        # agent_claim must never be presented as an in-force authoritative fact:
        # allowed only as a non-current record (advisory/superseded/rejected/pending).
        if o["authority"] == "agent_claim" and o["status"] == "current":
            raise SchemaError(
                "%s: agent_claim may not have status 'current' "
                "(it cannot stand as an authoritative fact)" % oid
            )

    for rel in state["relations"]:
        if rel.get("kind") not in enums["relation_kind"]:
            raise SchemaError("bad relation kind %r" % rel.get("kind"))
        for end in ("from", "to"):
            ref = rel.get(end)
            if ref not in obs_ids:
                raise SchemaError("relation %s endpoint -> unknown %r" % (rel.get("kind"), ref))
