"""Shared library for the Scope N1 public-log pilot.

Reuses the frozen N0 corpus tools as a library (capture / snapshots / receipts /
hashing / jsonschema_mini) — the bundle format is identical; only the N0-specific
"demonstration only, zero benchmark" CLI rules are not reused. Adds:

  * public-development capture orchestration over a per-case *primary stream*
    (RTK runs over the stream that actually carries the tool payload);
  * a pilot snapshot manifest (anchors_sha256 in place of the N0
    evidence_map_sha256);
  * qodec envelope + o200k token metering for the four comparison arms.

No model calls. No shell. Canonical evidence is raw.*/rtk.* bytes only.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

PILOT_DIR = Path(__file__).resolve().parents[1]
# Bundle root is overridable so the four-arm runner / reproducibility gate can
# operate over a freshly captured bundle set (CI) as well as the committed cases.
CASES_DIR = Path(os.environ["PILOT_BUNDLES_DIR"]) if os.environ.get("PILOT_BUNDLES_DIR") \
    else PILOT_DIR / "cases"
SCHEMAS_DIR = PILOT_DIR / "schemas"
MANIFEST_PATH = PILOT_DIR / "pilot-manifest.json"
V2_DIR = PILOT_DIR.parent
CORPUS_TOOLS = V2_DIR / "corpus" / "tools"
REPO_ROOT = V2_DIR.parents[3]

# Reuse the frozen N0 corpus tools as a library.
sys.path.insert(0, str(CORPUS_TOOLS))
import capture  # noqa: E402
import jsonschema_mini as js  # noqa: E402
import receipts as rcpt  # noqa: E402
import snapshots as snap  # noqa: E402
from hashing import sha256_bytes, sha256_file  # noqa: E402

CODEC = "fold-grep-guarded"        # the VG policy, scored as a lossless notation layer
METER = "o200k"
PILOT_SNAPSHOT_VERSION = "pilot-snapshot-v1"
STREAM_FILE = {
    "raw.stdout": snap.RAW_STDOUT, "raw.stderr": snap.RAW_STDERR,
    "rtk.stdout": snap.RTK_STDOUT, "rtk.stderr": snap.RTK_STDERR,
}
INPUT_FILES = ("case.json", "provenance.json", "capture-recipe.json", "anchors.json")


def load_json(path: Path):
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def load_schema(name: str):
    return load_json(SCHEMAS_DIR / name)


def resolve_exe(argv0: str) -> str:
    if argv0 == "rtk":
        return os.environ.get("RTK_BIN") or shutil.which("rtk") or argv0
    if argv0 == "qodec":
        return os.environ.get("QODEC_BIN") or shutil.which("qodec") or argv0
    if argv0 in ("python3", "python"):
        return shutil.which(argv0) or sys.executable
    return shutil.which(argv0) or argv0


def case_ids() -> list[str]:
    return list(load_json(MANIFEST_PATH)["cases"])


def bundle_dir(case_id: str) -> Path:
    return CASES_DIR / case_id


# --------------------------------------------------------------------------- #
# Capture (native + RTK over the per-case primary stream)
# --------------------------------------------------------------------------- #
def _child_env(recipe: dict, home: str) -> dict:
    env = capture.build_child_env(recipe["environment_allowlist"], recipe, home)
    env["PYTHONDONTWRITEBYTECODE"] = "1"  # never leave .pyc beside python fixtures
    return env


def copy_inputs(case_id: str, dst: Path):
    src = bundle_dir(case_id)
    dst.mkdir(parents=True, exist_ok=True)
    for name in INPUT_FILES:
        if (src / name).exists():
            shutil.copy2(src / name, dst / name)
    if (src / "fixture").exists():
        shutil.copytree(src / "fixture", dst / "fixture", dirs_exist_ok=True)


def capture_case(case_id: str, work_dir: Path, out_dir: Path) -> dict:
    """Run the native command then RTK over the primary stream, writing snapshots
    and receipts into out_dir. work_dir holds the (writable) bundle inputs."""
    case = load_json(work_dir / "case.json")
    recipe = load_json(work_dir / case["capture_recipe_path"])
    identity = rcpt.assemble_identity(REPO_ROOT)
    home = tempfile.mkdtemp(prefix="pilot-home-")
    env = _child_env(recipe, home)
    (out_dir / "snapshots").mkdir(parents=True, exist_ok=True)
    (out_dir / "receipts").mkdir(parents=True, exist_ok=True)

    nat = recipe["native"]
    exec_argv = [resolve_exe(nat["argv"][0])] + nat["argv"][1:]
    r = capture.run_step(work_dir, exec_argv, nat.get("cwd", "."), nat.get("stdin_path"),
                         env, recipe["timeout_s"], record_argv=nat["argv"])
    if r["timed_out"]:
        raise capture.CaptureError(f"native capture timed out for {case_id}")
    if not capture.exit_code_matches(r["exit_code"], recipe["expected_exit_code_class"],
                                     recipe.get("expected_exit_code")):
        raise capture.CaptureError(
            f"{case_id}: native exit {r['exit_code']} violates "
            f"{recipe['expected_exit_code_class']} {recipe.get('expected_exit_code')}")
    (out_dir / snap.RAW_STDOUT).write_bytes(r["stdout"])
    (out_dir / snap.RAW_STDERR).write_bytes(r["stderr"])
    tid, tsha = nat["argv"][0], _sha_of(resolve_exe(nat["argv"][0]))
    rcpt.write_json(out_dir / snap.NATIVE_RECEIPT,
                    rcpt.build_receipt(case_id, "native", r, recipe, identity, tid, tsha))

    primary = {"raw.stdout": r["stdout"], "raw.stderr": r["stderr"]}[case["primary_stream"]]
    rtk_argv = recipe["rtk"]["argv"]
    r2 = capture.run_step(work_dir, [resolve_exe(rtk_argv[0])] + rtk_argv[1:], ".", None,
                          env, recipe["timeout_s"], stdin_bytes=primary, record_argv=rtk_argv)
    if r2["timed_out"]:
        raise capture.CaptureError(f"rtk capture timed out for {case_id}")
    changed = r2["stdout"] != primary
    mode = case.get("rtk_mode")
    if r2["exit_code"] != 0:
        classification = "failed"
    elif mode == "explicit-passthrough":
        classification = "explicit-passthrough"
    elif not changed:
        classification = "passthrough-never-worse"
    else:
        classification = "reduced"
    (out_dir / snap.RTK_STDOUT).write_bytes(r2["stdout"])
    (out_dir / snap.RTK_STDERR).write_bytes(r2["stderr"])
    rtk_extra = {
        "rtk_source_sha": os.environ.get("RTK_SOURCE_SHA"),
        "rtk_argv": rtk_argv,
        "rtk_classification": classification,
        "payload_changed": changed,
        "never_worse_returned_raw": (not changed) and mode != "explicit-passthrough",
    }
    rtid, rtsha = rtk_argv[0], _sha_of(resolve_exe(rtk_argv[0]))
    rcpt.write_json(out_dir / snap.RTK_RECEIPT,
                    rcpt.build_receipt(case_id, "rtk", r2, recipe, identity, rtid, rtsha, rtk_extra))
    shutil.rmtree(home, ignore_errors=True)
    if classification == "failed":
        raise capture.CaptureError(f"RTK failed for {case_id} (exit {r2['exit_code']})")
    return {"native_exit": r["exit_code"], "rtk_exit": r2["exit_code"],
            "rtk_classification": classification, "primary_stream": case["primary_stream"]}


def capture_into_temp(case_id: str) -> Path:
    tmp = Path(tempfile.mkdtemp(prefix=f"pilot-cap-{case_id}-"))
    copy_inputs(case_id, tmp)
    capture_case(case_id, tmp, tmp)
    return tmp


def _sha_of(path: str) -> str | None:
    p = Path(path)
    return sha256_file(p) if p.exists() and p.is_file() else None


# --------------------------------------------------------------------------- #
# Pilot snapshot manifest (anchors_sha256 replaces N0 evidence_map_sha256)
# --------------------------------------------------------------------------- #
def build_snapshot_manifest(bundle: Path, case: dict) -> dict:
    h = lambda rel: sha256_file(bundle / rel) if (bundle / rel).exists() else None
    from hashing import tree_sha256
    return {
        "case_id": case["case_id"],
        "snapshot_version": PILOT_SNAPSHOT_VERSION,
        "raw_stdout_sha256": h(snap.RAW_STDOUT),
        "raw_stderr_sha256": h(snap.RAW_STDERR),
        "rtk_stdout_sha256": h(snap.RTK_STDOUT),
        "rtk_stderr_sha256": h(snap.RTK_STDERR),
        "native_receipt_sha256": h(snap.NATIVE_RECEIPT),
        "rtk_receipt_sha256": h(snap.RTK_RECEIPT),
        "fixture_tree_sha256": tree_sha256(bundle / "fixture"),
        "capture_recipe_sha256": h(case["capture_recipe_path"]),
        "provenance_sha256": h(case["provenance_path"]),
        "anchors_sha256": h(case["anchors_path"]),
    }


def verify_snapshot_manifest(bundle: Path, case: dict, manifest: dict) -> list[str]:
    errs, computed = [], build_snapshot_manifest(bundle, case)
    for key, want in manifest.items():
        if key in ("case_id", "snapshot_version"):
            if want != computed.get(key):
                errs.append(f"snapshot-manifest {key}: {want!r} != {computed.get(key)!r}")
            continue
        got = computed.get(key)
        if got is None:
            errs.append(f"snapshot file missing for {key}")
        elif got != want:
            errs.append(f"snapshot hash mismatch for {key}: manifest {want} != file {got}")
    return errs


def rebuild_snapshot_manifest(case_id: str, bundle: Path | None = None) -> dict:
    bundle = bundle or bundle_dir(case_id)
    case = load_json(bundle / "case.json")
    sm = build_snapshot_manifest(bundle, case)
    rcpt.write_json(bundle / case["snapshot_manifest_path"], sm)
    return sm


# --------------------------------------------------------------------------- #
# qodec envelope + o200k token metering (four comparison arms)
# --------------------------------------------------------------------------- #
def _run(cmd: list[str], stdin: bytes) -> dict:
    t0 = time.monotonic()
    p = subprocess.run(cmd, input=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    return {"argv": cmd, "stdout": p.stdout, "stderr": p.stderr,
            "exit_code": p.returncode, "wall_time_s": round(time.monotonic() - t0, 6)}


def qodec_envelope(qodec_bin: str, data: bytes) -> tuple[dict, dict]:
    rec = _run([qodec_bin, "encode", "--codec", CODEC, "--meter", METER,
                "--passthrough-on-no-gain", "--json"], data)
    if rec["exit_code"] != 0:
        raise RuntimeError(f"qodec encode failed: {rec['stderr'][:400]!r}")
    return json.loads(rec["stdout"].decode("utf-8").strip()), rec


def qodec_decode(qodec_bin: str, content: str) -> bytes:
    rec = _run([qodec_bin, "decode"], content.encode("utf-8"))
    if rec["exit_code"] != 0:
        raise RuntimeError(f"qodec decode failed: {rec['stderr'][:400]!r}")
    return rec["stdout"]


def token_count(qodec_bin: str, data: bytes) -> int:
    env, _ = qodec_envelope(qodec_bin, data)
    return int(env["tokens_in"])
