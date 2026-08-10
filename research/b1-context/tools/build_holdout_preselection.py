#!/usr/bin/env python3
"""Real-source holdout — pre-selection record.

Written before any fixture is curated and before any projector runs, because a
selection rule composed after looking at outcomes is not a selection rule. It
fixes the candidate inventory, a stable ordering, eligibility criteria that
depend only on source metadata and parseability, the exclusion set, and the
fallback rule -- and then reports how many candidates survive.

The inventory is exhaustive over every location on this machine that holds
capture-shaped material: the durable content-addressed store, the private
case-0002 workspace, and the local agent-session transcripts.
"""
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

RB = Path(__file__).resolve().parent.parent
RESEARCH = Path.home() / ".local/share/o7-research"
CAS = RESEARCH / "cas/sha256"
SESSIONS = Path.home() / ".claude/projects"
OUT = RB / "results" / "real-source-holdout"

THIS_DEVELOPMENT_SESSION = "be909b9c-a365-400f-aafd-058b173f9f58"

ELIGIBILITY = {
    "e1_real_capture": "the object is a raw platform capture, not an artifact derived by B1",
    "e2_parseable_by_frozen_compiler": "parses as the capture format the frozen compiler accepts",
    "e3_substantive": ("carries enough events to support gold observations with relations and "
                       "status transitions; a handful of events cannot"),
    "e4_not_previously_used": ("its digest or native conversation id appears in no case-0001 / "
                               "case-0002 fixture, result, or manifest"),
    "e5_independent_of_development": ("the capture does not itself discuss B1/Qodec development; a "
                                      "transcript of the work being tested is not a holdout for it"),
}

EXCLUSION_REASONS = {
    "used_case_0001": "bound as a source of the case-0001 development fixture",
    "used_case_0002": "bound by the case-0002 round-0 source lock",
    "b1_derived_artifact": "produced by the B1 pipeline; not a raw source",
    "development_session": "this is the session in which B1/Qodec development is being conducted",
    "development_adjacent": "the capture discusses B1/Qodec development, so it is not independent",
    "not_substantive": "too few events to carry gold observations, relations and status transitions",
}

FALLBACK_RULE = ("candidates are ordered by ascending sha256 of their bytes; if a selected source "
                 "cannot be processed by the frozen compiler, it is dropped with its refusal "
                 "recorded and the next candidate in that order is taken. The order is fixed "
                 "before any processing so the fallback cannot be steered.")


def sha_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def used_digests():
    used = set()
    for m in (RESEARCH / "manifests").glob("*.yaml"):
        used |= set(re.findall(r"[0-9a-f]{64}", m.read_text()))
    for f in (RESEARCH / "case-0002").rglob("*"):
        if f.is_file():
            try:
                used |= set(re.findall(r"[0-9a-f]{64}", f.read_text(errors="ignore")[:2_000_000]))
            except Exception:
                pass
    repo = subprocess.run(["grep", "-rhoE", "[0-9a-f]{64}", str(RB)],
                          capture_output=True, text=True).stdout
    used |= set(repo.split())
    return used


def where_used(digest):
    out = subprocess.run(["grep", "-rl", digest, str(RB)], capture_output=True, text=True).stdout
    return sorted({str(Path(p).relative_to(RB)) for p in out.split() if p})[:6]


