#!/usr/bin/env python3
"""Reproducible corpus compiler CLI.

Subcommands: validate, capture-native, capture-rtk, regenerate, verify, diff,
list, changed. No model calls. No shell. Network disabled during capture.

Canonical evidence is raw.*/rtk.* only; qodec/VG/hybrid outputs are derived and
rejected inside a bundle. `regenerate` is compare-only unless `--write` is given.
"""
from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import capture  # noqa: E402
import jsonschema_mini as js  # noqa: E402
import receipts as rcpt  # noqa: E402
import snapshots as snap  # noqa: E402
from hashing import sha256_bytes, sha256_file, sha256_json_file  # noqa: E402

CORPUS_DIR = Path(__file__).resolve().parents[1]
SCHEMAS_DIR = CORPUS_DIR / "schemas"
EXAMPLES_DIR = CORPUS_DIR / "examples"
MANIFEST_PATH = CORPUS_DIR / "manifest.json"
CONTRACT_PATH = CORPUS_DIR / "corpus-contract.json"
REPO_ROOT = CORPUS_DIR.parents[4]

MOVING_REFS = {"main", "master", "latest", "head", "trunk", "develop", "stable"}
DEMO_MARKERS = {
    "NON-BENCHMARK", "NON-GATING", "NOT PART OF THE 48 BASE CASES",
    "NOT PUBLIC-DEVELOPMENT", "NOT PUBLIC-VALIDATION", "NOT HELD-OUT",
}
STREAM_FILE = {
    "raw.stdout": snap.RAW_STDOUT, "raw.stderr": snap.RAW_STDERR,
    "rtk.stdout": snap.RTK_STDOUT, "rtk.stderr": snap.RTK_STDERR,
}


def load_json(path: Path):
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def load_schema(name: str):
    return load_json(SCHEMAS_DIR / name)


def resolve_bundle(case_id: str) -> Path:
    return EXAMPLES_DIR / case_id


def resolve_exe(argv0: str) -> str:
    if argv0 == "rtk":
        return os.environ.get("RTK_BIN") or shutil.which("rtk") or argv0
    if argv0 in ("python3", "python"):
        return shutil.which(argv0) or sys.executable
    return shutil.which(argv0) or argv0


# --------------------------------------------------------------------------- #
# Validation
# --------------------------------------------------------------------------- #
class Result:
    def __init__(self):
        self.violations: list[str] = []

    def fail(self, code: str, msg: str):
        self.violations.append(f"[{code}] {msg}")


def _read_lines(path: Path) -> list[str]:
    return path.read_text(encoding="utf-8", errors="replace").splitlines()


def validate_evidence(bundle: Path, evidence: dict, res: Result):
    seen_facts = set()
    for fact in evidence.get("facts", []):
        fid = fact.get("fact_id")
        if fid in seen_facts:
            res.fail("dup-fact-id", f"duplicate evidence fact_id {fid!r}")
        seen_facts.add(fid)
        stream = fact.get("stream")
        if stream not in STREAM_FILE:
            res.fail("evidence-stream", f"fact {fid}: stream {stream!r} is not a canonical raw/rtk stream")
            continue
        sfile = bundle / STREAM_FILE[stream]
        if not sfile.exists():
            res.fail("evidence-stream", f"fact {fid}: stream file {STREAM_FILE[stream]} missing")
            continue
        lines = _read_lines(sfile)
        span_text_parts = []
        for span in fact.get("evidence", []):
            s, e = span["start_line"], span["end_line"]
            if s > e:
                res.fail("evidence-span", f"fact {fid}: start_line {s} > end_line {e}")
                continue
            if s < 1 or e > len(lines):
                res.fail("evidence-span", f"fact {fid}: span {s}-{e} outside file (1-{len(lines)})")
                continue
            span_text_parts.append("\n".join(lines[s - 1:e]))
        span_text = "\n".join(span_text_parts)
        kind = fact.get("kind")
        val = fact.get("value")
        if kind in ("exact", "entity", "decision-fact"):
            if not isinstance(val, str):
                res.fail("evidence-value", f"fact {fid}: kind {kind} requires a string value")
            elif val not in span_text:
                res.fail("evidence-literal", f"fact {fid}: literal {val!r} not present in its evidence span")
        elif kind == "absence":
            whole = "\n".join(lines)
            if isinstance(val, str) and val in whole:
                res.fail("evidence-absence", f"fact {fid}: declared-absent literal {val!r} is actually present in {stream}")
        elif kind in ("count", "set", "relation", "ordering"):
            if val is None:
                res.fail("evidence-value", f"fact {fid}: kind {kind} requires a value")


