"""Shared helpers for corpus compiler tests: build an isolated temp copy of the
corpus and point the CLI module's path globals at it, so tests can mutate case
bundles without touching the committed corpus."""
import json
import shutil
import sys
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))

import corpus_tool as ct  # noqa: E402

REAL_CORPUS = Path(__file__).resolve().parents[1]
DEMO_ID = "deterministic-log-demo"


def make_temp_corpus(tmp: Path) -> Path:
    shutil.copytree(REAL_CORPUS / "schemas", tmp / "schemas")
    shutil.copytree(REAL_CORPUS / "examples", tmp / "examples")
    shutil.copy2(REAL_CORPUS / "manifest.json", tmp / "manifest.json")
    shutil.copy2(REAL_CORPUS / "corpus-contract.json", tmp / "corpus-contract.json")
    ct.CORPUS_DIR = tmp
    ct.SCHEMAS_DIR = tmp / "schemas"
    ct.EXAMPLES_DIR = tmp / "examples"
    ct.MANIFEST_PATH = tmp / "manifest.json"
    ct.CONTRACT_PATH = tmp / "corpus-contract.json"
    return tmp


def load(path: Path):
    return json.loads(Path(path).read_text())


def dump(path: Path, obj):
    Path(path).write_text(json.dumps(obj, indent=2) + "\n")


def run_validate() -> list[str]:
    manifest = load(ct.MANIFEST_PATH)
    res = ct.Result()
    ct.validate_manifest(manifest, res)
    ids = sorted(set(manifest.get("demonstration_cases", []) + manifest.get("benchmark_cases", [])))
    for cid in ids:
        ct.validate_bundle(cid, manifest, res)
    return res.violations


def has_code(violations, code) -> bool:
    return any(v.startswith(f"[{code}]") for v in violations)
