"""Deterministic 3-arm evaluation. No LLM-as-judge anywhere.

Arms
----
A  full_derived      the concatenation of all derived transcripts (the naive
                     "dump everything" context).
B  negative_control  the sealed advisory agent reconstruction.
C  projection        the compiled, task-dependent projection.

Metric families are kept SEPARATE on purpose
--------------------------------------------
Corrective round `#issuecomment-5182632187` §3/§4/§5 found three distinct
problems that all came from one mistake: forcing every arm through one metric
family whose name implied more than it measured. So there are now three:

1. STRUCTURAL COMPILATION coverage — uniform across arms, structural only.
2. PROJECTION VALIDITY — computed from the presented records themselves
   (`validity.py`), so it can actually fail. Applies to the projection arm.
3. NEGATIVE-CONTROL DIAGNOSTICS — split into independently supported fields.
   Applies to the negative-control arm.

Non-applicable families are `null`, never a flattering zero.

Structural support model (unchanged, and still the honest part)
--------------------------------------------------------------
Each arm exposes a set of AVAILABLE sources (source_ids + digests). An arm
"carries" an in-force authoritative gold observation iff every provenance source
that observation cites is available in the arm. Applied identically to all three
arms — the asymmetry in the numbers is a property of the arms, not the scoring.

What this DOES NOT measure: whether a fact is semantically present in, or
answerable from, an arm's text. A raw transcript can perfectly well contain the
information behind an observation while carrying neither the compiled
observation nor its provenance graph — it would still score 0 here. That is why
these fields are named `compiled_observation_coverage` and
`structural_question_support` rather than "recall" or "answerability". A real
recall number requires a separate extraction/readout experiment that does not
exist yet. In particular, `full_derived`'s low coverage must NOT be read as
"only that fraction of project state is present in the transcripts".

`evidence_source_presence` is a softer companion: does the arm contain at least
one cited source for a required observation (is the raw material in there at
all), even if not lifted into a typed observation.
"""
from __future__ import annotations

from . import validity as vl
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
    have corrected the superseded belief."""
    raw = manifest["raw_sources"]
    for pr in truth_obs["provenance"]:
        for r in raw:
            if pr.get("source_id") == r["id"] or (
                    pr.get("digest") and pr["digest"] == r.get("digest")):
                if r.get("native_conversation_id"):
                    return r["id"], r["native_conversation_id"]
    return None, None


def _contrary_markers(claim: dict) -> list[str]:
    sv = claim.get("structured_value")
    if isinstance(sv, dict):
        markers = sv.get("contrary_markers")
        if isinstance(markers, list):
            return [str(m) for m in markers if str(m).strip()]
    return []


def detect_nc_divergences(gold: dict, nc_obj: dict, manifest: dict) -> dict:
    """Report the negative control's divergence as SEPARATE, independently
    supported claims (corrective round §7).

    Three different propositions were previously collapsed into one boolean:

    * `fixture_expected_divergence` — the digest-bound manifest records that this
      sealed reconstruction is already known to diverge here.
    * `corrective_evidence_absent_from_negative_control` — the NC bytes do not
      reference the corrective capture's native conversation id. This is what
      the byte check actually establishes: the reconstruction did not have that
      corrective reference.
    * `contrary_claim_detected` — whether the NC positively asserts the opposite
      proposition. Absence of a corrective reference does NOT establish this.
      Detected only when the superseded claim supplies `contrary_markers` in its
      `structured_value` and one is found in the NC text; otherwise `null`
      (= not evaluated), never silently folded into the fields above.
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
        corrective_absent = native is not None and native not in nc_text

        markers = _contrary_markers(claim)
        if markers:
            hits = sorted(m for m in markers if m in nc_text)
            contrary = bool(hits)
        else:
            hits = []
            contrary = None

        out[claim["observation_id"]] = {
            "superseded_by": claim.get("superseded_by"),
            "fixture_expected_divergence": bool(divergence_record),
            "corrective_evidence_absent_from_negative_control": corrective_absent,
            "contrary_claim_detected": contrary,
            "contrary_markers_found": hits,
            "corrective_capture": rid,
            "corrective_native_id_resolved": native is not None,
            "establishes": (
                "the reconstruction lacks corrective capture %s; this does not by "
                "itself prove it asserts the opposite" % rid
                if corrective_absent else
                "no corrective-reference absence established"),
        }
    return out