def validate_bundle(case_id: str, manifest: dict, res: Result):
    bundle = resolve_bundle(case_id)
    if not (bundle / "case.json").exists():
        res.fail("missing-case", f"case {case_id}: case.json not found at {bundle}")
        return
    case = load_json(bundle / "case.json")

    for e in js.validate(case, load_schema("case-bundle.schema.json")):
        res.fail("schema", f"case {case_id} case.json: {e}")

    # status / membership rules
    status = case.get("status")
    if case_id in manifest.get("benchmark_cases", []):
        res.fail("benchmark-leak", f"case {case_id} appears in benchmark_cases (N0 allows zero benchmark cases)")
    if status == "demonstration":
        if case_id not in manifest.get("demonstration_cases", []):
            res.fail("manifest-membership", f"demonstration case {case_id} not listed in demonstration_cases")
        if case_id in manifest.get("benchmark_cases", []):
            res.fail("demo-leak", f"demonstration case {case_id} must never be in benchmark_cases")
        if not DEMO_MARKERS.issubset(set(case.get("markers", []))):
            res.fail("demo-markers", f"demonstration case {case_id} missing required NON-BENCHMARK marker set")
    else:
        res.fail("status", f"case {case_id}: status {status!r} not permitted in Scope N0 (demonstration only)")

    # path safety + derived leakage
    for e in snap.check_path_safety(bundle):
        res.fail("path", f"case {case_id}: {e}")
    for e in snap.check_derived_leakage(bundle):
        res.fail("qodec-leak", f"case {case_id}: {e}")

    # provenance
    prov_path = bundle / case.get("provenance_path", "provenance.json")
    if prov_path.exists():
        prov = load_json(prov_path)
        for e in js.validate(prov, load_schema("provenance.schema.json")):
            res.fail("schema", f"case {case_id} provenance: {e}")
        if not prov.get("license"):
            res.fail("license", f"case {case_id}: provenance missing license")
        if prov.get("origin_kind") == "external-sanitized":
            up = prov.get("upstream_revision", "")
            if not up or up.lower() in MOVING_REFS:
                res.fail("mutable-revision", f"case {case_id}: external source needs an immutable upstream_revision (got {up!r})")
            if not prov.get("upstream_license"):
                res.fail("license", f"case {case_id}: external source missing upstream_license")
    else:
        res.fail("missing-provenance", f"case {case_id}: provenance.json missing")

    # capture recipe + shell safety
    recipe_path = bundle / case.get("capture_recipe_path", "capture-recipe.json")
    recipe = None
    if recipe_path.exists():
        recipe = load_json(recipe_path)
        for e in js.validate(recipe, load_schema("capture-recipe.schema.json")):
            res.fail("schema", f"case {case_id} capture-recipe: {e}")
        for step in recipe.get("setup", []) + [recipe.get("native", {}), {"argv": recipe.get("rtk", {}).get("argv", [])}]:
            argv = step.get("argv") or []
            try:
                capture.assert_argv_no_shell(argv)
            except capture.CaptureError as ce:
                res.fail("shell", f"case {case_id}: {ce}")
        for name in recipe.get("environment_allowlist", []):
            if capture.env_name_is_forbidden(name):
                res.fail("env-injection", f"case {case_id}: forbidden env var {name} in allowlist")
        if status == "demonstration" and recipe.get("network_policy") != "disabled":
            res.fail("network", f"case {case_id}: demonstration must set network_policy=disabled")
        if recipe.get("expected_exit_code_class") == "exact" and "expected_exit_code" not in recipe:
            res.fail("exit-code", f"case {case_id}: expected_exit_code_class=exact requires expected_exit_code")
    else:
        res.fail("missing-recipe", f"case {case_id}: capture-recipe.json missing")

    # evidence map
    ev_path = bundle / case.get("evidence_map_path", "evidence-map.json")
    if ev_path.exists():
        ev = load_json(ev_path)
        for e in js.validate(ev, load_schema("evidence-map.schema.json")):
            res.fail("schema", f"case {case_id} evidence-map: {e}")
        validate_evidence(bundle, ev, res)
    else:
        res.fail("missing-evidence", f"case {case_id}: evidence-map.json missing")

    # snapshot manifest + hashes + receipts
    sm_path = bundle / case.get("snapshot_manifest_path", "snapshot-manifest.json")
    if sm_path.exists():
        sm = load_json(sm_path)
        for e in js.validate(sm, load_schema("snapshot-manifest.schema.json")):
            res.fail("schema", f"case {case_id} snapshot-manifest: {e}")
        for e in snap.verify_hashes(bundle, case, sm):
            res.fail("hash", f"case {case_id}: {e}")
        _validate_receipts(bundle, case_id, case, recipe, res)
    else:
        res.fail("missing-snapshot-manifest", f"case {case_id}: snapshot-manifest.json missing")


