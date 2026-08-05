"""Standalone deterministic STRUCTURAL evaluator for o7.b1.evaluation/v1.

This is the R4B implementation of the ruler designed in R4A/R4A.1/R4A.2. It is
*contract-driven*: every identifier, requirement, digest and gate target comes
from the supplied `o7.b1.evaluation-contract/v1` (revision 3) and the supplied
frozen inputs. There is no case literal in this module — grep it.

What it does NOT do (by construction):
  * never reruns extraction or projection;
  * never uses an LLM judge or any semantic-answerability heuristic;
  * never modifies report.json / evaluator v0 / selector / state schema;
  * never trusts a serialized `used_bytes` — it recomputes byte usage from the
    selected observation list through the one canonical serializer.

Operational result vs evaluation result are kept distinct:
  * a schema-valid `evaluation-v1.json` was emitted  -> operational exit 0,
    regardless of whether `overall` is PASS / FAIL / UNAVAILABLE;
  * inability to emit a trustworthy artifact (bad contract schema, bad evaluator
    identity, malformed invocation, undeclared input access, output-schema
    failure, internal exception) -> operational refusal, nonzero exit, no file.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile

import yaml
from jsonschema import Draft202012Validator

from .canonical import canonical_bytes, canonical_file_bytes, sha256_hex
from . import validity as vl

# ---------------------------------------------------------------------------
# Fixed layout constants (implementation closure, not case content).
# ---------------------------------------------------------------------------

TOOLS_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))          # .../tools
REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(TOOLS_DIR)))          # repo root
SCHEMA_DIR = os.path.join(os.path.dirname(TOOLS_DIR), "schema")

CONTRACT_SCHEMA = os.path.join(SCHEMA_DIR, "evaluation-contract-v1-r3.schema.json")
OUTPUT_SCHEMA = os.path.join(SCHEMA_DIR, "evaluation-output-v1-r2.schema.json")
MANIFEST_SCHEMA = os.path.join(SCHEMA_DIR, "evaluator-implementation-manifest-v0-r2.schema.json")

# The evaluator's own runtime closure, as repository-relative paths. Every code
# or schema resource opened as part of evaluator OPERATION appears here. Tests
# and fixture builders are deliberately excluded (production code never imports
# them). `evaluate_v1.py` (the CLI) is the entrypoint; `o7b1/evaluate_v1.py`
# (this module) is the module.
ENTRYPOINT_REL = "research/b1-context/tools/evaluate_v1.py"
MODULE_REL = "research/b1-context/tools/o7b1/evaluate_v1.py"
IMPL_CLOSURE_RELPATHS = [
    ENTRYPOINT_REL,
    MODULE_REL,
    "research/b1-context/tools/o7b1/__init__.py",
    "research/b1-context/tools/o7b1/canonical.py",
    "research/b1-context/tools/o7b1/validity.py",
    "research/b1-context/schema/evaluation-contract-v1-r3.schema.json",
    "research/b1-context/schema/evaluation-output-v1-r2.schema.json",
    "research/b1-context/schema/evaluator-implementation-manifest-v0-r2.schema.json",
]

CONTRACT_KEY_BY_ROLE = {
    "gold_state": "gold_state",
    "fixture_manifest": "fixture_manifest",
    "source_selectors": "source_selectors",
    "task_registry": "task_registry",
    "budget": "budget",
    "report": "r3_1_report",
    "projection_comparison": "r3_1_projection_comparison",
    "derived_manifest": "r3_1_derived_manifest",
    "task_file": "task_files",
    "questions_file": "question_files",
    "task_context": "task_contexts",
    "derived_body": "derived_bodies_private",
}
SINGLETON_ROLES = {"gold_state", "fixture_manifest", "source_selectors",
                   "task_registry", "budget", "report", "projection_comparison",
                   "derived_manifest"}
KEYED_ROLES = {"task_file", "questions_file", "task_context"}
BODY_ROLES = {"derived_body"}
STRUCTURED_ROLES = SINGLETON_ROLES | KEYED_ROLES

IN_FORCE = ("current", "pending")
BUDGET_REASON_RE = re.compile(r"\bbudget\b", re.IGNORECASE)


class Refusal(Exception):
    """Operational refusal: nonzero exit, no output file. Carries a taxonomy tag."""
    def __init__(self, layer: str, message: str):
        super().__init__(message)
        self.layer = layer
        self.message = message


# ---------------------------------------------------------------------------
# Small helpers
# ---------------------------------------------------------------------------

def _sha(b: bytes) -> str:
    return "sha256:" + sha256_hex(b)


def _canon_sha(obj) -> str:
    return "sha256:" + sha256_hex(canonical_bytes(obj))


def _load_schema_validator(path: str) -> Draft202012Validator:
    with open(path, "rb") as fh:
        return Draft202012Validator(json.loads(fh.read()))


def _parse_structured(role: str, raw: bytes):
    text = raw.decode("utf-8")
    if role in ("gold_state", "report", "projection_comparison", "derived_manifest") \
            or role == "task_context":
        return json.loads(text)
    # YAML-family inputs (also accepts JSON, which is a YAML subset)
    return yaml.safe_load(text)


def _git(args: list[str]) -> str | None:
    try:
        out = subprocess.run(["git", "-C", REPO_ROOT] + args,
                             capture_output=True, text=True, check=True)
        return out.stdout.strip()
    except Exception:
        return None


def execution_identity(mode: str) -> tuple[str, str]:
    commit = _git(["rev-parse", "HEAD"]) or ("0" * 40)
    tree = _git(["rev-parse", "HEAD^{tree}"]) or ("0" * 40)
    if mode in ("qualification", "authoritative"):
        status = _git(["status", "--porcelain"])
        if status is None:
            raise Refusal("implementation_identity",
                          "cannot determine worktree state for %s output" % mode)
        if status.strip():
            raise Refusal("implementation_identity",
                          "worktree is dirty; refusing to emit %s output" % mode)
    return commit, tree


# ---------------------------------------------------------------------------
# Evaluator implementation identity (self-manifest)
# ---------------------------------------------------------------------------

def _normalize_relpath(p: str) -> str:
    return os.path.normpath(p).replace(os.sep, "/")


def check_manifest_schema_and_mechanics(manifest: dict):
    """Schema + schema-inexpressible mechanics. Pure (no filesystem). Raises."""
    errs = sorted(_load_schema_validator(MANIFEST_SCHEMA).iter_errors(manifest),
                  key=lambda e: list(e.path))
    if errs:
        raise Refusal("implementation_identity",
                      "self-manifest fails schema: %s (%s)" % (errs[0].message, errs[0].json_path))
    norm = [_normalize_relpath(f["relative_path"]) for f in manifest["files"]]
    if len(set(norm)) != len(norm):
        raise Refusal("implementation_identity", "duplicate normalized relative_path")
    if _normalize_relpath(manifest["entrypoint"]) not in norm:
        raise Refusal("implementation_identity", "entrypoint absent from files closure")


def verify_manifest_files(manifest: dict):
    """Every listed file exists and its recorded digest matches on disk. Raises."""
    for f in manifest["files"]:
        abspath = os.path.join(REPO_ROOT, f["relative_path"])
        if not os.path.isfile(abspath):
            raise Refusal("implementation_identity", "closure file missing: %s" % f["relative_path"])
        with open(abspath, "rb") as fh:
            if _sha(fh.read()) != f["artifact_bytes_sha256"]:
                raise Refusal("implementation_identity",
                              "closure digest mismatch: %s" % f["relative_path"])


def _evaluator_o7b1_imports() -> set[str]:
    """The o7b1 submodules the evaluator's OWN source files import, by static AST
    scan of the entrypoint + module. This is pollution-free — unlike sys.modules,
    it is unaffected by whatever a shared test runner has imported elsewhere."""
    import ast
    mods: set[str] = set()
    for src_rel in (ENTRYPOINT_REL, MODULE_REL):
        with open(os.path.join(REPO_ROOT, src_rel), "rb") as fh:
            tree = ast.parse(fh.read())
        for node in ast.walk(tree):
            if isinstance(node, ast.ImportFrom):
                if node.level and node.level > 0:              # from . import x / from .x import y
                    if node.module:
                        mods.add(node.module.split(".")[0])
                    else:
                        for n in node.names:
                            mods.add(n.name)
                elif node.module == "o7b1":                    # from o7b1 import x
                    for n in node.names:
                        mods.add(n.name)
                elif node.module and node.module.startswith("o7b1."):  # from o7b1.x import y
                    mods.add(node.module.split(".")[1])
            elif isinstance(node, ast.Import):
                for n in node.names:
                    if n.name.startswith("o7b1."):
                        mods.add(n.name.split(".")[1])
    return mods


def unlisted_production_imports(declared_relpaths) -> list[str]:
    """o7b1 submodules the evaluator imports that are NOT in the declared closure."""
    declared = {_normalize_relpath(r) for r in declared_relpaths}
    out = []
    for mod in sorted(_evaluator_o7b1_imports()):
        rel = _normalize_relpath("research/b1-context/tools/o7b1/%s.py" % mod)
        if rel not in declared:
            out.append(rel)
    return out


def build_and_validate_manifest() -> dict:
    """Construct the evaluator's own implementation manifest from the FIXED
    declared closure and run every identity check: schema, path-uniqueness,
    entrypoint membership, existence, digest match, and no unlisted import."""
    files = []
    for rel in IMPL_CLOSURE_RELPATHS:
        abspath = os.path.join(REPO_ROOT, rel)
        if not os.path.isfile(abspath):
            raise Refusal("implementation_identity", "closure file missing: %s" % rel)
        with open(abspath, "rb") as fh:
            files.append({"relative_path": rel, "artifact_bytes_sha256": _sha(fh.read())})
    manifest = {
        "schema": "o7.b1.evaluator-implementation-manifest/v0",
        "files": sorted(files, key=lambda f: f["relative_path"]),
        "entrypoint": ENTRYPOINT_REL,
        "python_version": "%d.%d.%d" % sys.version_info[:3],
        "dependency_versions": {
            "PyYAML": getattr(yaml, "__version__", "unknown"),
            "jsonschema": _jsonschema_version(),
        },
        "schema_note": ("Path-uniqueness by normalized relative_path and "
                        "entrypoint-presence in files are evaluator consistency "
                        "checks, not expressible in JSON Schema draft 2020-12."),
    }
    check_manifest_schema_and_mechanics(manifest)
    verify_manifest_files(manifest)
    unlisted = unlisted_production_imports(IMPL_CLOSURE_RELPATHS)
    if unlisted:
        raise Refusal("implementation_identity",
                      "unlisted production module imported: %s" % ", ".join(unlisted))
    return manifest


def _jsonschema_version() -> str:
    try:
        import importlib.metadata as md
        return md.version("jsonschema")
    except Exception:
        return "unknown"


# ---------------------------------------------------------------------------
# Input closure loading (safe order, dual-digest, JSONL bodies)
# ---------------------------------------------------------------------------

def parse_input_arg(spec: str) -> tuple[str, str, str]:
    """`role:logical_id=path` -> (role, logical_id, path)."""
    if "=" not in spec or ":" not in spec.split("=", 1)[0]:
        raise Refusal("input_closure", "malformed --input (want role:logical=path): %r" % spec)
    lhs, path = spec.split("=", 1)
    role, logical = lhs.split(":", 1)
    role, logical, path = role.strip(), logical.strip(), path.strip()
    if role not in CONTRACT_KEY_BY_ROLE:
        raise Refusal("input_closure", "unknown input role: %r" % role)
    if not logical or not path:
        raise Refusal("input_closure", "empty logical id or path in %r" % spec)
    return role, logical, path


def _bound_entry(contract: dict, role: str, logical: str) -> dict | None:
    key = CONTRACT_KEY_BY_ROLE[role]
    bound = contract["bound_inputs"].get(key)
    if role in SINGLETON_ROLES:
        if isinstance(bound, dict) and bound.get("logical_id") == logical:
            return bound
        # singleton logical_id must match the declared one
        if isinstance(bound, dict):
            return None
        return None
    if role in KEYED_ROLES:
        return bound.get(logical) if isinstance(bound, dict) else None
    if role in BODY_ROLES:
        for e in (bound or []):
            if e.get("session_id") == logical:
                return e
    return None


def load_inputs(contract: dict, input_args: list[str]) -> dict:
    """Load every supplied artifact in safe order with full digest verification.

    Returns a structure with: parsed objects by role, the input inventory
    (for the output), the set of missing required logical ids, and the exact set
    of file paths opened (for the undeclared-access assertion)."""
    # Parse + duplicate detection.
    parsed_args = []
    seen = set()
    for spec in input_args:
        role, logical, path = parse_input_arg(spec)
        key = (role, logical)
        if key in seen:
            raise Refusal("input_closure", "duplicate mapping: %s:%s" % (role, logical))
        seen.add(key)
        parsed_args.append((role, logical, path))

    # Enumerate the FULL declared closure from the contract.
    declared = _declared_closure(contract)
    supplied = {(r, l) for (r, l, _) in parsed_args}

    # Extra (undeclared) mappings fail closed.
    for (r, l, _) in parsed_args:
        if (r, l) not in declared:
            raise Refusal("input_closure", "undeclared input mapping: %s:%s" % (r, l))

    inventory = []
    opened_paths = set()
    singleton = {}
    keyed = {"task_file": {}, "questions_file": {}, "task_context": {}}
    bodies = {}
    role_visibility = {"derived_body": "private"}

    for (role, logical, path) in parsed_args:
        if not os.path.isfile(path):
            raise Refusal("input_closure", "input path does not exist: %s" % path)
        with open(path, "rb") as fh:
            raw = fh.read()
        opened_paths.add(os.path.abspath(path))
        bound = _bound_entry(contract, role, logical)
        if bound is None:
            raise Refusal("input_closure",
                          "supplied %s:%s not bound by contract" % (role, logical))
        ab = _sha(raw)
        if bound.get("artifact_bytes_sha256") != ab:
            # A present-but-contradictory artifact is a FAIL of the consuming axis,
            # not a refusal; but a wrong-bytes STRUCTURED contract input undermines
            # the whole closure. We surface it as an input-integrity failure that
            # the contract_input_consistency axis will read.
            _mark_integrity(inventory, role, logical, "artifact_bytes_mismatch")
        byte_length = len(raw)

        if role in BODY_ROLES:
            _validate_jsonl(raw)  # fail-closed on blank/malformed lines
            rec_count = _jsonl_record_count(raw)
            if bound.get("record_count") is not None and bound["record_count"] != rec_count:
                _mark_integrity(inventory, role, logical, "record_count_mismatch")
            bodies[logical] = {"byte_length": byte_length, "record_count": rec_count,
                               "artifact_bytes_sha256": ab}
            inventory.append({"role": role, "logical_id": logical, "visibility": "private",
                              "artifact_kind": "body", "artifact_bytes_sha256": ab,
                              "byte_length": byte_length})
            continue

        # structured
        obj = _parse_structured(role, raw)
        co = _canon_sha(obj)
        if bound.get("canonical_object_sha256") != co:
            _mark_integrity(inventory, role, logical, "canonical_object_mismatch")
        rec = {"role": role, "logical_id": logical, "visibility": "public",
               "artifact_kind": "structured", "artifact_bytes_sha256": ab,
               "canonical_object_sha256": co, "byte_length": byte_length}
        inventory.append(rec)
        if role in SINGLETON_ROLES:
            singleton[role] = obj
        else:
            keyed[role][logical] = obj

    missing = sorted("%s:%s" % (r, l) for (r, l) in declared - supplied)

    inventory.sort(key=lambda r: (r["role"], r["logical_id"]))
    # Undeclared-access assertion: every opened evaluation-input path is one we
    # were explicitly handed. (Schemas / own source are implementation closure,
    # not evaluation inputs, and are never counted here.)
    return {
        "singleton": singleton, "keyed": keyed, "bodies": bodies,
        "inventory": inventory, "missing": missing, "opened_paths": opened_paths,
    }


def _declared_closure(contract: dict) -> set[tuple[str, str]]:
    closure = set()
    bi = contract["bound_inputs"]
    for role in SINGLETON_ROLES:
        key = CONTRACT_KEY_BY_ROLE[role]
        ent = bi.get(key)
        if isinstance(ent, dict) and ent.get("logical_id"):
            closure.add((role, ent["logical_id"]))
    for role in KEYED_ROLES:
        key = CONTRACT_KEY_BY_ROLE[role]
        for logical in (bi.get(key) or {}):
            closure.add((role, logical))
    for e in (bi.get("derived_bodies_private") or []):
        closure.add(("derived_body", e["session_id"]))
    return closure


def _mark_integrity(inventory: list, role: str, logical: str, kind: str):
    # Recorded on a side-channel list attribute so axis logic can read it.
    _INTEGRITY_FAILURES.append({"role": role, "logical_id": logical, "kind": kind})


_INTEGRITY_FAILURES: list = []


def _validate_jsonl(raw: bytes):
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        raise Refusal("input_closure", "derived body is not valid UTF-8")
    if text == "":
        raise Refusal("input_closure", "derived body is empty")
    lines = text.split("\n")
    # Exactly one trailing LF convention: last split element is "".
    if lines and lines[-1] == "":
        lines = lines[:-1]
    if not lines:
        raise Refusal("input_closure", "derived body has no records")
    for i, ln in enumerate(lines):
        if ln.strip() == "":
            raise Refusal("input_closure", "blank JSONL line at %d" % (i + 1))
        try:
            json.loads(ln)
        except Exception:
            raise Refusal("input_closure", "malformed JSONL line at %d" % (i + 1))


def _jsonl_record_count(raw: bytes) -> int:
    lines = raw.decode("utf-8").split("\n")
    if lines and lines[-1] == "":
        lines = lines[:-1]
    return len(lines)


# ---------------------------------------------------------------------------
# Gold-graph indices
# ---------------------------------------------------------------------------

def _gold_index(gold: dict) -> dict:
    obs = {o["observation_id"]: o for o in gold["observations"]}
    edges: dict[tuple[str, str], set] = {}
    for r in gold.get("relations", []):
        edges.setdefault((r["from"], r["kind"]), set()).add(r["to"])
    return {
        "obs": obs,
        "edges": edges,
        "relation_kinds": {r["kind"] for r in gold.get("relations", [])},
        "kinds_vocab": set(gold.get("topic_vocabulary", []) or []),  # placeholder, see below
        "topic_vocab": set(gold.get("topic_vocabulary", []) or []),
        "obs_kinds": {o.get("kind") for o in gold["observations"]},
    }


def _status_of(idx: dict, oid: str) -> str | None:
    o = idx["obs"].get(oid)
    return o.get("status") if o else None


def _gold_targets(idx: dict, frm: str, kind: str) -> set:
    return set(idx["edges"].get((frm, kind), set()))


# ---------------------------------------------------------------------------
# Contract-input consistency gates 01..11 (+ verifier constraints)
# ---------------------------------------------------------------------------

def _gate(gid: str, status: str, reason: str, evidence=None) -> dict:
    return {"gate_id": gid, "status": status, "reason": reason,
            "evidence": evidence if evidence is not None else {}}


def run_gates(contract: dict, loaded: dict, idx: dict) -> list[dict]:
    s = loaded["singleton"]
    keyed = loaded["keyed"]
    gold = s.get("gold_state")
    manifest = s.get("fixture_manifest")
    selectors = s.get("source_selectors")
    registry = s.get("task_registry")
    budget = s.get("budget")
    report = s.get("report")
    comparison = s.get("projection_comparison")
    dmanifest = s.get("derived_manifest")
    task_files = keyed["task_file"]
    q_files = keyed["questions_file"]
    contexts = keyed["task_context"]
    gates = []

    def unavailable(gid, what):
        gates.append(_gate(gid, "UNAVAILABLE", "missing required input: %s" % what))

    # gate-01 fixture identity agreement
    if any(x is None for x in (gold, manifest, selectors, registry, budget, report, comparison, dmanifest)):
        unavailable("gate-01", "one of the fixture-scoped structured inputs")
    else:
        fids = {
            "gold_state": gold.get("fixture_id"), "fixture_manifest": manifest.get("fixture_id"),
            "source_selectors": selectors.get("fixture_id"), "task_registry": registry.get("fixture_id"),
            "budget": budget.get("fixture_id"), "report": report.get("fixture_id"),
            "projection_comparison": comparison.get("fixture_id"), "derived_manifest": dmanifest.get("fixture_id"),
        }
        distinct = {v for v in fids.values() if v is not None}
        gates.append(_gate("gate-01", "PASS" if len(distinct) == 1 else "FAIL",
                           "fixture_id agreement" if len(distinct) == 1 else "fixture_id disagreement",
                           fids))

    # gate-02 registry filename and digest agreement
    if report is None or comparison is None or registry is None:
        unavailable("gate-02", "report/comparison/registry")
    else:
        reg_logical = contract["bound_inputs"]["task_registry"]["logical_id"]
        reg_ab = contract["bound_inputs"]["task_registry"]["artifact_bytes_sha256"]
        names_ok = report.get("registry_filename") == comparison.get("registry_filename") == reg_logical
        digs_ok = report.get("registry_digest") == comparison.get("registry_digest") == reg_ab
        gates.append(_gate("gate-02", "PASS" if names_ok and digs_ok else "FAIL",
                           "registry filename+digest agreement",
                           {"report_filename": report.get("registry_filename"),
                            "comparison_filename": comparison.get("registry_filename"),
                            "contract_logical_id": reg_logical, "names_ok": names_ok, "digests_ok": digs_ok}))

    # exact task-set agreement (gate-03)
    contract_tasks = set(contract["tasks"].keys())
    if report is None or comparison is None or not task_files or not contexts:
        unavailable("gate-03", "report/comparison/task_files/task_contexts")
    else:
        from_files = {tf.get("task_id") for tf in task_files.values()}
        from_report = {t.get("task_id") for t in report.get("tasks", [])}
        from_cmp = {t.get("task_id") for t in comparison.get("tasks", [])}
        from_ctx = set(contexts.keys())
        allsets = [contract_tasks, from_files, from_report, from_cmp, from_ctx]
        ok = all(x == contract_tasks for x in allsets)
        gates.append(_gate("gate-03", "PASS" if ok else "FAIL", "exact task-set agreement",
                           {"contract": sorted(contract_tasks), "task_files": sorted(from_files),
                            "report": sorted(from_report), "comparison": sorted(from_cmp),
                            "contexts": sorted(from_ctx)}))

    # gate-04 questions filename + dual-digest agreement
    if registry is None or not q_files or not task_files:
        unavailable("gate-04", "registry/questions_files/task_files")
    else:
        reg_map = {entry.get("task_file"): entry.get("questions_file")
                   for entry in registry.get("tasks", [])}
        tid_to_file = {tf.get("task_id"): fname for fname, tf in task_files.items()}
        problems = []
        for tid, tspec in contract["tasks"].items():
            qfile = tspec["questions_file"]
            reg_q = reg_map.get(tid_to_file.get(tid))
            if reg_q != qfile:
                problems.append("%s registry questions_file %r != contract %r" % (tid, reg_q, qfile))
            if qfile not in q_files:
                problems.append("missing questions_file input %s" % qfile)
            if contract["bound_inputs"]["question_files"].get(qfile) is None:
                problems.append("questions_file %s not bound" % qfile)
        gates.append(_gate("gate-04", "PASS" if not problems else "FAIL",
                           "questions filename + dual-digest agreement", {"problems": problems}))

    # gate-05 referenced question existence
    if not q_files:
        unavailable("gate-05", "questions_files")
    else:
        missing_q = []
        for tid, tspec in contract["tasks"].items():
            qfile = tspec["questions_file"]
            qobj = q_files.get(qfile, {})
            qids = {q["id"] for q in qobj.get("questions", [])}
            for rr in tspec.get("relation_requirements", []):
                if rr["question"] not in qids:
                    missing_q.append("%s: relation question %s" % (tid, rr["question"]))
            for fs in tspec.get("forbidden_stale_as_current", []):
                if fs["question"] not in qids:
                    missing_q.append("%s: forbidden question %s" % (tid, fs["question"]))
        gates.append(_gate("gate-05", "PASS" if not missing_q else "FAIL",
                           "referenced questions exist", {"missing": missing_q}))

    # Requirement-vs-gold gates 06/07/09/10 share a scan.
    if gold is None:
        for gid in ("gate-06", "gate-07", "gate-09", "gate-10"):
            unavailable(gid, "gold_state")
    else:
        reqs = []
        for tid, tspec in contract["tasks"].items():
            for rr in tspec.get("relation_requirements", []):
                reqs.append((tid, rr))
        governed = {(rr["from"], rr["kind"]) for _, rr in reqs}
        # gate-07 no invented requirement identity (from,kind must exist in gold)
        invented = sorted("%s/%s" % fk for fk in governed if not _gold_targets(idx, *fk)
                          or fk[0] not in idx["obs"])
        gates.append(_gate("gate-07", "PASS" if not invented else "FAIL",
                           "no invented relation requirement", {"invented": invented}))
        # gate-06 completeness: every gold edge of a governed (from,kind) covered
        covered = {}
        for _, rr in reqs:
            covered.setdefault((rr["from"], rr["kind"]), set()).update(rr["gold_derived_targets"])
        dropped = []
        for fk in governed:
            gold_e = _gold_targets(idx, *fk)
            if not gold_e <= covered.get(fk, set()):
                dropped.append("%s/%s missing %s" % (fk[0], fk[1], sorted(gold_e - covered[fk])))
        gates.append(_gate("gate-06", "PASS" if not dropped else "FAIL",
                           "every governed gold relation path expanded", {"dropped": dropped}))
        # gate-09 target exactness
        inexact = []
        for tid, rr in reqs:
            gold_e = _gold_targets(idx, rr["from"], rr["kind"])
            if set(rr["gold_derived_targets"]) != gold_e:
                inexact.append("%s %s/%s declared=%s gold=%s" % (
                    tid, rr["from"], rr["kind"], sorted(rr["gold_derived_targets"]), sorted(gold_e)))
        gates.append(_gate("gate-09", "PASS" if not inexact else "FAIL",
                           "gold-derived targets recomputed exactly", {"inexact": inexact}))
        # gate-10 target-status agreement + policy consistency
        status_bad = []
        for tid, rr in reqs:
            statuses = {t: _status_of(idx, t) for t in rr["gold_derived_targets"]}
            declared = set(rr.get("target_status", []))
            recomputed = {v for v in statuses.values() if v is not None}
            if declared != recomputed:
                status_bad.append("%s status declared=%s gold=%s" % (tid, sorted(declared), sorted(recomputed)))
            pol = rr["endpoint_policy"]
            if pol == "edge_witness" and any(v in IN_FORCE for v in statuses.values()):
                status_bad.append("%s edge_witness but a target is in-force" % tid)
            if pol == "all_current_targets_materialized" and any(v not in IN_FORCE for v in statuses.values()):
                status_bad.append("%s all_current but a target is stale" % tid)
        gates.append(_gate("gate-10", "PASS" if not status_bad else "FAIL",
                           "target-status agreement", {"problems": status_bad}))

    # gate-08 forbidden-stale set equality (declared == gold-stale among declared)
    if gold is None:
        unavailable("gate-08", "gold_state")
    else:
        fbad = []
        for tid, tspec in contract["tasks"].items():
            for fs in tspec.get("forbidden_stale_as_current", []):
                for oid in fs["ids"]:
                    st = _status_of(idx, oid)
                    if st is None:
                        fbad.append("%s forbidden %s absent from gold" % (tid, oid))
                    elif st in IN_FORCE:
                        fbad.append("%s forbidden %s is in-force in gold" % (tid, oid))
        gates.append(_gate("gate-08", "PASS" if not fbad else "FAIL",
                           "forbidden-stale set equality", {"problems": fbad}))

    # gate-11 unknown identifiers / closed-vocabulary rejection
    if gold is None or not q_files:
        unavailable("gate-11", "gold_state/questions_files")
    else:
        unknown = []
        obs_ids = set(idx["obs"].keys())
        for tid, tspec in contract["tasks"].items():
            for rr in tspec.get("relation_requirements", []):
                if rr["from"] not in obs_ids:
                    unknown.append("from %s" % rr["from"])
                for t in rr["gold_derived_targets"]:
                    if t not in obs_ids:
                        unknown.append("target %s" % t)
            for fs in tspec.get("forbidden_stale_as_current", []):
                for oid in fs["ids"]:
                    if oid not in obs_ids:
                        unknown.append("forbidden %s" % oid)
            qobj = q_files.get(tspec["questions_file"], {})
            for q in qobj.get("questions", []):
                for oid in q.get("required_observation_ids", []):
                    if oid not in obs_ids:
                        unknown.append("required %s" % oid)
        for tf in task_files.values():
            sels = tf.get("selectors", {})
            for k in sels.get("required_kinds", []):
                if k not in idx["obs_kinds"]:
                    unknown.append("kind %s" % k)
        gates.append(_gate("gate-11", "PASS" if not unknown else "FAIL",
                           "unknown identifiers/closed-vocabulary rejected", {"unknown": sorted(set(unknown))}))

    return gates


def verifier_constraints(contract: dict, loaded: dict, matrix: list) -> list[dict]:
    """Declared verifier-only consistency constraints (not schema-expressible)."""
    out = []
    tasks = sorted(contract["tasks"].keys())
    n = len(tasks)
    expected_pairs = n * (n - 1) // 2
    pe = contract["task_dependence"]["pair_expectations"]
    # endpoints distinct
    distinct = all(p["left"] != p["right"] for p in pe)
    out.append(_gate("verifier-pair-distinct", "PASS" if distinct else "FAIL",
                     "contract pair endpoints distinct"))
    # contract pair list exactly N-choose-2 over the task set, unique unordered
    keys = {frozenset((p["left"], p["right"])) for p in pe}
    complete = (len(pe) == expected_pairs == len(keys)
                and all({p["left"], p["right"]} <= set(tasks) for p in pe))
    out.append(_gate("verifier-pair-complete", "PASS" if complete else "FAIL",
                     "contract pair list is exactly N-choose-2",
                     {"expected": expected_pairs, "declared": len(pe)}))
    # output matrix exactly N-choose-2
    mkeys = {frozenset(m["pair"]) for m in matrix}
    out.append(_gate("verifier-matrix-complete",
                     "PASS" if len(matrix) == expected_pairs == len(mkeys) else "FAIL",
                     "output task-dependence matrix is exactly N-choose-2",
                     {"expected": expected_pairs, "emitted": len(matrix)}))
    return out


# ---------------------------------------------------------------------------
# Structural per-task computation
# ---------------------------------------------------------------------------

def _selected(context: dict) -> list[dict]:
    return context.get("selected", []) or []


def _selected_ids(context: dict) -> set:
    return {o["observation_id"] for o in _selected(context)}


def _projected_edges(context: dict) -> set:
    return {(r["from"], r["kind"], r["to"]) for r in context.get("relations", []) or []}


def _recompute_used_bytes(selected: list[dict]) -> int:
    return sum(len(canonical_bytes(o)) + 1 for o in selected)


def question_support(task_spec: dict, qobj: dict, selected_ids: set) -> list[dict]:
    out = []
    for q in qobj.get("questions", []):
        req = list(q.get("required_observation_ids", []))
        req_set = set(req)
        sel_req = sorted(req_set & selected_ids)
        missing = sorted(req_set - selected_ids)
        coverage = round(len(sel_req) / len(req_set), 6) if req_set else 1.0
        out.append({
            "question_id": q["id"],
            "required_observation_ids": sorted(req_set),
            "selected_required_ids": sel_req,
            "missing_required_ids": missing,
            "required_observation_coverage": coverage,
            "fully_supported": bool(req_set) and req_set <= selected_ids,
        })
    return out


def relation_support(task_spec: dict, idx: dict, context: dict, selected_ids: set) -> list[dict]:
    proj = _projected_edges(context)
    out = []
    for rr in task_spec.get("relation_requirements", []):
        frm, kind, pol = rr["from"], rr["kind"], rr["endpoint_policy"]
        gold_e = _gold_targets(idx, frm, kind)
        proj_targets = {to for (f, k, to) in proj if f == frm and k == kind}
        present = sorted(proj_targets & gold_e)
        fabricated = sorted(proj_targets - gold_e)
        rec = {
            "question_id": rr["question"], "from": frm, "kind": kind,
            "endpoint_policy": pol,
            "gold_edges": sorted(gold_e), "present_edges": present,
            "fabricated_edges": fabricated,
        }
        if pol == "edge_witness":
            rec["satisfied"] = (set(present) == gold_e) and not fabricated
        else:  # all_current_targets_materialized
            current_targets = {t for t in gold_e if _status_of(idx, t) in IN_FORCE}
            materialized = sorted(t for t in current_targets if t in selected_ids)
            unmaterialized = sorted(t for t in current_targets if t not in selected_ids)
            rec["materialized_targets"] = materialized
            rec["unmaterialized_current_targets"] = unmaterialized
            rec["satisfied"] = (set(present) == gold_e) and not fabricated and not unmaterialized
        out.append(rec)
    return out


def stale_state_checks(task_spec: dict, idx: dict, context: dict, selected_ids: set) -> list[dict]:
    selected = {o["observation_id"]: o for o in _selected(context)}
    endpoint_ids = set()
    for (f, k, to) in _projected_edges(context):
        endpoint_ids.add(to)
    out = []
    forbidden = []
    for fs in task_spec.get("forbidden_stale_as_current", []):
        forbidden.extend(fs["ids"])
    for oid in sorted(set(forbidden)):
        if oid in selected:
            st = selected[oid].get("status")
            if st in IN_FORCE:
                cls = "selected_as_current_or_pending"
            else:
                cls = "selected_with_stale_status"
        elif oid in endpoint_ids:
            cls = "stale_relation_endpoint_only"
        else:
            cls = "absent"
        out.append({"forbidden_id": oid, "classification": cls})
    return out


def projection_validity(task_file: dict, idx: dict, context: dict, budget: dict) -> tuple[str, dict]:
    selected = _selected(context)
    used = _recompute_used_bytes(selected)
    sel_spec = task_file.get("selectors", {})
    gold = {"observations": list(idx["obs"].values()),
            "relations": [{"from": f, "kind": k, "to": t}
                          for (f, k), tos in idx["edges"].items() for t in tos]}
    findings = vl.check_projection(gold, sel_spec, selected, used,
                                   {"byte_budget": budget["byte_budget"],
                                    "record_budget": budget["record_budget"]})
    findings["recomputed_used_bytes"] = used
    return ("PASS" if findings["valid"] else "FAIL"), findings


# ---------------------------------------------------------------------------
# All-pairs task dependence
# ---------------------------------------------------------------------------

def _omitted_reason(context: dict, oid: str) -> str | None:
    for o in context.get("omitted", []) or []:
        if o.get("observation_id") == oid:
            return o.get("reason")
    return None


def _difference_source(ctx_l: dict, ctx_r: dict, sl: set, sr: set) -> str:
    if sl == sr:
        return "none"
    distinguishing_budget = True
    for oid in (sl - sr):  # in left, not right -> right omitted it
        reason = _omitted_reason(ctx_r, oid)
        if not (reason and BUDGET_REASON_RE.search(reason)):
            distinguishing_budget = False
    for oid in (sr - sl):
        reason = _omitted_reason(ctx_l, oid)
        if not (reason and BUDGET_REASON_RE.search(reason)):
            distinguishing_budget = False
    return "budget_only" if distinguishing_budget else "selector_relevance"


def task_dependence_matrix(contract: dict, contexts: dict) -> list[dict]:
    rows = []
    seen_unordered = set()
    for p in contract["task_dependence"]["pair_expectations"]:
        left, right = p["left"], p["right"]
        key = frozenset((left, right))
        if key in seen_unordered:
            # A duplicate unordered pair cannot yield a second schema-valid matrix
            # row (task_dependence_matrix uniqueItems); the verifier-pair-complete
            # gate flags the contract's duplicate, so task_dependence FAILs cleanly.
            continue
        if left == right:
            # A self-pair cannot produce a schema-valid matrix row (pair uniqueItems);
            # it is flagged by the verifier-pair-distinct gate instead of emitting an
            # invalid artifact, so task_dependence FAILs cleanly with a valid output.
            continue
        cl, cr = contexts.get(left), contexts.get(right)
        if cl is None or cr is None:
            continue
        seen_unordered.add(key)
        sl, sr = _selected_ids(cl), _selected_ids(cr)
        set_eq = sl == sr
        left_sup = sr <= sl
        right_sup = sl <= sr
        symdiff = len(sl ^ sr)
        ctx_dig_eq = _canon_sha(cl) == _canon_sha(cr)
        shape = p["expected_shape"]
        if shape == "incomparable":
            satisfied = (not set_eq) and (not left_sup) and (not right_sup)
        elif shape == "left_strict_superset":
            satisfied = left_sup and not set_eq
        else:
            satisfied = False
        rows.append({
            "pair": [left, right],
            "set_equality": set_eq,
            "left_superset_of_right": left_sup,
            "right_superset_of_left": right_sup,
            "symmetric_difference_count": symdiff,
            "context_digest_equality": ctx_dig_eq,
            "difference_source": _difference_source(cl, cr, sl, sr),
            "expected_shape": shape,
            "shape_satisfied": satisfied,
        })
    rows.sort(key=lambda r: tuple(r["pair"]))
    return rows


# ---------------------------------------------------------------------------
# Axis aggregation + overall
# ---------------------------------------------------------------------------

def _axis_from_gates(gates: list[dict]) -> str:
    statuses = [g["status"] for g in gates]
    if not statuses:
        return "UNAVAILABLE"
    if "FAIL" in statuses:
        return "FAIL"
    if "UNAVAILABLE" in statuses:
        return "UNAVAILABLE"
    return "PASS"


def source_compilation(selectors: dict, dmanifest: dict, bodies: dict) -> tuple[str, dict]:
    if selectors is None or dmanifest is None:
        return "UNAVAILABLE", {"reason": "missing source_selectors or derived_manifest"}
    sessions_by_id = {sn["session_id"]: sn for sn in dmanifest.get("sessions", [])}
    req_avail = []
    completeness = []
    extractor_identity = {}
    any_fail = False
    any_missing = False
    for sel in selectors.get("sessions", []):
        sid = sel["session_id"]
        sn = sessions_by_id.get(sid)
        body = bodies.get(sid)
        present = sn is not None
        # A supplied body that disagrees is a hard FAIL; an absent session/body
        # (missing required material) is UNAVAILABLE, never a flattering FAIL.
        if sn is None or body is None:
            any_missing = True
            length_ok = digest_ok = rec_ok = False
        else:
            length_ok = sn.get("derived_byte_length") == body["byte_length"]
            digest_ok = sn.get("derived_digest") == body["artifact_bytes_sha256"]
            rec_ok = sn.get("record_count") == body["record_count"]
            raw_ok = bool(sn.get("raw_source"))
            extractor_ok = all(sn.get(k) for k in
                               ("extractor_id", "extractor_version", "extractor_impl_digest"))
            if not (length_ok and digest_ok and rec_ok and raw_ok and extractor_ok):
                any_fail = True
        completeness.append({"session_id": sid, "present": present,
                             "byte_length_matches": length_ok, "sha256_matches": digest_ok,
                             "record_count_reproduced": rec_ok})
        req_avail.append({"id": sid,
                          "requested_digest": (sn or {}).get("raw_source_digest", "sha256:" + "0" * 64),
                          "requested_bytes": (sn or {}).get("derived_byte_length", 0),
                          "cas_resolvable": "OK" if (sn and sn.get("raw_source")) else "UNAVAILABLE",
                          "round_trip": digest_ok})
        if sn:
            extractor_identity[sid] = {
                "extractor_id": sn.get("extractor_id", ""),
                "extractor_version": str(sn.get("extractor_version", "")),
                "extractor_impl_digest": sn.get("extractor_impl_digest", "sha256:" + "0" * 64),
            }
    if not selectors.get("sessions"):
        axis = "UNAVAILABLE"
    elif any_fail:
        axis = "FAIL"
    elif any_missing:
        axis = "UNAVAILABLE"
    else:
        axis = "PASS"
    return axis, {"requested_source_availability": req_avail,
                  "derived_session_completeness": completeness,
                  "extractor_identity": extractor_identity}


def compute_overall(axes: dict, axis_requirements: dict) -> str:
    required = [k for k, v in axis_requirements.items() if v == "required"]
    vals = [axes[k] for k in required]
    if "FAIL" in vals:
        return "FAIL"
    if "UNAVAILABLE" in vals:
        return "UNAVAILABLE"
    return "PASS"


# ---------------------------------------------------------------------------
# Output assembly + self-validation
# ---------------------------------------------------------------------------

METRIC_SEMANTICS = {
    "required_observation_coverage": (
        "structural: fraction of a question's required observation IDs present in "
        "the projection's selected set. NOT semantic recall or answerability."),
    "symmetric_difference_count": (
        "structural: number of observation IDs selected by exactly one task in a "
        "pair. A set-cardinality metric, not a quality score."),
}


def assemble(contract: dict, loaded: dict, idx: dict, manifest: dict,
             commit: str, tree: str) -> dict:
    s = loaded["singleton"]
    keyed = loaded["keyed"]
    gold = s.get("gold_state")
    budget = s.get("budget")
    contexts = keyed["task_context"]
    task_files = keyed["task_file"]
    q_files = keyed["questions_file"]

    gates = run_gates(contract, loaded, idx)
    matrix = task_dependence_matrix(contract, contexts) if gold is not None else []
    vgates = verifier_constraints(contract, loaded, matrix)
    consistency_gates = gates + vgates
    if _INTEGRITY_FAILURES:
        # A present-but-contradictory structured input (wrong artifact or canonical
        # digest, or a body whose record count disagrees) is a FAIL, not UNAVAILABLE.
        consistency_gates = consistency_gates + [_gate(
            "gate-input-integrity", "FAIL", "present-but-contradictory input artifact",
            {"failures": list(_INTEGRITY_FAILURES)})]

    # arms availability
    manifest_present = s.get("fixture_manifest") is not None
    nc_list = (s.get("fixture_manifest") or {}).get("negative_control", []) if manifest_present else []
    arms = {
        "full_derived": "AVAILABLE" if loaded["bodies"] else "UNAVAILABLE",
        "projection": "AVAILABLE" if contexts else "UNAVAILABLE",
        "negative_control": "AVAILABLE" if nc_list else "NOT_APPLICABLE",
    }

    tasks_out = []
    proj_axis_all = []
    qsupport_all = []
    rel_all = []
    stale_all = []
    if gold is not None and budget is not None:
        for tid in sorted(contract["tasks"].keys()):
            tspec = contract["tasks"][tid]
            context = contexts.get(tid, {})
            selected_ids = _selected_ids(context)
            qfile = tspec["questions_file"]
            qobj = q_files.get(qfile, {})
            tfile = None
            for tf in task_files.values():
                if tf.get("task_id") == tid:
                    tfile = tf
                    break
            qsup = question_support(tspec, qobj, selected_ids)
            rels = relation_support(tspec, idx, context, selected_ids)
            stale = stale_state_checks(tspec, idx, context, selected_ids)
            pv_axis, _pv = projection_validity(tfile or {}, idx, context, budget)
            tasks_out.append({
                "task_id": tid,
                "question_observation_support": qsup,
                "relation_support": rels,
                "projection_validity_axis": pv_axis,
                "stale_state_checks": stale,
            })
            proj_axis_all.append(pv_axis)
            qsupport_all.extend(qsup)
            rel_all.extend(rels)
            stale_all.extend(stale)

    # source compilation axis
    sc_axis, sc_detail = source_compilation(s.get("source_selectors"),
                                            s.get("derived_manifest"), loaded["bodies"])

    # axis values
    axes = {}
    axes["contract_input_consistency"] = _axis_from_gates(consistency_gates)
    axes["source_compilation"] = sc_axis
    if gold is None or budget is None:
        axes["projection_validity"] = "UNAVAILABLE"
        axes["question_observation_support"] = "UNAVAILABLE"
        axes["relation_support"] = "UNAVAILABLE"
        axes["stale_state_safety"] = "UNAVAILABLE"
        axes["task_dependence"] = "UNAVAILABLE"
    else:
        axes["projection_validity"] = "FAIL" if "FAIL" in proj_axis_all else (
            "PASS" if proj_axis_all else "UNAVAILABLE")
        axes["question_observation_support"] = "PASS" if (qsupport_all and all(
            q["fully_supported"] for q in qsupport_all)) else ("FAIL" if qsupport_all else "UNAVAILABLE")
        if rel_all:
            axes["relation_support"] = "PASS" if all(r["satisfied"] for r in rel_all) else "FAIL"
        else:
            axes["relation_support"] = "PASS"  # vacuous: no relation requirements declared
        axes["stale_state_safety"] = "FAIL" if any(
            c["classification"] == "selected_as_current_or_pending" for c in stale_all) else "PASS"
        vmatrix_ok = all(g["status"] == "PASS" for g in vgates)
        shapes_ok = all(m["shape_satisfied"] for m in matrix)
        axes["task_dependence"] = "PASS" if (matrix and shapes_ok and vmatrix_ok) else "FAIL"
    # negative control diagnostics: NOT_APPLICABLE if absent; UNAVAILABLE if present.
    axes["negative_control_diagnostics"] = "NOT_APPLICABLE" if not nc_list else "UNAVAILABLE"

    overall = compute_overall(axes, contract["axis_requirements"])

    contract_digest = {"artifact_bytes_sha256": loaded.get("contract_artifact_bytes"),
                       "canonical_object_sha256": _canon_sha(contract)}
    budget_input = next((r for r in loaded["inventory"] if r["role"] == "budget"), None)
    budget_digest = ({"artifact_bytes_sha256": budget_input["artifact_bytes_sha256"],
                      "canonical_object_sha256": budget_input["canonical_object_sha256"]}
                     if budget_input else
                     {"artifact_bytes_sha256": "sha256:" + "0" * 64,
                      "canonical_object_sha256": "sha256:" + "0" * 64})

    out = {
        "schema": "o7.b1.evaluation/v1",
        "contract_id": "o7.b1.evaluation-contract/v1",
        "contract_revision": contract["contract_revision"],
        "contract_digest": contract_digest,
        "evaluator_implementation_manifest_digest": _canon_sha(manifest),
        "evaluator_entrypoint_sha256": _closure_digest(manifest, ENTRYPOINT_REL),
        "evaluator_module_sha256": _closure_digest(manifest, MODULE_REL),
        "execution_checkout_commit": commit,
        "execution_checkout_tree": tree,
        "inputs": {"artifacts": _strip_inventory(loaded["inventory"])},
        "arms": arms,
        "source_compilation": dict({"axis": sc_axis}, **sc_detail),
        "tasks": tasks_out if tasks_out else [_empty_task_placeholder()],
        "task_dependence_matrix": matrix if matrix else [_empty_pair_placeholder()],
        "budget": {
            "budget_digest": budget_digest,
            "byte_budget": (budget or {}).get("byte_budget", 0),
            "record_budget": (budget or {}).get("record_budget", 0),
            "unit": "utf8_bytes+records",
            "counted_representation_id": "selector-v0-canonical-record-jsonl",
        },
        "outcome_axes": axes,
        "overall": overall,
        "metric_semantics": METRIC_SEMANTICS,
    }
    return out


def _strip_inventory(inv: list[dict]) -> list[dict]:
    out = []
    for r in inv:
        rec = {"role": r["role"], "logical_id": r["logical_id"], "visibility": r["visibility"],
               "artifact_kind": r["artifact_kind"], "artifact_bytes_sha256": r["artifact_bytes_sha256"],
               "byte_length": r["byte_length"]}
        if r["artifact_kind"] == "structured":
            rec["canonical_object_sha256"] = r["canonical_object_sha256"]
        out.append(rec)
    return out


def _closure_digest(manifest: dict, rel: str) -> str:
    for f in manifest["files"]:
        if f["relative_path"] == rel:
            return f["artifact_bytes_sha256"]
    return "sha256:" + "0" * 64


def _empty_task_placeholder() -> dict:
    return {"task_id": "no-task-available", "question_observation_support": [],
            "relation_support": [], "projection_validity_axis": "UNAVAILABLE",
            "stale_state_checks": []}


def _empty_pair_placeholder() -> dict:
    return {"pair": ["no-task-a", "no-task-b"], "set_equality": False,
            "left_superset_of_right": False, "right_superset_of_left": False,
            "symmetric_difference_count": 0, "context_digest_equality": False,
            "difference_source": "none", "expected_shape": "incomparable",
            "shape_satisfied": False}


def validate_output(out: dict):
    errs = sorted(_load_schema_validator(OUTPUT_SCHEMA).iter_errors(out),
                  key=lambda e: list(e.path))
    if errs:
        raise Refusal("output_schema", "evaluation output fails schema: %s at %s"
                      % (errs[0].message, errs[0].json_path))
    # metric_semantics key completeness (verifier-only)
    for m in METRIC_SEMANTICS:
        if not out["metric_semantics"].get(m):
            raise Refusal("output_schema", "metric_semantics missing key %s" % m)


# ---------------------------------------------------------------------------
# Orchestration + CLI
# ---------------------------------------------------------------------------

def run(argv: list[str]) -> int:
    _INTEGRITY_FAILURES.clear()
    ap = argparse.ArgumentParser(prog="evaluate_v1", add_help=True)
    ap.add_argument("--contract", required=True)
    ap.add_argument("--input", action="append", default=[], dest="inputs")
    ap.add_argument("--out", required=True)
    ap.add_argument("--mode", choices=["dev", "qualification", "authoritative"], default="dev")
    try:
        args = ap.parse_args(argv)
    except SystemExit:
        raise Refusal("internal_evaluator", "malformed invocation")

    manifest = build_and_validate_manifest()
    commit, tree = execution_identity(args.mode)

    with open(args.contract, "rb") as fh:
        craw = fh.read()
    contract_obj = yaml.safe_load(craw.decode("utf-8"))
    cerrs = sorted(_load_schema_validator(CONTRACT_SCHEMA).iter_errors(contract_obj),
                   key=lambda e: list(e.path))
    if cerrs:
        raise Refusal("contract_consistency", "contract fails schema: %s" % cerrs[0].message)

    loaded = load_inputs(contract_obj, args.inputs)
    loaded["contract_artifact_bytes"] = _sha(craw)
    idx = _gold_index(loaded["singleton"]["gold_state"]) if loaded["singleton"].get("gold_state") else {
        "obs": {}, "edges": {}, "relation_kinds": set(), "topic_vocab": set(), "obs_kinds": set()}

    out = assemble(contract_obj, loaded, idx, manifest, commit, tree)
    validate_output(out)

    data = canonical_file_bytes(out)
    _atomic_write(args.out, data)
    return 0


def _atomic_write(path: str, data: bytes):
    d = os.path.dirname(os.path.abspath(path))
    os.makedirs(d, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=d, prefix=".eval-v1-", suffix=".tmp")
    try:
        with os.fdopen(fd, "wb") as fh:
            fh.write(data)
        os.replace(tmp, path)
    finally:
        if os.path.exists(tmp):
            os.remove(tmp)


def main(argv: list[str] | None = None) -> int:
    argv = list(sys.argv[1:] if argv is None else argv)
    try:
        return run(argv)
    except Refusal as r:
        sys.stderr.write("REFUSAL[%s]: %s\n" % (r.layer, r.message))
        return 2
    except Exception as e:  # internal evaluator exception -> refusal
        sys.stderr.write("REFUSAL[internal_evaluator]: %r\n" % e)
        return 3

