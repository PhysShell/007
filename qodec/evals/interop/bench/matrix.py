"""Level-2 matrix orchestration: immutable run identity, directory policy,
crash-durable pass 1 / pass 2, a verified pass-1 receipt, and resume.

Kept free of the endpoint and of qodec so a fake deterministic reader can drive
the *real* resume path in tests. The caller supplies:

  - `manifest`: the current run identity (built by the runner from preflight +
    args). On a fresh run it is written to `run-manifest.json` before the first
    request; on `--resume` it is compared field-by-field against the stored one
    and ANY mismatch aborts before a single matrix request is sent.
  - `run_one(case, q, arm, repeat) -> record`: performs one request.
  - `crash_hook(phase, n)`: test seam; may raise to simulate a crash.

The journal (`records.jsonl`) is the source of truth. `run-state.json` is only a
progress receipt. `pass1-complete.json` is written once, after the primary key
set is verified exact.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

from . import durability

ARMS = ["raw", "raw+brief", "encoded+brief"]

RECORDS = "records.jsonl"
MANIFEST = "run-manifest.json"
STATE = "run-state.json"
PASS1 = "pass1-complete.json"


class DirectoryPolicyError(RuntimeError):
    """A fresh run pointed at a non-empty run dir, or --resume at one lacking a
    manifest. Never append over an existing run by accident."""


class ManifestMismatch(RuntimeError):
    """The resumed environment is not the run that was started. Carries the list
    of mismatching identity fields."""

    def __init__(self, mismatches: list[str]):
        self.mismatches = mismatches
        super().__init__("run identity changed since start:\n  " + "\n  ".join(mismatches))


class Pass1Incomplete(RuntimeError):
    """The primary (repeat 0) key set is not exactly unique_questions × arms."""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def diff_manifest(stored: dict, current: dict, prefix: str = "") -> list[str]:
    """Dotted paths where the stored identity and the current one disagree.
    Recurses into dicts (so a changed nested hash is reported precisely) and
    compares everything else by value."""
    out: list[str] = []
    for k in sorted(set(stored) | set(current)):
        a_present, b_present = k in stored, k in current
        a, b = stored.get(k), current.get(k)
        path = f"{prefix}{k}"
        if not a_present:
            out.append(f"{path}: absent in manifest, now {b!r}")
        elif not b_present:
            out.append(f"{path}: was {a!r}, absent now")
        elif isinstance(a, dict) and isinstance(b, dict):
            out += diff_manifest(a, b, path + ".")
        elif a != b:
            out.append(f"{path}: {a!r} != {b!r}")
    return out


def assert_dir_policy(run_dir: Path, resume: bool) -> None:
    """Enforce the existing-directory policy before anything is written. Fresh
    runs refuse a dir that already holds a journal/manifest/state; a resume
    requires the dir and its manifest to exist (no empty new run via --resume)."""
    run_dir = Path(run_dir)
    manifest_path = run_dir / MANIFEST
    if resume:
        if not run_dir.exists():
            raise DirectoryPolicyError(f"--resume: run dir {run_dir} does not exist")
        if not manifest_path.exists():
            raise DirectoryPolicyError(
                f"--resume: {run_dir}/{MANIFEST} missing — nothing to resume (an empty "
                "new run via --resume is refused)")
    else:
        clashes = [n for n in (RECORDS, MANIFEST, STATE) if (run_dir / n).exists()]
        if clashes:
            raise DirectoryPolicyError(
                f"run dir {run_dir} already holds {', '.join(clashes)} — refusing to append; "
                "use --resume to continue it or pick a new --name")


def _group(items: list[dict]) -> dict[str, list[dict]]:
    by_case: dict[str, list[dict]] = {}
    for q in items:
        by_case.setdefault(q["case"], []).append(q)
    return by_case


def flagged_questions(records: list[dict], arms=ARMS) -> list[dict]:
    """Questions to re-run in pass 2, decided from the primary (repeat 0)
    results: malformed JSON, alias leakage, an invalid identifier, a
    raw/raw+brief disagreement, or a codec loss (raw+brief right, encoded wrong).
    Returns task-shaped dicts (case, id) in first-seen order."""
    primary = [r for r in records if r["repeat"] == 0]
    by_q: dict[tuple, dict[str, dict]] = {}
    order: list[tuple] = []
    for r in primary:
        qk = (r["case"], r["question"])
        if qk not in by_q:
            by_q[qk] = {}
            order.append(qk)
        by_q[qk][r["arm"]] = r

    out = []
    for qk in order:
        byarm = by_q[qk]
        rs = list(byarm.values())
        flag = False
        if any(r["malformed"] for r in rs):
            flag = True
        elif any(r.get("alias_leaks") for r in rs) or any(r.get("invalid_identifiers") for r in rs):
            flag = True
        else:
            raw, rb, eb = byarm.get("raw"), byarm.get("raw+brief"), byarm.get("encoded+brief")
            if raw and rb and raw["correct"] != rb["correct"]:
                flag = True
            elif rb and eb and rb["correct"] and not eb["correct"]:
                flag = True
        if flag:
            out.append({"case": qk[0], "id": qk[1]})
    return out


def _verify_pass1(records: list[dict], tasks: list[dict], arms) -> int:
    got = [(r["case"], r["question"], r["arm"]) for r in records if r["repeat"] == 0]
    expected = {(q["case"], q["id"], arm) for q in tasks for arm in arms}
    got_set = set(got)
    duplicates = len(got) - len(got_set)
    missing = sorted(expected - got_set)
    extra = sorted(got_set - expected)
    if duplicates or missing or extra:
        raise Pass1Incomplete(
            f"primary key set invalid: duplicates={duplicates} "
            f"missing={len(missing)} extra={len(extra)} "
            f"(examples missing={missing[:3]} extra={extra[:3]})")
    return len(got)


def _write_pass1_receipt(run_dir: Path, tasks: list[dict], arms, n_primary: int) -> None:
    man = json.loads((run_dir / MANIFEST).read_text())
    receipt = {
        "pass1_complete": True,
        "expected": len(tasks) * len(arms),
        "actual": n_primary,
        "duplicates": 0,
        "missing": [],
        "unique_questions": len(tasks),
        "arms": list(arms),
        "records_sha256": sha256_bytes((run_dir / RECORDS).read_bytes()),
        "tasks_snapshot_sha256": man.get("tasks_snapshot_sha256"),
        "manifest_sha256": sha256_bytes((run_dir / MANIFEST).read_bytes()),
    }
    durability.atomic_write(run_dir / PASS1, json.dumps(receipt, indent=2) + "\n")


def run_matrix(run_dir, *, manifest: dict, tasks: list[dict], run_one,
               resume: bool = False, repeats: int = 3, arms=ARMS,
               crash_hook=None) -> dict:
    run_dir = Path(run_dir)
    arms = list(arms)
    assert_dir_policy(run_dir, resume)

    manifest_path = run_dir / MANIFEST
    if resume:
        stored = json.loads(manifest_path.read_text())
        mismatches = diff_manifest(stored, manifest)
        if mismatches:
            raise ManifestMismatch(mismatches)   # aborts before any request
    else:
        run_dir.mkdir(parents=True, exist_ok=True)
        durability.atomic_write(manifest_path, json.dumps(manifest, indent=2, sort_keys=True) + "\n")

    log = durability.RecordLog(run_dir / RECORDS)
    if resume:
        log.load_existing()
    log.open()

    by_case = _group(tasks)
    expected_primary = len(tasks) * len(arms)
    state_path = run_dir / STATE
    executed = {"pass1": 0, "pass2": 0}
    last_key = {"k": None}

    def write_state(phase: str, expected: int) -> None:
        # Progress receipt only — the journal is the source of truth, so this is
        # written without fsync (atomic replace still prevents partial reads).
        durability.atomic_write(state_path, json.dumps({
            "phase": phase,
            "last_completed_key": list(last_key["k"]) if last_key["k"] else None,
            "completed": len(log.records),
            "expected": expected,
        }, indent=2) + "\n", fsync=False)

    def do(case: str, q: dict, arm: str, repeat: int, phase: str, expected: int) -> None:
        key = (case, q["id"], arm, repeat)
        if log.has(key):
            return
        log.append(run_one(case, q, arm, repeat))
        last_key["k"] = key
        write_state(phase, expected)
        executed[phase] += 1
        if crash_hook is not None:
            crash_hook(phase, executed[phase])

    # Pass 1 — every (case, question) once, content-grouped for prefix reuse.
    write_state("pass1", expected_primary)
    for case, qs in by_case.items():
        for arm in arms:
            for q in qs:
                do(case, q, arm, 0, "pass1", expected_primary)

    # Pass-1 receipt: verify the exact primary key set every time (defensive),
    # but write the receipt only once — on resume its records.jsonl hash would
    # include pass-2 lines and no longer describe pass-1 completion.
    n_primary = _verify_pass1(log.records, tasks, arms)
    if not (run_dir / PASS1).exists():
        _write_pass1_receipt(run_dir, tasks, arms, n_primary)

    # Flag from the primary results, then pass 2 (repeats of flagged only).
    # Resolve the flagged keys back to the FULL task dicts (flagging only carries
    # case/id, but run_one needs the question text and category).
    flagged_keys = {(f["case"], f["id"]) for f in flagged_questions(log.records, arms)}
    to_repeat = [q for q in tasks if (q["case"], q["id"]) in flagged_keys]
    expected_total = expected_primary + len(to_repeat) * len(arms) * max(0, repeats - 1)
    write_state("pass2", expected_total)
    for repeat in range(1, max(1, repeats)):
        for case, qs in _group(to_repeat).items():
            for arm in arms:
                for q in qs:
                    do(case, q, arm, repeat, "pass2", expected_total)

    log.close()
    write_state("complete", expected_total)
    return {"records": log.records, "to_repeat": to_repeat,
            "n_primary": n_primary, "expected_total": expected_total}