def _validate_receipts(bundle: Path, case_id: str, case: dict, recipe: dict | None, res: Result):
    schema = load_schema("execution-receipt.schema.json")
    nat_p = bundle / snap.NATIVE_RECEIPT
    rtk_p = bundle / snap.RTK_RECEIPT
    if nat_p.exists():
        nat = load_json(nat_p)
        for e in js.validate(nat, schema):
            res.fail("schema", f"case {case_id} native receipt: {e}")
        if nat.get("phase") != "native":
            res.fail("receipt", f"case {case_id}: native receipt phase != native")
        if recipe is not None and not capture.exit_code_matches(
                nat.get("exit_code", 99), recipe["expected_exit_code_class"], recipe.get("expected_exit_code")):
            res.fail("exit-code", f"case {case_id}: native exit {nat.get('exit_code')} violates class {recipe['expected_exit_code_class']}")
    else:
        res.fail("missing-receipt", f"case {case_id}: native receipt missing")
    if rtk_p.exists():
        rtk = load_json(rtk_p)
        for e in js.validate(rtk, schema):
            res.fail("schema", f"case {case_id} rtk receipt: {e}")
        if rtk.get("phase") != "rtk":
            res.fail("receipt", f"case {case_id}: rtk receipt phase != rtk")
        if rtk.get("rtk_classification") == "failed" or rtk.get("exit_code") != 0:
            res.fail("rtk-failed", f"case {case_id}: RTK phase failed (exit {rtk.get('exit_code')}) — not an accepted snapshot")
        if case.get("snapshot_policy") == "raw-and-rtk":
            rtk_out = bundle / snap.RTK_STDOUT
            if rtk_out.exists() and rtk_out.stat().st_size == 0:
                res.fail("rtk-empty", f"case {case_id}: RTK stdout is empty for a raw-and-rtk case")
    else:
        res.fail("missing-receipt", f"case {case_id}: rtk receipt missing")


def validate_manifest(manifest: dict, res: Result):
    for e in js.validate(manifest, load_schema("corpus-manifest.schema.json")):
        res.fail("schema", f"manifest: {e}")
    contract = load_json(CONTRACT_PATH)
    if manifest.get("contract_version") != contract.get("contract_version"):
        res.fail("contract-version", "manifest contract_version != corpus-contract.json")
    if manifest.get("benchmark_cases"):
        res.fail("benchmark-data", f"N0 forbids benchmark cases; found {manifest['benchmark_cases']}")
    both = set(manifest.get("benchmark_cases", [])) & set(manifest.get("demonstration_cases", []))
    if both:
        res.fail("dup-case-id", f"cases in both benchmark and demonstration lists: {sorted(both)}")


# --------------------------------------------------------------------------- #
# Capture
# --------------------------------------------------------------------------- #
def _tool_identity_and_sha(argv0: str) -> tuple[str, str | None]:
    resolved = resolve_exe(argv0)
    sha = sha256_file(resolved) if resolved and Path(resolved).exists() else None
    return argv0, sha