def _superseded_claims(gold: dict) -> list[dict]:
    return [o for o in gold["observations"]
            if o["authority"] == "agent_claim" and o["status"] == "superseded"]


def evaluate(gold: dict, questions: dict, source_selectors: dict, manifest: dict,
             nc_obj: dict, derived_summary: dict, projection: dict,
             context_json_bytes: int, budget: dict) -> dict:
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
    nc_stale = sorted(
        oid for oid, info in nc_div.items()
        if info["corrective_evidence_absent_from_negative_control"])
    nc_contrary = sorted(
        oid for oid, info in nc_div.items() if info["contrary_claim_detected"] is True)
    nc_superseders_missing = sorted(
        {nc_div[oid]["superseded_by"] for oid in nc_stale
         if nc_div[oid]["superseded_by"] in required_union})

    arms = {}
    matrix = []
    for arm in (FULL_DERIVED, NEGATIVE_CONTROL, PROJECTION):
        carried = _carried(gold, avail[arm])
        carried_req = carried & required_union
        ev_present = _evidence_present(gold, required_union, avail[arm])

        fully_supported_questions = 0
        for q in questions["questions"]:
            req = set(q["required_observation_ids"])
            supported = sorted(req & carried)
            missing = sorted(req - carried)
            if req and req <= carried:
                fully_supported_questions += 1
            evidence_refs = sorted({sid for oid in supported
                                    for sid in _prov_ids(by_id[oid])})
            matrix.append({
                "question_id": q["id"],
                "arm": arm,
                "required_observation_ids": sorted(req),
                "structurally_supported_observation_ids": supported,
                "missing_observation_ids": missing,
                "evidence_refs": evidence_refs,
            })

        n_req = len(required_union)
        entry = {
            "input_bytes": input_bytes[arm],
            "record_count": record_count[arm],
            # --- structural compilation family (uniform, structural only) ---
            "compiled_observation_coverage": round(len(carried_req) / n_req, 6),
            "structural_question_support": round(
                fully_supported_questions / len(questions["questions"]), 6),
            "evidence_source_presence": round(len(ev_present) / n_req, 6),
            "has_carried_provenance": bool(carried),
            "reduction_ratio_vs_full_derived": round(input_bytes[arm] / full_bytes, 6),
            "carried_required_observation_ids": sorted(carried_req),
            # --- family-specific blocks, null where not applicable ---
            "projection_validity": None,
            "presented_provenance_ratio": None,
            "negative_control_diagnostics": None,
        }
        if arm == PROJECTION:
            entry["projection_validity"] = vl.check_projection(
                gold, projection["selector_spec"], projection["selected"],
                projection["used_bytes"], budget)
            entry["presented_provenance_ratio"] = vl.presented_provenance_ratio(
                projection["selected"])
        if arm == NEGATIVE_CONTROL:
            entry["negative_control_diagnostics"] = {
                "stale_belief_ids": nc_stale,
                "stale_belief_count": len(nc_stale),
                "contrary_claim_detected_ids": nc_contrary,
                "contrary_claim_detected_count": len(nc_contrary),
                "required_superseders_missing": nc_superseders_missing,
                "required_superseders_missing_count": len(nc_superseders_missing),
            }
        arms[arm] = entry

    return {
        "required_observation_union": sorted(required_union),
        "metric_semantics": {
            "compiled_observation_coverage":
                "fraction of required observations whose COMPILED form and full "
                "provenance are structurally present in this arm; NOT semantic "
                "recall and NOT answerability",
            "structural_question_support":
                "fraction of questions whose every required observation is "
                "structurally carried; NOT evidence the question can be answered "
                "from the arm's prose",
            "evidence_source_presence":
                "fraction of required observations for which at least one cited "
                "source is present in this arm",
            "has_carried_provenance":
                "boolean: this arm carries at least one observation with complete "
                "provenance (replaces the former `provenance_coverage`, which was "
                "this same boolean reported as a 0.0/1.0 ratio)",
            "presented_provenance_ratio":
                "projection only: fraction of PRESENTED records carrying complete "
                "provenance; null for an empty projection",
        },
        "arms": arms,
        "question_support_matrix": matrix,
        "negative_control_divergence": nc_div,
    }
