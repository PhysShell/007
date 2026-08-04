#!/usr/bin/env python3
"""run_case.py — the single documented command that runs the whole B1 vertical.

    python3 research/b1-context/tools/run_case.py \
      --fixture case-0001 \
      --data-root "$HOME/.local/share/o7-research" \
      --out /tmp/o7-b1-case-0001

It is offline, needs no secrets, and is READ-ONLY with respect to its inputs: it
reads RAW blobs from the local CAS, writes derived transcripts and canonical
artifacts only under --out, and never writes to the CAS or to any source blob.
It fails closed: a digest mismatch or a broken determinism/expectation check
exits non-zero and never fabricates a report.

Canonical, digest-bound outputs (byte-identical across runs): context.json,
context.md, context.meta.json, report.json, report.md. A separate run-receipt
(with wall-clock time; NOT part of any canonical digest) is written alongside.
"""
from __future__ import annotations

import argparse
import json
import os
import sys

import yaml

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _HERE)

from o7b1 import evaluate as ev
from o7b1 import projector as pj
from o7b1 import report as rp
from o7b1.canonical import (canonical_bytes, canonical_file_bytes, jsonl_bytes,
                            sha256_hex, sha256_of_file)
from o7b1.cas import BlobDigestMismatch, BlobUnavailable, Cas
from o7b1.extractors import ChatGPTBackendApiV0, ClaudeCodeV0
from o7b1.schema import default_schema_path, validate_gold_state

BYTE_BUDGET = 20000
RECORD_BUDGET = 32

_EXTRACTOR_BY_SYSTEM = {
    "chatgpt-backend-api": ChatGPTBackendApiV0,
    "claude-code-container": ClaudeCodeV0,
}


class FailClosed(Exception):
    pass


def _load_yaml(path: str):
    with open(path, encoding="utf-8") as fh:
        return yaml.safe_load(fh)


def _fixture_dir(fixture: str) -> str:
    return os.path.normpath(os.path.join(_HERE, "..", "fixtures", fixture))


def _raw_source_index(manifest: dict) -> dict:
    idx = {r["id"]: r for r in manifest["raw_sources"]}
    return idx


def _run_extractors(cas: Cas, manifest: dict, source_selectors: dict, out_dir: str):
    """Return (derived_summary, per_session list, availability)."""
    raw_idx = _raw_source_index(manifest)
    derived_dir = os.path.join(out_dir, "derived")
    os.makedirs(derived_dir, exist_ok=True)

    per_session = []
    total_bytes = 0
    total_records = 0
    all_available = True

    for s in source_selectors["sessions"]:
        sid = s["session_id"]
        rid = s["raw_source"]
        raw = raw_idx.get(rid)
        if raw is None:
            raise FailClosed("session %s references unknown raw_source %s" % (sid, rid))
        digest = raw["digest"]
        size = raw["byte_length"]
        system = raw["source_system"]
        extractor = _EXTRACTOR_BY_SYSTEM.get(system)
        if extractor is None:
            raise FailClosed("no extractor for source_system %s" % system)

        try:
            path = cas.resolve(digest, expected_size=size)
        except BlobUnavailable:
            all_available = False
            per_session.append({"session_id": sid, "raw_source": rid,
                                "status": "UNAVAILABLE", "digest": digest})
            continue
        except BlobDigestMismatch as e:
            raise FailClosed("digest mismatch for %s: %s" % (rid, e))

        if extractor is ChatGPTBackendApiV0:
            with open(path, "rb") as fh:
                raw_obj = json.loads(fh.read())
            records, policy = extractor.extract(
                raw_obj, source_id=sid, raw_source_digest=digest)
        else:
            with open(path, "rb") as fh:
                raw_bytes = fh.read()
            records, policy = extractor.extract(
                raw_bytes, source_id=sid, raw_source_digest=digest)

        blob = jsonl_bytes(records)
        derived_digest = "sha256:" + sha256_hex(blob)
        dpath = os.path.join(derived_dir, "%s.jsonl" % sid)
        with open(dpath, "wb") as fh:
            fh.write(blob)

        total_bytes += len(blob)
        total_records += len(records)
        per_session.append({
            "session_id": sid,
            "raw_source": rid,
            "status": "OK",
            "source_system": system,
            "raw_source_digest": digest,
            "extractor_id": extractor.extractor_id,
            "extractor_version": extractor.version,
            "extractor_impl_digest": "sha256:" + sha256_of_file(
                os.path.join(_HERE, "o7b1", "extractors",
                             "chatgpt_backend_api_v0.py" if extractor is ChatGPTBackendApiV0
                             else "claude_code_v0.py")),
            "derived_digest": derived_digest,
            "derived_byte_length": len(blob),
            "record_count": len(records),
            "inclusion_policy": policy["inclusion_policy"],
        })

    summary = {"total_bytes": total_bytes, "total_records": total_records}
    return summary, per_session, all_available