def capture_native(case_id: str, bundle: Path, out_dir: Path | None = None) -> dict:
    out_dir = out_dir or bundle
    case = load_json(bundle / "case.json")
    recipe = load_json(bundle / case["capture_recipe_path"])
    identity = rcpt.assemble_identity(REPO_ROOT)
    home = tempfile.mkdtemp(prefix="corpus-home-")
    env = capture.build_child_env(recipe["environment_allowlist"], recipe, home)

    for step in recipe.get("setup", []):
        exec_argv = [resolve_exe(step["argv"][0])] + step["argv"][1:]
        r = capture.run_step(bundle, exec_argv, step.get("cwd", "."), step.get("stdin_path"),
                             env, recipe["timeout_s"], record_argv=step["argv"])
        if r["exit_code"] != 0 or r["timed_out"]:
            raise capture.CaptureError(f"setup step failed: {step['argv']} exit={r['exit_code']}")

    nat = recipe["native"]
    exec_argv = [resolve_exe(nat["argv"][0])] + nat["argv"][1:]
    r = capture.run_step(bundle, exec_argv, nat.get("cwd", "."), nat.get("stdin_path"),
                         env, recipe["timeout_s"], record_argv=nat["argv"])
    if r["timed_out"]:
        raise capture.CaptureError(f"native capture timed out for {case_id}")
    if not capture.exit_code_matches(r["exit_code"], recipe["expected_exit_code_class"], recipe.get("expected_exit_code")):
        raise capture.CaptureError(f"native exit {r['exit_code']} violates {recipe['expected_exit_code_class']}")

    (out_dir / "snapshots").mkdir(parents=True, exist_ok=True)
    (out_dir / "receipts").mkdir(parents=True, exist_ok=True)
    (out_dir / snap.RAW_STDOUT).write_bytes(r["stdout"])
    (out_dir / snap.RAW_STDERR).write_bytes(r["stderr"])
    tid, tsha = _tool_identity_and_sha(nat["argv"][0])
    receipt = rcpt.build_receipt(case_id, "native", r, recipe, identity, tid, tsha)
    rcpt.write_json(out_dir / snap.NATIVE_RECEIPT, receipt)
    shutil.rmtree(home, ignore_errors=True)
    return receipt


def capture_rtk(case_id: str, bundle: Path, out_dir: Path | None = None) -> dict:
    out_dir = out_dir or bundle
    case = load_json(bundle / "case.json")
    recipe = load_json(bundle / case["capture_recipe_path"])
    identity = rcpt.assemble_identity(REPO_ROOT)
    raw_path = out_dir / snap.RAW_STDOUT
    if not raw_path.exists():
        raise capture.CaptureError("capture-rtk requires an existing raw.stdout (run capture-native first)")
    raw = raw_path.read_bytes()

    home = tempfile.mkdtemp(prefix="corpus-home-")
    env = capture.build_child_env(recipe["environment_allowlist"], recipe, home)
    rtk_argv = recipe["rtk"]["argv"]
    exec_argv = [resolve_exe(rtk_argv[0])] + rtk_argv[1:]
    r = capture.run_step(bundle, exec_argv, ".", None, env, recipe["timeout_s"],
                         stdin_bytes=raw, record_argv=rtk_argv)
    if r["timed_out"]:
        raise capture.CaptureError(f"rtk capture timed out for {case_id}")

    changed = r["stdout"] != raw
    mode = case.get("rtk_mode")
    if r["exit_code"] != 0:
        classification = "failed"
    elif mode == "explicit-passthrough":
        classification = "explicit-passthrough"
    elif mode == "unsupported":
        classification = "unsupported"
    elif not changed:
        classification = "passthrough-never-worse"
    else:
        classification = "reduced"

    (out_dir / snap.RTK_STDOUT).write_bytes(r["stdout"])
    (out_dir / snap.RTK_STDERR).write_bytes(r["stderr"])
    tid, tsha = _tool_identity_and_sha(rtk_argv[0])
    rtk_extra = {
        "rtk_source_sha": load_json(CONTRACT_PATH).get("rtk_source_sha"),
        "rtk_argv": rtk_argv,
        "rtk_classification": classification,
        "payload_changed": changed,
        "never_worse_returned_raw": (not changed) and mode != "explicit-passthrough",
    }
    receipt = rcpt.build_receipt(case_id, "rtk", r, recipe, identity, tid, tsha, rtk_extra)
    rcpt.write_json(out_dir / snap.RTK_RECEIPT, receipt)
    shutil.rmtree(home, ignore_errors=True)
    if classification == "failed":
        raise capture.CaptureError(f"RTK failed for {case_id} (exit {r['exit_code']}); refusing snapshot")
    return receipt


