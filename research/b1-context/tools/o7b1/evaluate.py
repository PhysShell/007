"""Deterministic 3-arm evaluation. No LLM-as-judge anywhere.

Arms
----
A  full_derived      the concatenation of all derived transcripts (the naive
                     "dump everything" context).
B  negative_control  the sealed advisory agent reconstruction.
C  projection        the task-conditioned compiled projection.

Support model (structural, transparent, uniform)
------------------------------------------------
Each arm exposes a set of AVAILABLE sources (source_ids + digests). An arm
"carries" an in-force authoritative gold observation iff every provenance source
that observation cites is available in the arm. The projection carries the full
provenance of what it selected; raw transcripts and the advisory reconstruction
do not reproduce the fixture/repo provenance, so they carry no compiled state.
This is applied identically to all three arms — the asymmetry in the numbers is a
property of the arms, not of the scoring.

`evidence_source_presence` is a separate, softer metric: does the arm contain at
least one cited source for a required observation (i.e. is the raw material in
there at all), even if not lifted into a typed observation.

The negative control's divergence is not hardcoded: it is confirmed from
digest-bound fixture metadata (manifest.known_divergence_already_observed) and
corroborated against the verified NC bytes (the NC references none of the
captured chatgpt conversation ids that would carry the B/D merge or session E).
"""
from __future__ import annotations

from .canonical import canonical_bytes

FULL_DERIVED = "full_derived"
NEGATIVE_CONTROL = "negative_control"
PROJECTION = "projection"


def _in_force_authoritative(o: dict) -> bool:
    return o["status"] in ("current", "pending") and o["authority"] != "agent_claim"


def _prov_ids(o: dict) -> set[str]:
    ids = set()
    for pr in o["provenance"]:
        ids.add(pr["source_id"])
        if pr.get("digest"):
            ids.add(pr["digest"])
    return ids


def _avail_full_derived(source_selectors: dict, manifest: dict) -> set[str]:
    avail: set[str] = set()
    raw_by_id = {r["id"]: r for r in manifest["raw_sources"]}
    for s in source_selectors["sessions"]:
        avail.add(s["session_id"])
        rid = s["raw_source"]
        avail.add(rid)
        r = raw_by_id.get(rid)
        if r and r.get("digest"):
            avail.add(r["digest"])
    # snapshot-1 content is a byte prefix of snapshot-2, so it is present too.
    for r in manifest["raw_sources"]:
        if r["id"] == "claude-code-session-c-snapshot-1":
            avail.add(r["id"])
            avail.add(r["digest"])
    return avail


def _avail_negative_control(manifest: dict) -> set[str]:
    nc = manifest["negative_control"][0]
    return {nc["id"], nc["digest"]}


def _avail_projection(selected: list[dict]) -> set[str]:
    avail: set[str] = set()
    for o in selected:
        avail |= _prov_ids(o)
    return avail


def _carried(gold: dict, avail: set[str]) -> set[str]:
    """In-force authoritative observations whose provenance is fully available."""
    carried = set()
    for o in gold["observations"]:
        if not _in_force_authoritative(o):
            continue
        if _prov_ids(o) <= avail:
            carried.add(o["observation_id"])
    return carried


def _evidence_present(gold: dict, required: set[str], avail: set[str]) -> set[str]:
    by_id = {o["observation_id"]: o for o in gold["observations"]}
    present = set()
    for rid in required:
        o = by_id.get(rid)
        if o and (_prov_ids(o) & avail):
            present.add(rid)
    return present


def _corrective_native_id(truth_obs: dict, manifest: dict) -> tuple[str | None, str | None]:
    """The captured conversation whose native id, if the NC referenced it, would
    have corrected the superseded belief. Resolved from the truth observation's
    provenance (by source_id or digest) against the manifest raw sources."""
    raw = manifest["raw_sources"]
    for pr in truth_obs["provenance"]:
        for r in raw:
            if pr.get("source_id") == r["id"] or (pr.get("digest") and pr["digest"] == r.get("digest")):
                if r.get("native_conversation_id"):
                    return r["id"], r["native_conversation_id"]
    return None, None


def detect_nc_divergences(gold: dict, nc_obj: dict, manifest: dict) -> dict:
    """Confirm the negative control's divergences from verified inputs.

    Driven entirely by the gold state: for each superseded agent_claim, resolve
    the captured conversation that supersedes it and check whether the NC bytes
    reference that capture's native id. Absence == the NC still holds the old
    belief. Corroborated by the digest-bound manifest divergence record. No
    observation ids or native ids are hardcoded here.
    """
    nc_text = canonical_bytes(nc_obj).decode("utf-8")
    by_id = {o["observation_id"]: o for o in gold["observations"]}
    divergence_record = manifest["negative_control"][0].get(
        "known_divergence_already_observed", "")
    out: dict[str, dict] = {}
    for claim in _superseded_claims(gold):
        truth = by_id.get(claim.get("superseded_by"))
        if truth is None:
            continue
        rid, native = _corrective_native_id(truth, manifest)
        asserted = native is not None and native not in nc_text
        out[claim["observation_id"]] = {
            "asserted_by_negative_control": asserted,
            "superseded_by": claim.get("superseded_by"),
            "corroboration": (
                "manifest.known_divergence + NC does not reference corrective "
                "capture %s native id %s" % (rid, native)
                if native is not None else
                "no corrective capture native id resolvable; not confirmed"),
            "divergence_record_present": bool(divergence_record),
        }
    return out


