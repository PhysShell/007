#!/usr/bin/env python3
"""run_case.py — the single documented command that runs the whole B1 vertical.

Requires the owner's local CAS. The task-dependent PROJECTION half needs no
private data at all and is reproducible by anyone via `project_case.py`; only the
3-arm evaluation (derived transcripts + sealed negative control) needs the CAS.
Both tools read the SAME task registry (`fixtures/<fixture>/tasks-v0.yaml`) via
`o7b1.registry` and drive projection through the SAME `o7b1.pipeline`, so they can
never disagree about which tasks exist or produce different projection artifacts.

    python3 research/b1-context/tools/run_case.py \
      --fixture case-0001 \
      --data-root "$HOME/.local/share/o7-research" \
      --out /tmp/o7-b1-case-0001

Read-only over inputs; writes derived transcripts and canonical artifacts only
under --out, never to the CAS or a source blob. Fails closed: any digest,
determinism, acceptance or expectation mismatch exits non-zero and never
fabricates a report.

Canonical, digest-bound outputs (byte-identical across runs): each task's
context.json/md/meta, projection-comparison.json, derived-manifest.actual.json,
report.json, report.md. The report contract is `o7.b1.report/v1` and the frozen
expectation is `fixtures/<fixture>/expected-report-v1.json`.

`--update-expectations` is transactional: it generates into a temporary sibling,
verifies every gate and two-run byte identity, and only then atomically replaces
the committed result artifacts and rewrites the v1 expectation. A failure leaves
the previously committed artifacts untouched.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import sys

import yaml

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, _HERE)

from o7b1 import evaluate as ev
from o7b1 import pipeline as pl
from o7b1 import projector as pj
from o7b1 import registry as rg
from o7b1 import report as rp
from o7b1 import selector as sl
from o7b1.canonical import (canonical_bytes, canonical_file_bytes, jsonl_bytes,
                            sha256_hex, sha256_of_file)
from o7b1.cas import BlobDigestMismatch, BlobUnavailable, Cas
from o7b1.extractors import ChatGPTBackendApiV0, ClaudeCodeV0
from o7b1.schema import default_schema_path, validate_gold_state

BYTE_BUDGET = 20000
RECORD_BUDGET = 32
EXPECTED_REPORT_FILENAME = "expected-report-v1.json"
LEGACY_EXPECTED_REPORT_FILENAME = "expected-report-v0.json"

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
    return {r["id"]: r for r in manifest["raw_sources"]}


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
        with open(os.path.join(derived_dir, "%s.jsonl" % sid), "wb") as fh:
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


def _build_derived_manifest(fixture: str, per_session: list) -> dict:
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
    return ("python3 research/b1-context/tools/run_case.py --fixture %s "
            "--data-root \"$HOME/.local/share/o7-research\" --out /tmp/o7-b1-%s"
            % (fixture, fixture))


def _guard_legacy_expectation(fdir: str) -> None:
    legacy = os.path.join(fdir, LEGACY_EXPECTED_REPORT_FILENAME)
    if os.path.isfile(legacy):
        raise FailClosed(
            "legacy %s is present. The report contract is now o7.b1.report/v1 and "
            "the expectation is %s; a v0 expectation must never be silently ignored "
            "or confused with v1 authority. Remove the v0 file."
            % (LEGACY_EXPECTED_REPORT_FILENAME, EXPECTED_REPORT_FILENAME))


def _generate(fixture: str, data_root: str, out_dir: str) -> dict:
    """Full generation into out_dir. Writes every canonical artifact; does NOT
    read, write or check the committed expectation. Returns the report bytes so a
    caller can either verify or (transactionally) freeze them."""
    fdir = _fixture_dir(fixture)
    _guard_legacy_expectation(fdir)
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

    # (2) extractors -> derived transcripts (written under out/derived, not the CAS)
    derived_summary, per_session, all_available = _run_extractors(
        cas, manifest, source_selectors, out_dir)
    derived_manifest = _build_derived_manifest(fixture, per_session)
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
    budget = {"byte_budget": BYTE_BUDGET, "record_budget": RECORD_BUDGET,
              "unit": "utf8_bytes+records"}

    # (3) task-dependent projection via the shared pipeline (writes context.* +
    #     enforces generic task-dependence acceptance + fixture expectation)
    proj = pl.run_projection(fixture, fdir, gold, schema_digest, out_dir, budget)
    comparison = proj["comparison"]
    registry_digest = proj["registry_digest"]
    _write_canonical(os.path.join(out_dir, "projection-comparison.json"), comparison)

    # (4) evaluation per task (needs the CAS-backed negative control)
    task_entries = []
    for entry in proj["entries"]:
        task, questions, sel, summary = (entry["task"], entry["questions"],
                                         entry["sel"], entry["summary"])
        evaluation = None
        if nc_obj is not None:
            evaluation = ev.evaluate(gold, questions, source_selectors, manifest, nc_obj,
                                     derived_summary, sel, summary["context_bytes"], budget)
        task_entries.append({
            "task_id": task["task_id"],
            "task_digest": "sha256:" + sha256_hex(canonical_bytes(task)),
            "questions_digest": "sha256:" + sha256_hex(canonical_bytes(questions)),
            "selector": sel["selector_ref"],
            "selectors": sel["selector_spec"],
            "projection": {
                "selected_count": summary["selected_count"],
                "selected_ids": summary["selected_ids"],
                "omitted_count": summary["omitted_count"],
                "used_bytes": summary["used_bytes"],
                "validity": summary["projection_validity"],
                "context_artifact_digests": summary["artifact_digests"],
            },
            "evaluation": evaluation,
        })

    development_result = _decide_result(all_available, task_entries, comparison)
    report = {
        "schema": "o7.b1.report/v1",
        "fixture_id": fixture,
        "cutoff_identity": gold["cutoff"]["identity"],
        "reproduce_command": _canonical_reproduce_command(fixture),
        "registry_digest": registry_digest,
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
        "task_dependence": comparison["task_dependence"],
        "status": {
            "deterministic_compilation_pipeline": "PASS" if all_available else "PARTIAL",
            "task_conditioned_projection": (
                "IMPLEMENTED" if comparison["task_dependence"]["projections_differ"]
                else "NOT_IMPLEMENTED"),
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
    _write_text(os.path.join(out_dir, "report.md"), rp.build_report_md(report))

    return {
        "report": report,
        "report_bytes": report_bytes,
        "report_json_digest": report_json_digest,
        "all_available": all_available,
        "out_dir": out_dir,
    }


def _decide_result(all_available: bool, task_entries: list, comparison: dict) -> str:
    """PASS requires accepted task-dependence AND a valid projection + healthy
    evaluation per task. Structural coverage alone is deliberately not enough:
    before the corrective round the projection arm's coverage was 1.0 by
    construction, so gating on it gated on nothing."""
    if not all_available:
        return "PARTIAL"
    if not comparison["task_dependence"]["acceptance"]["accepted"]:
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


def run(fixture: str, data_root: str, out_dir: str) -> dict:
    """Non-update run: generate, then verify against the committed v1 expectation."""
    g = _generate(fixture, data_root, out_dir)
    fdir = _fixture_dir(fixture)
    expected_path = os.path.join(fdir, EXPECTED_REPORT_FILENAME)
    if not os.path.isfile(expected_path):
        raise FailClosed(
            "no committed %s to check against. The corrective round retired the v0 "
            "expectation; re-freeze once with --update-expectations on a CAS-equipped "
            "machine." % EXPECTED_REPORT_FILENAME)
    with open(expected_path, "rb") as fh:
        expected_bytes = fh.read()
    if expected_bytes != g["report_bytes"]:
        raise FailClosed(
            "report.json does not match committed %s (expected %s, got %s). If this "
            "follows a deliberate contract change, re-freeze with "
            "--update-expectations on a CAS-equipped machine."
            % (EXPECTED_REPORT_FILENAME, "sha256:" + sha256_hex(expected_bytes),
               g["report_json_digest"]))
    return {"report": g["report"], "report_json_digest": g["report_json_digest"],
            "out_dir": out_dir}


# --- transactional --update-expectations -----------------------------------

_CANONICAL_TOP = ("report.json", "report.md", "projection-comparison.json",
                  "derived-manifest.actual.json")


def _canonical_files(root: str) -> list[str]:
    """Relative paths of canonical artifacts under root, excluding derived/."""
    out = []
    for name in _CANONICAL_TOP:
        if os.path.isfile(os.path.join(root, name)):
            out.append(name)
    tasks_root = os.path.join(root, "tasks")
    for dirpath, _dirs, files in os.walk(tasks_root):
        for f in sorted(files):
            full = os.path.join(dirpath, f)
            out.append(os.path.relpath(full, root))
    return sorted(out)


def _assert_two_run_identity(a: str, b: str) -> None:
    fa, fb = _canonical_files(a), _canonical_files(b)
    if fa != fb:
        raise FailClosed("two runs produced different canonical file sets: %s vs %s"
                         % (fa, fb))
    for rel in fa:
        with open(os.path.join(a, rel), "rb") as fh:
            ba = fh.read()
        with open(os.path.join(b, rel), "rb") as fh:
            bb = fh.read()
        if ba != bb:
            raise FailClosed("canonical output %s differs between two runs" % rel)


def _atomic_write_bytes(path: str, data: bytes) -> None:
    tmp = path + ".tmp"
    with open(tmp, "wb") as fh:
        fh.write(data)
    os.replace(tmp, path)


def run_update(fixture: str, data_root: str, target_out_dir: str) -> dict:
    """Transactional freeze: generate + verify in temp dirs, then atomically
    replace committed artifacts and rewrite expected-report-v1.json. A failure
    leaves the previously committed artifacts and expectation untouched."""
    fdir = _fixture_dir(fixture)
    _guard_legacy_expectation(fdir)
    target_out_dir = os.path.abspath(target_out_dir)
    tmp1 = target_out_dir + ".update-tmp1"
    tmp2 = target_out_dir + ".update-tmp2"
    bak = target_out_dir + ".update-bak"
    for d in (tmp1, tmp2, bak):
        shutil.rmtree(d, ignore_errors=True)
    try:
        g1 = _generate(fixture, data_root, tmp1)
        if not g1["all_available"]:
            raise FailClosed("refusing to freeze from an incomplete run: the CAS did "
                             "not provide every input blob")
        if g1["report"]["status"]["development_result"] != "PASS":
            raise FailClosed("refusing to freeze a non-PASS report (%s)"
                             % g1["report"]["status"]["development_result"])
        # second independent generation, byte-identity gate
        _generate(fixture, data_root, tmp2)
        _assert_two_run_identity(tmp1, tmp2)

        # derived transcripts are external CAS blobs; never commit their bodies
        shutil.rmtree(os.path.join(tmp1, "derived"), ignore_errors=True)

        # atomic swap of the committed results dir + the v1 expectation
        if os.path.exists(target_out_dir):
            os.rename(target_out_dir, bak)
        try:
            os.rename(tmp1, target_out_dir)
            _atomic_write_bytes(os.path.join(fdir, EXPECTED_REPORT_FILENAME),
                                g1["report_bytes"])
        except Exception:
            shutil.rmtree(target_out_dir, ignore_errors=True)
            if os.path.exists(bak):
                os.rename(bak, target_out_dir)
            raise
        shutil.rmtree(bak, ignore_errors=True)
        return {"report": g1["report"], "report_json_digest": g1["report_json_digest"],
                "out_dir": target_out_dir}
    finally:
        shutil.rmtree(tmp1, ignore_errors=True)
        shutil.rmtree(tmp2, ignore_errors=True)


def main(argv=None):
    ap = argparse.ArgumentParser(description="Run the B1 development vertical for one fixture.")
    ap.add_argument("--fixture", default="case-0001")
    ap.add_argument("--data-root", default=os.path.expanduser("~/.local/share/o7-research"))
    ap.add_argument("--out", required=True)
    ap.add_argument("--update-expectations", action="store_true",
                    help="transactionally re-freeze %s from THIS run (temp-generate, "
                         "verify every gate + two-run byte identity, then atomically "
                         "replace committed artifacts); refuses unless every CAS input "
                         "was available and the report is PASS" % EXPECTED_REPORT_FILENAME)
    args = ap.parse_args(argv)
    try:
        if args.update_expectations:
            result = run_update(args.fixture, args.data_root, args.out)
        else:
            result = run(args.fixture, args.data_root, args.out)
    except (FailClosed, pl.PipelineError, rg.RegistryError, sl.SelectorError) as e:
        sys.stderr.write("FAIL CLOSED: %s\n" % e)
        return 2
    rep = result["report"]
    st = rep["status"]
    for k in ("deterministic_compilation_pipeline", "task_conditioned_projection",
              "development_result"):
        sys.stderr.write("%s: %s\n" % (k, st[k]))
    sys.stderr.write("report.json: %s\n" % result["report_json_digest"])
    return 0 if st["development_result"] in ("PASS", "PARTIAL") else 3


if __name__ == "__main__":
    raise SystemExit(main())