def _copy_bundle_inputs(case_id: str, dst: Path):
    """Copy the non-generated bundle inputs (case/recipe/fixture/...) into dst so
    a capture can run against them in isolation."""
    src = resolve_bundle(case_id)
    for name in ("case.json", "provenance.json", "capture-recipe.json", "evidence-map.json"):
        if (src / name).exists():
            shutil.copy2(src / name, dst / name)
    if (src / "fixture").exists():
        shutil.copytree(src / "fixture", dst / "fixture")


def capture_into_temp(case_id: str) -> Path:
    """Capture both phases into an isolated temp copy of the bundle inputs."""
    tmp = Path(tempfile.mkdtemp(prefix=f"corpus-cap-{case_id}-"))
    _copy_bundle_inputs(case_id, tmp)
    capture_native(case_id, tmp, tmp)
    capture_rtk(case_id, tmp, tmp)
    return tmp


def rebuild_snapshot_manifest(case_id: str, bundle: Path | None = None):
    bundle = bundle or resolve_bundle(case_id)
    case = load_json(bundle / "case.json")
    sm = snap.build_snapshot_manifest(bundle, case)
    rcpt.write_json(bundle / case["snapshot_manifest_path"], sm)
    return sm


# --------------------------------------------------------------------------- #
# Compare helpers
# --------------------------------------------------------------------------- #
SNAP_FILES = [snap.RAW_STDOUT, snap.RAW_STDERR, snap.RTK_STDOUT, snap.RTK_STDERR]


def compare_captures(a: Path, b: Path) -> list[str]:
    diffs = []
    for rel in SNAP_FILES:
        ha, hb = sha256_file(a / rel), sha256_file(b / rel)
        if ha != hb:
            diffs.append(f"snapshot differs: {rel} ({ha[:12]} != {hb[:12]})")
    for rel in (snap.NATIVE_RECEIPT, snap.RTK_RECEIPT):
        ra, rb = rcpt.semantic_view(load_json(a / rel)), rcpt.semantic_view(load_json(b / rel))
        if ra != rb:
            keys = [k for k in ra if ra.get(k) != rb.get(k)]
            diffs.append(f"receipt semantic diff in {rel}: fields {keys}")
    return diffs


# --------------------------------------------------------------------------- #
# Commands
# --------------------------------------------------------------------------- #
def cmd_validate(args) -> int:
    manifest = load_json(MANIFEST_PATH)
    res = Result()
    validate_manifest(manifest, res)
    ids = [args.case] if args.case else sorted(set(manifest.get("demonstration_cases", []) + manifest.get("benchmark_cases", [])))
    for cid in ids:
        validate_bundle(cid, manifest, res)
    return _report(res)


def cmd_verify(args) -> int:
    # verify committed bundles without executing any tool (integrity + schemas only)
    return cmd_validate(args)


def cmd_capture_native(args) -> int:
    capture_native(args.case, resolve_bundle(args.case))
    print(f"captured native snapshots for {args.case}")
    return 0


def cmd_capture_rtk(args) -> int:
    capture_rtk(args.case, resolve_bundle(args.case))
    print(f"captured rtk snapshots for {args.case}")
    return 0


def cmd_regenerate(args) -> int:
    case_id = args.case
    if args.write:
        bundle = resolve_bundle(case_id)
        capture_native(case_id, bundle)
        capture_rtk(case_id, bundle)
        rebuild_snapshot_manifest(case_id, bundle)
        print(f"[--write] regenerated snapshots + manifest for {case_id}")
        return 0
    fresh = capture_into_temp(case_id)
    diffs = compare_captures(resolve_bundle(case_id), fresh)
    shutil.rmtree(fresh, ignore_errors=True)
    if diffs:
        print("REGENERATE: committed snapshots differ from a fresh capture:", file=sys.stderr)
        for d in diffs:
            print("  " + d, file=sys.stderr)
        return 1
    print(f"regenerate compare-only: {case_id} matches committed snapshots")
    return 0