def _write_canonical(path: str, obj) -> str:
    data = canonical_file_bytes(obj)
    with open(path, "wb") as fh:
        fh.write(data)
    return "sha256:" + sha256_hex(data)


def _write_text(path: str, text: str) -> str:
    data = text.encode("utf-8")
    if not data.endswith(b"\n"):
        data += b"\n"
    with open(path, "wb") as fh:
        fh.write(data)
    return "sha256:" + sha256_hex(data)


def _build_derived_manifest(fixture: str, manifest_digests: dict, per_session: list) -> dict:
    return {
        "schema": "o7.b1.derived-manifest/v0",
        "fixture_id": fixture,
        "note": ("Derived transcripts are external CAS blobs; only their digests, "
                 "sizes, counts, extractor identity and provenance live here. Full "
                 "transcript text is never committed."),
        "sessions": [
            {k: d[k] for k in (
                "session_id", "raw_source", "raw_source_digest", "source_system",
                "extractor_id", "extractor_version", "extractor_impl_digest",
                "derived_digest", "derived_byte_length", "record_count",
                "inclusion_policy")}
            for d in per_session if d["status"] == "OK"
        ],
    }


def _canonical_reproduce_command(fixture: str) -> str:
    # Stable, independent of the actual --out path, so report.json stays
    # byte-identical no matter where you run it.
    return ("python3 research/b1-context/tools/run_case.py --fixture %s "
            "--data-root \"$HOME/.local/share/o7-research\" --out /tmp/o7-b1-%s"
            % (fixture, fixture))


