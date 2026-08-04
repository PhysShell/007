#!/usr/bin/env python3
"""run_case.py — the single documented command that runs the whole B1 vertical.

Requires the owner's local CAS. The task-dependent PROJECTION half needs no
private data at all and is reproducible by anyone via `project_case.py`; only
the 3-arm evaluation (derived transcripts + sealed negative control) needs the
CAS. Both tools read the same `TASKS` list, so they can never disagree about
which tasks exist.

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
from o7b1 import selector as sl
from o7b1 import report as rp
from o7b1.canonical import (canonical_bytes, canonical_file_bytes, jsonl_bytes,
                            sha256_hex, sha256_of_file)
from o7b1.cas import BlobDigestMismatch, BlobUnavailable, Cas
from o7b1.extractors import ChatGPTBackendApiV0, ClaudeCodeV0
from o7b1.schema import default_schema_path, validate_gold_state

BYTE_BUDGET = 20000
RECORD_BUDGET = 32

#: (task file, questions file) per fixture, deterministic order. Task A first.
#: Kept identical to project_case.TASKS so the offline projection artifacts and
#: the CAS-backed report can never describe different task sets.
TASKS = [
    ("task-v0.yaml", "questions-v0.yaml"),
    ("task-b-v0.yaml", "questions-b-v0.yaml"),
]

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
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)
    return "sha256:" + sha256_hex(data)


def _write_text(path: str, text: str) -> str:
    data = text.encode("utf-8")
    if not data.endswith(b"\n"):
        data += b"\n"
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
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


def run(fixture: str, data_root: str, out_dir: str, update_expectations: bool = False) -> dict:
    reproduce_command = _canonical_reproduce_command(fixture)
    fdir = _fixture_dir(fixture)
    schema_path = default_schema_path()
    cas = Cas(os.path.join(data_root, "cas"))
    os.makedirs(out_dir, exist_ok=True)

    manifest = _load_yaml(os.path.join(fdir, "manifest.yaml"))
    source_selectors = _load_yaml(os.path.join(fdir, "source-selectors.yaml"))
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

    derived_manifest = _build_derived_manifest(fixture, input_digests, per_session)
    committed_dm_path = os.path.join(fdir, "derived-manifest.yaml")
    if os.path.isfile(committed_dm_path):
        committed = _load_yaml(committed_dm_path)
        exp = {s["session_id"]: (s["derived_digest"], s["derived_byte_length"], s["record_count"])
               for s in committed.get("sessions", [])}
        for s in derived_manifest["sessions"]:
            got = (s["derived_digest"], s["derived_byte_length"], s["record_count"])
            if exp.get(s["session_id"]) != got:
                raise FailClosed("derived transcript for %s does not match committed "
                                 "derived-manifest: expected %s got %s"
                                 % (s["session_id"], exp.get(s["session_id"]), got))
    _write_canonical(os.path.join(out_dir, "derived-manifest.actual.json"), derived_manifest)

    nc_meta = manifest["negative_control"][0]
    nc_obj = None
    if input_digests[nc_meta["id"]]["status"] == "OK":
        nc_obj = json.loads(cas.read_bytes(nc_meta["digest"], expected_size=nc_meta["byte_length"]))

    schema_digest = "sha256:" + sha256_of_file(schema_path)
    input_observation_digest = "sha256:" + sha256_hex(canonical_bytes(gold["observations"]))
    budget = {"byte_budget": BYTE_BUDGET, "record_budget": RECORD_BUDGET,
              "unit": "utf8_bytes+records"}

    # (3) task-dependent projection + (4) evaluation, PER TASK
    task_entries = []
    selection_by_task = {}
    for task_file, questions_file in TASKS:
        task = _load_yaml(os.path.join(fdir, task_file))
        questions = _load_yaml(os.path.join(fdir, questions_file))
        if questions.get("task_id") != task["task_id"]:
            raise FailClosed("%s declares task_id %r but %s is %r"
                             % (questions_file, questions.get("task_id"),
                                task_file, task["task_id"]))
        try:
            sel = pj.select(gold, task, byte_budget=BYTE_BUDGET, record_budget=RECORD_BUDGET)
        except sl.SelectorError as e:
            raise FailClosed("selector refused task %s: %s" % (task["task_id"], e))
        validity = pj.validate(gold, sel, byte_budget=BYTE_BUDGET, record_budget=RECORD_BUDGET)
        selection_by_task[task["task_id"]] = set(sel["selected_ids"])

        task_dir = os.path.join(out_dir, "tasks", task["task_id"])
        context_json = pj.build_context_json(gold, task, sel)
        context_md = pj.build_context_md(gold, task, sel)
        ctx_json_digest = _write_canonical(os.path.join(task_dir, "context.json"), context_json)
        ctx_md_digest = _write_text(os.path.join(task_dir, "context.md"), context_md)
        artifact_digests = {"context.json": ctx_json_digest, "context.md": ctx_md_digest}
        task_digest = "sha256:" + sha256_hex(canonical_bytes(task))
        questions_digest = "sha256:" + sha256_hex(canonical_bytes(questions))
        meta = pj.build_context_meta(
            gold, task, task_digest, questions_digest, schema_digest, sel, budget,
            artifact_digests, input_observation_digest, validity)
        ctx_meta_digest = _write_canonical(
            os.path.join(task_dir, "context.meta.json"), meta)

        evaluation = None
        if nc_obj is not None:
            evaluation = ev.evaluate(gold, questions, source_selectors, manifest, nc_obj,
                                     derived_summary, sel,
                                     len(canonical_file_bytes(context_json)), budget)
        task_entries.append({
            "task_id": task["task_id"],
            "task_digest": task_digest,
            "questions_digest": questions_digest,
            "selector": sel["selector_ref"],
            "selectors": sel["selector_spec"],
            "projection": {
                "selected_count": len(sel["selected"]),
                "selected_ids": sel["selected_ids"],
                "omitted_count": len(sel["omitted"]),
                "used_bytes": sel["used_bytes"],
                "validity": validity,
                "context_artifact_digests": {**artifact_digests,
                                             "context.meta.json": ctx_meta_digest},
            },
            "evaluation": evaluation,
        })

    # task dependence, as data
    ids = [t["task_id"] for t in task_entries]
    sa, sb = selection_by_task[ids[0]], selection_by_task[ids[1]]
    task_dependence = {
        "task_a": ids[0],
        "task_b": ids[1],
        "selected_only_by_a": sorted(sa - sb),
        "selected_only_by_b": sorted(sb - sa),
        "selected_by_both": sorted(sa & sb),
        "symmetric_difference_count": len(sa ^ sb),
        "projections_differ": sa != sb,
        "neither_is_a_superset": not (sa <= sb) and not (sb <= sa),
    }

    # (5) status + report
    development_result = _decide_result(all_available, task_entries, task_dependence)
    report = {
        "schema": "o7.b1.report/v1",
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
        "selector_contract": {
            "selector_id": sl.SELECTOR_ID,
            "selector_version": sl.SELECTOR_VERSION,
            "selector_impl_digest": sl.selector_impl_digest(),
        },
        "budget": budget,
        "tasks": task_entries,
        "task_dependence": task_dependence,
        "status": {
            "deterministic_compilation_pipeline": "PASS" if all_available else "PARTIAL",
            "task_conditioned_projection": (
                "IMPLEMENTED" if task_dependence["projections_differ"] else "NOT_IMPLEMENTED"),
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
    if update_expectations:
        if not all_available:
            raise FailClosed(
                "refusing to freeze an expectation from an incomplete run: the "
                "CAS did not provide every input blob")
        with open(expected_path, "wb") as fh:
            fh.write(report_bytes)
        sys.stderr.write("expectation re-frozen: %s -> %s\n"
                         % (expected_path, report_json_digest))
    elif os.path.isfile(expected_path):
        with open(expected_path, "rb") as fh:
            expected_bytes = fh.read()
        if expected_bytes != report_bytes:
            raise FailClosed(
                "report.json does not match committed expected-report-v0.json "
                "(expected %s, got %s). If this follows a deliberate contract "
                "change, re-freeze with --update-expectations on a CAS-equipped "
                "machine."
                % ("sha256:" + sha256_hex(expected_bytes), report_json_digest))
    else:
        raise FailClosed(
            "no committed expected-report-v0.json to check against. The corrective "
            "round changed the schema, selector contract and metric names, so the "
            "previous expectation was retired rather than left silently wrong. "
            "Re-freeze it once with --update-expectations on a CAS-equipped machine.")

    return {
        "report": report,
        "report_json_digest": report_json_digest,
        "out_dir": out_dir,
    }


def _decide_result(all_available: bool, task_entries: list, task_dependence: dict) -> str:
    """PASS requires task-dependent selection AND a valid projection per task.

    Structural coverage alone is deliberately NOT enough any more: before the
    corrective round the projection arm's coverage was 1.0 by construction, so
    gating on it gated on nothing.
    """
    if not all_available:
        return "PARTIAL"
    if not task_dependence["projections_differ"] or not task_dependence["neither_is_a_superset"]:
        return "FAIL"
    for t in task_entries:
        if not t["projection"]["validity"]["valid"]:
            return "FAIL"
        evl = t["evaluation"]
        if evl is None:
            return "PARTIAL"
        C = evl["arms"]["projection"]
        B = evl["arms"]["negative_control"]
        if C["compiled_observation_coverage"] != 1.0:
            return "FAIL"
        if C["structural_question_support"] != 1.0:
            return "FAIL"
        if B["negative_control_diagnostics"]["stale_belief_count"] < 1:
            return "FAIL"
    return "PASS"


def main(argv=None):
    ap = argparse.ArgumentParser(description="Run the B1 development vertical for one fixture.")
    ap.add_argument("--fixture", default="case-0001")
    ap.add_argument("--data-root", default=os.path.expanduser("~/.local/share/o7-research"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--update-expectations", action="store_true",
                    help="re-freeze expected-report-v0.json from THIS run; refuses "
                         "unless every CAS input was available")
    args = ap.parse_args(argv)

    # Canonical, stable reproduce command (independent of the actual --out path,
    # so report.json stays byte-identical no matter where you run it).
    try:
        result = run(args.fixture, args.data_root, args.out,
                     update_expectations=args.update_expectations)
    except FailClosed as e:
        sys.stderr.write("FAIL CLOSED: %s\n" % e)
        return 2
    rep = result["report"]
    st = rep["status"]
    for k in ("deterministic_compilation_pipeline", "task_conditioned_projection",
              "development_result"):
        sys.stderr.write("%s: %s\n" % (k, st[k]))
    sys.stderr.write("report.json: %s\n" % result["report_json_digest"])
    return 0 if rep["status"]["development_result"] in ("PASS", "PARTIAL") else 3


if __name__ == "__main__":
    raise SystemExit(main())