def _superseded_claims(gold: dict) -> list[dict]:
    return [o for o in gold["observations"]
            if o["authority"] == "agent_claim" and o["status"] == "superseded"]


def evaluate(gold: dict, questions: dict, source_selectors: dict, manifest: dict,
             nc_obj: dict, derived_summary: dict, projection: dict,
             context_json_bytes: int) -> dict:
    by_id = {o["observation_id"]: o for o in gold["observations"]}
    required_union = set()
    for q in questions["questions"]:
        required_union |= set(q["required_observation_ids"])

    full_bytes = derived_summary["total_bytes"]
    full_records = derived_summary["total_records"]
    nc_bytes = manifest["negative_control"][0]["byte_length"]
    nc_records = len(nc_obj.get("events", []))

    avail = {
        FULL_DERIVED: _avail_full_derived(source_selectors, manifest),
        NEGATIVE_CONTROL: _avail_negative_control(manifest),
        PROJECTION: _avail_projection(projection["selected"]),
    }
    input_bytes = {
        FULL_DERIVED: full_bytes,
        NEGATIVE_CONTROL: nc_bytes,
        PROJECTION: context_json_bytes,
    }
    record_count = {
        FULL_DERIVED: full_records,
        NEGATIVE_CONTROL: nc_records,
        PROJECTION: len(projection["selected"]),
    }

    nc_div = detect_nc_divergences(gold, nc_obj, manifest)
    superseded = _superseded_claims(gold)

    def asserted_superseded_for(arm: str) -> list[dict]:
        """Which superseded beliefs does this arm present as if in force?"""
        if arm != NEGATIVE_CONTROL:
            return []
        out = []
        for claim in superseded:
            info = nc_div.get(claim["observation_id"])
            if info and info["asserted_by_negative_control"]:
                out.append(claim)
        return out

    arms = {}
    matrix = []
    for arm in (FULL_DERIVED, NEGATIVE_CONTROL, PROJECTION):
        carried = _carried(gold, avail[arm])
        carried_req = carried & required_union
        ev_present = _evidence_present(gold, required_union, avail[arm])
        asserted = asserted_superseded_for(arm)
        # supersession/contradiction: the superseded belief's superseded_by is a
        # required, in-force observation that the arm contradicts by asserting the
        # old belief.
        contradicted = sorted({c["superseded_by"] for c in asserted
                               if c["superseded_by"] in required_union})
        unsupported_claim_ids = sorted(c["observation_id"] for c in asserted)

        fully_supported_questions = 0
        for q in questions["questions"]:
            req = set(q["required_observation_ids"])
            supported = sorted(req & carried)
            missing = sorted(req - carried)
            q_contra = sorted({c["superseded_by"] for c in asserted
                               if c["superseded_by"] in req})
            q_unsupported = sorted(c["observation_id"] for c in asserted
                                   if c["superseded_by"] in req)
            if req and req <= carried:
                fully_supported_questions += 1
            evidence_refs = sorted({sid for oid in supported
                                    for sid in _prov_ids(by_id[oid])})
            matrix.append({
                "question_id": q["id"],
                "arm": arm,
                "required_observation_ids": sorted(req),
                "supported_observation_ids": supported,
                "missing_observation_ids": missing,
                "contradicted_observation_ids": q_contra,
                "unsupported_claim_ids": q_unsupported,
                "evidence_refs": evidence_refs,
            })

        n_req = len(required_union)
        arms[arm] = {
            "input_bytes": input_bytes[arm],
            "record_count": record_count[arm],
            "required_observation_recall": round(len(carried_req) / n_req, 6),
            "evidence_source_presence": round(len(ev_present) / n_req, 6),
            "question_support_coverage": round(
                fully_supported_questions / len(questions["questions"]), 6),
            "unsupported_claim_count": len(unsupported_claim_ids),
            "contradiction_count": len(contradicted),
            "supersession_error_count": len(asserted),
            "provenance_coverage": round(1.0 if carried else 0.0, 6),
            "reduction_ratio_vs_full_derived": round(input_bytes[arm] / full_bytes, 6),
            "carried_required_observation_ids": sorted(carried_req),
            "contradicted_required_observation_ids": contradicted,
        }

    return {
        "required_observation_union": sorted(required_union),
        "arms": arms,
        "question_support_matrix": matrix,
        "negative_control_divergence": nc_div,
    }