def contaminated(path):
    """Does this capture discuss the work under test?"""
    hits = {}
    for pat in ("b1-context", "qodec", "case-0002"):
        n = subprocess.run(["grep", "-c", "--", pat, str(path)],
                           capture_output=True, text=True).stdout.strip()
        hits[pat] = int(n or 0)
    return hits, sum(hits.values()) > 0


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    used = used_digests()
    candidates = []

    # --- content-addressed store: raw conversation exports ------------------
    for p in sorted(CAS.rglob("*")):
        if not p.is_file():
            continue
        with p.open("rb") as fh:
            head = fh.readline(4000).decode("utf-8", "ignore")
        digest = p.parent.name + p.name
        is_export = '"conversation_id"' in head
        is_derived = '"o7.b1.' in head or '"record"' in head
        if not (is_export or is_derived):
            continue
        row = {"locator": f"cas:sha256:{digest}", "artifact_bytes_sha256": f"sha256:{digest}",
               "byte_length": p.stat().st_size, "source_class": None,
               "previously_used_for_development": None, "eligible": False, "exclusion": None}
        if is_derived:
            row["source_class"] = "b1-derived-artifact"
            row["previously_used_for_development"] = True
            row["exclusion"] = "b1_derived_artifact"
        else:
            row["source_class"] = "chatgpt-export"
            try:
                o = json.loads(p.read_text(errors="ignore"))
                row["native_conversation_id"] = o.get("conversation_id")
                row["mapping_nodes"] = len(o.get("mapping") or {})
            except Exception:
                row["mapping_nodes"] = -1
            hits = where_used(digest)
            row["previously_used_for_development"] = bool(hits)
            row["used_by"] = hits
            if hits:
                row["exclusion"] = ("used_case_0002" if any("case-0002" in h for h in hits)
                                    else "used_case_0001")
        candidates.append(row)

    # --- local agent-session transcripts ------------------------------------
    for p in sorted(SESSIONS.rglob("*.jsonl")):
        if "subagents" in p.parts:
            continue  # not independent: belongs to a parent session
        sid = p.stem
        digest = sha_file(p)
        lines = sum(1 for _ in p.open("rb"))
        row = {"locator": str(p), "artifact_bytes_sha256": f"sha256:{digest}",
               "byte_length": p.stat().st_size, "source_class": "claude-code-session",
               "native_session_id": sid, "events": lines,
               "previously_used_for_development": False, "eligible": False, "exclusion": None}
        if sid == THIS_DEVELOPMENT_SESSION:
            row["previously_used_for_development"] = True
            row["exclusion"] = "development_session"
        elif lines < 100:
            row["exclusion"] = "not_substantive"
        else:
            hits, bad = contaminated(p)
            row["development_mentions"] = hits
            if bad:
                row["previously_used_for_development"] = True
                row["exclusion"] = "development_adjacent"
            else:
                row["eligible"] = True
        candidates.append(row)

    candidates.sort(key=lambda r: r["artifact_bytes_sha256"])  # stable order, fixed before use
    eligible = [c for c in candidates if c["eligible"]]

    record = {
        "record": "o7.b1.real-source-holdout-preselection/v1",
        "written_before": "any fixture curation, any projector run, any model call",
        "digest_function": "sha256 over raw file bytes (not a git object id)",
        "required_fixture_count": 3,
        "eligibility_criteria": ELIGIBILITY,
        "exclusion_reasons": EXCLUSION_REASONS,
        "stable_ordering": "ascending artifact_bytes_sha256",
        "fallback_rule": FALLBACK_RULE,
        "inventory_locations": [str(CAS), str(RESEARCH / "case-0002"), str(SESSIONS)],
        "candidate_inventory": candidates,
        "candidate_count": len(candidates),
        "eligible_count": len(eligible),
        "eligible": [{k: c.get(k) for k in
                      ("locator", "artifact_bytes_sha256", "byte_length", "source_class",
                       "native_session_id", "events")} for c in eligible],
        "sufficient_for_holdout": len(eligible) >= 3,
        "verdict": ("REAL_SOURCE_HOLDOUT_MATERIAL_INSUFFICIENT" if len(eligible) < 3
                    else "REAL_SOURCE_HOLDOUT_MATERIAL_AVAILABLE"),
    }
    body = (json.dumps(record, indent=2, sort_keys=True) + "\n").encode()
    (OUT / "preselection-record.json").write_bytes(body)
    print(f"candidates inventoried: {len(candidates)}")
    for c in candidates:
        mark = "ELIGIBLE" if c["eligible"] else f"excluded[{c['exclusion']}]"
        ident = c.get("native_session_id") or c.get("native_conversation_id") or ""
        print(f"  {c['artifact_bytes_sha256'][7:23]}  {c['byte_length']:>11,}b  "
              f"{c['source_class']:20} {mark:32} {ident}")
    print(f"\neligible: {len(eligible)} / required 3 -> {record['verdict']}")
    print("preselection record: sha256:" + hashlib.sha256(body).hexdigest())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