def run(fixture: str, data_root: str, out_dir: str) -> dict:
    reproduce_command = _canonical_reproduce_command(fixture)
    fdir = _fixture_dir(fixture)
    schema_path = default_schema_path()
    cas = Cas(os.path.join(data_root, "cas"))
    os.makedirs(out_dir, exist_ok=True)

    manifest = _load_yaml(os.path.join(fdir, "manifest.yaml"))
    source_selectors = _load_yaml(os.path.join(fdir, "source-selectors.yaml"))
    task = _load_yaml(os.path.join(fdir, "task-v0.yaml"))
    questions = _load_yaml(os.path.join(fdir, "questions-v0.yaml"))
    with open(os.path.join(fdir, "gold-state-v0.json"), encoding="utf-8") as fh:
        gold = json.load(fh)

    # (1) verify inputs + gold state
    validate_gold_state(gold, schema_path)
    input_digests = {}
    for r in manifest["raw_sources"] + manifest["negative_control"]:
        try:
            cas.resolve(r["digest"], expected_size=r["byte_length"])
            input_digests[r["id"]] = {"digest": r["digest"], "byte_length": r["byte_length"],
                                      "status": "OK"}
        except BlobUnavailable:
            input_digests[r["id"]] = {"digest": r["digest"], "status": "UNAVAILABLE"}
        except BlobDigestMismatch as e:
            raise FailClosed("input digest mismatch for %s: %s" % (r["id"], e))

    # (2) extractors -> derived transcripts (written under out/, not the CAS)
    derived_summary, per_session, all_available = _run_extractors(
        cas, manifest, source_selectors, out_dir)

    # verify against the committed derived-manifest expectation, if present
    derived_manifest = _build_derived_manifest(fixture, input_digests, per_session)
    committed_dm_path = os.path.join(fdir, "derived-manifest.yaml")
    derived_mismatch = None
    if os.path.isfile(committed_dm_path):
        committed = _load_yaml(committed_dm_path)
        exp = {s["session_id"]: (s["derived_digest"], s["derived_byte_length"], s["record_count"])
               for s in committed.get("sessions", [])}
        for s in derived_manifest["sessions"]:
            got = (s["derived_digest"], s["derived_byte_length"], s["record_count"])
            if exp.get(s["session_id"]) != got:
                derived_mismatch = (s["session_id"], exp.get(s["session_id"]), got)
                raise FailClosed("derived transcript for %s does not match committed "
                                 "derived-manifest: expected %s got %s"
                                 % derived_mismatch)
    _write_canonical(os.path.join(out_dir, "derived-manifest.actual.json"), derived_manifest)

    # load negative control object for the evaluation
    nc_meta = manifest["negative_control"][0]
    nc_obj = None
    if input_digests[nc_meta["id"]]["status"] == "OK":
        nc_obj = json.loads(cas.read_bytes(nc_meta["digest"], expected_size=nc_meta["byte_length"]))

    # (3) projection
    sel = pj.select(gold, byte_budget=BYTE_BUDGET, record_budget=RECORD_BUDGET)
    task_digest = "sha256:" + sha256_hex(canonical_bytes(task))
    questions_digest = "sha256:" + sha256_hex(canonical_bytes(questions))
    schema_digest = "sha256:" + sha256_of_file(schema_path)
    input_observation_digest = "sha256:" + sha256_hex(canonical_bytes(gold["observations"]))

    context_json = pj.build_context_json(gold, task, sel)
    context_md = pj.build_context_md(gold, task, sel)
    ctx_json_digest = _write_canonical(os.path.join(out_dir, "context.json"), context_json)
    ctx_md_digest = _write_text(os.path.join(out_dir, "context.md"), context_md)
    context_json_bytes = len(canonical_file_bytes(context_json))

    artifact_digests = {"context.json": ctx_json_digest, "context.md": ctx_md_digest}
    context_meta = pj.build_context_meta(
        gold, task_digest, questions_digest, schema_digest, sel,
        budget={"byte_budget": BYTE_BUDGET, "record_budget": RECORD_BUDGET,
                "unit": "utf8_bytes+records"},
        artifact_digests=artifact_digests, input_observation_digest=input_observation_digest)
    ctx_meta_digest = _write_canonical(os.path.join(out_dir, "context.meta.json"), context_meta)

    # (4) evaluation (requires the negative control blob)
    if nc_obj is None:
        evaluation = None
    else:
        evaluation = ev.evaluate(gold, questions, source_selectors, manifest, nc_obj,
                                 derived_summary, sel, context_json_bytes)

    # (5) status + report
    development_result = _decide_result(all_available, evaluation)
    report = {
        "schema": "o7.b1.report/v0",
        "fixture_id": fixture,
        "cutoff_identity": gold["cutoff"]["identity"],
        "reproduce_command": reproduce_command,
        "input_digests": input_digests,
        "derived": {
            "total_records": derived_summary["total_records"],
            "total_bytes": derived_summary["total_bytes"],
            "sessions": derived_manifest["sessions"],
        },
        "schema_digest": schema_digest,
        "gold_state_digest": "sha256:" + sha256_hex(canonical_bytes(gold)),
        "task_digest": task_digest,
        "questions_digest": questions_digest,
        "projection": {
            "selected_count": len(sel["selected"]),
            "omitted_count": len(sel["omitted"]),
            "used_bytes": sel["used_bytes"],
            "context_artifact_digests": {**artifact_digests, "context.meta.json": ctx_meta_digest},
        },
        "evaluation": evaluation,
        "status": {
            "development_result": development_result,
            "generalization": "NOT_EVALUATED",
            "source_set_complete": False,
            "holdout_evaluated": False,
            "authoritative_for_a_series": False,
        },
    }
    report_bytes = canonical_file_bytes(report)
    report_json_digest = "sha256:" + sha256_hex(report_bytes)
    with open(os.path.join(out_dir, "report.json"), "wb") as fh:
        fh.write(report_bytes)
    report_md = rp.build_report_md(report)
    _write_text(os.path.join(out_dir, "report.md"), report_md)

    # Fail-closed regression gate against the committed frozen expectation.
    expected_path = os.path.join(fdir, "expected-report-v0.json")
    if os.path.isfile(expected_path):
        with open(expected_path, "rb") as fh:
            expected_bytes = fh.read()
        if expected_bytes != report_bytes:
            raise FailClosed(
                "report.json does not match committed expected-report-v0.json "
                "(expected %s, got %s)"
                % ("sha256:" + sha256_hex(expected_bytes), report_json_digest))

    return {
        "report": report,
        "report_json_digest": report_json_digest,
        "canonical_digests": {
            "context.json": ctx_json_digest,
            "context.md": ctx_md_digest,
            "context.meta.json": ctx_meta_digest,
            "report.json": report_json_digest,
        },
        "out_dir": out_dir,
    }


def _decide_result(all_available: bool, evaluation) -> str:
    if not all_available or evaluation is None:
        return "PARTIAL"
    C = evaluation["arms"]["projection"]
    B = evaluation["arms"]["negative_control"]
    ok = (
        C["required_observation_recall"] == 1.0
        and C["question_support_coverage"] == 1.0
        and C["contradiction_count"] == 0
        and C["supersession_error_count"] == 0
        and B["contradiction_count"] >= 1  # the fixture must detect the known loss
    )
    return "PASS" if ok else "FAIL"


def main(argv=None):
    ap = argparse.ArgumentParser(description="Run the B1 development vertical for one fixture.")
    ap.add_argument("--fixture", default="case-0001")
    ap.add_argument("--data-root", default=os.path.expanduser("~/.local/share/o7-research"))
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)

    # Canonical, stable reproduce command (independent of the actual --out path,
    # so report.json stays byte-identical no matter where you run it).
    try:
        result = run(args.fixture, args.data_root, args.out)
    except FailClosed as e:
        sys.stderr.write("FAIL CLOSED: %s\n" % e)
        return 2
    rep = result["report"]
    sys.stderr.write("development_result: %s\n" % rep["status"]["development_result"])
    sys.stderr.write("report.json: %s\n" % result["report_json_digest"])
    return 0 if rep["status"]["development_result"] in ("PASS", "PARTIAL") else 3


if __name__ == "__main__":
    raise SystemExit(main())