def cmd_diff(args) -> int:
    case_id = args.case
    bundle = resolve_bundle(case_id)
    fresh = capture_into_temp(case_id)
    print(f"diff for {case_id} (committed vs fresh capture):")
    for rel in SNAP_FILES:
        ho, hn = sha256_file(bundle / rel), sha256_file(fresh / rel)
        so, sn = (bundle / rel).stat().st_size, (fresh / rel).stat().st_size
        flag = "CHANGED" if ho != hn else "same"
        print(f"  {rel:22s} {flag}  size {so}->{sn}  {ho[:12]}->{hn[:12]}")
    for rel in (snap.NATIVE_RECEIPT, snap.RTK_RECEIPT):
        o, n = load_json(bundle / rel), load_json(fresh / rel)
        print(f"  {rel:22s} exit {o.get('exit_code')}->{n.get('exit_code')}"
              + (f"  rtk_class {o.get('rtk_classification')}->{n.get('rtk_classification')}" if "rtk" in rel else ""))
    shutil.rmtree(fresh, ignore_errors=True)
    return 0


def cmd_list(args) -> int:
    manifest = load_json(MANIFEST_PATH)
    print(f"contract_version: {manifest.get('contract_version')}")
    print(f"benchmark_cases: {len(manifest.get('benchmark_cases', []))}")
    for cid in manifest.get("demonstration_cases", []):
        bundle = resolve_bundle(cid)
        case = load_json(bundle / "case.json") if (bundle / "case.json").exists() else {}
        complete = all((bundle / f).exists() for f in
                       [snap.RAW_STDOUT, snap.RTK_STDOUT, snap.NATIVE_RECEIPT, snap.RTK_RECEIPT,
                        case.get("snapshot_manifest_path", "snapshot-manifest.json")])
        print(f"  {cid}  status={case.get('status')}  rtk_mode={case.get('rtk_mode')}  complete={complete}")
    return 0


def cmd_changed(args) -> int:
    manifest = load_json(MANIFEST_PATH)
    files = []
    if args.files:
        files = [f.strip() for f in args.files.replace(",", "\n").split("\n") if f.strip()]
    elif args.base and args.head:
        import subprocess
        r = subprocess.run(["git", "-C", str(REPO_ROOT), "diff", "--name-only", f"{args.base}..{args.head}"],
                           capture_output=True, text=True)
        files = [f for f in r.stdout.splitlines() if f.strip()]
    affected = set()
    prefix = "qodec/evals/interop/v2/corpus/examples/"
    for f in files:
        if f.startswith(prefix):
            rest = f[len(prefix):]
            cid = rest.split("/", 1)[0]
            if cid:
                affected.add(cid)
    for cid in sorted(affected):
        print(cid)
    if not affected:
        print("# no case bundles affected (manifest/schema/docs-only change)", file=sys.stderr)
    return 0


def _report(res: Result) -> int:
    if res.violations:
        print(f"CORPUS INVALID — {len(res.violations)} violation(s):", file=sys.stderr)
        for v in res.violations:
            print("  " + v, file=sys.stderr)
        return 1
    print("CORPUS VALID")
    return 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Reproducible corpus compiler.")
    sub = ap.add_subparsers(dest="command", required=True)
    for name in ("validate", "verify"):
        p = sub.add_parser(name)
        p.add_argument("--case", default=None)
    for name in ("capture-native", "capture-rtk", "diff"):
        p = sub.add_parser(name)
        p.add_argument("--case", required=True)
    p = sub.add_parser("regenerate")
    p.add_argument("--case", required=True)
    p.add_argument("--write", action="store_true")
    sub.add_parser("list")
    p = sub.add_parser("changed")
    p.add_argument("--files", default=None)
    p.add_argument("--base", default=None)
    p.add_argument("--head", default=None)

    args = ap.parse_args(argv)
    dispatch = {
        "validate": cmd_validate, "verify": cmd_verify,
        "capture-native": cmd_capture_native, "capture-rtk": cmd_capture_rtk,
        "regenerate": cmd_regenerate, "diff": cmd_diff, "list": cmd_list, "changed": cmd_changed,
    }
    return dispatch[args.command](args)


if __name__ == "__main__":
    raise SystemExit(main())
